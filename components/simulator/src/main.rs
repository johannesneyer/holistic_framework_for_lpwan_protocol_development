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

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use rand::{RngCore, SeedableRng};
use std::{cmp::max, env, time::Duration};

use sorted_linked_list::SortedLinkedList;

use protocol_api::{Action, Protocol, ProtocolData};

// TODO: use feature flags to switch between different protocol implementations
use lightning::Lightning as ProtocolImpl;

type Channel = <ProtocolImpl as Protocol>::Channel;
type Data = <ProtocolImpl as Protocol>::Data;
type Message = <ProtocolImpl as Protocol>::Message;
type NodeId = <ProtocolImpl as Protocol>::NodeId;
type Payload = <ProtocolImpl as Protocol>::Payload;
type TimeMs = <ProtocolImpl as Protocol>::TimeMs;

mod logger;
mod sim;

use crate::sim::*;

/// Minimum distance between nodes. Avoids overlapping nodes.
const MIN_NODE_DISTANCE: u32 = 10;
/// Height and width of area
const AREA_SIZE: u32 = 100;
/// Approximate time a message spends in the air.
/// In the LoRa test network (SF8, BW 125KHz, 12 symbols preamble, 4/6 coding rate) a 10 byte payload has a time-on-air of 100 ms.
const TIME_ON_AIR: TimeMs = 80;
const STARTUP_DELAY_RANGE_MS: TimeMs = 5 * 60 * 1000;
/// Probability of a transmission error in parts per thousand
const PACKET_ERROR_RATE_PPT: Option<u32> = None;

const EVENT_FILE_PATH: &str = "/tmp/protocol_events.csv";
const SIMULATION_METADATA_FILE_PATH: &str = "/tmp/protocol_sim_meta.json";

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut rng_seed: u64 = 0;
    let mut num_nodes: usize = 15;
    let mut num_sinks: Option<usize> = None;
    let mut simulation_minutes: usize = 90;
    // Nodes that are farther apart from each other than this value are not in range of each other
    let mut range: u32 = 30;

    for chunk in args[1..].chunks_exact(2) {
        let (arg, val) = (&chunk[0], &chunk[1]);
        match arg.as_str() {
            "--seed" => {
                rng_seed = val.parse().expect("invalid rng seed");
            }
            "--nodes" => {
                num_nodes = val.parse().expect("invalid number of sinks");
            }
            "--sinks" => {
                num_sinks = Some(val.parse().expect("invalid number of nodes"));
            }
            "--range" => {
                range = val.parse().expect("invalid range");
            }
            "--time_min" => {
                simulation_minutes = val.parse().expect("invalid number of simulation minutes");
            }
            _ => panic!("unknown argument: {}", arg),
        }
    }

    let num_sinks: usize = match num_sinks {
        Some(ns) => ns,
        None => max(1, 33 * num_nodes / 100),
    };

    assert!(num_sinks <= num_nodes, "can't have more sinks than nodes");

    let mut rng = get_rng(rng_seed);

    let mut node_coordinates = Vec::with_capacity(num_nodes);
    while node_coordinates.len() != num_nodes {
        let coordinates = Coordinates {
            x: (rng.next_u32() % AREA_SIZE) as i64,
            y: (rng.next_u32() % AREA_SIZE) as i64,
        };
        if !node_coordinates
            .iter()
            .any(|c| get_distance(&coordinates, c) < MIN_NODE_DISTANCE as f32)
        {
            node_coordinates.push(coordinates);
        }
    }

    // create nodes
    let mut nodes: Vec<ProtocolWrapper> = Vec::with_capacity(num_nodes);
    let mut sinks_remaining = num_sinks;
    for _ in 0..num_nodes {
        // vector index is node id
        let mut protocol: ProtocolImpl = Protocol::new(nodes.len() as NodeId);
        protocol.set_is_sink(sinks_remaining > 0);
        nodes.push(ProtocolWrapper::new(
            protocol,
            node_coordinates.remove(rng.next_u32() as usize % node_coordinates.len()),
        ));
        sinks_remaining = sinks_remaining.saturating_sub(1);
    }

    write_metadata_to_file(&nodes, range, SIMULATION_METADATA_FILE_PATH).unwrap();

    logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();

    let data = run(nodes, simulation_minutes, rng, |a, b| {
        check_visibility_based_on_distance(a, b, range)
    });

    let mut nodes_that_sent_data: Vec<_> = data.iter().map(|nd| nd.get_source()).collect();
    nodes_that_sent_data.sort_unstable();
    nodes_that_sent_data.dedup();
    println!("{:?}", nodes_that_sent_data);
}

