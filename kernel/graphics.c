#include "includes/pinx/graphics.h"
static struct limine_framebuffer *fb;
void graphics_init(struct limine_framebuffer *framebuffer) {
    fb = framebuffer;
}
void draw_pixel(uint64_t x, uint64_t y, uint32_t color);
