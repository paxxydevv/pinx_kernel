#include "includes/pinx/gdt.h"
#include "includes/pinx/idt.h"
#include "includes/pinx/graphics.h"
#include "includes/3party/limine.h"
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

__attribute__((used, section(".limine_requests_end")))
static volatile uint64_t limine_requests_end_marker[] =
    LIMINE_REQUESTS_END_MARKER;

void kmain(void)
{
    gdt_init();
    idt_init();
    if (framebuffer_request.response == 0 ||
        framebuffer_request.response->framebuffer_count < 1) {
        for (;;) {
            __asm__ volatile ("hlt");
        }
    }
    struct limine_framebuffer *framebuffer = framebuffer_request.response->framebuffers[0];
    graphics_init(framebuffer);
    draw_pixel(100, 100, 0xFF0000);
    draw_char('a', 100, 150, 0xFF0000);
    draw_string("Hello World", 100, 250, 0xFF0000);
    
    for (;;) {
        __asm__ volatile ("hlt");
    }
}