fn get_rng(rng_seed: u64) -> impl RngCore {
    println!("RNG seed: {rng_seed:#x}");
    rand_chacha::ChaCha8Rng::seed_from_u64(rng_seed)
}

fn run(
    mut nodes: Vec<ProtocolWrapper>,
    minutes: usize,
    mut rng: impl RngCore,
    mut check_visibility: impl FnMut(&ProtocolWrapper, &ProtocolWrapper) -> bool,
) -> Vec<Data> {
    // Stores timestamps of the next time a node can make progress
    let mut event_queue = SortedLinkedList::new();

    let mut data = Vec::default();

    let mut time: TimeMs = 0;

    // random delay to mimic asynchronous startup
    for node in &nodes {
        let startup_delay = rng.next_u32() as TimeMs % STARTUP_DELAY_RANGE_MS;
        event_queue.push(Event::new(startup_delay, node.id(), None));
    }

    loop {
        assert!(
            event_queue.len() == nodes.len(),
            "bug: invalid number of elements in event queue: {} (!= {})\n{:#?}",
            event_queue.len(),
            nodes.len(),
            event_queue
        );

        let event = event_queue.pop().unwrap();

        assert!(event.time >= time, "bug: time cannot go backwards");

        if event.time > time {
            // advance time
            time = event.time;
            info!(
                "{:=^30}{:=^30}",
                format!(" node {:x} ", event.node_id),
                format!(
                    " {}min {:>7?} ({}ms) ",
                    time / (1000 * 60),
                    Duration::from_millis(time % (1000 * 60)),
                    time
                )
            );
        } else {
            info!("{:-^30}{:-^30}", format!(" node {:x} ", event.node_id), "");
        }

        // forward message to nodes that are in range of the sender and listening on the sender's
        // channel
        if let Some(MessageWrapper {
            kind: MessageKind::Transmit,
            channel,
            ref message,
            is_corrupt: _,
        }) = event.message
        {
            forward_message(
                time,
                event.node_id,
                channel,
                message,
                &mut event_queue,
                &nodes,
                &mut check_visibility,
                PACKET_ERROR_RATE_PPT,
                &mut rng,
            );
            // sender makes progress after message is sent
            event_queue.push(Event::new(time + TIME_ON_AIR, event.node_id, None));
            continue;
        }

        let received_message = match event.message {
            Some(MessageWrapper {
                kind: MessageKind::Receive,
                channel: _,
                message,
                is_corrupt,
            }) if !is_corrupt => Some(message),
            _ => None,
        };

        let (action, uplink_data) =
            nodes[event.node_id as usize].progress(time, received_message.clone(), &mut rng);

        match action {
            Action::Wait { end } | Action::Receive { end, .. } => {
                if end < time {
                    panic!("end of action is in the past ({} < {})", end, time);
                }
            }
            Action::Transmit { .. } | Action::None => {}
        }

        if let Some(uplink_data) = uplink_data {
            data.extend(uplink_data);
        }

        match action {
            Action::Wait { end } => {
                info!("waiting for {:?}", Duration::from_millis(end - time));
                event_queue.push(Event::new(end, event.node_id, None));
            }
            Action::Receive { end, channel } => {
                info!(
                    "receiving for {:?} on channel {}",
                    Duration::from_millis(end - time),
                    channel
                );
                event_queue.push(Event::new(end, event.node_id, None));
            }
            Action::Transmit {
                channel,
                message,
                delay,
            } => {
                info!("transmitting message on channel {}", channel);
                event_queue.push(Event::new(
                    time + delay.unwrap_or(0),
                    event.node_id,
                    Some(MessageWrapper::new(MessageKind::Transmit, message, channel)),
                ));
            }
            Action::None => {
                event_queue.push(Event::new(time, event.node_id, None));
            }
        }

        if minutes <= (time / (1000 * 60)) as usize {
            break;
        }
    }

    data
}

#[cfg(test)]
mod tests {
    use crate::*;
    use std::collections::HashMap;

    #[derive(Debug)]
    struct VisibilitytMap(HashMap<(NodeId, NodeId), bool>);

