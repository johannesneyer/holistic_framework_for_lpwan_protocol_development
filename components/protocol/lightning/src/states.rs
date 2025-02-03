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

/// Protocol state
///
/// Content of a state is what is required to produce the state's action or information for the
/// following state.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum State {
    #[default]
    Reset,
    ListenForBeacons {
        end: TimeMs,
        channel: Channel,
    },
    WaitBeforeFindingParent {
        end: TimeMs,
    },
    WaitForBestBeacon {
        best_beacon_hops: Hops,
        end: TimeMs,
    },
    ListenForBestBeacon {
        best_beacon_hops: Hops,
        end: TimeMs,
        channel: Channel,
    },
    DelayConnect {
        end: TimeMs,
        connect_ack_listen_time: TimeMs,
    },
    SendConnect {
        channel: Channel,
        id: NodeId,
        connect_ack_listen_time: TimeMs,
    },
    WaitForConnectAck {
        end: TimeMs,
        id: NodeId,
    },
    ListenForConnectAck {
        end: TimeMs,
        channel: Channel,
        id: NodeId,
    },
    Idle {
        end: TimeMs,
    },
    SendBeacon {
        channel: Channel,
        hops: Hops,
        children_channel: Channel,
        parent_channel: Option<Channel>,
    },
    ListenForData {
        end: TimeMs,
        channel: Channel,
    },
    SendDataAck {
        child_window: Window,
        channel: Channel,
        next_child_window_min: u8,
    },
    SendData {
        channel: Channel,
        data: OwnAndChildData,
    },
    ListenForDataAck {
        end: TimeMs,
        channel: Channel,
    },
    ListenForConnect {
        end: TimeMs,
        channel: Channel,
    },
    DelayConnectAck {
        end: TimeMs,
        id: NodeId,
    },
    SendConnectAck {
        child_window: Window,
        channel: Channel,
        next_child_window_min: u8,
        id: NodeId,
    },
}

impl State {
    /// Returns a state's action
    pub(crate) fn get_action(&self) -> LightningAction {
        match self {
            State::Reset => Action::None,
            State::ListenForBeacons { channel, end } => Action::Receive {
                end: *end,
                channel: *channel,
            },
            State::WaitBeforeFindingParent { end } => Action::Wait { end: *end },
            State::WaitForBestBeacon {
                end,
                best_beacon_hops: _,
            } => Action::Wait { end: *end },
            State::ListenForBestBeacon {
                end,
                channel,
                best_beacon_hops: _,
            } => Action::Receive {
                end: *end,
                channel: *channel,
            },
            State::SendConnect {
                channel,
                id,
                connect_ack_listen_time: _,
            } => Action::Transmit {
                channel: *channel,
                message: Message::Connect { id: *id },
                delay: Some(SEND_DELAY),
            },
            State::WaitForConnectAck { end, id: _ } => Action::Wait { end: *end },
            State::ListenForConnectAck {
                channel,
                end,
                id: _,
            } => Action::Receive {
                end: *end,
                channel: *channel,
            },
            State::Idle { end } => Action::Wait { end: *end },
            State::SendBeacon {
                channel,
                hops,
                children_channel,
                parent_channel,
            } => Action::Transmit {
                channel: *channel,
                message: Message::Beacon {
                    hops: *hops,
                    children_channel: *children_channel,
                    parent_channel: *parent_channel,
                },
                delay: Some(SEND_DELAY),
            },
            State::ListenForConnect { channel, end } => Action::Receive {
                end: *end,
                channel: *channel,
            },
            State::SendConnectAck {
                child_window: _,
                channel,
                next_child_window_min: next_window_min,
                id,
            } => Action::Transmit {
                channel: *channel,
                message: Message::ConnectAck {
                    next_window_min: *next_window_min,
                    id: *id,
                },
                delay: Some(SEND_DELAY),
            },
            State::DelayConnectAck { end, id: _ } => Action::Wait { end: *end },
            State::ListenForData { channel, end } => Action::Receive {
                end: *end,
                channel: *channel,
            },
            State::SendData { channel, data } => Action::Transmit {
                channel: *channel,
                message: Message::Data(data.clone()),
                delay: Some(SEND_DELAY),
            },
            State::DelayConnect {
                end,
                connect_ack_listen_time: _,
            } => Action::Wait { end: *end },
            State::SendDataAck {
                child_window: _,
                channel,
                next_child_window_min: next_window_min,
            } => Action::Transmit {
                channel: *channel,
                message: Message::DataAck {
                    next_window_min: *next_window_min,
                },
                delay: Some(SEND_DELAY),
            },
            State::ListenForDataAck { channel, end } => Action::Receive {
                channel: *channel,
                end: *end,
            },
        }
    }

    /// state as JSON to make it parseable
    fn state_as_string(&self) -> &str {
        match self {
            State::DelayConnect { .. } => "DelayConnect",
            State::DelayConnectAck { .. } => "DelayConnectAck",
            State::Idle { .. } => "Idle",
            State::ListenForBeacons { .. } => "ListenForBeacons",
            State::ListenForBestBeacon { .. } => "ListenForBestBeacon",
            State::ListenForConnect { .. } => "ListenForConnect",
            State::ListenForConnectAck { .. } => "ListenForConnectAck",
            State::ListenForData { .. } => "ListenForData",
            State::ListenForDataAck { .. } => "ListenForDataAck",
            State::Reset => "Reset",
            State::SendBeacon { .. } => "SendBeacon",
            State::SendConnect { .. } => "SendConnect",
            State::SendConnectAck { .. } => "SendConnectAck",
            State::SendData { .. } => "SendData",
            State::SendDataAck { .. } => "SendDataAck",
            State::WaitBeforeFindingParent { .. } => "WaitBeforeFindingParent",
            State::WaitForBestBeacon { .. } => "WaitForBestBeacon",
            State::WaitForConnectAck { .. } => "WaitForConnectAck",
        }
    }
}

impl core::fmt::Display for State {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(fmt, "{}", self.state_as_string())
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for State {
    fn format(&self, fmt: defmt::Formatter) {
        use defmt::write;
        write!(fmt, "{}", self.state_as_string())
    }
}
