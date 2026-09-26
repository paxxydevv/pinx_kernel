#pragma once

#include <stdint.h>
#include "../../3party/limine.h"

void draw_pixel(uint64_t x, uint64_t y, uint32_t color);
void draw_char(char c, uint64_t x, uint64_t y, uint32_t color);
void graphics_init(struct limine_framebuffer *framebuffer);
void draw_string(const char* string, uint64_t x, uint64_t y, uint32_t color);
void clear_screen(uint32_t color);
void scroll_framebuffer(uint64_t rows, uint32_t fill_color);
uint64_t get_framebuffer_width(void);
uint64_t get_framebuffer_height(void);
