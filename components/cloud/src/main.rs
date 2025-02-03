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

//! Network design inspired by https://github.com/tokio-rs/mio/blob/master/examples/tcp_server.rs

use anyhow::{anyhow, Context, Result};
use mio::net::TcpListener;
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use protocol_event_writer::ProtocolEventFileWriter;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Read, Write};
use std::mem::size_of;
use std::time::{Duration, Instant};

mod client;
mod crc;
mod elf;
mod slab;
use crate::{client::*, crc::*, elf::*, slab::*};

// TODO: default tcp timeout when a client disconnects is ~11 min on my machine, try to reduce this?
// this is a socket option but rust does not have an API to change it

const DEFAULT_ELF_PATH: &str =
    "/tmp/cargo/target/thumbv7em-none-eabi/release/lightning_firmware_for_stm32wl55";
const EVENT_FILE_PATH: &str = "/tmp/protocol_cloud_events.csv";
/// address where flash of stm32wl5x starts
const FLASH_OFFSET: u32 = 0x800_0000;
/// flash size of the stm32wl55jc in bytes
const FLASH_SIZE_BYTES: usize = 256_000;
/// value of flash after mass erase
const ERASED_BYTE_VALUE: u8 = 0xff;
const WORD_SIZE: usize = size_of::<u32>();
const BOOTLOADER_WRITE_MAX_SIZE: usize = 256;

