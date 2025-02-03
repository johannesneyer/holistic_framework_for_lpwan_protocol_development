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

//! Calculate CRC32 like the stm32 bootloader

use anyhow::{bail, Result};
use crc::Crc;

const CRC_SETTINGS: crc::Algorithm<u32> = crc::Algorithm {
    width: 32,
    poly: 0x04C1_1DB7,
    init: 0x0000_0000,
    refin: false,
    refout: false,
    xorout: 0x0000_0000,
    residue: 0x0000_0000,
    check: 0, // TODO: what does this parameter do?
};

pub fn calc_crc(binary: &[u8], padding: Option<(u8, usize)>, word_size: usize) -> Result<u32> {
    let crc = Crc::<u32>::new(&CRC_SETTINGS);
    let mut digest = crc.digest();

    let word_iter = binary.chunks_exact(word_size);

    if !word_iter.remainder().is_empty() {
        bail!("length of binary is not a multiple of {word_size} bytes");
    }

    for chunk in word_iter {
        let mut chunk = chunk.to_vec();
        chunk.reverse(); // convert endianness
        digest.update(chunk.as_slice());
    }

    if let Some((value, size)) = padding {
        let padding = vec![value; size];
        digest.update(padding.as_slice());
    }

    Ok(digest.finalize())
}
