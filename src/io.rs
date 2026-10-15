//! Port I/O and interrupt-flag helpers.
//!
//! Every function here is a thin wrapper around one instruction. They are the
//! only place the kernel talks to the x86 I/O ports and the interrupt flag, so
//! the rest of the code never has to carry an `asm!` block around.

use core::arch::asm;

/// `RFLAGS.IF`, the interrupt-enable flag.
const X86_FLAG_IF: u64 = 0x200;

#[inline]
pub unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nostack));
}

#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    asm!("in al, dx", in("dx") port, out("al") ret, options(nostack));
    ret
}

#[inline]
pub unsafe fn outl(port: u16, val: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack));
}

#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let ret: u32;
    asm!("in eax, dx", in("dx") port, out("eax") ret, options(nostack));
    ret
}

/// A short, implementation-defined delay, used to give slow devices time to
/// catch up on the legacy `0x80` port.
#[inline]
pub fn io_wait() {
    unsafe { outb(0x80, 0) }
}

#[inline]
pub fn irq_disable() {
    unsafe { asm!("cli", options(nomem, nostack)) }
}

#[inline]
pub fn irq_enable() {
    unsafe { asm!("sti", options(nomem, nostack)) }
}

/// Disable interrupts and return the previous RFLAGS so they can be put back
/// with [`irq_restore`]. The save and the `cli` happen back to back, so no
/// interrupt can slip in between.
#[inline]
pub fn irq_save() -> u64 {
    let flags: u64;
    unsafe {
        asm!("pushfq; pop {}", out(reg) flags);
        irq_disable();
    }
    flags
}

#[inline]
pub fn irq_restore(flags: u64) {
    if flags & X86_FLAG_IF != 0 {
        irq_enable();
    }
}

/// Stop the CPU until the next interrupt. Safe to call with interrupts off:
/// the CPU keeps the pending interrupt latched and wakes on it once enabled.
#[inline]
pub fn hlt() {
    unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) }
}
