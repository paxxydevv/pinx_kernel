#pragma once
#include "stdint.h"
void draw_pixel(uint64_t x, uint64_t y, uint32_t color);
void graphics_init(struct limine_framebuffer *framebuffer);