//! ext4 filesystem access: read, write, and on-disk metadata updates.
//!
//! This driver serves one mounted partition through a [`BlockDevice`] and is
//! deliberately a kernel module rather than a VFS: the caller decides where
//! mounts live, how files are named, and what gets cached.
//!
//! Supported feature set (what a normally-formatted volume carries): 64-bit
//! block numbers, metadata checksums (CRC32C), extents, directory file types,
//! flex_bg, large files, and huge files. Linear directories are updated in
//! place; indexed (htree) directories are readable but not writable. Features
//! that would corrupt an image if mishandled — inline data, MMP, meta_bg,
//! encrypted/casefolded names, real bigalloc clusters — are rejected at mount.
//!
//! The journal is intentionally ignored: no dirty-journal detection or replay
//! is attempted, exactly as the kernel behaves on a read-only mount. Writes go
//! straight to the live filesystem and every touched checksum is recomputed so
//! `e2fsck` still accepts the result. Each metadata checksum matches the exact
//! algorithm in fs/ext4 (super block, group descriptors, bitmaps, inodes,
//! directory tails, and extent tails).

use crate::block::{BlockDevice, BlockError};
use crate::crc;

pub const EXT4_MAGIC: u16 = 0xEF53;
pub const MAX_BLOCK_SIZE: usize = 4096;
pub const MAX_INODE_SIZE: usize = 256;
pub const EXT4_NAME_LEN: usize = 255;
pub const EXT4_EXT_MAGIC: u16 = 0xF30A;
pub const ROOT_INO: u32 = 2;

const GOOD_OLD_INODE_SIZE: usize = 128;

const INCOMPAT_FILETYPE: u32 = 0x0002;
const INCOMPAT_EXTENTS: u32 = 0x0040;
const INCOMPAT_64BIT: u32 = 0x0080;
const INCOMPAT_FLEX_BG: u32 = 0x0200;
const INCOMPAT_CSUM_SEED: u32 = 0x2000;

const RO_COMPAT_METADATA_CSUM: u32 = 0x0400;
const RO_COMPAT_REJECT: u32 = 0x0000_1000; // READONLY

const SB_INODES_COUNT: usize = 0x00;
const SB_BLOCKS_COUNT_LO: usize = 0x04;
const SB_FREE_BLOCKS_LO: usize = 0x0C;
const SB_FREE_INODES: usize = 0x10;
const SB_FIRST_DATA_BLOCK: usize = 0x14;
const SB_LOG_BLOCK_SIZE: usize = 0x18;
const SB_LOG_CLUSTER_SIZE: usize = 0x1C;
const SB_BLOCKS_PER_GROUP: usize = 0x20;
const SB_INODES_PER_GROUP: usize = 0x28;
const SB_MAGIC: usize = 0x38;
const SB_REV_LEVEL: usize = 0x4C;
const SB_INODE_SIZE: usize = 0x58;
const SB_FEATURE_INCOMPAT: usize = 0x60;
const SB_FEATURE_RO_COMPAT: usize = 0x64;
const SB_UUID: usize = 0x68;
const SB_VOLUME_NAME: usize = 0x78;
const SB_RESERVED_GDT_BLOCKS: usize = 0xCE;
const SB_BLOCKS_COUNT_HI: usize = 0x150;
const SB_FREE_BLOCKS_HI: usize = 0x158;
const SB_DESC_SIZE: usize = 0xFE;
const SB_CHECKSUM_SEED: usize = 0x270;
const SB_CHECKSUM: usize = 0x3FC;

const GD_BLOCK_BITMAP_LO: usize = 0x00;
const GD_INODE_BITMAP_LO: usize = 0x04;
const GD_INODE_TABLE_LO: usize = 0x08;
const GD_FREE_BLOCKS_LO: usize = 0x0C;
const GD_FREE_INODES_LO: usize = 0x0E;
const GD_USED_DIRS_LO: usize = 0x10;
const GD_BLOCK_BITMAP_CSUM_LO: usize = 0x18;
const GD_INODE_BITMAP_CSUM_LO: usize = 0x1A;
const GD_ITABLE_UNUSED_LO: usize = 0x1C;
const GD_CHECKSUM: usize = 0x1E;
const GD_BLOCK_BITMAP_HI: usize = 0x20;
const GD_INODE_BITMAP_HI: usize = 0x24;
const GD_INODE_TABLE_HI: usize = 0x28;
const GD_FREE_BLOCKS_HI: usize = 0x2C;
const GD_FREE_INODES_HI: usize = 0x2E;
const GD_USED_DIRS_HI: usize = 0x30;
const GD_ITABLE_UNUSED_HI: usize = 0x32;
const GD_BLOCK_BITMAP_CSUM_HI: usize = 0x38;
const GD_INODE_BITMAP_CSUM_HI: usize = 0x3A;

const INO_MODE: usize = 0x00;
const INO_SIZE_LO: usize = 0x04;
const INO_LINKS_COUNT: usize = 0x1A;
const INO_BLOCKS_LO: usize = 0x1C;
const INO_FLAGS: usize = 0x20;
const INO_IBLOCK: usize = 0x28;
const INO_GENERATION: usize = 0x64;
const INO_SIZE_HIGH: usize = 0x6C;
const INO_BLOCKS_HIGH: usize = 0x74;
const INO_CHECKSUM_LO: usize = 0x7C;
const INO_EXTRA_ISIZE: usize = 0x80;
const INO_CHECKSUM_HI: usize = 0x82;

const INO_FLAG_EXTENTS: u32 = 0x0008_0000;
const INO_FLAG_INDEX: u32 = 0x0000_1000;
const S_IFDIR: u16 = 0x4000;
const S_IFREG: u16 = 0x8000;
const S_IFMT: u16 = 0xF000;

const FT_DIR: u8 = 2;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FsError {
    Io,
    NoSpace,
    NotFound,
    AlreadyExists,
    Corrupt,
    ReadOnly,
    Unsupported,
    Invalid,
}

fn le16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn put_le16(b: &mut [u8], off: usize, v: u16) {
    b[off] = v as u8;
    b[off + 1] = (v >> 8) as u8;
}
fn put_le32(b: &mut [u8], off: usize, v: u32) {
    b[off] = v as u8;
    b[off + 1] = (v >> 8) as u8;
    b[off + 2] = (v >> 16) as u8;
    b[off + 3] = (v >> 24) as u8;
}

fn round_dir_size(name_len: usize) -> usize {
    8 + ((name_len + 3) & !3)
}

#[derive(Clone)]
pub struct Inode {
    pub ino: u32,
    pub mode: u16,
    pub size: u64,
    pub blocks: u64,
    pub flags: u32,
    raw: [u8; MAX_INODE_SIZE],
}

impl Inode {
    pub fn is_dir(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }
    pub fn is_file(&self) -> bool {
        self.mode & S_IFMT == S_IFREG
    }
}

