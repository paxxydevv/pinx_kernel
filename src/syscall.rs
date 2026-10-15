//! System calls.
//!
//! User code reaches the kernel with `int 0x80`. The stub in `asm.rs` saves the
//! user register state into an `InterruptFrame` and calls [`dispatch`] here.
//! The calling convention is:
//!
//! ```text
//! rax                         syscall number
//! rdi, rsi, rdx, r10, r8, r9  arguments
//! rax                         return value (written back before iretq)
//! ```

use crate::interrupts::InterruptFrame;
use crate::spinlock::SpinLock;
use crate::user_abi::{PinxEvent, PinxFramebufferInfo, PINX_EVENT_KEY};
use crate::{apic, graphics, keyboard, mm, scheduler, terminal};

pub const SYS_WRITE: u64 = 0;
pub const SYS_EXIT: u64 = 1;
pub const SYS_YIELD: u64 = 2;
pub const SYS_SPAWN: u64 = 3;
pub const SYS_FB_MAP: u64 = 4;
pub const SYS_EVENT_READ: u64 = 5;
pub const SYS_MMAP: u64 = 6;
pub const SYS_SLEEP: u64 = 7;

/// The split between user and kernel virtual addresses. Anything at or above it
/// must never be dereferenced on behalf of ring 3.
const USER_ADDRESS_MAX: u64 = 0x0000_8000_0000_0000;

/// `u64::MAX` doubles as `-1`; kept as a named constant so call sites read as
/// "invalid argument" rather than a magic number.
const EINVAL: u64 = u64::MAX;

/// Entry point for `int 0x80`. Reads the arguments out of the saved frame,
/// dispatches, and writes the result back to `rax` so `iretq` returns it.
/// Returns the frame to resume: identical to `frame` unless the call rescheduled
/// this CPU onto another task.
#[no_mangle]
pub extern "C" fn dispatch(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    let f = unsafe { &mut *frame };

    match f.rax {
        SYS_WRITE => {
            f.rax = sys_write(f.rdi, f.rsi, f.rdx);
            frame
        }
        SYS_EXIT => {
            // Retire this task and let the scheduler pick a replacement. The
            // frame this call came in on belongs to the dead task, so it must
            // not be resumed.
            scheduler::mark_current_dead();
            unsafe { scheduler::schedule(frame) }
        }
        SYS_YIELD => {
            f.rax = 0;
            unsafe { scheduler::schedule(frame) }
        }
        SYS_SPAWN => {
            f.rax = sys_spawn(f.rdi, f.rsi);
            frame
        }
        SYS_FB_MAP => {
            f.rax = sys_fb_map(f.rdi);
            frame
        }
        SYS_EVENT_READ => match sys_event_read(f.rdi, f.rsi) {
            Err(()) => {
                f.rax = EINVAL;
                frame
            }
            // Nothing buffered and the caller asked to block: park the task.
            // It resumes at the return address with `rax = 0`, which the libc
            // wrapper treats as "woken without events, ask again" and retries
            // the (now successful) syscall. That keeps the kernel from having
            // to finish the read on another CPU's IRQ.
            Ok(0) if f.rdx != 0 => {
                f.rax = 0;
                scheduler::block_current_input();
                unsafe { scheduler::schedule(frame) }
            }
            Ok(count) => {
                f.rax = count;
                frame
            }
        },
        SYS_MMAP => {
            f.rax = sys_mmap(f.rdi);
            frame
        }
        SYS_SLEEP => {
            let ms = f.rdi;
            f.rax = 0;
            if ms == 0 {
                unsafe { scheduler::schedule(frame) }
            } else {
                scheduler::block_current_until(apic::uptime_ms() + ms);
                unsafe { scheduler::schedule(frame) }
            }
        }
        _ => {
            f.rax = EINVAL;
            frame
        }
    }
}

/// `spawn(path)`. Load `path` from the data disk and register it as a new
/// ring-3 task in the shared address space. Returns the new task id, or
/// `EINVAL` on any failure. Does not block: the caller keeps running, and the
/// new task starts whenever a CPU next schedules it.
#[cfg(feature = "ahci")]
fn sys_spawn(path_ptr: u64, path_len: u64) -> u64 {
    if path_len == 0 || path_len > 256 || !user_range_valid(path_ptr, path_len) {
        return EINVAL;
    }

    let path = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    match crate::user::spawn_program(path) {
        Some(pid) => pid as u64,
        None => EINVAL,
    }
}

#[cfg(not(feature = "ahci"))]
fn sys_spawn(_path_ptr: u64, _path_len: u64) -> u64 {
    EINVAL
}

/// `write(fd, buffer, length)`. Only the console (`1`) and stderr (`2`) exist.
fn sys_write(fd: u64, buffer: u64, length: u64) -> u64 {
    if fd != 1 && fd != 2 {
        return EINVAL;
    }
    if length == 0 {
        return 0;
    }
    if !user_range_valid(buffer, length) {
        return EINVAL;
    }

    let bytes = buffer as *const u8;
    let mut written = 0;
    while written < length {
        let byte = unsafe { *bytes.add(written as usize) };
        terminal::putc(byte);
        written += 1;
    }

    length
}

/// Where anonymous mappings are carved from. Above the program load bases and
/// the framebuffer, below the user stacks (which grow down from
/// `0x7FFF_0000_0000`).
const MMAP_BASE: u64 = 0x0000_6000_0000_0000;

/// Next free address in the anonymous-mapping region.
static mut MMAP_NEXT: u64 = MMAP_BASE;
/// Serialises address carving and frame allocation across CPUs.
static MMAP_LOCK: SpinLock = SpinLock::new();

