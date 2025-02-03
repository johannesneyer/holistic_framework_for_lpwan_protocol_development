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

use rand_core::RngCore;

/// A states' action
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Action<TIME, MESSAGE, CHANNEL> {
    /// Do nothing
    None,
    /// Wait until `end`
    Wait { end: TIME },
    /// Listen for message until `end`
    Receive { end: TIME, channel: CHANNEL },
    /// Send message with optional delay
    Transmit {
        channel: CHANNEL,
        message: MESSAGE,
        delay: Option<TIME>,
    },
}

pub trait ProtocolData<P: Protocol + ?Sized> {
    fn get_source(&self) -> P::NodeId;
    fn get_payload(&self) -> P::Payload;
}

pub trait Protocol {
    type TimeMs: Copy + Eq + Ord;
    type NodeId: Copy + Eq;
    type Channel: Copy + Eq;
    type Message: Clone + PartialEq;
    type Payload: Clone + Default;
    type Data: Clone + ProtocolData<Self>;

    fn new(id: Self::NodeId) -> Self;

    /// Make progress in state machine
    ///
    /// Returns action to execute and node data if node is a sink. This function must be called
    /// again after the returned action has been executed.
    #[must_use]
    #[allow(clippy::type_complexity)]
    fn progress<T: RngCore>(
        &mut self,
        time: Self::TimeMs,
        message: Option<Self::Message>,
        rng: T,
    ) -> (
        Action<Self::TimeMs, Self::Message, Self::Channel>,
        Option<impl IntoIterator<Item = Self::Data>>,
    );

    /// Get the node's ID
    fn id(&self) -> Self::NodeId;

    fn set_is_sink(&mut self, is_sink: bool);

    fn is_sink(&self) -> bool;

    fn set_payload(&mut self, payload: Self::Payload);

    fn has_payload(&self) -> bool;
}
