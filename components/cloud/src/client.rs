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

use anyhow::{bail, Context, Result};
use colored::Colorize;
use mio::net::TcpStream;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt::Display;
use std::io::Write;
use std::{thread, time};

use protocol_event_writer::{ProtocolEventFileWriter, EVENT_INDICATOR_CHAR};

use crate::BOOTLOADER_WRITE_MAX_SIZE;

type NodeId = u32;
type RGBColor = (u8, u8, u8);

pub struct Client<'a> {
    pub node_id: Option<NodeId>,
    pub firmware_state: FirmwareState,
    pub halted: Option<bool>,
    // uptime: u64,
    pub connection: TcpStream,
    pub log_decoder: Box<dyn defmt_decoder::StreamDecoder + 'a>,
    /// buffer for storing bytes of yet to complete message
    pub buffer: VecDeque<u8>,
    pub color: RGBColor,
}

impl Display for Client<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut props = Vec::new();

        props.push(format!(
            "id: {:10}",
            match self.node_id {
                Some(id) => format!("0x{:08x}", id),
                None => "UNKNOWN".to_string(),
            }
        ));

        props.push(format!("halted: {:3}", self.halted_as_string()));

        if let Ok(addr) = self.connection.peer_addr() {
            props.push(format!("addr: {}", addr));
        }

        write!(f, "{{ ")?;
        write!(f, "{}", props.join(", "))?;
        write!(f, " }}")?;
        Ok(())
    }
}

impl Client<'_> {
    pub fn identifier_str(&self) -> String {
        match self.node_id {
            Some(id) => format!("{:08x}", id),
            None => self.connection.peer_addr().unwrap().to_string(),
        }
    }

    pub fn halted_as_string(&self) -> &str {
        match self.halted {
            Some(halted) => {
                if halted {
                    "yes"
                } else {
                    "no"
                }
            }
            None => "UNKNOWN",
        }
    }

    pub fn decode_log_data(&mut self, data: &[u8], event_writer: &mut ProtocolEventFileWriter) {
        self.log_decoder.received(data);
        // data might contain multiple log messages
        let id_str = self.identifier_str();
        loop {
            match self.log_decoder.decode() {
                Ok(frame) => {
                    let msg = frame.display_message().to_string();
                    if msg.starts_with(EVENT_INDICATOR_CHAR) {
                        event_writer.write_event(&msg);
                    }
                    println!(
                        "[{}] {}",
                        id_str.truecolor(self.color.0, self.color.1, self.color.2),
                        frame.display(true)
                    );
                }
                Err(defmt_decoder::DecodeError::UnexpectedEof) => break,
                Err(defmt_decoder::DecodeError::Malformed) => {
                    // assume defmt encoding is recoverable
                    println!("defmt_decoder: malformed frame skipped")
                }
            }
        }
    }

    pub fn send_message(&mut self, message: Message) -> Result<()> {
        let mut cbor_encoded = Vec::with_capacity(1024);
        ciborium::into_writer(&message, &mut cbor_encoded)?;
        let mut cobs_encoded = cobs::encode_vec(cbor_encoded.as_slice());
        cobs_encoded.push(0x00);

        // TODO: this feels like a hack
        // maybe start a thread for each client that waits on a channel and then makes sure the data
        // is sent completely
        let mut buf = cobs_encoded.as_slice();
        while !buf.is_empty() {
            match self.connection.write(buf) {
                Ok(0) => {
                    bail!("failed to write whole buffer");
                }
                Ok(n) => buf = &buf[n..],
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(time::Duration::from_millis(50))
                }
                Err(ref err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err) => Err(err)?,
            }
        }

        Ok(())
    }

    pub fn update_firmware(&mut self, binary: &[u8]) -> Result<()> {
        self.send_message(Message::InitFwUpdate)
            .context("could not init firmware update")?;

        let mut offset = 0;
        for chunk in binary.chunks(BOOTLOADER_WRITE_MAX_SIZE) {
            self.send_message(Message::FwChunk {
                offset: offset as u32,
                data: chunk.to_owned(),
            })
            .context("could not send firmware chunk")?;
            offset += chunk.len();
        }

        self.send_message(Message::FinishFwUpdate)
            .context("could not finish firmware update")?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum FirmwareState {
    Unknown,
    Correct,
    Incorrect,
}

impl std::fmt::Display for FirmwareState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FirmwareState::Unknown => "unknown",
                FirmwareState::Correct => "correct",
                FirmwareState::Incorrect => "incorrect",
            }
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Log(Vec<u8>),
    InitFwUpdate,
    FwChunk {
        offset: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    FinishFwUpdate,
    Reset,
    Halt,
    GetInfo,
    Info {
        id: NodeId,
        crc: Option<u32>,
    },
    Error(String),
    Halted(bool),
}

pub struct Colors(HashMap<NodeId, RGBColor>, Vec<RGBColor>);

impl Colors {
    pub fn new() -> Self {
        Self(
            HashMap::new(),
            // https://colorbrewer2.org/#type=qualitative&scheme=Set3&n=8
            vec![
                (41, 211, 199),
                (255, 255, 179),
                (190, 186, 218),
                (251, 128, 114),
                (128, 177, 211),
                (253, 180, 98),
                (179, 222, 105),
                (252, 205, 229),
            ],
        )
    }

    pub fn get_color(&mut self, node_id: NodeId) -> RGBColor {
        if let Some(color) = self.0.get(&node_id) {
            return *color;
        }
        match self.1.pop() {
            Some(color) => {
                self.0.insert(node_id, color);
                color
            }
            None => panic!("ran out of node colors"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color() {
        let mut cs = Colors::new();
        assert_eq!(cs.get_color(1), cs.get_color(1));
        assert_ne!(cs.get_color(1), cs.get_color(2));
    }
}
