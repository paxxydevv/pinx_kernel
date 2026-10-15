//! Interrupt Descriptor Table.
//!
//! Every gate points at an assembly stub defined in `asm.rs`. The exceptions
//! look after themselves, the IRQs all funnel through `irq_common_handler`, and
//! the two spurious vectors get gates of their own so an interrupt the CPU
//! raises on its own cannot escalate into a triple fault.

use core::arch::asm;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64,
}

const ZERO_ENTRY: IdtEntry = IdtEntry {
    offset_low: 0,
    selector: 0,
    ist: 0,
    type_attr: 0,
    offset_mid: 0,
    offset_high: 0,
    reserved: 0,
};

/// The 8259 reports an IRQ7 that never asserted on vector 15.
const PIC_SPURIOUS_VECTOR: usize = 15;
/// The local APIC's spurious interrupt vector, from `apic::SPURIOUS_VECTOR`.
const APIC_SPURIOUS_VECTOR: usize = 0xFF;

static mut IDT: [IdtEntry; 256] = [ZERO_ENTRY; 256];
static mut IDTR: IdtPtr = IdtPtr { limit: 0, base: 0 };

extern "C" {
    fn isr0();
    fn isr1();
    fn isr2();
    fn isr3();
    fn isr4();
    fn isr5();
    fn isr6();
    fn isr7();
    fn isr8();
    fn isr10();
    fn isr11();
    fn isr12();
    fn isr13();
    fn isr14();
    fn isr17();
    fn isr18();
    fn isr19();
    fn isr21();

    fn irq32();
    fn irq33();
    fn irq34();
    fn irq35();
    fn irq36();
    fn irq37();
    fn irq38();
    fn irq39();
    fn irq40();
    fn irq41();
    fn irq42();
    fn irq43();
    fn irq44();
    fn irq45();
    fn irq46();
    fn irq47();

    fn spurious_irq15();
    fn spurious_irq255();

    fn int80();
}

/// Software syscall vector, reached from user code with `int 0x80`.
pub const SYSCALL_VECTOR: usize = 0x80;

#[inline]
fn addr(handler: unsafe extern "C" fn()) -> u64 {
    handler as usize as u64
}

unsafe fn set_gate(entry: *mut IdtEntry, offset: u64, selector: u16, type_attr: u8) {
    (*entry).offset_low = (offset & 0xFFFF) as u16;
    (*entry).selector = selector;
    (*entry).ist = 0;
    (*entry).type_attr = type_attr;
    (*entry).offset_mid = ((offset >> 16) & 0xFFFF) as u16;
    (*entry).offset_high = ((offset >> 32) & 0xFFFF_FFFF) as u32;
    (*entry).reserved = 0;
}

/// Load the IDT that `init` built onto the current CPU. Application processors
/// share the BSP's table, so they only need to point their `IDTR` at it.
pub fn load() {
    unsafe {
        asm!(
            "lidt [{}]",
            in(reg) core::ptr::addr_of!(IDTR),
            options(readonly, nostack, preserves_flags)
        );
    }
}

pub fn init() {
    unsafe {
        let idt = core::ptr::addr_of_mut!(IDT) as *mut IdtEntry;

        core::ptr::write_bytes(idt, 0, 256);

        macro_rules! gate {
            ($vector:expr, $handler:ident) => {
                set_gate(idt.add($vector), addr($handler), 0x08, 0x8E)
            };
        }

        gate!(0, isr0);
        gate!(1, isr1);
        gate!(2, isr2);
        gate!(3, isr3);
        gate!(4, isr4);
        gate!(5, isr5);
        gate!(6, isr6);
        gate!(7, isr7);
        gate!(8, isr8);
        gate!(10, isr10);
        gate!(11, isr11);
        gate!(12, isr12);
        gate!(13, isr13);
        gate!(14, isr14);
        gate!(17, isr17);
        gate!(18, isr18);
        gate!(19, isr19);
        gate!(21, isr21);

        gate!(32, irq32);
        gate!(33, irq33);
        gate!(34, irq34);
        gate!(35, irq35);
        gate!(36, irq36);
        gate!(37, irq37);
        gate!(38, irq38);
        gate!(39, irq39);
        gate!(40, irq40);
        gate!(41, irq41);
        gate!(42, irq42);
        gate!(43, irq43);
        gate!(44, irq44);
        gate!(45, irq45);
        gate!(46, irq46);
        gate!(47, irq47);

        gate!(PIC_SPURIOUS_VECTOR, spurious_irq15);
        gate!(APIC_SPURIOUS_VECTOR, spurious_irq255);

        // The syscall gate: same kernel selector as everything else, but with
        // DPL 3 (type_attr 0xEE) so ring 3 is allowed to invoke it. The `gate!`
        // macro uses 0x8E, which is DPL 0 and would fault on `int 0x80`.
        set_gate(idt.add(SYSCALL_VECTOR), addr(int80), 0x08, 0xEE);

        core::ptr::write(
            core::ptr::addr_of_mut!(IDTR),
            IdtPtr {
                limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
                base: idt as u64,
            },
        );

        asm!(
            "lidt [{}]",
            in(reg) core::ptr::addr_of!(IDTR),
            options(readonly, nostack, preserves_flags)
        );
    }
}
