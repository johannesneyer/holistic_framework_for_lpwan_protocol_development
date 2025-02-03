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

//! Prints log messages but also extracts protocol events and writes them to a file

use log::{Level, Metadata, Record, SetLoggerError};
use std::{cell::Cell, sync::Mutex};

use protocol_event_writer::{ProtocolEventFileWriter, EVENT_INDICATOR_CHAR};

const LOG_COLOR_CODE_DEFAULT: &str = "\x1B[0m";
const LOG_COLOR_CODE_RED: &str = "\x1B[1;31m";
const LOG_COLOR_CODE_GREEN: &str = "\x1B[1;32m";
const LOG_COLOR_CODE_YELLOW: &str = "\x1B[1;33m";
const LOG_COLOR_CODE_BLUE: &str = "\x1B[1;34m";

pub struct SimLogger {
    max_level: Level,
    event_writer: Option<Mutex<Cell<ProtocolEventFileWriter>>>,
}

impl log::Log for SimLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let color = match record.level() {
            Level::Error => LOG_COLOR_CODE_RED,
            Level::Warn => LOG_COLOR_CODE_YELLOW,
            Level::Info => LOG_COLOR_CODE_GREEN,
            Level::Debug => LOG_COLOR_CODE_BLUE,
            Level::Trace => "",
        };

        let msg = record.args().to_string();

        if msg.starts_with(EVENT_INDICATOR_CHAR) {
            if let Some(writer) = self.event_writer.as_ref() {
                let mut writer = writer.lock().unwrap();
                let writer = writer.get_mut();
                writer.write_event(&msg);
            }
        }

        println!(
            "[{}] {}{}{}",
            record.target(),
            color,
            msg,
            LOG_COLOR_CODE_DEFAULT
        );
    }

    fn flush(&self) {
        if let Some(file) = self.event_writer.as_ref() {
            file.lock().unwrap().get_mut().flush();
        }
    }
}

pub fn init(max_level: Level, output_file_path: Option<&str>) -> Result<(), SetLoggerError> {
    let event_writer = output_file_path.map(ProtocolEventFileWriter::new);
    let logger = Box::new(SimLogger {
        max_level,
        event_writer: event_writer.map(|f| Mutex::new(Cell::new(f))),
    });
    log::set_logger(Box::leak(logger))?;
    log::set_max_level(max_level.to_level_filter());
    Ok(())
}
