//! x2APIC and IOAPIC bring-up.
//!
//! The kernel drives the local APIC entirely through the x2APIC MSR interface,
//! and routes the single PS/2 keyboard line through the IOAPIC. The 8259 pair
//! is masked so it cannot inject anything on the legacy vectors.

use core::arch::x86_64::__cpuid;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::io::outb;
use crate::keyboard::IRQ_VECTOR as KB_IRQ_VECTOR;
use crate::{kpanic, mm};

const IOAPIC_BASE: u64 = 0xFEC0_0000;
const IOAPIC_VIRT: u64 = 0xFFFF_C000_0000_0000;
const IOAPIC_IOREGSEL: u64 = 0x00;
const IOAPIC_IOWIN: u64 = 0x10;

/* Bit layout of an IOAPIC redirection entry. */
const IOAPIC_REDIR_DEST_ID_SHIFT: u32 = 24;
const IOAPIC_REDIR_MASK_SHIFT: u32 = 16;
const IOAPIC_REDIR_TRIGGER_SHIFT: u32 = 15;
const IOAPIC_REDIR_POLARITY_SHIFT: u32 = 13;
const IOAPIC_REDIR_DEST_PHYS_SHIFT: u32 = 11;
const IOAPIC_REDIR_DEST_MODE_SHIFT: u32 = 8;
const IOAPIC_REDIR_BASE: u32 = 0x10;

const PIC1_CMD: u16 = 0x21;
const PIC2_CMD: u16 = 0xA1;

const X2APIC_MSR_BASE: u32 = 0x800;
const X2APIC_MSR_EOI: u32 = X2APIC_MSR_BASE + 0x0B;
const X2APIC_MSR_APICID: u32 = X2APIC_MSR_BASE + 0x02;
const X2APIC_MSR_SPIVR: u32 = X2APIC_MSR_BASE + 0x0F;
const X2APIC_MSR_LVT_TIMER: u32 = X2APIC_MSR_BASE + 0x32;
const X2APIC_MSR_LVT_LINT0: u32 = X2APIC_MSR_BASE + 0x35;
const X2APIC_MSR_LVT_LINT1: u32 = X2APIC_MSR_BASE + 0x36;
const X2APIC_MSR_LVT_ERROR: u32 = X2APIC_MSR_BASE + 0x37;
const X2APIC_MSR_TMRINITCNT: u32 = X2APIC_MSR_BASE + 0x38;
const X2APIC_MSR_TMRDIV: u32 = X2APIC_MSR_BASE + 0x3E;

const IA32_APIC_BASE_MSR: u32 = 0x1B;

/// LVT Timer / LVT Error bits, shared by every LVT register.
const APIC_LVT_MASKED: u32 = 1 << 16;
const APIC_LVT_PERIODIC: u32 = 1 << 17;

pub const TIMER_VECTOR: u8 = 32;
pub const SPURIOUS_VECTOR: u8 = 0xFF;

static APIC_TICKS: AtomicU32 = AtomicU32::new(0);

/// Milliseconds since boot. Only the BSP advances it (its timer fires once per
/// millisecond), so it reads as wall-clock time regardless of the CPU count.
static UPTIME_MS: AtomicU64 = AtomicU64::new(0);

/// LAPIC timer frequency source shared with the APs, set once by the BSP.
static mut TSC_FREQ: u32 = 0;

#[inline]
fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack));
    }
    ((high as u64) << 32) | low as u64
}

#[inline]
fn wrmsr(msr: u32, value: u64) {
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack)
        );
    }
}

#[inline]
fn x2apic_write(reg: u32, val: u32) {
    wrmsr(reg, val as u64);
}

#[inline]
fn ioapic_addr(reg: u64) -> *mut u32 {
    (IOAPIC_VIRT + reg) as *mut u32
}

fn ioapic_write(reg: u32, val: u32) {
    unsafe {
        core::ptr::write_volatile(ioapic_addr(IOAPIC_IOREGSEL), reg);
        core::ptr::write_volatile(ioapic_addr(IOAPIC_IOWIN), val);
    }
}

fn cpuid_has_x2apic() -> bool {
    let result = __cpuid(1);
    (result.ecx >> 21) & 1 != 0
}

/// In x2APIC mode the APIC ID is the low 32 bits of the APICID MSR.
fn apic_get_id() -> u32 {
    (rdmsr(X2APIC_MSR_APICID) & 0xFFFF_FFFF) as u32
}

/// This CPU's x2APIC id. Used to find the per-CPU slot a thread is running on.
pub fn local_id() -> u32 {
    apic_get_id()
}

