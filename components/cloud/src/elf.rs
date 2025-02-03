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

use anyhow::{anyhow, bail, Result};
use object::{
    elf::{FileHeader32, PT_LOAD},
    read::elf::{FileHeader, ProgramHeader},
    Endianness, Object, ObjectSection,
};
use std::cmp::max;

// inspired by https://github.com/probe-rs/probe-rs/blob/73acf92686a62489049b6da6fce940bf94b07da8/probe-rs/src/flashing/download.rs#L218-L307
pub fn extract_from_elf(elf_file: &[u8], start_addr: u32) -> Result<Vec<u8>> {
    let file_kind = object::FileKind::parse(elf_file)?;

    if !matches!(file_kind, object::FileKind::Elf32) {
        bail!("Unsupported ELF file type");
    }

    let elf_header = FileHeader32::<Endianness>::parse(elf_file)?;

    let elf_data = object::read::elf::ElfFile::<FileHeader32<Endianness>>::parse(elf_file)?;

    let endian = elf_header.endian()?;

    let mut extracted_data: Vec<(u32, &[u8])> = Vec::new();
    let mut end_addr: u32 = 0;

    for segment in elf_header.program_headers(endian, elf_file)? {
        let physical_addr: u64 = segment.p_paddr(endian).into();

        let segment_data = segment
            .data(endian, elf_file)
            .map_err(|_| anyhow!("could not access data for an ELF segment"))?;

        if segment_data.is_empty() || segment.p_type(endian) != PT_LOAD {
            continue;
        }

        let (segment_offset, segment_filesize) = segment.file_range(endian);

        let segment_end = segment_offset
            .checked_add(segment_filesize)
            .ok_or(anyhow!("segment offset or filesize out of range"))?;

        // check if segment contains at least one section
        let mut has_sections = false;
        for section in elf_data.sections() {
            let (section_offset, section_filesize) = section
                .file_range()
                .ok_or(anyhow!("could not extract section file range"))?;
            let section_end = section_offset
                .checked_add(section_filesize)
                .ok_or(anyhow!("section offset or filesize out of range"))?;
            if section_offset >= segment_offset && section_end <= segment_end {
                has_sections = true;
                break;
            }
        }
        if !has_sections {
            continue;
        }

        let section_data = &elf_file[segment_offset as usize..][..segment_filesize as usize];

        let segment_end_addr = physical_addr
            .checked_add(segment_filesize)
            .ok_or(anyhow!("physical address or segment filesize out of range"))?;
        end_addr = max(segment_end_addr as u32, end_addr);

        extracted_data.push((physical_addr as u32, section_data));
    }

    let bin_size = end_addr
        .checked_sub(start_addr)
        .ok_or(anyhow!("binary end address out of range"))?;

    if bin_size % 4 != 0 {
        bail!("length of binary is not a multiple of 4 bytes");
    }

    let mut bin = vec![0xff; bin_size as usize];
    for (addr, data) in extracted_data.iter() {
        let start = addr
            .checked_sub(start_addr)
            .ok_or(anyhow!("segment address out of range"))? as usize;
        bin[start..][..data.len()].copy_from_slice(data);
    }

    Ok(bin)

    // let mut dst = vec![0u32; (bin_size / 4) as usize];
    // // not sure if the encoding of the binary is specified by the endianness in the elf header
    // let from_bytes = match endian {
    //     Endianness::Little => <u32>::from_le_bytes,
    //     Endianness::Big => <u32>::from_be_bytes,
    // };
    // // convert Vec<u8> to Vec<u32>
    // for (bin, dst) in bin.chunks_exact(4).zip(dst.iter_mut()) {
    //     *dst = from_bytes(bin.try_into()?);
    // }
    // Ok(dst)
}
