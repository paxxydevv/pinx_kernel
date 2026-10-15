//! Persistent ext4 mount for the data disk.
//!
//! The storage stack (AHCI -> GPT -> ext4) is brought up once during boot and
//! kept alive here. It has to live in a `static` because [`Ext4`] borrows its
//! backing block device for as long as it exists, so it cannot be held in a
//! local or returned by value. Consumers (the shell, future syscalls) reach the
//! live mount through [`with`].

use crate::ext4::{DirEnt, Ext4, FsError, Inode};
use crate::gpt::{self, GptPartition};
use crate::{ahci, kerror, kinfo};

/// The partition the mount borrows. A `static mut` so `Ext4` can borrow it for
/// `'static`; it is written once, before the mount is created.
static mut PART: GptPartition = GptPartition {
    disk: 0,
    start_lba: 0,
    last_lba: 0,
    type_guid: [0; 16],
    guid: [0; 16],
    name: [0; 72],
};

static mut MOUNT: Option<Ext4<'static>> = None;

/// Find the first AHCI disk, parse its GPT, and mount the first Linux ext4
/// partition found on it. Returns `true` if a filesystem is now available.
pub fn init() -> bool {
    if ahci::disk_count() == 0 {
        kerror!("fs: no AHCI disk to mount\n");
        return false;
    }
    if !gpt::detect(0) {
        kerror!("fs: no valid GPT on disk 0\n");
        return false;
    }

    let parts = gpt::partition_count(0);
    for i in 0..parts {
        let Some(entry) = gpt::partition(0, i) else {
            continue;
        };
        if !entry.type_is_linux() {
            continue;
        }

        unsafe {
            PART = *entry;
            let dev: &'static mut GptPartition = &mut *core::ptr::addr_of_mut!(PART);
            match Ext4::mount(dev) {
                Ok(fs) => {
                    kinfo!(
                        "fs: mounted ext4 on disk 0 part {} (block_size={}, groups={}, inodes={}, free_blocks={})\n",
                        i,
                        fs.block_size(),
                        fs.group_count(),
                        fs.inode_count(),
                        fs.free_blocks()
                    );
                    MOUNT = Some(fs);
                    return true;
                }
                Err(e) => kerror!("fs: ext4 mount failed on part {}: {:?}\n", i, e),
            }
        }
    }

    kerror!("fs: no mountable ext4 partition found\n");
    false
}

pub fn is_mounted() -> bool {
    unsafe { (*core::ptr::addr_of!(MOUNT)).is_some() }
}

/// Run `f` against the live mount. Returns `None` when nothing is mounted, so
/// callers can distinguish "no filesystem" from a filesystem error.
pub fn with<T>(f: impl FnOnce(&mut Ext4<'static>) -> T) -> Option<T> {
    unsafe { (*core::ptr::addr_of_mut!(MOUNT)).as_mut().map(f) }
}

// --------------------------------------------------------------- accessors
// Thin wrappers so callers do not need to import Ext4 for the common calls.
// `None` means "not mounted"; the inner `Result` is the filesystem's answer.

pub fn root() -> Option<u32> {
    with(|fs| fs.root())
}

pub fn read_inode(ino: u32) -> Option<Result<Inode, FsError>> {
    with(|fs| fs.read_inode(ino))
}

pub fn read_dir(ino: u32, out: &mut [DirEnt]) -> Option<Result<usize, FsError>> {
    with(|fs| fs.read_dir(ino, out))
}

pub fn lookup(dir: u32, name: &[u8]) -> Option<Result<Option<u32>, FsError>> {
    with(|fs| fs.lookup(dir, name))
}

pub fn read_file(ino: u32, offset: u64, buf: &mut [u8]) -> Option<Result<usize, FsError>> {
    with(|fs| fs.read_file(ino, offset, buf))
}

pub fn write_file(ino: u32, offset: u64, buf: &[u8]) -> Option<Result<(), FsError>> {
    with(|fs| fs.write_file(ino, offset, buf))
}

pub fn create_file(dir: u32, name: &[u8]) -> Option<Result<u32, FsError>> {
    with(|fs| fs.create_file(dir, name))
}

pub fn create_dir(dir: u32, name: &[u8]) -> Option<Result<u32, FsError>> {
    with(|fs| fs.create_dir(dir, name))
}

pub fn sync() -> Option<Result<(), FsError>> {
    with(|fs| fs.sync())
}
