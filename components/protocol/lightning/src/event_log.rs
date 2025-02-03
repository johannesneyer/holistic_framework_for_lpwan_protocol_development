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

//! Macros for generating parseable event log messages

#[macro_export]
macro_rules! event_log {
    ($uptime:expr,$node_id:expr,$kind:expr,$content:expr) => {
        info!("${};{};{};{}", $uptime, $node_id, $kind, $content);
    };
}

#[macro_export]
macro_rules! event_log_msg {
    ($uptime:expr,$node_id:expr,$content:expr) => {
        event_log!($uptime, $node_id, "message", $content);
    };
}

#[macro_export]
macro_rules! event_log_action {
    ($uptime:expr,$node_id:expr,$action:expr) => {
        event_log!($uptime, $node_id, "action", $action);
    };
}

#[macro_export]
macro_rules! event_log_reset {
    ($uptime:expr,$node_id:expr,$is_sink:expr) => {
        info!(
            "${};{};reset;{{\"is_sink\":{}}}",
            $uptime, $node_id, $is_sink
        );
    };
}

#[macro_export]
macro_rules! event_log_state {
    ($uptime:expr,$node_id:expr,$new_state:expr) => {
        info!("${};{};state;\"{}\"", $uptime, $node_id, $new_state);
    };
}

#[macro_export]
macro_rules! event_log_new_child {
    ($uptime:expr,$node_id:expr,$child_id:expr) => {
        info!("${};{};new_child;\"{}\"", $uptime, $node_id, $child_id);
    };
}
