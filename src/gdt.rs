//! Global Descriptor Table, per CPU.
//!
//! Long mode ignores segment bases and limits, so the descriptors here exist
//! only to name privilege levels. The layout is:
//!
//! ```text
//! 0  null
//! 1  kernel code   0x08
//! 2  kernel data   0x10
//! 3  user code     0x18 (loaded as 0x1B, RPL 3)
//! 4  user data     0x20 (loaded as 0x23, RPL 3)
//! 5  TSS           0x28 (16 bytes, fills slots 5 and 6)
//! ```
//!
//! Long mode has no hardware task switching, but the CPU still reads `RSP0`
//! from the TSS to find the ring-0 stack whenever code traps from ring 3 into
//! ring 0.
//!
//! Every CPU needs its own GDT because the TSS descriptor embeds the address of
//! that CPU's TSS, and each CPU's TSS has a different `RSP0` (the kernel stack
//! of whatever task it is currently running).

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GdtPtr {
    limit: u16,
    base: u64,
}

/// The 64-bit Task State Segment. Only `rsp0` matters here: it is the stack the
/// CPU switches to on a ring 3 -> ring 0 transition.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}

/// A long-mode system descriptor. Unlike the 8-byte code and data entries, a
/// system descriptor is 16 bytes, so it occupies two GDT slots.
#[repr(C, packed)]
struct TssDescriptor {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    flags: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

const ZERO_ENTRY: GdtEntry = GdtEntry {
    limit_low: 0,
    base_low: 0,
    base_middle: 0,
    access: 0,
    granularity: 0,
    base_high: 0,
};

const ZERO_TSS: Tss = Tss {
    reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    iomap_base: 0,
};

const ZERO_PTR: GdtPtr = GdtPtr { limit: 0, base: 0 };

/// Upper bound on CPUs the tables below are sized for.
pub const MAX_CPUS: usize = 32;

/// null, kernel code/data, user code/data, then the 16-byte TSS descriptor.
const GDT_ENTRIES: usize = 7;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
/// User code, with RPL 3 so `CS` reports ring 3 once loaded.
pub const USER_CODE_SELECTOR: u16 = 0x18 | 3;
/// User data, with RPL 3 so `SS` matches the code selector's ring.
pub const USER_DATA_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 5 * 8;

const RING0_STACK_SIZE: usize = 16 * 1024;

/// Per-CPU ring-0 stack, used as the `RSP0` target for traps out of ring 3 and
/// as the boot stack for an application processor.
#[repr(align(16))]
struct Ring0Stacks([[u8; RING0_STACK_SIZE]; MAX_CPUS]);

static mut GDT_TABLE: [[GdtEntry; GDT_ENTRIES]; MAX_CPUS] = [[ZERO_ENTRY; GDT_ENTRIES]; MAX_CPUS];
static mut GDTR_TABLE: [GdtPtr; MAX_CPUS] = [ZERO_PTR; MAX_CPUS];
static mut TSS_TABLE: [Tss; MAX_CPUS] = [ZERO_TSS; MAX_CPUS];
static mut RING0_STACKS: Ring0Stacks = Ring0Stacks([[0; RING0_STACK_SIZE]; MAX_CPUS]);

extern "C" {
    fn gdt_flush(ptr: *const GdtPtr);
}

unsafe fn set_entry(entry: *mut GdtEntry, base: u32, limit: u32, access: u8, granularity: u8) {
    (*entry).base_low = (base & 0xFFFF) as u16;
    (*entry).base_middle = ((base >> 16) & 0xFF) as u8;
    (*entry).base_high = ((base >> 24) & 0xFF) as u8;

    (*entry).limit_low = (limit & 0xFFFF) as u16;

    (*entry).granularity = (((limit >> 16) & 0x0F) as u8) | (granularity & 0xF0);

    (*entry).access = access;
}

/// Fill in the 16-byte TSS descriptor. `base` is the full 64-bit address of the
/// TSS, split across `base_high` and `base_upper`; that upper half is what makes
/// this descriptor twice the size of the code and data ones.
unsafe fn set_tss_entry(entry: *mut TssDescriptor, base: u64, limit: u32) {
    (*entry).limit_low = (limit & 0xFFFF) as u16;
    (*entry).base_low = (base & 0xFFFF) as u16;
    (*entry).base_middle = ((base >> 16) & 0xFF) as u8;
    // Present, ring 0, 64-bit TSS (available).
    (*entry).access = 0x89;
    // Byte granularity, so the limit is the raw 20-bit value.
    (*entry).flags = ((limit >> 16) & 0x0F) as u8;
    (*entry).base_high = ((base >> 24) & 0xFF) as u8;
    (*entry).base_upper = (base >> 32) as u32;
    (*entry).reserved = 0;
}

unsafe fn load_tss(selector: u16) {
    core::arch::asm!("ltr {0:x}", in(reg) selector, options(nostack, preserves_flags));
}

/// Top (highest address) of CPU `cpu`'s ring-0 stack, 16-byte aligned.
pub fn kstack_top(cpu: usize) -> u64 {
    unsafe {
        let base = core::ptr::addr_of!(RING0_STACKS.0[cpu]) as u64;
        (base + RING0_STACK_SIZE as u64) & !0xF
    }
}

/// Point the current CPU's `RSP0` at the stack the CPU should use for the next
/// ring 3 -> ring 0 transition (the kernel stack of the task being scheduled).
pub fn set_kernel_stack(cpu: usize, rsp0: u64) {
    unsafe {
        (*core::ptr::addr_of_mut!(TSS_TABLE[cpu])).rsp0 = rsp0;
    }
}

/// Build and load the GDT (and TSS) for CPU `cpu`.
pub fn init_cpu(cpu: usize) {
    unsafe {
        let gdt = core::ptr::addr_of_mut!(GDT_TABLE[cpu]) as *mut GdtEntry;

        // Entry 0: null descriptor.
        set_entry(gdt.add(0), 0, 0, 0x00, 0x00);
        // Entry 1: kernel 64-bit code, present, ring 0, readable, long mode.
        set_entry(gdt.add(1), 0, 0, 0x9A, 0xA0);
        // Entry 2: kernel data, present, ring 0, writable.
        set_entry(gdt.add(2), 0, 0, 0x92, 0x00);
        // Entry 3: user 64-bit code, present, ring 3 (DPL 0b11), readable.
        set_entry(gdt.add(3), 0, 0, 0xFA, 0xA0);
        // Entry 4: user data, present, ring 3 (DPL 0b11), writable.
        set_entry(gdt.add(4), 0, 0, 0xF2, 0x00);

        // The TSS must name its ring-0 stack before it is loaded: the first
        // trap out of ring 3 reads `rsp0` and switches to it.
        let tss = core::ptr::addr_of_mut!(TSS_TABLE[cpu]);
        (*tss).rsp0 = kstack_top(cpu);
        // Any offset at or past the TSS size means "no I/O permission bitmap".
        (*tss).iomap_base = core::mem::size_of::<Tss>() as u16;

        // Entries 5 and 6: the TSS descriptor, which is 16 bytes wide.
        let tss_entry = gdt.add(5) as *mut TssDescriptor;
        set_tss_entry(tss_entry, tss as u64, (core::mem::size_of::<Tss>() - 1) as u32);

        core::ptr::write(
            core::ptr::addr_of_mut!(GDTR_TABLE[cpu]),
            GdtPtr {
                limit: (core::mem::size_of::<[GdtEntry; GDT_ENTRIES]>() - 1) as u16,
                base: gdt as u64,
            },
        );

        gdt_flush(core::ptr::addr_of!(GDTR_TABLE[cpu]));
        load_tss(TSS_SELECTOR);
    }
}
