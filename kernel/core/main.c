#include "pinx/gdt.h"
#include "pinx/idt.h"
#include "pinx/graphics/graphics.h"
#include "pinx/terminal.h"
#include "pinx/apic.h"
#include "pinx/delay.h"
#include "pinx/io.h"
#include "pinx/keyboard.h"
#include "3party/limine.h"
#include "pinx/mm/memory_management.h"
#include "pinx/kdebug.h"
#include "pinx/shell.h"
#include "pinx/pci_enumrate.h"
#include "pinx/scheduler.h"
#include <stdint.h>
__attribute__((used, section(".limine_requests_start")))
static volatile uint64_t limine_requests_start_marker[] =
    LIMINE_REQUESTS_START_MARKER;

__attribute__((used, section(".limine_requests")))
static volatile uint64_t limine_base_revision[] =
    LIMINE_BASE_REVISION(6);

__attribute__((used, section(".limine_requests")))
    volatile struct limine_framebuffer_request framebuffer_request = {
    .id = LIMINE_FRAMEBUFFER_REQUEST_ID,
    .revision = 0,
    .response = nullptr
};
__attribute__((used, section(".limine_requests")))
volatile struct limine_memmap_request memmap_request = {
    .id = LIMINE_MEMMAP_REQUEST_ID,
    .revision = 0,
    .response = nullptr
};
__attribute__((used, section(".limine_requests")))
volatile struct limine_hhdm_request hhdm_request = {
    .id = LIMINE_HHDM_REQUEST_ID,
    .revision = 0,
    .response = nullptr
};

__attribute__((used, section(".limine_requests")))
static volatile struct limine_tsc_frequency_request tsc_freq_request = {
    .id = LIMINE_TSC_FREQUENCY_REQUEST_ID,
    .revision = 0,
    .response = nullptr
};

__attribute__((used, section(".limine_requests_end")))
static volatile uint64_t limine_requests_end_marker[] =
    LIMINE_REQUESTS_END_MARKER;

void kmain(void)
{
    if (framebuffer_request.response == nullptr ||
        framebuffer_request.response->framebuffer_count < 1) {
        halt();
    }
    struct limine_framebuffer *framebuffer = framebuffer_request.response->framebuffers[0];
    graphics_init(framebuffer);
    klog_init();
    KINFO("GRAPHICS_SUB: OK\n");
    gdt_init();
    KINFO("GDT: OK\n");
    idt_init();
    KINFO("IDT: OK\n");
    pmm_init();
    KINFO("PMM INIT: OK\n");
    vmm_init();
    KINFO("VMM INIT: OK\n");
    kheap_init();
    KINFO("KERNEL HEAP INIT: OK\n");
    uint32_t tsc_freq = 0;
    if (tsc_freq_request.response != nullptr) {
        KINFO("Using tsc_freq from limine.\n");
        tsc_freq = (uint32_t)tsc_freq_request.response->frequency;
    } else {
        KINFO("Using tsc_freq from kernel. Not accurate\n");
        tsc_freq = 2000000000;
    }
    apic_init();
    KINFO("x2APIC: OK\n");
    keyboard_init();
    KINFO("KEYBOARD: OK\n");
    delay_init(tsc_freq);
    KINFO("APIC TIMER: OK\n");
    irq_enable();
    KINFO("IRQ: OK\n");
    pci_enumerate();

    // init_shell();
    // scheduler_init();
    // scheduler_yield();

    for (;;) {
        __asm__ volatile ("hlt");
    }
}