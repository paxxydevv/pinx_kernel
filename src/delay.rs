//! Millisecond delays on top of the LAPIC timer.

use crate::{apic, io};

pub fn init(tsc_freq: u32) {
    apic::timer_init(tsc_freq);
}

pub fn sleep_ms(ms: u32) {
    let flags = io::irq_save();
    let start = apic::get_ticks();

    // HLT only wakes on an interrupt, so interrupts have to be on inside the
    // loop. They are put back the way the caller had them on the way out.
    io::irq_enable();

    // Unsigned subtraction instead of `ticks < start + ms`, so the compare
    // still behaves across the 32-bit tick counter wrapping.
    while apic::get_ticks().wrapping_sub(start) < ms {
        io::hlt();
    }

    io::irq_restore(flags);
}