#[derive(Clone, Copy)]
pub struct DirEnt {
    pub ino: u32,
    pub file_type: u8,
    pub name_len: usize,
    pub name: [u8; EXT4_NAME_LEN],
}

impl DirEnt {
    pub const fn empty() -> DirEnt {
        DirEnt {
            ino: 0,
            file_type: 0,
            name_len: 0,
            name: [0u8; EXT4_NAME_LEN],
        }
    }

    pub fn name_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.name[..self.name_len]).ok()
    }
}

fn map_block_err(e: BlockError) -> FsError {
    match e {
        BlockError::Io => FsError::Io,
        BlockError::NoSpace => FsError::NoSpace,
        BlockError::NotFound => FsError::NotFound,
        BlockError::Corrupt => FsError::Corrupt,
        BlockError::Unsupported => FsError::Unsupported,
    }
}

// --------------------------------------------------------- extent helpers

/// Read an extent header from `b` at `base`. Returns (entries, max, depth).
fn rd_hdr(b: &[u8], base: usize) -> Option<(u16, u16, u16)> {
    if b.len() < base + 12 {
        return None;
    }
    if le16(b, base) != EXT4_EXT_MAGIC {
        return None;
    }
    Some((le16(b, base + 2), le16(b, base + 4), le16(b, base + 6)))
}

fn wr_hdr(b: &mut [u8], base: usize, entries: u16, max: u16, depth: u16) {
    put_le16(b, base, EXT4_EXT_MAGIC);
    put_le16(b, base + 2, entries);
    put_le16(b, base + 4, max);
    put_le16(b, base + 6, depth);
    put_le32(b, base + 8, 0);
}

/// Read extent `i` from `b` at `base`: (first logical block, length, pblock).
/// Unwritten extents are reported with a zero length.
fn rd_extent(b: &[u8], base: usize, i: usize) -> Option<(u32, u16, u64)> {
    let o = base + 12 + 12 * i;
    if b.len() < o + 12 {
        return None;
    }
    let eb = le32(b, o);
    let mut elen = le16(b, o + 4);
    if elen & 0x8000 != 0 {
        elen = 0;
    }
    if elen == 0 {
        return None;
    }
    let pb = ((le16(b, o + 6) as u64) << 32) | (le32(b, o + 8) as u64);
    Some((eb, elen, pb))
}

fn wr_extent(b: &mut [u8], base: usize, i: usize, eb: u32, elen: u16, pblock: u64) {
    let o = base + 12 + 12 * i;
    put_le32(b, o, eb);
    put_le16(b, o + 4, elen);
    put_le16(b, o + 6, (pblock >> 32) as u16);
    put_le32(b, o + 8, pblock as u32);
}

/// Read index `i` from `b` at `base`: (first covered block, child leaf).
fn rd_index(b: &[u8], base: usize, i: usize) -> Option<(u32, u64)> {
    let o = base + 12 + 12 * i;
    if b.len() < o + 12 {
        return None;
    }
    let eb = le32(b, o);
    let child = ((le16(b, o + 8) as u64) << 32) | (le32(b, o + 4) as u64);
    Some((eb, child))
}

fn wr_index(b: &mut [u8], base: usize, i: usize, eb: u32, child: u64) {
    let o = base + 12 + 12 * i;
    put_le32(b, o, eb);
    put_le32(b, o + 4, child as u32);
    put_le16(b, o + 8, (child >> 32) as u16);
    put_le16(b, o + 10, 0);
}

/// Fill one directory entry at `pos` (0 = unused slot reuse path uses `rec`).
fn fill_entry(b: &mut [u8], pos: usize, rec: u16, ino: u32, ft: u8, name: &[u8]) {
    put_le32(b, pos, ino);
    put_le16(b, pos + 4, rec);
    b[pos + 6] = name.len() as u8;
    b[pos + 7] = ft;
    b[pos + 8..pos + 8 + name.len()].copy_from_slice(name);
}

pub struct Ext4<'a> {
    dev: &'a mut dyn BlockDevice,
    bs: u32,
    bpg: u32,
    ipg: u32,
    inode_size: u16,
    desc_size: u16,
    num_groups: u32,
    first_data_block: u32,
    gdt_fsb: u64,
    reserved_gdt_blocks: u16,
    uuid: [u8; 16],
    volume_name: [u8; 16],
    csum_seed: u32,
    meta_csum: bool,
    has_64bit: bool,
    blocks_total: u64,
    inodes_total: u32,
    free_blocks: u64,
    free_inodes: u64,
}

