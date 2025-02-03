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

use crate::*;

#[derive(Debug)]
pub struct Lightning {
    pub(crate) id: NodeId,
    pub(crate) state: State,
    pub(crate) context: Context,
    /// Whether node can uplink data (e.g. has reception to a gateway, ...)
    pub is_sink: bool,
    /// Payload to send to the parent
    pub payload: Option<Payload>,
}

impl protocol_api::Protocol for Lightning {
    type TimeMs = TimeMs;
    type NodeId = NodeId;
    type Message = Message;
    type Channel = Channel;
    type Payload = Payload;
    type Data = NodeData;

    fn new(id: Self::NodeId) -> Self {
        #[allow(clippy::assertions_on_constants)]
        const {
            assert!(RANDOM_CONNECT_RANGE_MS > CONNECT_RESPONSE_DELAY_MS);
        }
        Self {
            id,
            state: State::default(),
            context: Context::default(),
            is_sink: false,
            payload: None,
        }
    }

    fn progress<T: RngCore>(
        &mut self,
        time: Self::TimeMs,
        message: Option<Self::Message>,
        rng: T,
    ) -> (
        LightningAction,
        Option<impl IntoIterator<Item = Self::Data>>,
    ) {
        if let Some(message) = &message {
            event_log_msg!(time, self.id, message);
        };

        let next_state = self.next(time, message, rng);
        event_log_state!(time, self.id, &next_state);
        self.state = next_state;

        let uplink_data = if self.is_sink && !self.context.child_data.is_empty() {
            let mut data: OwnAndChildData =
                heapless::Vec::from_slice(self.context.child_data.as_slice()).unwrap();
            self.context.child_data.clear();
            if let Some(d) = self.payload.take() {
                data.push(NodeData {
                    source: self.id,
                    payload: d,
                })
                .unwrap();
            } else {
                warn!("no data set");
            }
            Some(data)
        } else {
            None
        };

        let action = self.state.get_action();
        event_log_action!(time, self.id, DisplayableAction(&action, time));
        (action, uplink_data)
    }

    fn id(&self) -> Self::NodeId {
        self.id
    }

    fn set_is_sink(&mut self, is_sink: bool) {
        self.is_sink = is_sink;
    }

    fn is_sink(&self) -> bool {
        self.is_sink
    }

    fn set_payload(&mut self, payload: Self::Payload) {
        self.payload.replace(payload);
    }

    fn has_payload(&self) -> bool {
        self.payload.is_some()
    }
}

struct DisplayableAction<'a>(&'a LightningAction, TimeMs);

/// action as JSON to make it parseable
macro_rules! action_to_json_string {
    ($fmt:expr,$write:tt,$action:expr,$time:expr) => {
        match $action {
            Action::None => $write!($fmt, "{{\"kind\":\"none\"}}"),
            Action::Wait { end } => {
                $write!(
                    $fmt,
                    "{{\"kind\":\"wait\",\"duration\":{}}}",
                    *end as i64 - $time as i64
                )
            }
            Action::Receive { end, channel } => $write!(
                $fmt,
                "{{\"kind\":\"receive\",\"duration\":{},\"channel\":{}}}",
                *end as i64 - $time as i64,
                channel,
            ),
            Action::Transmit {
                channel,
                delay,
                message: _,
            } => $write!(
                $fmt,
                "{{\"kind\":\"transmit\",\"channel\":{},\"delay_ms\":{}}}",
                channel,
                delay.unwrap_or(0),
            ),
        }
    };
}

impl core::fmt::Display for DisplayableAction<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        action_to_json_string!(f, write, self.0, self.1)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for DisplayableAction<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        use defmt::write;
        action_to_json_string!(fmt, write, self.0, self.1)
    }
}

impl Lightning {
    pub fn next_data_transmission(&self) -> TimeMs {
        self.context.windows.next_kind(WindowKind::Parent).unwrap()
    }
}