    impl VisibilitytMap {
        pub fn get(&self, a: NodeId, b: NodeId) -> bool {
            *self.0.get(&Self::sort((a, b))).unwrap_or(&false)
        }

        fn sort(pair: (NodeId, NodeId)) -> (NodeId, NodeId) {
            (pair.0.min(pair.1), pair.0.max(pair.1))
        }

        // pub fn from_iter(iter: impl Iterator<Item = (NodeId, NodeId)>) -> Self {
        //     let mut map = HashMap::with_capacity(iter.size_hint().0);
        //     for e in iter {
        //         map.insert(Self::sort(e), true);
        //     }
        //     Self(map)
        // }

        pub fn from_array<const N: usize>(array: [(NodeId, NodeId); N]) -> Self {
            let mut map = HashMap::with_capacity(N);
            for e in array {
                map.insert(Self::sort(e), true);
            }
            Self(map)
        }
    }

    fn create_nodes(number_of_nodes: NodeId, sink_nodes: &[NodeId]) -> Vec<ProtocolWrapper> {
        for sn_id in sink_nodes {
            assert!(*sn_id < number_of_nodes, "invalid sink node id");
        }
        (0..number_of_nodes)
            .map(|id| {
                let mut protocol = ProtocolImpl::new(id);
                protocol.set_is_sink(sink_nodes.contains(&id));
                ProtocolWrapper::new(protocol, Coordinates::default())
            })
            .collect()
    }

    #[test]
    fn basic() {
        // logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
        let nodes = create_nodes(2, &[0]);
        let data = run(nodes, 60, get_rng(0), |_, _| true);
        assert!(data.iter().any(|d| d.source == 1));
    }

    #[test]
    fn chain3() {
        // logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
        let nodes = create_nodes(3, &[0]);
        let visibility_map = VisibilitytMap::from_array([(0, 1), (1, 2)]);
        let data = run(nodes, 60, get_rng(0), |a, b| {
            visibility_map.get(a.id(), b.id())
        });
        for n in 1..=2 {
            assert!(data.iter().any(|d| d.source == n));
        }
    }

    #[test]
    fn chain4() {
        // logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
        let nodes = create_nodes(4, &[0]);
        let visibility_map = VisibilitytMap::from_array([(0, 1), (1, 2), (2, 3)]);
        let data = run(nodes, 60, get_rng(0), |a, b| {
            visibility_map.get(a.id(), b.id())
        });
        for n in 1..=3 {
            assert!(data.iter().any(|d| d.source == n));
        }
    }

    /// One sink with four children, all nodes see each other
    #[test]
    fn children() {
        // logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
        let num_nodes = 5;
        let nodes = create_nodes(num_nodes, &[0]);
        let data = run(nodes, 60 * 2, get_rng(0), |_, _| true);
        for n in 1..=num_nodes - 1 {
            assert!(data.iter().any(|d| d.source == n as u32));
        }
    }

    /// One sink with many children, all nodes see each other
    #[test]
    fn more_children() {
        // logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
        let num_nodes = 9;
        let nodes = create_nodes(num_nodes, &[0]);
        let data = run(nodes, 60 * 4, get_rng(0), |_, _| true);
        for n in 1..=num_nodes - 1 {
            assert!(data.iter().any(|d| d.source == n as u32));
        }
    }

    // #[test]
    // fn report_poc_scenario() {
    //     logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
    //     let nodes = create_nodes(6, &[0]);
    //     let visibility_map = VisibilitytMap::from_array([(0, 1), (1, 2), (1, 3), (2, 4), (3, 5)]);
    //     let data = run(nodes, 15_000, get_rng(1), |a, b| {
    //         visibility_map.get(a.id(), b.id())
    //     });
    //     for n in 1..=5 {
    //         assert!(data.iter().any(|d| d.source == n));
    //     }
    // }

    #[test]
    fn extra_scenario_1() {
        // logger::init(log::Level::Trace, Some(EVENT_FILE_PATH)).unwrap();
        let nodes = create_nodes(5, &[0]);
        let visibility_map = VisibilitytMap::from_array([(0, 1), (1, 2), (1, 3), (1, 4)]);
        let data = run(nodes, 60, get_rng(0), |a, b| {
            visibility_map.get(a.id(), b.id())
        });
        for n in 1..=4 {
            assert!(data.iter().any(|d| d.source == n));
        }
    }
}