/// `mmap(size)`. Allocate `size` bytes of zeroed, writable anonymous memory and
/// return its user virtual address (page aligned), or `EINVAL`. Every task
/// shares one address space, so the mapping is visible to all of them. There is
/// no `munmap`: memory is only ever handed out, never reused.
fn sys_mmap(size: u64) -> u64 {
    if size == 0 || size > (1 << 40) {
        return EINVAL;
    }

    let _guard = MMAP_LOCK.lock();
    let pages = (size + mm::PAGE_SIZE - 1) / mm::PAGE_SIZE;
    let length = pages * mm::PAGE_SIZE;

    let base = unsafe { core::ptr::read(core::ptr::addr_of!(MMAP_NEXT)) };
    let end = match base.checked_add(length) {
        Some(end) if end <= USER_ADDRESS_MAX => end,
        _ => return EINVAL,
    };

    // Reserve the address range up front so a mid-way failure cannot hand the
    // same range to the next caller.
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(MMAP_NEXT), end) };

    let mut page = 0;
    while page < pages {
        let frame = mm::pmm_alloc_page();
        if frame == mm::PMM_NO_PAGE {
            return EINVAL;
        }
        // Anonymous memory reads as zero.
        unsafe {
            core::ptr::write_bytes(
                mm::phys_to_virt(frame) as *mut u8,
                0,
                mm::PAGE_SIZE as usize,
            )
        };
        mm::vmm_map_user_page(base + page * mm::PAGE_SIZE, frame);
        page += 1;
    }

    base
}

/// Where user code reaches the screen. Below the user stacks
/// (`0x7FFF_0000_0000`) and above the fixed program load bases.
const FB_USER_BASE: u64 = 0x0000_5000_0000_0000;

/// Whether the framebuffer has been mapped into the shared user address space
/// yet. Every task shares one-page table, so this only has to happen once.
static mut FB_MAPPED: bool = false;

/// `map_framebuffer(info)`. Map the linear framebuffer into the caller's
/// address space (once, since all tasks share page tables) and fill `info` with
/// its user address and geometry. Returns 0, or `EINVAL`.
fn sys_fb_map(info_ptr: u64) -> u64 {
    if !user_range_valid(info_ptr, core::mem::size_of::<PinxFramebufferInfo>() as u64) {
        return EINVAL;
    }

    let width = graphics::framebuffer_width();
    let height = graphics::framebuffer_height();
    let pitch = graphics::framebuffer_pitch();
    let bpp = graphics::framebuffer_bpp();
    if width == 0 || height == 0 || pitch == 0 || bpp == 0 {
        return EINVAL;
    }

    let base = graphics::framebuffer_address();
    if base == 0 {
        return EINVAL;
    }
    // The Limine pointer is a kernel (HHDM) virtual address; strip the offset
    // to get the physical base the framebuffer actually lives at.
    let physical = base - mm::hhdm_offset();

    let size = match pitch.checked_mul(height) {
        Some(size) => size,
        None => return EINVAL,
    };

    unsafe {
        if !core::ptr::read(core::ptr::addr_of!(FB_MAPPED)) {
            let offset = physical & (mm::PAGE_SIZE - 1);
            let physical_base = physical - offset;
            let pages = (size + offset + mm::PAGE_SIZE - 1) / mm::PAGE_SIZE;
            let mut page = 0;
            while page < pages {
                mm::vmm_map_user_mmio(
                    FB_USER_BASE + page * mm::PAGE_SIZE,
                    physical_base + page * mm::PAGE_SIZE,
                );
                page += 1;
            }
            core::ptr::write(core::ptr::addr_of_mut!(FB_MAPPED), true);
        }

        let info = info_ptr as *mut PinxFramebufferInfo;
        core::ptr::write(
            info,
            PinxFramebufferInfo {
                address: FB_USER_BASE,
                width,
                height,
                pitch,
                bpp,
            },
        );
    }

    0
}

/// `read_events(buffer, max)`. Copy up to `max` pending input events into
/// `buffer`. Returns the number written (0 when none are ready) or `Err` on a
/// bad buffer. Blocking is handled by the caller: a `Ok(0)` with the block flag
/// set parks the task.
fn sys_event_read(buffer: u64, max_events: u64) -> Result<u64, ()> {
    if max_events == 0 {
        return Ok(0);
    }
    let bytes = max_events
        .checked_mul(core::mem::size_of::<PinxEvent>() as u64)
        .ok_or(())?;
    if !user_range_valid(buffer, bytes) {
        return Err(());
    }

    let events = buffer as *mut PinxEvent;
    let mut count = 0;
    while count < max_events && keyboard::has_input() {
        let code = keyboard::input();
        if code == keyboard::KEY_NONE {
            break;
        }
        let event = PinxEvent {
            kind: PINX_EVENT_KEY,
            code: code as u32,
            value: 1,
            _reserved: 0,
        };
        unsafe { core::ptr::write(events.add(count as usize), event) };
        count += 1;
    }
    Ok(count)
}

/// Check that `[address, address + length)` sits entirely in user space and is
/// mapped. Without it a program could point the kernel at its own page tables,
/// the HHDM or the kernel image and have the kernel read them for it.
fn user_range_valid(address: u64, length: u64) -> bool {
    let end = match address.checked_add(length) {
        Some(end) => end,
        None => return false,
    };
    if address == 0 || end > USER_ADDRESS_MAX {
        return false;
    }

    let mut page = address & !(mm::PAGE_SIZE - 1);
    while page < end {
        if mm::vmm_virt_to_phys(page) == mm::VMM_NO_PHYS {
            return false;
        }
        page += mm::PAGE_SIZE;
    }

    true
}
