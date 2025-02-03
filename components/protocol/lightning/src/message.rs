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

use core::fmt::Display;
use serde::{Deserialize, Serialize};

use crate::*;

/// Lightning message
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Used for advertising the network
    Beacon {
        /// Number of hops to the next sink
        hops: Hops,
        /// Sender's children channel
        children_channel: u8,
        /// Sender's parent channel
        parent_channel: Option<u8>,
    },
    /// Used to connect to the network, sent as response to a beacon
    Connect {
        /// ID of the sender
        id: NodeId,
    },
    /// Used to acknowledge a connect message
    ConnectAck {
        /// Time offset in minutes at which receiver is expected to send a message
        next_window_min: u8,
        /// ID of the intended recipient
        id: NodeId,
    },
    /// Data of multiple nodes
    Data(OwnAndChildData),
    /// Used to acknowledge data messages
    DataAck {
        /// Time offset in minutes at which receiver is expected to send a message
        next_window_min: u8,
    },
    Nack,
}

/// message as JSON to make it parseable
macro_rules! message_to_json_string {
    ($fmt:expr,$write:tt,$message:expr) => {
        match $message {
            Message::Beacon {
                hops,
                children_channel,
                parent_channel,
            } => {
                $write!(
                    $fmt,
                    "{{\"kind\":\"beacon\",\"hops\":{},\"children_channel\":{}",
                    hops,
                    children_channel
                )?;
                if let Some(parent_channel) = parent_channel {
                    $write!($fmt, ",\"parent_channel\":{}", parent_channel)?;
                }
                $write!($fmt, "}}")
            }
            Message::Connect { id } => {
                $write!($fmt, "{{\"kind\":\"connect\",\"id\":{}}}", id)
            }
            Message::ConnectAck {
                next_window_min,
                id,
            } => {
                $write!(
                    $fmt,
                    "{{\"kind\":\"ack\",\"next_window_min\":{},\"id\":{}}}",
                    next_window_min,
                    id
                )
            }
            Message::Data(data) => {
                $write!($fmt, "{{\"kind\":\"data\",\"data\":[")?;
                let mut iter = data.iter();
                let mut next = iter.next();
                while let Some(NodeData { source, payload }) = next {
                    $write!($fmt, "{{\"source\":{},\"payload\":{}}}", source, payload)?;
                    next = iter.next();
                    if next.is_some() {
                        $write!($fmt, ",")?;
                    }
                }
                $write!($fmt, "]}}")
            }
            Message::DataAck { next_window_min } => {
                $write!(
                    $fmt,
                    "{{\"kind\":\"ack\",\"next_window_min\":{}}}",
                    next_window_min
                )
            }
            Message::Nack => {
                $write!($fmt, "{{\"kind\":\"nack\"}}")
            }
        }
    };
}

impl Display for Message {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        message_to_json_string!(fmt, write, self)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Message {
    fn format(&self, fmt: defmt::Formatter) {
        fn wrapper(msg: &Message, fmt: defmt::Formatter) -> core::fmt::Result {
            message_to_json_string!(fmt, defmt_write_wrapper, msg)
        }
        let _ = wrapper(self, fmt);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NodeData {
    pub source: NodeId,
    pub payload: Payload,
}

impl protocol_api::ProtocolData<Lightning> for NodeData {
    fn get_source(&self) -> NodeId {
        self.source
    }

    fn get_payload(&self) -> Payload {
        self.payload
    }
}