impl<'a> Ext4<'a> {
    pub fn mount(dev: &'a mut dyn BlockDevice) -> Result<Ext4<'a>, FsError> {
        if dev.block_size() != 512 {
            return Err(FsError::Unsupported);
        }

        let mut buf = [0u8; MAX_BLOCK_SIZE];
        dev.read_blocks(1024 / 512, &mut buf[..1024])
            .map_err(map_block_err)?;
        if le16(&buf, SB_MAGIC) != EXT4_MAGIC {
            return Err(FsError::Corrupt);
        }

        let log_block_size = le32(&buf, SB_LOG_BLOCK_SIZE);
        if log_block_size > 2 {
            return Err(FsError::Unsupported);
        }
        let bs = 1024u32 << log_block_size;

        // bigalloc is only transparent when cluster == block size.
        if le32(&buf, SB_LOG_CLUSTER_SIZE) != log_block_size {
            return Err(FsError::Unsupported);
        }

        let sb_fsb: u64 = if bs == 1024 { 1 } else { 0 };
        if sb_fsb != 1 {
            dev.read_blocks(sb_fsb * bs as u64 / 512, &mut buf[..bs as usize])
                .map_err(map_block_err)?;
            if le16(&buf, SB_MAGIC) != EXT4_MAGIC {
                return Err(FsError::Corrupt);
            }
        }

        let inodes_total = le32(&buf, SB_INODES_COUNT);
        let bpg = le32(&buf, SB_BLOCKS_PER_GROUP);
        let ipg = le32(&buf, SB_INODES_PER_GROUP);
        if inodes_total == 0 || bpg == 0 || ipg == 0 {
            return Err(FsError::Corrupt);
        }
        if le32(&buf, SB_REV_LEVEL) != 1 {
            return Err(FsError::Unsupported);
        }

        let mut inode_size = le16(&buf, SB_INODE_SIZE);
        if inode_size < GOOD_OLD_INODE_SIZE as u16 {
            inode_size = GOOD_OLD_INODE_SIZE as u16;
        }
        if inode_size > MAX_INODE_SIZE as u16 {
            return Err(FsError::Unsupported);
        }

        let incompat = le32(&buf, SB_FEATURE_INCOMPAT);
        let ro_compat = le32(&buf, SB_FEATURE_RO_COMPAT);

        if incompat & !(INCOMPAT_FILETYPE | INCOMPAT_EXTENTS | INCOMPAT_64BIT | INCOMPAT_FLEX_BG | INCOMPAT_CSUM_SEED) != 0 {
            return Err(FsError::Unsupported);
        }
        if incompat & INCOMPAT_EXTENTS == 0 || incompat & INCOMPAT_FILETYPE == 0 {
            return Err(FsError::Unsupported);
        }
        if ro_compat & RO_COMPAT_REJECT != 0 {
            return Err(FsError::Unsupported);
        }

        let has_64bit = incompat & INCOMPAT_64BIT != 0;
        let meta_csum = ro_compat & RO_COMPAT_METADATA_CSUM != 0 && incompat & INCOMPAT_CSUM_SEED != 0;

        let mut desc_size = le16(&buf, SB_DESC_SIZE);
        if desc_size == 0 {
            desc_size = 32;
        }
        if desc_size < 32 || (has_64bit && desc_size < 64) {
            return Err(FsError::Corrupt);
        }

        let mut blocks_total = le32(&buf, SB_BLOCKS_COUNT_LO) as u64;
        let mut free_blocks = le32(&buf, SB_FREE_BLOCKS_LO) as u64;
        if has_64bit {
            blocks_total |= (le32(&buf, SB_BLOCKS_COUNT_HI) as u64) << 32;
            free_blocks |= (le32(&buf, SB_FREE_BLOCKS_HI) as u64) << 32;
        }
        let free_inodes = le32(&buf, SB_FREE_INODES) as u64;

        let num_groups = ((blocks_total + bpg as u64 - 1) / bpg as u64) as u32;
        let first_data_block = le32(&buf, SB_FIRST_DATA_BLOCK);
        let gdt_fsb = first_data_block as u64 + 1;
        let reserved_gdt_blocks = le16(&buf, SB_RESERVED_GDT_BLOCKS);

        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&buf[SB_UUID..SB_UUID + 16]);
        let mut volume_name = [0u8; 16];
        volume_name.copy_from_slice(&buf[SB_VOLUME_NAME..SB_VOLUME_NAME + 16]);

        let csum_seed = if incompat & INCOMPAT_CSUM_SEED != 0 {
            le32(&buf, SB_CHECKSUM_SEED)
        } else {
            crc::crc32c(0xFFFF_FFFF, &uuid)
        };

        if meta_csum && le32(&buf, SB_CHECKSUM) != crc::crc32c(0xFFFF_FFFF, &buf[..0x3FC]) {
            return Err(FsError::Corrupt);
        }

        Ok(Ext4 {
            dev,
            bs,
            bpg,
            ipg,
            inode_size,
            desc_size,
            num_groups,
            first_data_block,
            gdt_fsb,
            reserved_gdt_blocks,
            uuid,
            volume_name,
            csum_seed,
            meta_csum,
            has_64bit,
            blocks_total,
            inodes_total,
            free_blocks,
            free_inodes,
        })
    }

    pub fn block_size(&self) -> u32 {
        self.bs
    }
    pub fn block_count(&self) -> u64 {
        self.blocks_total
    }
    pub fn free_blocks(&self) -> u64 {
        self.free_blocks
    }
    pub fn free_inodes(&self) -> u64 {
        self.free_inodes
    }
    pub fn inode_count(&self) -> u32 {
        self.inodes_total
    }
    pub fn group_count(&self) -> u32 {
        self.num_groups
    }
    pub fn volume_name(&self) -> &[u8; 16] {
        &self.volume_name
    }
    pub fn root(&self) -> u32 {
        ROOT_INO
    }

    pub fn sync(&mut self) -> Result<(), FsError> {
        self.dev.flush().map_err(map_block_err)
    }

    // --------------------------------------------------------- primitives

    fn fsb_lba(&self, fsb: u64) -> u64 {
        fsb * self.bs as u64 / 512
    }

    fn sblock_fsb(&self) -> u64 {
        if self.bs == 1024 {
            1
        } else {
            0
        }
    }

    fn read_fsb(&mut self, fsb: u64, buf: &mut [u8]) -> Result<(), FsError> {
        self.dev
            .read_blocks(self.fsb_lba(fsb), &mut buf[..self.bs as usize])
            .map_err(map_block_err)
    }

    fn write_fsb(&mut self, fsb: u64, buf: &[u8]) -> Result<(), FsError> {
        self.dev
            .write_blocks(self.fsb_lba(fsb), &buf[..self.bs as usize])
            .map_err(map_block_err)
    }

    fn read_sblock(&mut self, buf: &mut [u8; MAX_BLOCK_SIZE]) -> Result<(), FsError> {
        let fsb = self.sblock_fsb();
        self.read_fsb(fsb, buf)
    }

    fn write_sblock(&mut self, buf: &[u8; MAX_BLOCK_SIZE]) -> Result<(), FsError> {
        let fsb = self.sblock_fsb();
        self.write_fsb(fsb, buf)
    }

    /// Read group `group`'s descriptor into `buf`; returns its offset.
    fn group_descriptor(&mut self, group: u32, buf: &mut [u8; MAX_BLOCK_SIZE]) -> Result<usize, FsError> {
        if group >= self.num_groups {
            return Err(FsError::Corrupt);
        }
        let byte = self.gdt_fsb * self.bs as u64 + group as u64 * self.desc_size as u64;
        let block = byte / self.bs as u64;
        let off = (byte % self.bs as u64) as usize;
        self.read_fsb(block, buf)?;
        Ok(off)
    }

    fn push_group(&mut self, group: u32, gd: &[u8]) -> Result<(), FsError> {
        let byte = self.gdt_fsb * self.bs as u64 + group as u64 * self.desc_size as u64;
        let block = byte / self.bs as u64;
        let off = (byte % self.bs as u64) as usize;
        let mut buf = [0u8; MAX_BLOCK_SIZE];
        self.read_fsb(block, &mut buf)?;
        buf[off..off + self.desc_size as usize].copy_from_slice(gd);
        self.write_fsb(block, &buf)
    }

    /// Apply `f` to a writable copy of group `group`'s descriptor, recompute
    /// its checksum, and write it back.
    fn with_group(&mut self, group: u32, f: impl FnOnce(&mut [u8])) -> Result<(), FsError> {
        let mut read = [0u8; MAX_BLOCK_SIZE];
        let mut local = [0u8; MAX_BLOCK_SIZE];
        {
            let off = self.group_descriptor(group, &mut read)?;
            local[..self.desc_size as usize]
                .copy_from_slice(&read[off..off + self.desc_size as usize]);
        }
        f(&mut local[..self.desc_size as usize]);

        if self.meta_csum {
            let d = &local[..self.desc_size as usize];
            let mut crc = crc::crc32c(self.csum_seed, &group.to_le_bytes());
            crc = crc::crc32c(crc, &d[..0x1E]);
            crc = crc::crc32c(crc, &[0, 0]);
            if 0x20 < self.desc_size as usize {
                crc = crc::crc32c(crc, &d[0x20..self.desc_size as usize]);
            }
            put_le16(&mut local, GD_CHECKSUM, crc as u16);
        }
        self.push_group(group, &local[..self.desc_size as usize])
    }

