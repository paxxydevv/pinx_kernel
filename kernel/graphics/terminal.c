#include "../includes/pinx/terminal.h"
#include "../includes/pinx/graphics/graphics.h"
static uint64_t cursor_x = 0;
static uint64_t cursor_y = 0;

void kclear(uint32_t color) {
    cursor_x = 0;
    cursor_y = 0;
    clear_screen(color);
}

void kputc(const char c) {
    if (c == '\n') {
        cursor_x = 0;
        cursor_y = cursor_y + 30;
        return;
    }
    draw_char(c, cursor_x, cursor_y, 0xFFFFFF);
    cursor_x = cursor_x + 8;
}
void kputc_color(const char c, uint32_t color) {
    if (c == '\n') {
        cursor_x = 0;
        cursor_y = cursor_y + 30;
        return;
    }
    draw_char(c, cursor_x, cursor_y, color);
    cursor_x = cursor_x + 8;
}
void kprintf(const char *text) {
    while (*text != '\0') {
        kputc(*text);
        text++;
    }
}
void kprintf_color(const char *text, uint32_t color) {
    while (*text != '\0') {
        kputc_color(*text, color);
        text++;
    }
}
void draw_cursor(void) {
    for (uint64_t i = 0; i < 8; i++) {
        draw_pixel(cursor_x + i, cursor_y + 8, 0xFFFFFF); // Could uses a underline. or whatever
    }
}

void erase_cursor(void) {
    for (uint64_t i = 0; i < 8; i++) {
        draw_pixel(cursor_x + i, cursor_y + 8, 0x000000); // Could use a underline. or whatever
    }
}

uint64_t get_cursor_x(void) {
    return cursor_x;
}

uint64_t get_cursor_y(void) {
    return cursor_y;
}

void set_cursor(uint64_t x, uint64_t y) {
    cursor_x = x;
    cursor_y = y;
}
void klog(log_level_t level, const char *message) {
    char buf[256];
    int pos = 0;
    const char *prefix;
    if (level == LOG_ERROR) {
        
    }
}