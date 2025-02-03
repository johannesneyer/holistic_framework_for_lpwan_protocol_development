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
use heapless::Vec;

impl Lightning {
    /// Get next state
    ///
    /// Each state's exit functionality is implemented here.
    #[must_use]
    pub(crate) fn next(
        &mut self,
        time: TimeMs,
        message: Option<Message>,
        mut rng: impl RngCore,
    ) -> State {
        match (&mut self.state, message) {
            (State::Reset, None) => {
                event_log_reset!(time, self.id, self.is_sink);
                self.context.reset();
                if self.is_sink {
                    self.context.hops_to_sink = Some(0);
                    self.context.channels.set_random_children_channel(&mut rng);
                    self.context.windows.push(Window {
                        kind: WindowKind::Beacon,
                        start: time + rng.next_u32() as TimeMs % BEACON_INTERVAL_MS,
                    });
                    State::Idle {
                        end: self.context.windows.next(),
                    }
                } else {
                    State::WaitBeforeFindingParent {
                        end: time + rng.next_u32() as TimeMs % BEACON_INTERVAL_MS,
                    }
                }
            }

            (State::WaitBeforeFindingParent { .. }, None) => State::ListenForBeacons {
                channel: self.context.channels.public,
                end: time + BEACON_INTERVAL_MS,
            },

            (State::ListenForBeacons { end, channel, .. }, Some(Message::Beacon { hops, .. })) => {
                if hops == 0 {
                    State::WaitForBestBeacon {
                        best_beacon_hops: hops,
                        end: time + adjust_for_clock_inaccuracies_sub(BEACON_INTERVAL_MS),
                    }
                } else {
                    self.context
                        .potential_connect_beacons
                        .push(BeaconInfo {
                            hops,
                            time_seen: time,
                        })
                        .unwrap();
                    State::ListenForBeacons {
                        end: *end,
                        channel: *channel,
                    }
                }
            }
            (State::ListenForBeacons { .. }, None) => {
                if self.context.potential_connect_beacons.is_empty() {
                    State::WaitBeforeFindingParent {
                        end: time
                            + BEACON_INTERVAL_MS / 2
                            + rng.next_u32() as TimeMs % BEACON_INTERVAL_MS,
                    }
                } else {
                    let best_beacon = self
                        .context
                        .potential_connect_beacons
                        .iter()
                        .reduce(|best_beacon, beacon| {
                            if beacon.hops < best_beacon.hops {
                                beacon
                            } else {
                                best_beacon
                            }
                        })
                        .unwrap();
                    State::WaitForBestBeacon {
                        best_beacon_hops: best_beacon.hops,
                        end: best_beacon.time_seen
                            + adjust_for_clock_inaccuracies_sub(BEACON_INTERVAL_MS),
                    }
                }
            }
            (State::ListenForBeacons { end, channel, .. }, Some(_)) => {
                // ignore non beacon messages
                State::ListenForBeacons {
                    end: *end,
                    channel: *channel,
                }
            }

            (
                State::WaitForBestBeacon {
                    best_beacon_hops, ..
                },
                None,
            ) => State::ListenForBestBeacon {
                best_beacon_hops: *best_beacon_hops,
                end: time + BEST_BEACON_LISTEN_TIME,
                channel: self.context.channels.public,
            },

            (
                State::ListenForBestBeacon {
                    best_beacon_hops,
                    end,
                    channel,
                },
                Some(Message::Beacon {
                    hops,
                    children_channel: parents_children_channel,
                    parent_channel: parents_parent_channel,
                }),
            ) => {
                if hops != *best_beacon_hops {
                    warn!("received wrong beacon");
                    // wrong beacon
                    State::ListenForBestBeacon {
                        best_beacon_hops: *best_beacon_hops,
                        end: *end,
                        channel: *channel,
                    }
                } else {
                    match hops.checked_add(1) {
                        Some(hops) => self.context.hops_to_sink = Some(hops),
                        None => panic!("hop count too large"),
                    }
                    self.context.channels.parent = Some(parents_children_channel);
                    self.context.channels.parents_parent_channel = parents_parent_channel;
                    State::DelayConnect {
                        end: time + rng.next_u32() as TimeMs % RANDOM_CONNECT_RANGE_MS,
                        connect_ack_listen_time: time
                            + RANDOM_CONNECT_RANGE_MS
                            + CONNECT_RESPONSE_DELAY_MS,
                    }
                }
            }
            (State::ListenForBestBeacon { .. }, None) => {
                self.context.potential_connect_beacons.clear();
                State::WaitBeforeFindingParent {
                    end: time
                        + BEACON_INTERVAL_MS / 2
                        + rng.next_u32() as TimeMs % BEACON_INTERVAL_MS,
                }
            }
            (
                State::ListenForBestBeacon {
                    best_beacon_hops,
                    end,
                    channel,
                },
                Some(_),
            ) => {
                // ignore non beacon messages
                State::ListenForBestBeacon {
                    best_beacon_hops: *best_beacon_hops,
                    end: *end,
                    channel: *channel,
                }
            }

            (
                State::DelayConnect {
                    connect_ack_listen_time,
                    ..
                },
                None,
            ) => State::SendConnect {
                channel: self.context.channels.parent.unwrap(),
                id: self.id,
                connect_ack_listen_time: *connect_ack_listen_time,
            },

            (
                State::SendConnect {
                    id,
                    connect_ack_listen_time,
                    ..
                },
                None,
            ) => State::WaitForConnectAck {
                end: *connect_ack_listen_time,
                id: *id,
            },

            (State::WaitForConnectAck { id, .. }, None) => State::ListenForConnectAck {
                channel: self.context.channels.parent.unwrap(),
                end: time + RESPONSE_LISTEN_DURATION_MS,
                id: *id,
            },

            (
                State::ListenForConnectAck { id, .. },
                Some(Message::ConnectAck {
                    next_window_min,
                    id: ack_id,
                }),
            ) if *id == ack_id => {
                info!("successfully connected to parent");
                self.context.channels.set_random_children_channel(&mut rng);
                self.context.windows.push(Window {
                    kind: WindowKind::Parent,
                    start: time
                        + adjust_for_clock_inaccuracies(next_window_min as TimeMs * MS_PER_MIN),
                });
                self.context.windows.push(Window {
                    kind: WindowKind::Beacon,
                    // add some randomness to reduce the probability of being in sync with siblings
                    start: time
                        + BEACON_INTERVAL_MS
                        + rng.next_u32() as TimeMs % BEACON_INTERVAL_MS,
                });
                State::Idle {
                    end: self.context.windows.next(),
                }
            }
            (State::ListenForConnectAck { id, .. }, message) => {
                warn!("expected connect ack for id {:x}, got: {:?}", id, message);
                State::Reset
            }

            (State::Idle { .. }, None) => {
                let next_window = self.context.windows.pop();
                assert_eq!(
                    next_window.start, time,
                    "{:?}, {}",
                    next_window, self.context.windows
                );
                match next_window {
                    Window {
                        kind: WindowKind::Beacon,
                        start: _,
                    } => State::SendBeacon {
                        channel: self.context.channels.public,
                        hops: self.context.hops_to_sink.unwrap(),
                        children_channel: self.context.channels.children.unwrap(),
                        parent_channel: self.context.channels.parent,
                    },
                    Window {
                        kind: WindowKind::Child,
                        start: _,
                    } => State::ListenForData {
                        channel: self.context.channels.children.unwrap(),
                        end: time + DATA_RECEIVE_WINDOW,
                    },
                    Window {
                        kind: WindowKind::Parent,
                        start: _,
                    } => {
                        let mut data: OwnAndChildData =
                            Vec::from_slice(self.context.child_data.as_slice()).unwrap();
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
                        State::SendData {
                            channel: self.context.channels.parent.unwrap(),
                            data,
                        }
                    }
                }
            }

            (State::SendBeacon { .. }, None) => {
                self.context.windows.push(Window {
                    kind: WindowKind::Beacon,
                    start: time + BEACON_INTERVAL_MS,
                });
                State::ListenForConnect {
                    channel: self.context.channels.children.unwrap(),
                    end: time + adjust_for_clock_inaccuracies(RANDOM_CONNECT_RANGE_MS + SEND_DELAY),
                }
            }

            (State::ListenForConnect { end, .. }, Some(Message::Connect { id })) => {
                event_log_new_child!(time, self.id, id);
                // Delay sending connect ack to after end of listening for connect window to avoid
                // collisions with other potential connect messages.
                State::DelayConnectAck {
                    end: *end + adjust_for_clock_inaccuracies(CONNECT_RESPONSE_DELAY_MS),
                    id,
                }
            }
            (State::ListenForConnect { .. }, None) => State::Idle {
                end: self.context.windows.next(),
            },
            (State::ListenForConnect { end, channel }, Some(message)) => {
                warn!("expected connect, got: {:?}", message);
                if *end > time {
                    State::ListenForConnect {
                        end: *end,
                        channel: *channel,
                    }
                } else {
                    State::Idle {
                        end: self.context.windows.next(),
                    }
                }
            }

            (State::DelayConnectAck { id, .. }, None) => {
                let mut child_window = Window {
                    kind: WindowKind::Child,
                    start: time + CHILD_DATA_INTERVAL_MIN as TimeMs * MS_PER_MIN,
                };
                child_window.delay(&self.context.windows, WindowDelayIncrement::Minutes);
                let next_child_window_min = child_window.get_offset_min(time) as u8;
                State::SendConnectAck {
                    child_window,
                    channel: self.context.channels.children.unwrap(),
                    next_child_window_min,
                    id: *id,
                }
            }

            (
                State::SendConnectAck {
                    next_child_window_min,
                    ref mut child_window,
                    ..
                },
                None,
            ) => {
                // adjust window start time to compensate for message time on air
                child_window.start = time + *next_child_window_min as TimeMs * MS_PER_MIN;
                self.context.windows.push(child_window.clone());
                if self.context.windows.is_full() {
                    self.context.windows.pop_kind(WindowKind::Beacon);
                }
                State::Idle {
                    end: self.context.windows.next(),
                }
            }

            (State::SendData { .. }, None) => State::ListenForDataAck {
                channel: self.context.channels.parent.unwrap(),
                end: time + RESPONSE_LISTEN_DURATION_MS,
            },

            (State::ListenForDataAck { .. }, Some(Message::DataAck { next_window_min })) => {
                info!("parent acked data");
                self.context.windows.push(Window {
                    kind: WindowKind::Parent,
                    start: time
                        + adjust_for_clock_inaccuracies(next_window_min as TimeMs * MS_PER_MIN),
                });
                State::Idle {
                    end: self.context.windows.next(),
                }
            }
            (State::ListenForDataAck { .. }, message) => {
                error!("expected ack, got: {:?}", message);
                error!("resetting protocol");
                State::Reset
            }

            (State::ListenForData { .. }, Some(Message::Data(child_data))) => {
                // TODO: handle case where child data buffer is not big enough
                self.context
                    .child_data
                    .extend_from_slice(child_data.as_slice())
                    .expect("child data buffer not big enough");
                // info!("new child data: {:?}", child_data);
                let mut child_window = Window {
                    kind: WindowKind::Child,
                    start: time + CHILD_DATA_INTERVAL_MIN as TimeMs * MS_PER_MIN,
                };
                child_window.delay(&self.context.windows, WindowDelayIncrement::Minutes);
                let next_child_window_min = child_window.get_offset_min(time) as u8;
                State::SendDataAck {
                    child_window,
                    channel: self.context.channels.children.unwrap(),
                    next_child_window_min,
                }
            }
            (State::ListenForData { .. }, message) => {
                error!("expected data, got: {:?}", message);
                error!("child gone");
                State::Idle {
                    end: self.context.windows.next(),
                }
            }

            (
                State::SendDataAck {
                    next_child_window_min,
                    ref mut child_window,
                    ..
                },
                None,
            ) => {
                // adjust window start time to compensate for message time on air
                child_window.start = time + *next_child_window_min as TimeMs * MS_PER_MIN;
                self.context.windows.push(child_window.clone());
                State::Idle {
                    end: self.context.windows.next(),
                }
            }

            //
            // invalid state/action combinations
            //
            (State::SendDataAck { .. }, Some(_)) => unreachable!(),
            (State::SendConnectAck { .. }, Some(_)) => unreachable!(),
            (State::Idle { .. }, Some(_)) => unreachable!(),
            (State::SendBeacon { .. }, Some(_)) => unreachable!(),
            (State::SendConnect { .. }, Some(_)) => unreachable!(),
            (State::SendData { .. }, Some(_)) => unreachable!(),
            (State::WaitBeforeFindingParent { .. }, Some(_)) => unreachable!(),
            (State::WaitForBestBeacon { .. }, Some(_)) => unreachable!(),
            (State::DelayConnect { .. }, Some(_)) => unreachable!(),
            (State::Reset, Some(_)) => unreachable!(),
            (State::DelayConnectAck { .. }, Some(_)) => unreachable!(),
            (State::WaitForConnectAck { .. }, Some(_)) => unreachable!(),
        }
    }
}