    fn gd_u32(&self, gd: &[u8], lo: usize, hi: usize) -> u64 {
        let mut v = le32(gd, lo) as u64;
        if self.has_64bit && hi < self.desc_size as usize {
            v |= (le32(gd, hi) as u64) << 32;
        }
        v
    }

    fn gd_u16(&self, gd: &[u8], lo: usize, hi: usize) -> u64 {
        let mut v = le16(gd, lo) as u64;
        if self.has_64bit && hi < self.desc_size as usize {
            v |= (le16(gd, hi) as u64) << 16;
        }
        v
    }

    fn persist_super_counts(&mut self, dblk: i64, dino: i64) -> Result<(), FsError> {
        self.free_blocks = (self.free_blocks as i64 + dblk).max(0) as u64;
        self.free_inodes = (self.free_inodes as i64 + dino).max(0) as u64;

        let mut sb = [0u8; MAX_BLOCK_SIZE];
        self.read_sblock(&mut sb)?;
        put_le32(&mut sb, SB_FREE_BLOCKS_LO, self.free_blocks as u32);
        put_le32(&mut sb, SB_FREE_INODES, self.free_inodes as u32);
        if self.has_64bit {
            put_le32(&mut sb, SB_FREE_BLOCKS_HI, (self.free_blocks >> 32) as u32);
        }
        if self.meta_csum {
            let csum = crc::crc32c(0xFFFF_FFFF, &sb[..0x3FC]);
            put_le32(&mut sb, SB_CHECKSUM, csum);
        }
        self.write_sblock(&sb)
    }

    // ------------------------------------------------------------- inodes

    fn inode_location(&mut self, ino: u32) -> Result<(u64, usize, u32), FsError> {
        let group = (ino - 1) / self.ipg;
        let index = (ino - 1) % self.ipg;
        let mut gd_buf = [0u8; MAX_BLOCK_SIZE];
        let off = self.group_descriptor(group, &mut gd_buf)?;
        let gd = &gd_buf[off..off + self.desc_size as usize];
        let table = self.gd_u32(gd, GD_INODE_TABLE_LO, GD_INODE_TABLE_HI);
        let byte = table * self.bs as u64 + index as u64 * self.inode_size as u64;
        Ok((byte / self.bs as u64, (byte % self.bs as u64) as usize, group))
    }

    pub fn read_inode(&mut self, ino: u32) -> Result<Inode, FsError> {
        let (block, off, _group) = self.inode_location(ino)?;
        let mut buf = [0u8; MAX_BLOCK_SIZE];
        self.read_fsb(block, &mut buf)?;
        let raw = &buf[off..off + self.inode_size as usize];

        let mode = le16(raw, INO_MODE);
        let size_lo = le32(raw, INO_SIZE_LO) as u64;
        let size_high = if self.has_64bit { le32(raw, INO_SIZE_HIGH) as u64 } else { 0 };
        let size = size_lo | (size_high << 32);
        let blocks = (le32(raw, INO_BLOCKS_LO) as u64)
            | if self.has_64bit { (le32(raw, INO_BLOCKS_HIGH) as u64) << 32 } else { 0 };
        let flags = le32(raw, INO_FLAGS);

        let mut full = [0u8; MAX_INODE_SIZE];
        full[..self.inode_size as usize].copy_from_slice(raw);

        Ok(Inode {
            ino,
            mode,
            size,
            blocks,
            flags,
            raw: full,
        })
    }

    fn write_inode(&mut self, inode: &mut Inode) -> Result<(), FsError> {
        let (block, off, _group) = self.inode_location(inode.ino)?;
        let mut buf = [0u8; MAX_BLOCK_SIZE];
        self.read_fsb(block, &mut buf)?;

        put_le16(&mut inode.raw, INO_MODE, inode.mode);
        put_le32(&mut inode.raw, INO_SIZE_LO, inode.size as u32);
        put_le32(&mut inode.raw, INO_BLOCKS_LO, inode.blocks as u32);
        put_le32(&mut inode.raw, INO_FLAGS, inode.flags);
        if self.has_64bit {
            put_le32(&mut inode.raw, INO_SIZE_HIGH, (inode.size >> 32) as u32);
            put_le32(&mut inode.raw, INO_BLOCKS_HIGH, (inode.blocks >> 32) as u32);
        }

        if self.meta_csum {
            let gen = le32(&inode.raw, INO_GENERATION);
            let mut seed = crc::crc32c(self.csum_seed, &inode.ino.to_le_bytes());
            seed = crc::crc32c(seed, &gen.to_le_bytes());

            let inode_size = self.inode_size as usize;
            let fits_hi =
                inode_size > GOOD_OLD_INODE_SIZE && le16(&inode.raw, INO_EXTRA_ISIZE) as usize >= 4;

            let mut csum = seed;
            csum = crc::crc32c(csum, &inode.raw[..INO_CHECKSUM_LO]);
            csum = crc::crc32c(csum, &[0, 0]);
            csum = crc::crc32c(csum, &inode.raw[INO_CHECKSUM_LO + 2..GOOD_OLD_INODE_SIZE]);

            if inode_size > GOOD_OLD_INODE_SIZE {
                csum = crc::crc32c(csum, &inode.raw[GOOD_OLD_INODE_SIZE..INO_CHECKSUM_HI]);
                let tail = if fits_hi {
                    csum = crc::crc32c(csum, &[0, 0]);
                    INO_CHECKSUM_HI + 2
                } else {
                    INO_CHECKSUM_HI
                };
                csum = crc::crc32c(csum, &inode.raw[tail..inode_size]);
            }

            put_le16(&mut inode.raw, INO_CHECKSUM_LO, csum as u16);
            if fits_hi {
                put_le16(&mut inode.raw, INO_CHECKSUM_HI, (csum >> 16) as u16);
            }
        }

        buf[off..off + self.inode_size as usize]
            .copy_from_slice(&inode.raw[..self.inode_size as usize]);
        self.write_fsb(block, &buf)
    }

    // --------------------------------------------------------- allocation

    fn read_bitmap(&mut self, group: u32, want_inode: bool) -> Result<u64, FsError> {
        let mut gd_buf = [0u8; MAX_BLOCK_SIZE];
        let off = self.group_descriptor(group, &mut gd_buf)?;
        let gd = &gd_buf[off..off + self.desc_size as usize];
        Ok(if want_inode {
            self.gd_u32(gd, GD_INODE_BITMAP_LO, GD_INODE_BITMAP_HI)
        } else {
            self.gd_u32(gd, GD_BLOCK_BITMAP_LO, GD_BLOCK_BITMAP_HI)
        })
    }

