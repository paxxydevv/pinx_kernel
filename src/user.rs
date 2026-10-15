//! Loading and entering the userspace init program.
//!
//! `/init` on the data disk is the first program the kernel runs. Before it is
//! started the file must carry a [`PinxInitHeader`] with the right magic and ABI
//! version, so a plain ELF on the disk cannot be started by mistake. Once
//! verified it is loaded into the lower half with user permissions, mapped a
//! stack, and registered with the scheduler as a ring-3 task. It then runs
//! whenever a CPU picks it up.

use core::mem::size_of;

use crate::fs;
use crate::mm;
use crate::user_abi::{
    PINX_INIT_MAGIC, PINX_INIT_VERSION, PINX_PROGRAM_MAGIC, PINX_PROGRAM_VERSION, PinxProgramHeader,
};
use crate::{kerror, kinfo, kpanic};

const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3E;

/// Largest init image the kernel will read, so a bogus inode size cannot ask
/// `kmalloc` for something absurd.
const MAX_INIT_SIZE: u64 = 4 * 1024 * 1024;

/// The user stack sits just below the top of the lower canonical half, so it
/// grows down from inside user space. Each spawned task gets its own slice of
/// this region, carved downward by [`USER_STACK_SPAN`], because all tasks share
/// one address space.
const USER_STACK_TOP: u64 = 0x0000_7FFF_0000_0000;
const USER_STACK_PAGES: u64 = 16;
/// Mapped stack plus an unmapped guard gap, so two stacks never touch.
const USER_STACK_SPAN: u64 = (USER_STACK_PAGES + 2) * mm::PAGE_SIZE;

/// Next free stack top. Handed out downward, one per spawned task.
static mut NEXT_STACK: u64 = USER_STACK_TOP;

/// Read `/init`, verify it carries the *init* header, load it and register it as
/// a ring-3 task. Returns `true` when a task was created, `false` when there is
/// nothing usable to run, letting the caller fall back to the kernel shell.
pub fn try_start() -> bool {
    if !fs::is_mounted() {
        return false;
    }

    match load_and_spawn(b"init", &PINX_INIT_MAGIC, PINX_INIT_VERSION) {
        Some(_) => {
            kinfo!("user: started /init\n");
            true
        }
        None => {
            kinfo!("user: /init not found\n");
            false
        }
    }
}

/// Load the program at `path`, verify it carries the *program* header, map it
/// into the shared address space and register it as a new ring-3 task. Returns
/// the task id, or `None` on any failure. The caller keeps running; the task
/// starts on the next schedule.
pub fn spawn_program(path: &[u8]) -> Option<usize> {
    load_and_spawn(path, &PINX_PROGRAM_MAGIC, PINX_PROGRAM_VERSION)
}

/// The shared body of [`try_start`] and [`spawn_program`]. `magic`/`version`
/// select which header the image must carry, which is what stops `/init` from
/// being spawned and an ordinary program from being used as init.
fn load_and_spawn(path: &[u8], magic: &[u8; 12], version: u32) -> Option<usize> {
    if !fs::is_mounted() {
        return None;
    }

    let (buffer, size) = match read_file(path) {
        Some(image) => image,
        None => {
            kerror!("user: {:?} not found\n", path);
            return None;
        }
    };
    let image = unsafe { core::slice::from_raw_parts(buffer as *const u8, size) };

    if !find_header(image, magic, version) {
        kerror!("user: {:?} has the wrong PINX header\n", path);
        mm::kfree(buffer);
        return None;
    }

    let entry = match load_elf(image) {
        Some(entry) => entry,
        None => {
            kerror!("user: {:?} is not a loadable x86-64 ELF\n", path);
            mm::kfree(buffer);
            return None;
        }
    };

    // Every segment was copied into its own frames, so the file image is no
    // longer needed.
    mm::kfree(buffer);

    let stack_top = map_user_stack();
    let pid = crate::scheduler::spawn_user(entry, stack_top);

    match pid {
        Some(_) => kinfo!("user: loaded {:?} at {:#x}\n", core::str::from_utf8(path).unwrap_or("?"), entry),
        None => kerror!("user: could not create a task for {:?}\n", core::str::from_utf8(path).unwrap_or("?")),
    }
    pid
}

/// Read the whole `path` file into a fresh kernel-heap buffer.
fn read_file(path: &[u8]) -> Option<(*mut u8, usize)> {
    let root = fs::root()?;

    let ino = match fs::lookup(root, path) {
        Some(Ok(Some(ino))) => ino,
        Some(Err(e)) => {
            kerror!("user: lookup failed: {:?}\n", e);
            return None;
        }
        _ => return None,
    };

    let inode = match fs::read_inode(ino) {
        Some(Ok(inode)) => inode,
        Some(Err(e)) => {
            kerror!("user: read_inode failed: {:?}\n", e);
            return None;
        }
        None => return None,
    };
    if !inode.is_file() {
        kerror!("user: not a regular file\n");
        return None;
    }
    if inode.size == 0 || inode.size > MAX_INIT_SIZE {
        kerror!("user: implausible size ({})\n", inode.size);
        return None;
    }

    let size = inode.size as usize;
    let buffer = mm::kmalloc(inode.size);
    if buffer.is_null() {
        kerror!("user: out of memory reading file\n");
        return None;
    }

    let slice = unsafe { core::slice::from_raw_parts_mut(buffer, size) };
    let mut done = 0;
    while done < size {
        match fs::read_file(ino, done as u64, &mut slice[done..]) {
            Some(Ok(0)) => break,
            Some(Ok(n)) => done += n,
            Some(Err(e)) => {
                kerror!("user: read failed: {:?}\n", e);
                mm::kfree(buffer);
                return None;
            }
            None => {
                mm::kfree(buffer);
                return None;
            }
        }
    }

    if done != size {
        kerror!("user: truncated ({} of {} bytes)\n", done, size);
        mm::kfree(buffer);
        return None;
    }

    Some((buffer, size))
}

