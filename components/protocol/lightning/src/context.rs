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
pub(crate) struct Context {
    pub(crate) channels: Channels,
    pub(crate) windows: Windows,
    pub(crate) hops_to_sink: Option<Hops>,
    pub(crate) child_data: ChildData,
    pub(crate) potential_connect_beacons: PotentialConnectBeacons,
}

/// Stores beacon info for selecting a parent
#[derive(Debug)]
pub(crate) struct BeaconInfo {
    /// Time beacon was received
    pub(crate) time_seen: TimeMs,
    /// Hop count from beacon
    pub(crate) hops: Hops,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            channels: Channels::default(),
            hops_to_sink: None,
            child_data: heapless::Vec::default(),
            windows: Windows::new(MIN_WINDOW_CLEARANCE),
            potential_connect_beacons: PotentialConnectBeacons::new(),
        }
    }
}

impl Context {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}