    fn block_reserved(&mut self, group: u32, fsb: u64) -> bool {
        if group == 0 {
            let table_bytes = (self.num_groups as u64) * (self.desc_size as u64);
            let reserved_end = self.gdt_fsb
                + (table_bytes + self.bs as u64 - 1) / self.bs as u64
                + self.reserved_gdt_blocks as u64;
            if fsb < reserved_end {
                return true;
            }
        }
        let mut gd_buf = [0u8; MAX_BLOCK_SIZE];
        match self.group_descriptor(group, &mut gd_buf) {
            Ok(off) => {
                let gd = &gd_buf[off..off + self.desc_size as usize];
                let bb = self.gd_u32(gd, GD_BLOCK_BITMAP_LO, GD_BLOCK_BITMAP_HI);
                let ib = self.gd_u32(gd, GD_INODE_BITMAP_LO, GD_INODE_BITMAP_HI);
                let it = self.gd_u32(gd, GD_INODE_TABLE_LO, GD_INODE_TABLE_HI);
                let it_blocks = (self.inodes_total as u64 / self.num_groups as u64
                    * self.inode_size as u64
                    + self.bs as u64 - 1)
                    / self.bs as u64;
                if fsb == bb || fsb == ib || (fsb >= it && fsb < it + it_blocks) {
                    return true;
                }
            }
            Err(_) => return true,
        }
        false
    }

    /// Find a free unit and mark it in the bitmap. Returns a block number (for
    /// data) or inode number (for inodes).
    fn alloc_bit(&mut self, want_inode: bool) -> Result<u64, FsError> {
        let units = if want_inode { self.ipg } else { self.bpg };
        let bitmap_bytes = (units / 8) as usize;

        for group in 0..self.num_groups {
            let bitmap_fsb = self.read_bitmap(group, want_inode)?;
            let mut bm = [0u8; MAX_BLOCK_SIZE];
            self.read_fsb(bitmap_fsb, &mut bm)?;

            for by in 0..bitmap_bytes {
                if bm[by] == 0xFF {
                    continue;
                }
                for bit in 0..8 {
                    if bm[by] & (1 << bit) != 0 {
                        continue;
                    }
                    let idx = by * 8 + bit;
                    if idx as u32 >= units {
                        continue;
                    }
                    if want_inode {
                        let ino = group as u64 * self.ipg as u64 + idx as u64 + 1;
                        bm[by] |= 1 << bit;
                        self.write_fsb(bitmap_fsb, &bm)?;
                        return Ok(ino);
                    } else {
                        let fsb = self.first_data_block as u64
                            + group as u64 * self.bpg as u64
                            + idx as u64;
                        if fsb >= self.blocks_total {
                            continue;
                        }
                        if self.block_reserved(group, fsb) {
                            continue;
                        }
                        bm[by] |= 1 << bit;
                        self.write_fsb(bitmap_fsb, &bm)?;
                        return Ok(fsb);
                    }
                }
            }
        }
        Err(FsError::NoSpace)
    }

    /// Update a group descriptor following an allocation/deallocation and
    /// persist the running superblock counters.
    #[allow(clippy::too_many_arguments)]
    fn commit_alloc(&mut self, group: u32, dblk: i64, dino: i64, ddir: i64, bb: Option<u32>, ib: Option<u32>, itu: Option<u16>) -> Result<(), FsError> {
        let desc64 = self.has_64bit;
        self.with_group(group, |gd| {
            let mut fb = (le16(gd, GD_FREE_BLOCKS_LO) as i64)
                | if desc64 { (le16(gd, GD_FREE_BLOCKS_HI) as i64) << 16 } else { 0 };
            fb += dblk;
            put_le16(gd, GD_FREE_BLOCKS_LO, fb as u16);
            if desc64 {
                put_le16(gd, GD_FREE_BLOCKS_HI, (fb >> 16) as u16);
            }

            let mut fi = (le16(gd, GD_FREE_INODES_LO) as i64)
                | if desc64 { (le16(gd, GD_FREE_INODES_HI) as i64) << 16 } else { 0 };
            fi += dino;
            put_le16(gd, GD_FREE_INODES_LO, fi as u16);
            if desc64 {
                put_le16(gd, GD_FREE_INODES_HI, (fi >> 16) as u16);
            }

            if ddir != 0 {
                let mut ud = (le16(gd, GD_USED_DIRS_LO) as i64)
                    | if desc64 { (le16(gd, GD_USED_DIRS_HI) as i64) << 16 } else { 0 };
                ud += ddir;
                put_le16(gd, GD_USED_DIRS_LO, ud as u16);
                if desc64 {
                    put_le16(gd, GD_USED_DIRS_HI, (ud >> 16) as u16);
                }
            }

            if let Some(c) = bb {
                put_le16(gd, GD_BLOCK_BITMAP_CSUM_LO, c as u16);
                if desc64 {
                    put_le16(gd, GD_BLOCK_BITMAP_CSUM_HI, (c >> 16) as u16);
                }
            }
            if let Some(c) = ib {
                put_le16(gd, GD_INODE_BITMAP_CSUM_LO, c as u16);
                if desc64 {
                    put_le16(gd, GD_INODE_BITMAP_CSUM_HI, (c >> 16) as u16);
                }
            }

            if let Some(u) = itu {
                put_le16(gd, GD_ITABLE_UNUSED_LO, u);
                if desc64 {
                    put_le16(gd, GD_ITABLE_UNUSED_HI, 0);
                }
            }
        })?;

        let _ = ddir;
        self.persist_super_counts(dblk, dino)
    }

    fn alloc_block(&mut self) -> Result<u64, FsError> {
        let fsb = self.alloc_bit(false)?;
        let group = ((fsb - self.first_data_block as u64) / self.bpg as u64) as u32;
        let bm_fsb = self.read_bitmap(group, false)?;
        let mut bm = [0u8; MAX_BLOCK_SIZE];
        self.read_fsb(bm_fsb, &mut bm)?;
        let bb = if self.meta_csum {
            crc::crc32c(self.csum_seed, &bm[..(self.bpg / 8) as usize])
        } else {
            0
        };
        self.commit_alloc(group, -1, 0, 0, Some(bb), None, None)?;
        Ok(fsb)
    }

    fn alloc_inode(&mut self, is_dir: bool) -> Result<u32, FsError> {
        let ino = self.alloc_bit(true)? as u32;
        let group = (ino - 1) / self.ipg;
        let index = (ino - 1) % self.ipg;
        let bm_fsb = self.read_bitmap(group, true)?;
        let mut bm = [0u8; MAX_BLOCK_SIZE];
        self.read_fsb(bm_fsb, &mut bm)?;
        let ib = if self.meta_csum {
            crc::crc32c(self.csum_seed, &bm[..(self.ipg / 8) as usize])
        } else {
            0
        };

        // `bg_itable_unused` counts unused inodes at the tail of the inode
        // table, so using a higher-numbered inode shrinks it.
        let mut gd_buf = [0u8; MAX_BLOCK_SIZE];
        let off = self.group_descriptor(group, &mut gd_buf)?;
        let current = {
            let gd = &gd_buf[off..off + self.desc_size as usize];
            self.gd_u16(gd, GD_ITABLE_UNUSED_LO, GD_ITABLE_UNUSED_HI)
        };
        let max_unused = self.ipg as u64 - index as u64 - 1;
        let new_unused = current.min(max_unused) as u16;

        self.commit_alloc(
            group,
            0,
            -1,
            if is_dir { 1 } else { 0 },
            None,
            Some(ib),
            Some(new_unused),
        )?;
        Ok(ino)
    }