/// Scan `image` for a PINX header whose magic is `magic` and whose version is
/// `version`. A byte scan is enough: the header is a global and lands in
/// `.rodata`, whose position is up to the linker.
fn find_header(image: &[u8], magic: &[u8; 12], version: u32) -> bool {
    let header_size = size_of::<PinxProgramHeader>();
    if image.len() < header_size {
        return false;
    }

    let mut i = 0;
    while i + header_size <= image.len() {
        if image[i..i + magic.len()] == *magic {
            let found = read_u32(image, i + magic.len());
            if found == version {
                return true;
            }
            kerror!(
                "user: ABI version {} does not match kernel {}\n",
                found,
                version
            );
            return false;
        }
        i += 1;
    }

    false
}

/// Parse an `ET_EXEC` x86-64 ELF, map every `PT_LOAD` segment into the lower
/// half with user permissions, and return the entry point.
fn load_elf(image: &[u8]) -> Option<u64> {
    if image.len() < 64 {
        return None;
    }
    // Magic, 64-bit class, little-endian.
    if &image[0..4] != b"\x7FELF" || image[4] != 2 || image[5] != 1 {
        return None;
    }
    if read_u16(image, 16) != ET_EXEC || read_u16(image, 18) != EM_X86_64 {
        return None;
    }

    let entry = read_u64(image, 24);
    let phoff = read_u64(image, 32) as usize;
    let phentsize = read_u16(image, 54) as usize;
    let phnum = read_u16(image, 56) as usize;

    // Every field read below indexes 56 bytes of a program header.
    if phentsize < 56 {
        return None;
    }

    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        if ph + 56 > image.len() {
            return None;
        }
        if read_u32(image, ph) != PT_LOAD {
            continue;
        }

        let offset = read_u64(image, ph + 8) as usize;
        let vaddr = read_u64(image, ph + 16);
        let filesz = read_u64(image, ph + 32) as usize;
        let memsz = read_u64(image, ph + 40) as usize;

        if filesz > memsz {
            return None;
        }
        if offset.checked_add(filesz)? > image.len() {
            return None;
        }
        if memsz == 0 {
            continue;
        }
        if !map_segment(image, offset, vaddr, filesz, memsz) {
            return None;
        }
    }

    Some(entry)
}

/// Map one `PT_LOAD` segment. `[vaddr, vaddr + filesz)` is copied from the file
/// and the rest of `[vaddr, vaddr + memsz()` (the `.bss` tail) is left zeroed.
fn map_segment(image: &[u8], file_offset: usize, vaddr: u64, filesz: usize, memsz: usize) -> bool {
    let mem_end = match vaddr.checked_add(memsz as u64) {
        Some(end) => end,
        None => return false,
    };
    let file_end = vaddr + filesz as u64;

    let mut page = vaddr & !(mm::PAGE_SIZE - 1);
    while page < mem_end {
        // Two PT_LOAD segments often share a page: the linker packs the tail of
        // one into the same 4 KiB frame as the head of the next. Only the first
        // segment to touch a page allocates and zeroes it; later ones reuse that
        // frame, or the second segment would map a fresh zeroed page over the
        // first and wipe it out.
        let mut frame = mm::vmm_virt_to_phys(page);
        if frame == mm::VMM_NO_PHYS {
            frame = mm::pmm_alloc_page();
            if frame == mm::PMM_NO_PAGE {
                kerror!("user: out of memory mapping /init segment\n");
                return false;
            }

            let dst = mm::phys_to_virt(frame) as *mut u8;
            unsafe { core::ptr::write_bytes(dst, 0, mm::PAGE_SIZE as usize) };
            mm::vmm_map_user_page(page, frame);
        }

        let dst = mm::phys_to_virt(frame) as *mut u8;

        // The part of this page that the file actually provides.
        let copy_start = core::cmp::max(page, vaddr);
        let copy_end = core::cmp::min(page + mm::PAGE_SIZE, file_end);
        if copy_start < copy_end {
            let count = (copy_end - copy_start) as usize;
            let source = file_offset + (copy_start - vaddr) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    image.as_ptr().add(source),
                    dst.add((copy_start - page) as usize),
                    count,
                );
            }
        }

        page += mm::PAGE_SIZE;
    }

    true
}

/// Map a fresh, zeroed user stack for one task and return its top (the initial
/// `RSP`). Each call carves a new slice out of the shared lower-half region; the
/// guard gap in [`USER_STACK_SPAN`] keeps consecutive stacks apart.
fn map_user_stack() -> u64 {
    unsafe {
        let top = NEXT_STACK;
        NEXT_STACK -= USER_STACK_SPAN;

        for i in 0..USER_STACK_PAGES {
            let frame = mm::pmm_alloc_page();
            if frame == mm::PMM_NO_PAGE {
                kpanic!("user: out of memory for a user stack");
            }

            let dst = mm::phys_to_virt(frame) as *mut u8;
            core::ptr::write_bytes(dst, 0, mm::PAGE_SIZE as usize);

            let page = top - (USER_STACK_PAGES - i) * mm::PAGE_SIZE;
            mm::vmm_map_user_page(page, frame);
        }

        top
    }
}

fn read_u16(image: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([image[offset], image[offset + 1]])
}

fn read_u32(image: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        image[offset],
        image[offset + 1],
        image[offset + 2],
        image[offset + 3],
    ])
}

fn read_u64(image: &[u8], offset: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&image[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}
