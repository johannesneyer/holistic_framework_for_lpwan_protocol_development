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

use std::{
    fs::File,
    io::{self, Write},
};

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use rand::RngCore;

use crate::*;

/// Type that adds meta data to protocol
#[derive(Debug)]
pub struct ProtocolWrapper {
    protocol: ProtocolImpl,
    location: Coordinates,
    receiving_channel: Option<Channel>,
}

impl ProtocolWrapper {
    pub fn new(protocol: ProtocolImpl, location: Coordinates) -> Self {
        Self {
            protocol,
            location,
            receiving_channel: None,
        }
    }

    pub fn location(&self) -> &Coordinates {
        &self.location
    }

    pub fn receiving_channel(&self) -> Option<Channel> {
        self.receiving_channel
    }

    #[doc(alias = "lightning::Lightning::id")]
    pub fn id(&self) -> NodeId {
        self.protocol.id()
    }

    #[must_use]
    #[doc(alias = "lightning::Lightning::progress")]
    pub fn progress(
        &mut self,
        time: TimeMs,
        message: Option<Message>,
        mut rng: impl RngCore,
    ) -> (Action<TimeMs, Message, Channel>, Option<Vec<Data>>) {
        let (action, uplink_data) = self.protocol.progress(time, message, &mut rng);

        self.receiving_channel = if let Action::Receive { channel, .. } = action {
            Some(channel)
        } else {
            None
        };

        let uplink_data = uplink_data.map(Vec::from_iter);

        if !self.protocol.is_sink() && uplink_data.is_some() {
            panic!("bug: node that is not a sink returned uplink data");
        }

        // dummy payload
        if !self.protocol.has_payload() {
            self.protocol.set_payload(Payload::default());
        }

        (action, uplink_data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    Transmit,
    Receive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageWrapper {
    pub kind: MessageKind,
    pub channel: Channel,
    pub message: Message,
    /// whether message collided with another
    pub is_corrupt: bool,
}

impl MessageWrapper {
    pub fn new(kind: MessageKind, message: Message, channel: Channel) -> Self {
        Self {
            kind,
            message,
            channel,
            is_corrupt: false,
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub struct Event {
    pub time: TimeMs,
    pub node_id: NodeId,
    pub message: Option<MessageWrapper>,
}

impl Event {
    pub fn new(time: TimeMs, node_id: NodeId, message: Option<MessageWrapper>) -> Self {
        Self {
            time,
            node_id,
            message,
        }
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time)
    }
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coordinates {
    pub x: i64,
    pub y: i64,
}

impl From<(i64, i64)> for Coordinates {
    fn from(value: (i64, i64)) -> Self {
        Self {
            x: value.0,
            y: value.1,
        }
    }
}

pub fn get_distance(a: &Coordinates, b: &Coordinates) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
}

/// Check if two nodes are in range of each other
pub fn check_visibility_based_on_distance(
    a: &ProtocolWrapper,
    b: &ProtocolWrapper,
    range: u32,
) -> bool {
    get_distance(a.location(), b.location()) < range as f32
}

/// Get nodes that are listening on the specified channel and that are in range of the sender
pub fn get_recipients(
    sender: &ProtocolWrapper,
    channel: Channel,
    nodes: &[ProtocolWrapper],
    mut check_visibility: impl FnMut(&ProtocolWrapper, &ProtocolWrapper) -> bool,
) -> Vec<NodeId> {
    nodes
        .iter()
        .filter(|node| {
            node.receiving_channel() == Some(channel)
                && check_visibility(sender, node)
                && node.id() != sender.id()
        })
        .map(|node| node.id())
        .collect()
}

/// Forward messages to nodes that are visible to the sender
#[allow(clippy::too_many_arguments)]
pub fn forward_message(
    departure_time: TimeMs,
    sender_id: NodeId,
    sender_channel: Channel,
    message: &Message,
    event_queue: &mut SortedLinkedList<Event>,
    nodes: &[ProtocolWrapper],
    mut check_visibility: impl FnMut(&ProtocolWrapper, &ProtocolWrapper) -> bool,
    packet_error_rate_ppt: Option<u32>,
    mut rng: impl RngCore,
) {
    let mut recipients = get_recipients(
        &nodes[sender_id as usize],
        sender_channel,
        nodes,
        &mut check_visibility,
    );

    // check for collisions with messages on the same channel from nodes that are visible to the
    // potential recipient
    for event in event_queue.iter_mut() {
        if departure_time >= event.time || departure_time + TIME_ON_AIR <= event.time - TIME_ON_AIR
        {
            // events don't overlap
            // events are sorted by time so all remaining events don't overlap as well
            break;
        }

        let event_message = match event.message.as_mut() {
            Some(message) => message,
            None => continue,
        };

        let channel = match event_message {
            MessageWrapper {
                kind: MessageKind::Receive,
                channel,
                ..
            } => channel,
            _ => continue,
        };

        if sender_channel != *channel {
            continue;
        }

        recipients.retain(|r| {
            if check_visibility(&nodes[sender_id as usize], &nodes[*r as usize]) {
                warn!(
                    "message collision at node {:x}:\nmessage from node {:x}: {}\nmessage from node {:x}: {}",
                    *r, sender_id, message, event.node_id, event_message.message
                );
                event_message.is_corrupt = true;
                false
            } else {
                true
            }
        })
    }

    if recipients.is_empty() {
        return;
    }

    info!(
        "forwarding message from {:x} to {:x?}",
        sender_id, recipients
    );

    // drop messages based on packet error rate
    if let Some(per) = packet_error_rate_ppt {
        recipients.retain(|_| {
            if rng.next_u32() % 1000 < per {
                warn!("packet error simulation: dropping message");
                false
            } else {
                true
            }
        });
    }

    // cancel receive time out events of recipients
    event_queue.retain(|e| !recipients.contains(&e.node_id));

    for recipient in recipients {
        event_queue.push(Event::new(
            departure_time + TIME_ON_AIR,
            recipient,
            Some(MessageWrapper::new(
                MessageKind::Receive,
                message.clone(),
                sender_channel,
            )),
        ));
    }
}

pub fn write_metadata_to_file(
    nodes: &[ProtocolWrapper],
    node_range: u32,
    file_path: &str,
) -> io::Result<()> {
    let mut node_loc_file = File::create(file_path)?;
    node_loc_file.write_all(format!("{{\n\"node_range\":{node_range},\n").as_bytes())?;
    node_loc_file.write_all("\"nodes\":\n[\n".as_bytes())?;
    let mut node_iter = nodes.iter();
    let mut next = node_iter.next();
    while let Some(node) = next {
        node_loc_file.write_all(
            format!(
                "{{\"id\":{},\"location\":{{\"x\":{},\"y\":{}}}}}",
                node.id(),
                node.location().x,
                node.location().y
            )
            .as_bytes(),
        )?;
        next = node_iter.next();
        if next.is_some() {
            node_loc_file.write_all(",".as_bytes())?;
        }
        node_loc_file.write_all("\n".as_bytes())?;
    }
    node_loc_file.write_all("]\n}\n".as_bytes())?;
    Ok(())
}