    // ------------------------------------------------------------- extents

    fn leaf_max(&self) -> u16 {
        ((self.bs as usize - 12) / 12) as u16
    }

    fn dir_limit(&self) -> usize {
        if self.meta_csum {
            self.bs as usize - 12
        } else {
            self.bs as usize
        }
    }

    /// Stamp the checksum tail on an on-disk extent block (`max` entries).
    fn extent_block_csum(&self, ino: u32, gen: u32, blk: &mut [u8; MAX_BLOCK_SIZE], max: u16) {
        if !self.meta_csum {
            return;
        }
        let tail = 12 + 12 * max as usize;
        let mut seed = crc::crc32c(self.csum_seed, &ino.to_le_bytes());
        seed = crc::crc32c(seed, &gen.to_le_bytes());
        let c = crc::crc32c(seed, &blk[..tail]);
        put_le32(blk, tail, c);
    }

    /// Map a logical file block to a physical filesystem block.
    fn map_block(&mut self, inode: &Inode, lb: u32) -> Result<Option<u64>, FsError> {
        let Some((mut entries, _, mut depth)) = rd_hdr(&inode.raw, INO_IBLOCK) else {
            return Ok(None);
        };
        let root_base = INO_IBLOCK;
        let mut node_base = root_base;
        let mut node_block = 0u64; // 0 means "inode-stored root"
        let mut cur_buf = [0u8; MAX_BLOCK_SIZE];

        loop {
            if depth == 0 {
                for i in 0..entries {
                    let (src, base) = if node_block == 0 {
                        (&inode.raw[..], node_base as u64 as usize)
                    } else {
                        (&cur_buf[..], node_base)
                    };
                    if let Some((eb, elen, pb)) = rd_extent(src, base, i as usize) {
                        if lb >= eb && lb < eb + (elen as u32) {
                            return Ok(Some(pb + (lb - eb) as u64));
                        }
                    }
                }
                return Ok(None);
            }

            // Index level: follow the index whose range covers `lb`.
            let mut chosen: Option<u64> = None;
            for i in 0..entries {
                let opt = if node_block == 0 {
                    rd_index(&inode.raw, node_base, i as usize)
                } else {
                    rd_index(&cur_buf, node_base, i as usize)
                };
                let Some((eb, child)) = opt else { continue };
                let next = if i + 1 < entries {
                    if node_block == 0 {
                        rd_index(&inode.raw, node_base, (i + 1) as usize).map(|v| v.0)
                    } else {
                        rd_index(&cur_buf, node_base, (i + 1) as usize).map(|v| v.0)
                    }
                    .unwrap_or(u32::MAX)
                } else {
                    u32::MAX
                };
                if lb >= eb && lb < next {
                    chosen = Some(child);
                    break;
                }
            }
            let Some(child) = chosen else { return Ok(None) };

            self.read_fsb(child, &mut cur_buf)?;
            let Some((e, _m, d)) = rd_hdr(&cur_buf, 0) else {
                return Ok(None);
            };
            node_block = child;
            node_base = 0;
            entries = e;
            depth = d;
        }
    }

    /// Append the mapping `lb -> pblock` to the inode's extent tree. The tree
    /// is grown on demand: an empty header is materialized, the inode-resident
    /// leaf is deepened into a single on-disk leaf when it fills, and leaves
    /// split in two when they fill. Only one index level is maintained.
    fn insert_extent(&mut self, inode: &mut Inode, lb: u32, pblock: u64) -> Result<(), FsError> {
        let base = INO_IBLOCK;
        let (entries, max, depth) = match rd_hdr(&inode.raw, base) {
            Some((e, m, d)) => (e, m, d),
            None => {
                wr_hdr(&mut inode.raw, base, 0, 4, 0);
                (0, 4, 0)
            }
        };
        let gen = le32(&inode.raw, INO_GENERATION);

        if depth == 0 {
            if entries > 0 {
                if let Some((eb, elen, pb)) = rd_extent(&inode.raw, base, (entries - 1) as usize) {
                    if eb + (elen as u32) == lb && pb + (elen as u64) == pblock {
                        wr_extent(&mut inode.raw, base, (entries - 1) as usize, eb, elen + 1, pb);
                        return Ok(());
                    }
                }
            }
            if entries < max {
                wr_extent(&mut inode.raw, base, entries as usize, lb, 1, pblock);
                wr_hdr(&mut inode.raw, base, entries + 1, max, 0);
                return Ok(());
            }
            // Deepen: migrate the inode-resident extents into a fresh leaf.
            let leaf = self.alloc_block()?;
            let lmax = self.leaf_max();
            let mut blk = [0u8; MAX_BLOCK_SIZE];
            for i in 0..entries {
                if let Some((eb, elen, pb)) = rd_extent(&inode.raw, base, i as usize) {
                    wr_extent(&mut blk, 0, i as usize, eb, elen, pb);
                }
            }
            wr_hdr(&mut blk, 0, entries, lmax, 0);
            self.extent_block_csum(inode.ino, gen, &mut blk, lmax);
            self.write_fsb(leaf, &blk)?;
            wr_hdr(&mut inode.raw, base, 1, max, 1);
            wr_index(&mut inode.raw, base, 0, 0, leaf);
        }

        // depth >= 1: place into the rightmost leaf, splitting if needed.
        for _ in 0..2 {
            if self.place_in_rightmost_leaf(inode, lb, pblock)? {
                return Ok(());
            }
        }
        Err(FsError::NoSpace)
    }

