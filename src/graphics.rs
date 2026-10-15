//! Raw framebuffer drawing.
//!
//! The framebuffer comes from Limine and is a linear array of 32-bit pixels
//! already mapped through the HHDM, so no paging work is needed here. The
//! drawing routines are deliberately primitive: a pixel, a glyph, a string,
//! a fill and a scroll. Layout lives in `terminal`.

use crate::font::FONT8X8_BASIC;
use crate::limine::LimineFramebuffer;

static mut FB: *mut LimineFramebuffer = core::ptr::null_mut();

#[inline]
fn fb() -> *mut LimineFramebuffer {
    unsafe { core::ptr::read(core::ptr::addr_of!(FB)) }
}

pub fn init(framebuffer: *mut LimineFramebuffer) {
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(FB), framebuffer) }
}

pub fn draw_pixel(x: u64, y: u64, color: u32) {
    let fb = fb();
    if fb.is_null() {
        return;
    }
    unsafe {
        // `>=`, not `>`: x == width is one past the last column and would
        // overwrite the first pixel of the next scanline.
        if x >= (*fb).width || y >= (*fb).height {
            return;
        }
        let pixels = (*fb).address as *mut u32;
        let pitch = (*fb).pitch / 4;
        *pixels.add((y * pitch + x) as usize) = color;
    }
}

pub fn draw_char(c: u8, x: u64, y: u64, color: u32) {
    let bitmap = &FONT8X8_BASIC[(c & 0x7F) as usize];
    for row in 0..8u64 {
        let bits = bitmap[row as usize];
        if bits == 0 {
            continue;
        }
        for col in 0..8u64 {
            if bits & (1u8 << col) != 0 {
                draw_pixel(x + col, y + row, color);
            }
        }
    }
}

pub fn draw_string(s: &str, x: u64, y: u64, color: u32) {
    let mut cursor_x = x;
    let mut cursor_y = y;
    let width = framebuffer_width();
    for byte in s.bytes() {
        if cursor_x + 8 > width {
            cursor_x = x;
            cursor_y += 8;
        }
        draw_char(byte, cursor_x, cursor_y, color);
        cursor_x += 8;
    }
}

pub fn clear_screen(color: u32) {
    let fb = fb();
    if fb.is_null() {
        return;
    }
    let (width, height) = unsafe { ((*fb).width, (*fb).height) };
    for y in 0..height {
        for x in 0..width {
            draw_pixel(x, y, color);
        }
    }
}

pub fn scroll_framebuffer(rows: u64, fill_color: u32) {
    let fb = fb();
    if fb.is_null() {
        return;
    }
    unsafe {
        let height = (*fb).height;
        let rows = rows.min(height);
        let pitch_pixels = (*fb).pitch / 4;
        let pixels = (*fb).address as *mut u32;
        let copy_pixels = (height - rows) * pitch_pixels;
        for i in 0..copy_pixels {
            *pixels.add(i as usize) = *pixels.add((i + rows * pitch_pixels) as usize);
        }
        for y in (height - rows)..height {
            for x in 0..pitch_pixels {
                *pixels.add((y * pitch_pixels + x) as usize) = fill_color;
            }
        }
    }
}

pub fn framebuffer_width() -> u64 {
    let fb = fb();
    if fb.is_null() {
        0
    } else {
        unsafe { (*fb).width }
    }
}

pub fn framebuffer_height() -> u64 {
    let fb = fb();
    if fb.is_null() {
        0
    } else {
        unsafe { (*fb).height }
    }
}

/// The framebuffer as a kernel (HHDM) virtual address. Subtract
/// `mm::hhdm_offset()` to get the physical base.
pub fn framebuffer_address() -> u64 {
    let fb = fb();
    if fb.is_null() {
        0
    } else {
        unsafe { (*fb).address as u64 }
    }
}

pub fn framebuffer_pitch() -> u64 {
    let fb = fb();
    if fb.is_null() {
        0
    } else {
        unsafe { (*fb).pitch }
    }
}

pub fn framebuffer_bpp() -> u64 {
    let fb = fb();
    if fb.is_null() {
        0
    } else {
        unsafe { (*fb).bpp as u64 }
    }
}
