#include "pinx/graphics/graphics.h"
#include "pinx/graphics/bitmap.h"

static struct limine_framebuffer *fb;

void graphics_init(struct limine_framebuffer *framebuffer) {
    fb = framebuffer;
}

void draw_pixel(uint64_t x, uint64_t y, uint32_t color) {
    if (fb == 0) {
        return;
    }
    if (x > fb->width || y > fb->height)
        return;
    uint32_t *pixels = fb->address;
    uint64_t pitch = fb->pitch / sizeof(uint32_t);
    pixels[(y * pitch) + x] = color;
}

void draw_char(char c, uint64_t x, uint64_t y, uint32_t color)
{
    const uint8_t *bitmap = font8x8_basic[(uint8_t)c];

    for (uint64_t row = 0; row < 8; row++) {
        for (uint64_t col = 0; col < 8; col++) {
            if (bitmap[row] & (1u << (7u - col))) {
                draw_pixel(x + col, y + row, color);
            }
        }
    }
}

void draw_string(const char* string, uint64_t x, uint64_t y, uint32_t color) {
    uint64_t cursor_x = x;
    uint64_t cursor_y = y;
    while (*string != '\0') {
        if (cursor_x + 8 > fb->width) {
            cursor_x = x;
            cursor_y = cursor_y + 8;
        }
        draw_char(*string, cursor_x, cursor_y, color);
        string++;
        cursor_x = cursor_x + 8;
    }
}

void clear_screen(uint32_t color) {
    for (int i = 0; i < fb->width; i++) {
        for (int b = 0; b < fb->height; b++) {
            draw_pixel(i, b, color);
        }
    }
}