    fn place_in_rightmost_leaf(&mut self, inode: &mut Inode, lb: u32, pblock: u64) -> Result<bool, FsError> {
        let base = INO_IBLOCK;
        let Some((entries, rmax, _depth)) = rd_hdr(&inode.raw, base) else {
            return Err(FsError::Corrupt);
        };
        let (_, leaf) = rd_index(&inode.raw, base, (entries - 1) as usize).ok_or(FsError::Corrupt)?;
        let gen = le32(&inode.raw, INO_GENERATION);

        let mut blk = [0u8; MAX_BLOCK_SIZE];
        self.read_fsb(leaf, &mut blk)?;
        let Some((le, lmax, _)) = rd_hdr(&blk, 0) else {
            return Err(FsError::Corrupt);
        };

        if le > 0 {
            if let Some((eb, elen, pb)) = rd_extent(&blk, 0, (le - 1) as usize) {
                if eb + (elen as u32) == lb && pb + (elen as u64) == pblock {
                    wr_extent(&mut blk, 0, (le - 1) as usize, eb, elen + 1, pb);
                    self.extent_block_csum(inode.ino, gen, &mut blk, lmax);
                    self.write_fsb(leaf, &blk)?;
                    return Ok(true);
                }
            }
        }
        if le < lmax {
            wr_extent(&mut blk, 0, le as usize, lb, 1, pblock);
            wr_hdr(&mut blk, 0, le + 1, lmax, 0);
            self.extent_block_csum(inode.ino, gen, &mut blk, lmax);
            self.write_fsb(leaf, &blk)?;
            return Ok(true);
        }

        // Split: move the upper half of the overflowing leaf to a new leaf.
        let half = le / 2;
        if half == 0 {
            return Err(FsError::NoSpace);
        }
        let new_leaf = self.alloc_block()?;
        let mut nb = [0u8; MAX_BLOCK_SIZE];
        let mut first_lb = 0u32;
        for i in half..le {
            if let Some((eb, elen, pb)) = rd_extent(&blk, 0, i as usize) {
                let j = (i - half) as usize;
                if i == half {
                    first_lb = eb;
                }
                wr_extent(&mut nb, 0, j, eb, elen, pb);
            }
        }
        wr_hdr(&mut blk, 0, half, lmax, 0);
        wr_hdr(&mut nb, 0, le - half, lmax, 0);
        self.extent_block_csum(inode.ino, gen, &mut blk, lmax);
        self.extent_block_csum(inode.ino, gen, &mut nb, lmax);
        self.write_fsb(leaf, &blk)?;
        self.write_fsb(new_leaf, &nb)?;

        if entries >= rmax {
            return Err(FsError::NoSpace);
        }
        wr_index(&mut inode.raw, base, entries as usize, first_lb, new_leaf);
        wr_hdr(&mut inode.raw, base, entries + 1, rmax, 1);
        let _ = new_leaf;
        Ok(false)
    }

    /// Sum allocated extents (in 512-byte units) under the tree.
    fn recount_blocks(&mut self, inode: &Inode) -> Result<u64, FsError> {
        let Some((_, _, depth)) = rd_hdr(&inode.raw, INO_IBLOCK) else {
            return Ok(0);
        };
        let total = self.node_sum(&inode.raw, INO_IBLOCK, depth)?;
        Ok(total * (self.bs as u64 / 512))
    }

    fn node_sum(&mut self, node: &[u8], base: usize, depth: u16) -> Result<u64, FsError> {
        let Some((entries, _, _)) = rd_hdr(node, base) else {
            return Ok(0);
        };
        let mut total = 0u64;
        if depth == 0 {
            for i in 0..entries {
                if let Some((_, elen, _)) = rd_extent(node, base, i as usize) {
                    total += elen as u64;
                }
            }
        } else {
            for i in 0..entries {
                let (_, child) = rd_index(node, base, i as usize).ok_or(FsError::Corrupt)?;
                let mut blk = [0u8; MAX_BLOCK_SIZE];
                self.read_fsb(child, &mut blk)?;
                total += self.node_sum(&blk, 0, depth - 1)?;
            }
        }
        Ok(total)
    }

    // ---------------------------------------------------------- directories

    fn dir_tail_csum(&self, ino: u32, gen: u32, blk: &mut [u8; MAX_BLOCK_SIZE]) {
        if !self.meta_csum {
            return;
        }
        let bs = self.bs as usize;
        put_le32(blk, bs - 12, 0);
        put_le16(blk, bs - 8, 12);
        blk[bs - 6] = 0;
        blk[bs - 5] = 0xDE;
        let mut seed = crc::crc32c(self.csum_seed, &ino.to_le_bytes());
        seed = crc::crc32c(seed, &gen.to_le_bytes());
        let c = crc::crc32c(seed, &blk[..bs - 12]);
        put_le32(blk, bs - 4, c);
    }

    pub fn read_dir(&mut self, ino: u32, out: &mut [DirEnt]) -> Result<usize, FsError> {
        let inode = self.read_inode(ino)?;
        if !inode.is_dir() {
            return Err(FsError::Invalid);
        }
        let bs = self.bs as usize;
        let limit = self.dir_limit();
        let blocks = ((inode.size as usize) + bs - 1) / bs;
        let mut n = 0usize;
        let mut blk = [0u8; MAX_BLOCK_SIZE];

        for lb in 0..blocks {
            if let Some(fsb) = self.map_block(&inode, lb as u32)? {
                self.read_fsb(fsb, &mut blk)?;
                let mut pos = 0usize;
                while pos + 8 <= limit {
                    let eino = le32(&blk, pos);
                    let rec = le16(&blk, pos + 4) as usize;
                    if rec == 0 {
                        break;
                    }
                    let nlen = blk[pos + 6] as usize;
                    let ft = blk[pos + 7];
                    if eino != 0 && nlen <= EXT4_NAME_LEN {
                        if n < out.len() {
                            out[n].ino = eino;
                            out[n].file_type = ft;
                            out[n].name_len = nlen;
                            out[n].name[..nlen].copy_from_slice(&blk[pos + 8..pos + 8 + nlen]);
                        }
                        n += 1;
                    }
                    pos += rec;
                }
            }
        }
        Ok(n)
    }

    pub fn lookup(&mut self, dir: u32, name: &[u8]) -> Result<Option<u32>, FsError> {
        let inode = self.read_inode(dir)?;
        if !inode.is_dir() {
            return Err(FsError::Invalid);
        }
        let bs = self.bs as usize;
        let limit = self.dir_limit();
        let blocks = ((inode.size as usize) + bs - 1) / bs;
        let mut blk = [0u8; MAX_BLOCK_SIZE];

        for lb in 0..blocks {
            if let Some(fsb) = self.map_block(&inode, lb as u32)? {
                self.read_fsb(fsb, &mut blk)?;
                let mut pos = 0usize;
                while pos + 8 <= limit {
                    let rec = le16(&blk, pos + 4) as usize;
                    if rec == 0 {
                        break;
                    }
                    let eino = le32(&blk, pos);
                    let nlen = blk[pos + 6] as usize;
                    if eino != 0 && nlen == name.len() {
                        if &blk[pos + 8..pos + 8 + nlen] == name {
                            return Ok(Some(eino));
                        }
                    }
                    pos += rec;
                }
            }
        }
        Ok(None)
    }

