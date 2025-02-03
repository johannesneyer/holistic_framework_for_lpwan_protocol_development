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

use std::{fs::File, io::Write};

pub const EVENT_INDICATOR_CHAR: char = '$';

const FILE_HEADER: &str = "uptime;node_id;kind;content";

pub struct ProtocolEventFileWriter {
    file: File,
}

impl ProtocolEventFileWriter {
    pub fn new(output_file_path: &str) -> Self {
        let mut file = File::create(output_file_path).expect("could not create event file");
        file.write_all(FILE_HEADER.as_bytes()).unwrap();
        file.write_all(b"\n").unwrap();
        Self { file }
    }

    pub fn write_event(&mut self, event: &str) {
        // strip indicator char
        let event = event.split_at(1).1;
        self.file.write_all(event.as_bytes()).unwrap();
        self.file.write_all(b"\n").unwrap();
    }

    pub fn flush(&mut self) {
        self.file.flush().unwrap();
    }
}
