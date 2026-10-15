//! Block-device abstraction.
//!
//! The AHCI driver hands out disks addressed by an index; GPT partitions are
//! windows over one of those disks. Both implement [`BlockDevice`] so that the
//! filesystem layer can read and write through a single interface without
//! knowing how a block is backed.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockError {
    Io,
    NoSpace,
    NotFound,
    Corrupt,
    Unsupported,
}

pub trait BlockDevice {
    fn block_count(&self) -> u64;
    fn block_size(&self) -> u32;

    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError>;
    fn flush(&mut self) -> Result<(), BlockError>;

    /// Read one sector (512 bytes for SATA) into `buf`, which must be at least
    /// one block long.
    fn read_block(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        let size = self.block_size() as usize;
        if buf.len() < size {
            return Err(BlockError::Io);
        }
        self.read_blocks(lba, &mut buf[..size])
    }

    fn write_block(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        let size = self.block_size() as usize;
        if buf.len() < size {
            return Err(BlockError::Io);
        }
        self.write_blocks(lba, &buf[..size])
    }
}