fn ioapic_route_pin(pin: u8, vector: u8) {
    let reg_low = IOAPIC_REDIR_BASE + 2 * pin as u32;
    let reg_high = reg_low + 1;

    let low = vector as u32
        | (0 << IOAPIC_REDIR_DEST_MODE_SHIFT)  /* fixed          */
        | (0 << IOAPIC_REDIR_DEST_PHYS_SHIFT)  /* physical       */
        | (0 << IOAPIC_REDIR_POLARITY_SHIFT)   /* active high    */
        | (0 << IOAPIC_REDIR_TRIGGER_SHIFT)    /* edge triggered */
        | (0 << IOAPIC_REDIR_MASK_SHIFT);      /* unmasked       */

    // The destination field lives in bits 31:24 of the high half. Writing 0
    // here only worked because the BSP happened to be APIC 0 under OVMF.
    let high = (apic_get_id() & 0xFF) << IOAPIC_REDIR_DEST_ID_SHIFT;

    ioapic_write(reg_low, low);
    ioapic_write(reg_high, high);
}

/// Per-CPU local APIC bring-up: switch to x2APIC, mask the unused local
/// interrupt lines and enable the APIC with a real spurious vector.
pub fn init_cpu() {
    if !cpuid_has_x2apic() {
        kpanic!("x2APIC is not supported by this CPU");
    }

    let mut base = rdmsr(IA32_APIC_BASE_MSR);
    base &= !(1u64 << 11);
    base |= (1u64 << 10) | (1u64 << 11);
    wrmsr(IA32_APIC_BASE_MSR, base);

    // LINT0/LINT1 are the two 8259 outputs: mask both, nothing is wired.
    x2apic_write(X2APIC_MSR_LVT_LINT0, APIC_LVT_MASKED);
    x2apic_write(X2APIC_MSR_LVT_LINT1, APIC_LVT_MASKED);

    // LVT Error is masked for now; unmasking needs a vector of its own.
    x2apic_write(X2APIC_MSR_LVT_ERROR, APIC_LVT_MASKED);

    // Enable the APIC and point its spurious vector at a real IDT gate.
    x2apic_write(X2APIC_MSR_SPIVR, SPURIOUS_VECTOR as u32 | (1 << 8));
}

/// Application processor bring-up: the local APIC only.
pub fn init_ap() {
    init_cpu();
}

/// Bootstrap processor bring-up: the local APIC, the 8259 mask, and IOAPIC
/// routing for the keyboard. Only the BSP owns the IOAPIC.
pub fn init_bsp() {
    init_cpu();

    unsafe {
        outb(PIC1_CMD, 0xFF);
        outb(PIC2_CMD, 0xFF);
    }

    mm::vmm_map_mmio(IOAPIC_VIRT, IOAPIC_BASE);
    ioapic_route_pin(1, KB_IRQ_VECTOR);
}

pub fn send_eoi() {
    x2apic_write(X2APIC_MSR_EOI, 0);
}

pub fn timer_init(tsc_freq: u32) {
    // Mask first, then program the divider/count, then unmask. Writing an
    // unmasked timer before TMRINITCNT means the first period is whatever was
    // left in the counter, which on a freshly reset APIC is 0.
    x2apic_write(
        X2APIC_MSR_LVT_TIMER,
        TIMER_VECTOR as u32 | APIC_LVT_MASKED | APIC_LVT_PERIODIC,
    );

    // Divide by 16.
    x2apic_write(X2APIC_MSR_TMRDIV, 0x03);

    // The counter decrements at tsc_freq / 16, so one millisecond of ticks is
    // tsc_freq / 16 / 1000 = tsc_freq / 16000.
    let ticks_per_ms = tsc_freq / (16 * 1000);
    if ticks_per_ms == 0 {
        kpanic!("apic_timer_init: tsc frequency too low for a 1ms period");
    }
    x2apic_write(X2APIC_MSR_TMRINITCNT, ticks_per_ms);

    // Periodic, unmasked.
    x2apic_write(X2APIC_MSR_LVT_TIMER, TIMER_VECTOR as u32 | APIC_LVT_PERIODIC);
}

/// Remember the TSC frequency so every CPU can program its own timer.
pub fn set_tsc_freq(tsc_freq: u32) {
    unsafe {
        TSC_FREQ = tsc_freq;
    }
}

/// Program this CPU's LAPIC timer using the frequency the BSP recorded.
pub fn timer_init_cpu() {
    let freq = unsafe { TSC_FREQ };
    if freq == 0 {
        kpanic!("timer_init_cpu: TSC frequency was never set");
    }
    timer_init(freq);
}

pub fn timer_irq() {
    APIC_TICKS.fetch_add(1, Ordering::Relaxed);
    // The BSP keeps a monotonic millisecond clock for `sleep` and friends. The
    // APs run their own periodic timers but must not add to it, or uptime would
    // advance once per CPU per millisecond.
    if crate::smp::cpu_index() == 0 {
        UPTIME_MS.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn get_ticks() -> u32 {
    APIC_TICKS.load(Ordering::Relaxed)
}

/// Milliseconds since boot.
pub fn uptime_ms() -> u64 {
    UPTIME_MS.load(Ordering::Relaxed)
}