const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:50000";
/// Set server token to maximum possible value as client tokens are allocated from 0
const SERVER_TOKEN: Token = Token(usize::MAX);
const STDIN_TOKEN: Token = Token(usize::MAX - 1);

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let listen_addr = args
        .get(1)
        .map(|arg| arg.as_str())
        .unwrap_or(DEFAULT_LISTEN_ADDR);

    let elf_path = args
        .get(2)
        .map(|arg| arg.as_str())
        .unwrap_or(DEFAULT_ELF_PATH);

    println!("reading firmware elf from {elf_path}");

    let elf_file = std::fs::read(elf_path).context("could not open firmware ELF")?;

    let binary =
        extract_from_elf(&elf_file, FLASH_OFFSET).context("could not extract binary from elf")?;
    // calc crc of expected flash content
    let expected_flash_crc = calc_crc(
        &binary,
        Some((ERASED_BYTE_VALUE, FLASH_SIZE_BYTES - binary.len())),
        WORD_SIZE,
    )
    .context("could not calculate CRC of firmware binary")?;

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(32);

    let mut clients = Slab::new();

    let defmt_table = Box::new(
        defmt_decoder::Table::parse(&elf_file)?
            .ok_or_else(|| anyhow!("defmt data not found in ELF"))?,
    );
    let defmt_table = Box::leak(defmt_table);
    assert!(defmt_table.encoding().can_recover());

    let mut event_writer = ProtocolEventFileWriter::new(EVENT_FILE_PATH);

    let mut client_colors = client::Colors::new();

    let mut tcp_listener = TcpListener::bind(listen_addr.parse()?)?;
    println!("listening on {listen_addr}");

    let mut receive_buffer = [0; 1024];

    let mut last_log_activity: Option<Instant> = None;

    let stdin = io::stdin();
    let mut input = String::new();

    poll.registry().register(
        &mut SourceFd(&libc::STDIN_FILENO),
        STDIN_TOKEN,
        Interest::READABLE,
    )?;

    poll.registry()
        .register(&mut tcp_listener, SERVER_TOKEN, Interest::READABLE)?;

    loop {
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            match event.token() {
                STDIN_TOKEN => {
                    input.clear();
                    stdin.read_line(&mut input)?;
                    if input.is_empty() {
                        // C-d
                        return Ok(());
                    }
                    let input = input.trim();
                    if let Err(err) = handle_command(input, &mut clients, &binary) {
                        println!("could not handle command: {err}")
                    }
                }
                SERVER_TOKEN => {
                    let (connection, client_addr) = match tcp_listener.accept() {
                        Ok((connection, client_addr)) => (connection, client_addr),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(err) => Err(err)?,
                    };

                    // block connections from outside of ZHAW
                    match client_addr {
                        std::net::SocketAddr::V4(addr) => {
                            if !addr.ip().is_private()
                                && !addr.ip().is_loopback()
                                && !matches!(addr.ip().octets(), [160, 85, _, _])
                            {
                                println!("ignoring connection from {:?}", client_addr);
                                let _ = connection.shutdown(std::net::Shutdown::Both);
                                continue;
                            }
                        }
                        std::net::SocketAddr::V6(_) => panic!("ipv6"),
                    }

                    println!("new connection from {:?}", client_addr);

                    let client = Client {
                        node_id: None,
                        firmware_state: FirmwareState::Unknown,
                        halted: None,
                        connection,
                        log_decoder: defmt_table.new_stream_decoder(),
                        buffer: VecDeque::with_capacity(1024),
                        color: (0xff, 0xff, 0xff),
                    };

                    let client_index = clients.insert(client);

                    poll.registry().register(
                        &mut clients.get_mut(client_index).unwrap().connection,
                        Token(client_index),
                        Interest::READABLE,
                    )?;
                }
                client_token => {
                    let client = clients
                        .get_mut(client_token.0)
                        .context("client token not in list")?;

                    let n = match client.connection.read(&mut receive_buffer) {
                        Ok(0) => {
                            // connection closed
                            println!("client {} disconnected", client);
                            clients
                                .try_remove(client_token.0)
                                .context("could not remove client: token not in list")?;
                            continue;
                        }
                        Ok(n) => n,
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(err) => {
                            println!("could not read from client socket: {}", err);
                            if let Err(err) = client.connection.shutdown(std::net::Shutdown::Both) {
                                println!("could not shutdown client connection: {}", err);
                            }
                            continue;
                        }
                    };

                    client.buffer.extend(&receive_buffer[..n]);

                    if client.buffer.len() > 10_000 {
                        // something is wrong with this client
                        let _ = client.connection.shutdown(std::net::Shutdown::Both);
                    }

                    loop {
                        let mut cobs_decoded = vec![0; client.buffer.len()];

                        let n_decoded = match cobs_decode_from_iter(
                            cobs_decoded.as_mut_slice(),
                            client.buffer.iter(),
                        ) {
                            Ok(None) => break,
                            Ok(Some((n_decoded, n_consumed))) => {
                                client.buffer.drain(..n_consumed);
                                n_decoded
                            }
                            Err(_n_written) => {
                                println!("could not decode COBS frame");
                                // skip to next message (next zero)
                                while let Some(x) = client.buffer.pop_front() {
                                    if x == 0x00 {
                                        break;
                                    }
                                }
                                continue;
                            }
                        };

                        let message: Message =
                            match ciborium::from_reader(&cobs_decoded[..n_decoded]) {
                                Ok(message) => message,
                                Err(err) => {
                                    println!("could not decode CBOR object: {}", err);
                                    continue;
                                }
                            };

                        if client.node_id.is_none()
                            && !matches!(message, Message::Info { id: _, crc: _ })
                        {
                            println!("message from unknown client: {client}");
                        }

                        match message {
                            Message::Log(ref data) => {
                                let now = Instant::now();
                                if let Some(last_log_activity) = last_log_activity {
                                    if now - last_log_activity > Duration::from_millis(750) {
                                        // print empty line to indicate a period of inactivity
                                        println!();
                                    }
                                }
                                last_log_activity = Some(now);
                                if matches!(client.firmware_state, FirmwareState::Correct) {
                                    client.decode_log_data(data, &mut event_writer);
                                } else {
                                    println!(
                                        "log message from client with unknown firmware: {}",
                                        client
                                    );
                                }
                            }
                            Message::Info { id, crc } => {
                                if client.node_id != Some(id) {
                                    let old_id = client.identifier_str();
                                    client.node_id = Some(id);
                                    println!(
                                        "{} changed ID to {}",
                                        old_id,
                                        client.identifier_str(),
                                    );
                                }

                                client.color = client_colors.get_color(id);

                                if let Some(crc) = crc {
                                    if crc == expected_flash_crc {
                                        client.firmware_state = FirmwareState::Correct;
                                        println!(
                                            "{} is running correct firmware",
                                            client.identifier_str()
                                        );
                                    } else {
                                        client.firmware_state = FirmwareState::Incorrect;
                                        println!(
                                            "{} is not running correct firmware",
                                            client.identifier_str()
                                        );
                                    }
                                } else {
                                    client.firmware_state = FirmwareState::Unknown;
                                    println!(
                                        "{} is running unknown firmware",
                                        client.identifier_str()
                                    );
                                }
                            }
                            Message::Error(ref msg) => println!("Error from {}: {}", client, msg),
                            Message::Halted(halted) => {
                                client.halted = Some(halted);
                                println!(
                                    "{} {}",
                                    client.identifier_str(),
                                    match halted {
                                        true => "halted",
                                        false => "running",
                                    }
                                );
                            }
                            _ => println!("unhandled msg received: {:?}", &message),
                        }
                    }
                }
            }
        }
    }
}

