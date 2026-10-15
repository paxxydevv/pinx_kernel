//! GPT partition table reader.
//!
//! Parses the primary header at LBA 1, verifies the header and partition
//! entry array CRCs, and exposes each entry as a [`BlockDevice`] window over
//! its parent disk. The protective MBR at LBA 0 is ignored.

use crate::ahci;
use crate::block::{BlockDevice, BlockError};
use crate::crc;

pub const SECTOR_SIZE: u32 = 512;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

#[derive(Copy, Clone)]
pub struct GptPartition {
    pub disk: usize,
    pub start_lba: u64,
    pub last_lba: u64,
    pub type_guid: [u8; 16],
    pub guid: [u8; 16],
    pub name: [u8; 72],
}

impl GptPartition {
    pub fn type_is_linux(&self) -> bool {
        self.type_guid == [
            0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47,
            0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
        ]
    }
}

struct GptDisk {
    parsed: bool,
    part_count: usize,
    entries_lba: u64,
    num_entries: u32,
    entry_size: u32,
    parts: [GptPartition; 16],
}

const MAX_PARTITIONS: usize = 16;

static mut GPT_DISKS: [GptDisk; 8] = {
    const EMPTY: GptPartition = GptPartition {
        disk: 0,
        start_lba: 0,
        last_lba: 0,
        type_guid: [0; 16],
        guid: [0; 16],
        name: [0; 72],
    };
    const EMPTY_DISK: GptDisk = GptDisk {
        parsed: false,
        part_count: 0,
        entries_lba: 0,
        num_entries: 0,
        entry_size: 0,
        parts: [EMPTY; MAX_PARTITIONS],
    };
    [EMPTY_DISK; 8]
};

fn gpt_crc(data: &[u8]) -> u32 {
    crc::crc32(0xFFFF_FFFF, data) ^ 0xFFFF_FFFF
}

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u64le(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off], b[off + 1], b[off + 2], b[off + 3],
        b[off + 4], b[off + 5], b[off + 6], b[off + 7],
    ])
}

/// Parse and cache the GPT for `disk`. Returns whether a valid table exists.
pub fn detect(disk: usize) -> bool {
    if disk >= 8 {
        return false;
    }

    let mut sector = [0u8; 512];

    if !ahci::read(disk, 1, &mut sector) {
        return false;
    }

    if &sector[0..8] != GPT_SIGNATURE {
        return false;
    }

    let header_size = u32le(&sector, 12) as usize;
    let header_crc = u32le(&sector, 16);
    let entries_lba = u64le(&sector, 72);
    let num_entries = u32le(&sector, 80);
    let entry_size = u32le(&sector, 84);

    if header_size < 92 || num_entries == 0 || entry_size < 128 {
        return false;
    }

    // CRC over the header with its own CRC field zeroed.
    let mut crc_buf = [0u8; 512];
    crc_buf[..512].copy_from_slice(&sector);
    crc_buf[16..20].fill(0);
    if gpt_crc(&crc_buf[..header_size]) != header_crc {
        return false;
    }

    let entry_size = entry_size as usize;
    // Entries are packed and never cross a sector boundary in practice; the
    // standard sizes (128/256/512) all divide the sector.
    if entry_size == 0 || entry_size > 512 || 512 % entry_size != 0 {
        return false;
    }

    let entries_bytes = (num_entries as usize) * entry_size;
    let entries_sectors = entries_bytes.div_ceil(512);
    let entries_per_sector = 512 / entry_size;
    let entry_crc = u32le(&sector, 88);

    let empty = GptPartition {
        disk,
        start_lba: 0,
        last_lba: 0,
        type_guid: [0; 16],
        guid: [0; 16],
        name: [0; 72],
    };
    let mut parts = [empty; MAX_PARTITIONS];
    let mut part_count = 0usize;

    // The header CRC covers the whole entry array, so hash it while reading,
    // sector by sector, keeping only the first few entries.
    let mut crc_acc = 0xFFFF_FFFFu32;
    let mut esec = [0u8; 512];
    let mut index = 0usize;

    for s in 0..entries_sectors {
        if !ahci::read(disk, entries_lba + s as u64, &mut esec) {
            return false;
        }
        let remaining = entries_bytes - s * 512;
        crc_acc = crc::crc32(crc_acc, &esec[..remaining.min(512)]);

        for slot in 0..entries_per_sector {
            if index >= num_entries as usize {
                break;
            }
            let off = slot * entry_size;
            let e = &esec[off..off + entry_size];
            index += 1;

            if e[0..16].iter().all(|&b| b == 0) {
                continue;
            }
            let start = u64le(e, 32);
            let last = u64le(e, 40);
            if start == 0 || last < start || part_count >= MAX_PARTITIONS {
                continue;
            }

            parts[part_count] = GptPartition {
                disk,
                start_lba: start,
                last_lba: last,
                type_guid: [0; 16],
                guid: [0; 16],
                name: [0; 72],
            };
            parts[part_count].type_guid.copy_from_slice(&e[0..16]);
            parts[part_count].guid.copy_from_slice(&e[16..32]);
            let name_off = 56;
            let name_len = entry_size.saturating_sub(name_off).min(72);
            if name_len > 0 {
                parts[part_count].name[..name_len]
                    .copy_from_slice(&e[name_off..name_off + name_len]);
            }
            part_count += 1;
        }
    }

    if crc_acc ^ 0xFFFF_FFFF != entry_crc {
        return false;
    }

    unsafe {
        let gpt = &mut GPT_DISKS[disk];
        gpt.parsed = true;
        gpt.entries_lba = entries_lba;
        gpt.num_entries = num_entries;
        gpt.entry_size = entry_size as u32;
        gpt.parts = parts;
        gpt.part_count = part_count;
    }

    true
}

/// Number of valid entries, or 0 if the table is not parsed.
pub fn partition_count(disk: usize) -> usize {
    unsafe {
        if disk >= 8 || !GPT_DISKS[disk].parsed {
            0
        } else {
            GPT_DISKS[disk].part_count
        }
    }
}

/// Index into the partition list of `disk`. Returns `None` when out of range.
pub fn partition(disk: usize, index: usize) -> Option<&'static GptPartition> {
    unsafe {
        if disk >= 8 || !GPT_DISKS[disk].parsed || index >= GPT_DISKS[disk].part_count {
            None
        } else {
            Some(&GPT_DISKS[disk].parts[index])
        }
    }
}

impl BlockDevice for GptPartition {
    fn block_count(&self) -> u64 {
        self.last_lba - self.start_lba + 1
    }

    fn block_size(&self) -> u32 {
        SECTOR_SIZE
    }

    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() % 512 != 0 {
            return Err(BlockError::Io);
        }
        let sectors = buf.len() / 512;
        if lba + sectors as u64 > self.block_count() {
            return Err(BlockError::Io);
        }
        if ahci::read(self.disk, self.start_lba + lba, buf) {
            Ok(())
        } else {
            Err(BlockError::Io)
        }
    }

    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        if buf.len() % 512 != 0 {
            return Err(BlockError::Io);
        }
        let sectors = buf.len() / 512;
        if lba + sectors as u64 > self.block_count() {
            return Err(BlockError::Io);
        }
        if ahci::write(self.disk, self.start_lba + lba, buf) {
            Ok(())
        } else {
            Err(BlockError::Io)
        }
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        if ahci::flush(self.disk) {
            Ok(())
        } else {
            Err(BlockError::Io)
        }
    }
}