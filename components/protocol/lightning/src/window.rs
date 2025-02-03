//  _____       ______   ____
// |_   _|     |  ____|/ ____|  Institute of Embedded Systems
//   | |  _ __ | |__  | (___    Zurich University of Applied Sciences
//   | | | '_ \|  __|  \___ \   8401 Winterthur, Switzerland
//  _| |_| | | | |____ ____) |
// |_____|_| |_|______|_____/
//
// Copyright 2025 Institute of Embedded Systems at Zurich University of Applied Sciences.
// All rights reserved.
// SPDX-License-Identifier: MIT

use core::mem;
use heapless::sorted_linked_list;

use crate::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum WindowKind {
    Beacon,
    Child,
    Parent,
}

impl WindowKind {
    /// Return approximate maximum duration of window.
    ///
    /// This duration does not contain the message time on air. The `MIN_WINDOW_CLEARANCE` parameter
    /// makes sure that there is enough time for the message time on air.
    #[cfg(not(test))]
    fn duration(&self) -> TimeMs {
        match self {
            WindowKind::Beacon => {
                RANDOM_CONNECT_RANGE_MS
                    + CONNECT_RESPONSE_DELAY_MS
                    + RESPONSE_LISTEN_DURATION_MS
                    + 2 * SEND_DELAY
            }
            WindowKind::Parent => RESPONSE_LISTEN_DURATION_MS + SEND_DELAY,
            WindowKind::Child => DATA_RECEIVE_WINDOW + SEND_DELAY,
        }
    }
}

#[derive(Debug, Clone, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct Window {
    pub(crate) kind: WindowKind,
    pub(crate) start: TimeMs,
}

impl Window {
    /// Delay window such that it does not overlap with others
    ///
    /// The window is delayed in integer multiples of the given increment. The delayed window keeps
    /// a distance of `windows.clearance` to adjacent windows.
    pub(crate) fn delay(&mut self, windows: &Windows, increment: WindowDelayIncrement) {
        let mut iter = windows.queue.iter();

        let first_window = match iter.next() {
            Some(w) => w,
            None => return,
        };

        if self.end() + windows.clearance() <= first_window.start {
            return;
        }

        let increment = increment.as_ms();

        // increment start of window such that it ends up after `time`
        let delay_window = |time: TimeMs| {
            self.start + increment * time.saturating_sub(self.start).div_ceil(increment)
        };

        // iterate over window gaps
        let mut previous_end = first_window.end();
        for gap in iter.map(|window| {
            let gap = previous_end + windows.clearance()..=window.start - windows.clearance();
            previous_end = window.end();
            gap
        }) {
            let delayed_window_start = delay_window(*gap.start());
            if gap.contains(&delayed_window_start)
                && gap.contains(&(delayed_window_start + self.duration()))
            {
                self.start = delayed_window_start;
                return;
            }
        }

        // return end of last window when no gap was found
        self.start = delay_window(previous_end + windows.clearance);
    }

    /// Returns offset of window to given time in minutes
    ///
    /// Panics when offset contains fractional minutes.
    pub(crate) fn get_offset_min(&self, time: TimeMs) -> usize {
        let offset = self.start - time;
        assert_eq!(offset % MS_PER_MIN, 0);
        (offset / MS_PER_MIN) as usize
    }

    fn duration(&self) -> TimeMs {
        self.kind.duration()
    }

    fn end(&self) -> TimeMs {
        self.start + self.duration()
    }
}

impl Ord for Window {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.start.cmp(&other.start)
    }
}

impl PartialOrd for Window {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Window {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start
    }
}

pub(crate) enum WindowDelayIncrement {
    Milliseconds,
    Minutes,
}