fn handle_command(input: &str, clients: &mut Slab<Client>, binary: &[u8]) -> Result<()> {
    let (command, argument) = match input.split_once(' ') {
        Some((cmd, arg)) => (cmd, arg),
        None => (input, ""),
    };
    match command.to_lowercase().as_str() {
        "help" | "?" => {
            println!(
                "
List of commands:

  help | ?
    print this message

  [l]ist
    list connected nodes

  [fwu]pdate
    update all nodes that run incorrect firmware

  [h]alt (INDEX|all)
    halt node with index INDEX or all nodes

  [r]eset (INDEX|all)
    reset node with index INDEX or all nodes
"
            );
        }
        "list" | "l" => {
            println!(
                "
| {:10} | {:10} | {:10} | {:10} | {:22} |
|------------+------------+------------+------------+------------------------|",
                "index", "id", "halted", "firmware", "address"
            );
            for (index, client) in clients.iter().flatten().enumerate() {
                if client.node_id.is_none() {
                    continue;
                }
                println!(
                    "| {:<10} | {:10} | {:10} | {:10} | {:22} |",
                    index,
                    client.identifier_str(),
                    client.halted_as_string(),
                    client.firmware_state.to_string(),
                    match client.connection.peer_addr() {
                        Ok(addr) => format!("{}", addr),
                        Err(_) => "UNKNOWN".to_string(),
                    }
                );
            }
            println!();
        }
        "fwupdate" | "fwu" => {
            for client in clients.iter_mut().flatten() {
                if matches!(client.firmware_state, FirmwareState::Incorrect) {
                    // TODO: this could be done in a separate thread
                    println!("updating firmware of {}", client.identifier_str());
                    if let Err(err) = client.update_firmware(binary) {
                        println!(
                            "could not update firmware of {}: {}",
                            client.identifier_str(),
                            err
                        )
                    };
                }
            }
        }
        "reset" | "r" => handle_reset_and_halt_command(true, argument, clients)?,
        "halt" | "h" => handle_reset_and_halt_command(false, argument, clients)?,
        "clear" | "c" => {
            const CSI: &[u8] = b"\x1b[";
            const CURSOR_HOME: &[u8] = b"H";
            const ERASE_SCREEN: &[u8] = b"2J";
            io::stdout().write_all(CSI).unwrap();
            io::stdout().write_all(CURSOR_HOME).unwrap();
            io::stdout().write_all(CSI).unwrap();
            io::stdout().write_all(ERASE_SCREEN).unwrap();
        }
        "" => {}
        cmd => println!("unknown command: {cmd}"),
    }

    // prompt
    print!("> ");
    io::stdout().flush().unwrap();

    Ok(())
}

fn handle_reset_and_halt_command(
    reset: bool,
    argument: &str,
    clients: &mut Slab<Client>,
) -> Result<()> {
    if argument == "all" {
        for client in clients.iter_mut().flatten() {
            client
                .send_message(if reset { Message::Reset } else { Message::Halt })
                .context(format!(
                    "could not {} {}",
                    if reset { "reset" } else { "halt" },
                    client
                ))?;
        }
    } else {
        let index: usize =
            str::parse(argument).context("index argument must be a decimal numeral or \"all\"")?;
        let client = clients
            .get_mut(index)
            .context(format!("no node at index {}", index))?;
        println!(
            "{}ing {}",
            if reset { "reset" } else { "halt" },
            client.identifier_str()
        );
        client
            .send_message(if reset { Message::Reset } else { Message::Halt })
            .context(format!(
                "could not {} {}",
                if reset { "reset" } else { "halt" },
                client
            ))?;
    }

    Ok(())
}

fn cobs_decode_from_iter<'a>(
    dest: &mut [u8],
    data: impl Iterator<Item = &'a u8>,
) -> Result<Option<(usize, usize)>, usize> {
    let mut decoder = cobs::CobsDecoder::new(dest);

    for (i, d) in data.enumerate() {
        if let Some(n_decoded) = decoder.feed(*d)? {
            return Ok(Some((n_decoded, i + 1)));
        };
    }

    Ok(None)
}
