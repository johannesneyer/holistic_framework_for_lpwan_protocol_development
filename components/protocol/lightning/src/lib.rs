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

#![cfg_attr(not(test), no_std)]

use heapless::Vec;
use rand_core::RngCore;

use protocol_api::*;

mod message;
mod window;
use crate::window::*;
mod channel;
use crate::channel::*;
mod context;
use crate::context::*;
mod states;
use crate::states::*;
mod event_log;
mod lightning;
mod state_machine;

pub use crate::{lightning::Lightning, message::Message, message::NodeData};

#[cfg(feature = "defmt")]
#[allow(unused_imports)]
use defmt::{debug, error, info, warn};

#[cfg(not(feature = "defmt"))]
#[allow(unused_imports)]
use log::{debug, error, info, warn};

// TODO: use proper time types
const MS_PER_S: TimeMs = 1000;
const MS_PER_MIN: TimeMs = 60 * MS_PER_S;

/// Time as milliseconds since start
pub type TimeMs = u64;
/// Node identifier
pub type NodeId = u32;
/// Channel index
pub type Channel = u8;
pub type OwnAndChildData = Vec<NodeData, { MAX_DESCENDANTS + 1 }>;

type LightningAction = Action<TimeMs, Message, Channel>;
type Payload = u16;
type Hops = u8;
type PotentialConnectBeacons = Vec<BeaconInfo, MAX_BEACONS_TO_COLLECT>;
type ChildData = Vec<NodeData, MAX_DESCENDANTS>;

// TODO: move these parameters elsewhere to make them configurable by the application

const BEACON_INTERVAL_MS: TimeMs = 30 * 1000;
const CHILD_DATA_INTERVAL_MIN: u8 = 5;
const NUM_CHANNELS: u8 = 8;
const MAX_CHILDREN: usize = 6;
const MAX_DESCENDANTS: usize = 16;
/// Maximum number of scheduled windows.
/// one window per child + connect window + parent window
/// (no beacon window because node does not send a beacon when it has max number of children)
const MAX_WINDOWS: usize = MAX_CHILDREN + 2;
const MAX_BEACONS_TO_COLLECT: usize = 16;

// the following parameter values are tweaked for the LoRa test network

const RESPONSE_LISTEN_DURATION_MS: TimeMs = 200;
/// Minimum distance that is maintained between windows.
/// Compensates for message time on air and time firmware requires to process actions. For the
/// beacon window (which is the window with the most messages) this is ~300ms in test network
/// (stm32wl55, SF8, BW 125KHz, 12 symbols preamble, 4/6 coding rate).
const MIN_WINDOW_CLEARANCE: TimeMs = 300;
const DATA_RECEIVE_WINDOW: TimeMs = 350;
const RANDOM_CONNECT_RANGE_MS: TimeMs = 400;
/// Must be longer than sender of the beacon takes to handle the SendConnect state
const CONNECT_RESPONSE_DELAY_MS: TimeMs = 100;
/// Maximum expected clock drift between two nodes.
const CLOCK_DRIFT_PPM: u32 = 30;
/// How long to enter receive mode at the time the best parent is expected to send a beacon.
const BEST_BEACON_LISTEN_TIME: TimeMs = MIN_WINDOW_CLEARANCE * 3;
/// Delay to give the receiver time to enter receive mode.
const SEND_DELAY: TimeMs = 5;

/// Extend duration to compensate for clock inaccuracies of two nodes.
///
/// When two nodes have agreed to talk to each other at a certain time in the future, this function
/// is used to adjust the senders wake up time to ensure the receiver is guaranteed to have entered
/// receive mode before the sender starts sending. Must be used to extend the wait time of each wait
/// state that precedes a send state. And the receive stop time.
pub(crate) fn adjust_for_clock_inaccuracies(duration: TimeMs) -> TimeMs {
    duration * (1_000_000 + CLOCK_DRIFT_PPM) as u64 / 1_000_000
}

/// Reduce duration to compensate for clock inaccuracies of two nodes.
///
/// Normally the sender extends its sleep time to compensate but this only works when two nodes have
/// agreed to talk to each other at a certain time. When waiting for a certain beacon to be sent
/// again this is not the case.
pub(crate) fn adjust_for_clock_inaccuracies_sub(duration: TimeMs) -> TimeMs {
    duration * (1_000_000 - CLOCK_DRIFT_PPM) as u64 / 1_000_000
}

/// Wraps defmt::write and returns Ok() to make it behave like core::write!.
#[cfg(feature = "defmt")]
#[macro_export]
macro_rules! defmt_write_wrapper {
    ($($arg:expr),*) => {{
        defmt::write!($($arg),*);
        Ok(())
    }};
}
