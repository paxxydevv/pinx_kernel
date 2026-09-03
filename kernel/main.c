#include "pinx/gdt.h"
#include "pinx/idt.h"
#include "pinx/graphics/graphics.h"
#include "pinx/terminal.h"
#include "3party/limine.h"
#include "pinx/memory_management.h"
#include <stdint.h>
__attribute__((used, section(".limine_requests_start")))
static volatile uint64_t limine_requests_start_marker[] =
    LIMINE_REQUESTS_START_MARKER;

__attribute__((used, section(".limine_requests")))
static volatile uint64_t limine_base_revision[] =
    LIMINE_BASE_REVISION(6);

__attribute__((used, section(".limine_requests")))
static volatile struct limine_framebuffer_request framebuffer_request = {
    .id = LIMINE_FRAMEBUFFER_REQUEST_ID,
    .revision = 0,
    .response = 0
};
__attribute__((used, section(".limine_requests")))
volatile struct limine_memmap_request memmap_request = {
    .id = LIMINE_MEMMAP_REQUEST_ID,
    .revision = 0,
    .response = 0
};
__attribute__((used, section(".limine_requests")))
volatile struct limine_hhdm_request hhdm_request = {
    .id = LIMINE_HHDM_REQUEST_ID,
    .revision = 0,
    .response = 0
};
__attribute__((used, section(".limine_requests_end")))
static volatile uint64_t limine_requests_end_marker[] =
    LIMINE_REQUESTS_END_MARKER;

void kmain(void)
{
    if (framebuffer_request.response == 0 ||
        framebuffer_request.response->framebuffer_count < 1) {
        for (;;) {
            __asm__ volatile ("hlt");
        }
    }
    struct limine_framebuffer *framebuffer = framebuffer_request.response->framebuffers[0];
    graphics_init(framebuffer);
    gdt_init(); // Without gdt our idt might not work even if limine configured gdt..
    kprintf("GDT: OK\n");
    idt_init();
    kprintf("IDT: OK\n");
    pmm_init();
    kprintf("PMM INIT: OK\n");
    vmm_init();
    kprintf("VMM INIT: OK\n");
    for (;;) {
        __asm__ volatile ("hlt");
    }
}
