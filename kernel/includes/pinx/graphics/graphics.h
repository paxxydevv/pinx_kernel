#pragma once

#include <stdint.h>
#include "../../3party/limine.h"

void draw_pixel(uint64_t x, uint64_t y, uint32_t color);
void draw_char(char c, uint64_t x, uint64_t y, uint32_t color);
void graphics_init(struct limine_framebuffer *framebuffer);
void draw_string(const char* string, uint64_t x, uint64_t y, uint32_t color);
void clear_screen(uint32_t color);