    /// Insert `name -> target` into directory `dir` as a linear directory.
    fn add_dirent(&mut self, dir: u32, name: &[u8], target: u32, ft: u8) -> Result<(), FsError> {
        if name.is_empty() || name.len() > EXT4_NAME_LEN {
            return Err(FsError::Invalid);
        }
        let needed = round_dir_size(name.len());
        let mut inode = self.read_inode(dir)?;
        if !inode.is_dir() {
            return Err(FsError::Invalid);
        }
        if inode.flags & INO_FLAG_INDEX != 0 {
            return Err(FsError::ReadOnly);
        }
        let bs = self.bs as usize;
        let limit = self.dir_limit();
        let blocks = ((inode.size as usize) + bs - 1) / bs;
        let gen = le32(&inode.raw, INO_GENERATION);
        let mut blk = [0u8; MAX_BLOCK_SIZE];

        for lb in 0..blocks {
            if let Some(fsb) = self.map_block(&inode, lb as u32)? {
                self.read_fsb(fsb, &mut blk)?;
                let mut pos = 0usize;
                let mut last_pos = 0usize;
                let mut last_rec = 0usize;
                let mut last_actual = 0usize;
                while pos + 8 <= limit {
                    let rec = le16(&blk, pos + 4) as usize;
                    if rec == 0 {
                        break;
                    }
                    let eino = le32(&blk, pos);
                    let nlen = blk[pos + 6] as usize;
                    if eino == 0 && rec >= needed {
                        fill_entry(&mut blk, pos, rec as u16, target, ft, name);
                        self.dir_tail_csum(inode.ino, gen, &mut blk);
                        self.write_fsb(fsb, &blk)?;
                        return Ok(());
                    }
                    last_pos = pos;
                    last_rec = rec;
                    last_actual = if eino == 0 { 0 } else { round_dir_size(nlen) };
                    pos += rec;
                }
                let slack = last_rec as i64 - last_actual as i64;
                if slack >= needed as i64 {
                    let new_pos = last_pos + last_actual;
                    put_le16(&mut blk, last_pos + 4, last_actual as u16);
                    fill_entry(&mut blk, new_pos, slack as u16, target, ft, name);
                    self.dir_tail_csum(inode.ino, gen, &mut blk);
                    self.write_fsb(fsb, &blk)?;
                    return Ok(());
                }
            }
        }

        // No room among existing blocks: allocate a fresh one.
        let new_lb = blocks as u32;
        let fsb = self.alloc_block()?;
        let mut nb = [0u8; MAX_BLOCK_SIZE];
        fill_entry(&mut nb, 0, limit as u16, target, ft, name);
        self.dir_tail_csum(inode.ino, gen, &mut nb);
        self.write_fsb(fsb, &nb)?;
        self.insert_extent(&mut inode, new_lb, fsb)?;
        inode.size += self.bs as u64;
        inode.blocks = self.recount_blocks(&inode)?;
        self.write_inode(&mut inode)
    }

    // ---------------------------------------------------------------- files

    pub fn read_file(&mut self, ino: u32, offset: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        let inode = self.read_inode(ino)?;
        if !inode.is_file() {
            return Err(FsError::Invalid);
        }
        if offset >= inode.size {
            return Ok(0);
        }
        let take = ((inode.size - offset) as usize).min(buf.len());
        let bs = self.bs as usize;
        let mut done = 0usize;
        let mut blk = [0u8; MAX_BLOCK_SIZE];

        while done < take {
            let cur = offset as usize + done;
            let lb = cur / bs;
            let in_off = cur % bs;
            let count = (bs - in_off).min(take - done);
            if let Some(fsb) = self.map_block(&inode, lb as u32)? {
                self.read_fsb(fsb, &mut blk)?;
                buf[done..done + count].copy_from_slice(&blk[in_off..in_off + count]);
            } else {
                buf[done..done + count].fill(0);
            }
            done += count;
        }
        Ok(take)
    }

    pub fn write_file(&mut self, ino: u32, offset: u64, buf: &[u8]) -> Result<(), FsError> {
        if buf.is_empty() {
            return Ok(());
        }
        let bs = self.bs as usize;
        let mut inode = self.read_inode(ino)?;
        if !inode.is_file() {
            return Err(FsError::Invalid);
        }
        let end = offset + buf.len() as u64;
        let mut done = 0usize;
        let mut blk = [0u8; MAX_BLOCK_SIZE];

        while done < buf.len() {
            let cur = offset as usize + done;
            let lb = cur / bs;
            let in_off = cur % bs;
            let count = (bs - in_off).min(buf.len() - done);

            if let Some(fsb) = self.map_block(&inode, lb as u32)? {
                self.read_fsb(fsb, &mut blk)?;
            } else {
                blk.fill(0);
                let fsb = self.alloc_block()?;
                self.insert_extent(&mut inode, lb as u32, fsb)?;
            }

            let fsb = self.map_block(&inode, lb as u32)?.ok_or(FsError::Corrupt)?;
            blk[in_off..in_off + count].copy_from_slice(&buf[done..done + count]);
            self.write_fsb(fsb, &blk)?;
            done += count;
        }

        if end > inode.size {
            inode.size = end;
        }
        inode.blocks = self.recount_blocks(&inode)?;
        self.write_inode(&mut inode)
    }

    /// Create a file (`is_dir == false`) or directory (`is_dir == true`) named
    /// `name` inside directory `dir`. Returns the new inode number.
    fn create(&mut self, dir: u32, name: &[u8], is_dir: bool) -> Result<u32, FsError> {
        if self.lookup(dir, name)?.is_some() {
            return Err(FsError::AlreadyExists);
        }
        let ino = self.alloc_inode(is_dir)?;
        let mut inode = Inode {
            ino,
            mode: if is_dir { S_IFDIR | 0o755 } else { S_IFREG | 0o644 },
            size: 0,
            blocks: 0,
            flags: INO_FLAG_EXTENTS,
            raw: [0u8; MAX_INODE_SIZE],
        };
        wr_hdr(&mut inode.raw, INO_IBLOCK, 0, 4, 0);
        put_le16(&mut inode.raw, INO_LINKS_COUNT, if is_dir { 2 } else { 1 });
        put_le16(&mut inode.raw, INO_EXTRA_ISIZE, 28);

        if is_dir {
            let fsb = self.alloc_block()?;
            let mut nb = [0u8; MAX_BLOCK_SIZE];
            let limit = self.dir_limit();
            fill_entry(&mut nb, 0, 12, ino, FT_DIR, b".");
            fill_entry(&mut nb, 12, (limit - 12) as u16, dir, FT_DIR, b"..");
            self.dir_tail_csum(ino, 0, &mut nb);
            self.write_fsb(fsb, &nb)?;
            inode.size = self.bs as u64;
            inode.blocks = self.bs as u64 / 512;
            wr_hdr(&mut inode.raw, INO_IBLOCK, 1, 4, 0);
            wr_extent(&mut inode.raw, INO_IBLOCK, 0, 0, 1, fsb);
        }

        self.write_inode(&mut inode)?;
        self.add_dirent(dir, name, ino, if is_dir { FT_DIR } else { 1 })?;

        // A new subdirectory adds its `..` entry as a link back to the parent.
        if is_dir {
            let mut parent = self.read_inode(dir)?;
            let links = le16(&parent.raw, INO_LINKS_COUNT);
            put_le16(&mut parent.raw, INO_LINKS_COUNT, links + 1);
            self.write_inode(&mut parent)?;
        }
        Ok(ino)
    }

    pub fn create_file(&mut self, dir: u32, name: &[u8]) -> Result<u32, FsError> {
        self.create(dir, name, false)
    }

    pub fn create_dir(&mut self, dir: u32, name: &[u8]) -> Result<u32, FsError> {
        self.create(dir, name, true)
    }
}