impl WindowDelayIncrement {
    fn as_ms(&self) -> TimeMs {
        match self {
            WindowDelayIncrement::Milliseconds => 1,
            WindowDelayIncrement::Minutes => MS_PER_MIN,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Windows {
    queue: sorted_linked_list::SortedLinkedList<
        Window,
        sorted_linked_list::LinkedIndexU8,
        sorted_linked_list::Min,
        MAX_WINDOWS,
    >,
    clearance: TimeMs,
}

impl Windows {
    pub(crate) fn new(clearance: TimeMs) -> Self {
        Windows {
            queue: sorted_linked_list::SortedLinkedList::new_u8(),
            clearance,
        }
    }

    pub(crate) fn clearance(&self) -> TimeMs {
        self.clearance
    }

    /// Add window to queue
    pub(crate) fn push(&mut self, mut new_window: Window) {
        // resolve overlapping windows
        // loop needed as window might overlap with multiple windows
        loop {
            if let Some(mut overlapping_window) = self.pop_overlapping_window(&new_window) {
                match (new_window.kind, overlapping_window.kind) {
                    (_, WindowKind::Beacon) => {
                        // beacon can just be delayed
                        overlapping_window.start = new_window.end() + self.clearance();
                        overlapping_window.delay(self, WindowDelayIncrement::Milliseconds);
                        self.queue.push(overlapping_window).unwrap();
                    }
                    (WindowKind::Beacon, _) => {
                        // beacon can just be delayed
                        self.queue.push(overlapping_window).unwrap();
                        new_window.delay(self, WindowDelayIncrement::Milliseconds);
                        self.queue.push(new_window).unwrap();
                        break;
                    }
                    (WindowKind::Child, WindowKind::Parent) => {
                        warn!("child window conflicts with parent window: dropping child");
                        self.queue.push(overlapping_window).unwrap();
                        break;
                    }
                    (WindowKind::Parent, WindowKind::Child) => {
                        warn!("child window conflicts with parent window: dropping child");
                    }
                    (WindowKind::Parent, WindowKind::Parent) => unreachable!(),
                    (WindowKind::Child, WindowKind::Child) => unreachable!(),
                }
            } else {
                self.queue.push(new_window).unwrap();
                break;
            }
        }
        // print window queue for debugging
        // warn!("{}", self);
    }

    /// Remove next window from the queue
    pub(crate) fn pop(&mut self) -> Window {
        self.queue.pop().unwrap()
    }

    /// Pop next window with given kind
    pub(crate) fn pop_kind(&mut self, kind: WindowKind) -> Option<Window> {
        self.queue
            .find_mut(|w| mem::discriminant(&w.kind) == mem::discriminant(&kind))
            .map(|w| w.pop())
    }

    /// Return start of next window
    pub(crate) fn next(&mut self) -> TimeMs {
        self.queue.peek().unwrap().start
    }

    /// Return start of next window with given kind
    pub(crate) fn next_kind(&self, kind: WindowKind) -> Option<TimeMs> {
        for window in self.queue.iter() {
            if mem::discriminant(&window.kind) == mem::discriminant(&kind) {
                return Some(window.start);
            }
        }
        None
    }

    /// Check if this node can no longer accept children
    pub(crate) fn is_full(&self) -> bool {
        self.queue
            .iter()
            .filter(|window| matches!(window.kind, WindowKind::Child { .. }))
            .count()
            == MAX_CHILDREN
    }

    fn pop_overlapping_window(&mut self, new_window: &Window) -> Option<Window> {
        let clearance = self.clearance();
        self.queue
            .find_mut(|window| {
                !(new_window.end() + clearance <= window.start
                    || new_window.start >= window.end() + clearance)
            })
            .map(|w| w.pop())
    }
}

macro_rules! windows_to_string {
    ($fmt:expr,$write:tt,$window:expr) => {{
        let mut ranges: heapless::Vec<_, MAX_WINDOWS> =
            heapless::Vec::from_iter($window.queue.iter());
        ranges.sort_unstable();
        $write!($fmt, "window queue:")?;
        for (i, window) in $window.queue.iter().enumerate() {
            $write!($fmt, "\n{:?}   \t", window.kind)?;
            for _ in 0..i {
                $write!($fmt, "                  ")?;
            }
            if window.duration() == 0 {
                $write!($fmt, "    ")?;
            }
            $write!(
                $fmt,
                "{} - {} ",
                window.start,
                window.start + window.duration() as TimeMs
            )?;
        }
        Ok(())
    }};
}

impl core::fmt::Display for Window {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // windows_to_string!(fmt, write, self)
        write!(fmt, "{:?} ({} - {})", self.kind, self.start, self.end())
    }
}

impl core::fmt::Display for Windows {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        windows_to_string!(fmt, write, self)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Windows {
    fn format(&self, fmt: defmt::Formatter) {
        fn wrapper(msg: &Windows, fmt: defmt::Formatter) -> core::fmt::Result {
            windows_to_string!(fmt, defmt_write_wrapper, msg)
        }
        let _ = wrapper(self, fmt);
    }
}

#[cfg(test)]
mod tests {
    use crate::{WindowKind::*, *};

    impl WindowKind {
        pub(crate) fn duration(&self) -> TimeMs {
            match self {
                Beacon => 200,
                Parent => 100,
                Child => 100,
            }
        }
    }

    #[test]
    fn basic() {
        let mut windows = Windows::new(50);
        windows.push(Window {
            kind: Parent,
            start: 10,
        });
        windows.push(Window {
            kind: Beacon,
            start: 0,
        });
        // println!("{}", windows);
        assert_eq!(
            windows.pop(),
            Window {
                kind: Parent,
                start: 10
            }
        );
        assert_eq!(
            windows.pop(),
            Window {
                kind: Beacon,
                start: 160
            }
        );
    }

    #[test]
    fn delay_beacon_when_parent_overlap() {
        let mut windows = Windows::new(50);
        windows.push(Window {
            kind: Beacon,
            start: 100,
        });
        let mut beacon_window = windows.pop_kind(WindowKind::Beacon).unwrap();
        windows.push(Window {
            kind: Parent,
            start: 100,
        });
        beacon_window.delay(&windows, WindowDelayIncrement::Milliseconds);
        windows.push(beacon_window);
        // println!("{}", windows);
        assert_eq!(
            windows.pop(),
            Window {
                kind: Parent,
                start: 100
            }
        );
        assert_eq!(
            windows.pop(),
            Window {
                kind: Beacon,
                start: 250
            }
        );
    }

    #[test]
    fn parent_child_conflict() {
        // window conflict cannot be resolved, expect child window to be removed
        let mut windows = Windows::new(10);
        windows.push(Window {
            kind: Child,
            start: 100,
        });
        windows.push(Window {
            kind: Parent,
            start: 150,
        });
        // println!("{}", windows);
        assert_eq!(
            windows.pop(),
            Window {
                kind: Parent,
                start: 150
            }
        );
        assert!(windows.queue.is_empty());
    }

    #[test]
    fn delay() {
        let windows = &mut Windows::new(50);

        windows.push(Window {
            kind: Beacon,
            start: 1000,
        });
        windows.push(Window {
            kind: Child,
            start: 1000,
        });

        println!("{windows}");

        assert_eq!(
            windows.pop(),
            Window {
                kind: Child,
                start: 1000
            }
        );
        assert_eq!(
            windows.pop(),
            Window {
                kind: Beacon,
                start: 1150
            }
        );
    }
}
