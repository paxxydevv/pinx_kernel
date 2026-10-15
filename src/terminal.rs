//! Text terminal layered on top of the framebuffer.
//!
//! Formatting is done with `core::fmt`, so there is no hand-rolled printf
//! grammar to keep in sync: `kprint!("{} {:#x}", a, b)` works exactly as it
//! does everywhere else in Rust. The only thing this module owns is where the
//! next glyph goes and what happens at the right and bottom edges.

use core::fmt;

use crate::graphics;

const LINE_HEIGHT: u64 = 30;
const FONT_WIDTH: u64 = 8;
const SCROLL_COLOR: u32 = 0x000000;
const TEXT_COLOR: u32 = 0xFFFFFF;

static mut CURSOR_X: u64 = 0;
static mut CURSOR_Y: u64 = 0;

#[inline]
fn cursor() -> (u64, u64) {
    unsafe {
        (
            core::ptr::read(core::ptr::addr_of!(CURSOR_X)),
            core::ptr::read(core::ptr::addr_of!(CURSOR_Y)),
        )
    }
}

#[inline]
fn set_cursor_raw(x: u64, y: u64) {
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(CURSOR_X), x);
        core::ptr::write(core::ptr::addr_of_mut!(CURSOR_Y), y);
    }
}

fn newline() {
    let (_, y) = cursor();
    let y = y + LINE_HEIGHT;
    if y >= graphics::framebuffer_height() {
        graphics::scroll_framebuffer(LINE_HEIGHT, SCROLL_COLOR);
        set_cursor_raw(0, graphics::framebuffer_height() - LINE_HEIGHT);
    } else {
        set_cursor_raw(0, y);
    }
}

fn advance() {
    let (x, y) = cursor();
    let x = x + FONT_WIDTH;
    if x > graphics::framebuffer_width().saturating_sub(FONT_WIDTH) {
        newline();
    } else {
        set_cursor_raw(x, y);
    }
}

pub fn clear(color: u32) {
    set_cursor_raw(0, 0);
    graphics::clear_screen(color);
}

pub fn putc(c: u8) {
    if c == b'\n' {
        newline();
        return;
    }
    let (x, y) = cursor();
    graphics::draw_char(c, x, y, TEXT_COLOR);
    advance();
}

pub fn putc_color(c: u8, color: u32) {
    if c == b'\n' {
        newline();
        return;
    }
    let (x, y) = cursor();
    graphics::draw_char(c, x, y, color);
    advance();
}

/// Plain, unformatted output. The cheapest way to emit a literal.
pub fn puts(s: &str) {
    for byte in s.bytes() {
        putc(byte);
    }
}

/// Formatted output through `core::fmt`.
pub fn print(args: fmt::Arguments) {
    use fmt::Write;

    struct Writer;

    impl fmt::Write for Writer {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            puts(s);
            Ok(())
        }
    }

    let _ = Writer.write_fmt(args);
}

pub fn draw_cursor() {
    let (x, y) = cursor();
    for i in 0..8 {
        graphics::draw_pixel(x + i, y + 8, TEXT_COLOR);
    }
}

pub fn erase_cursor() {
    let (x, y) = cursor();
    for i in 0..8 {
        graphics::draw_pixel(x + i, y + 8, SCROLL_COLOR);
    }
}

pub fn get_cursor_x() -> u64 {
    cursor().0
}

pub fn get_cursor_y() -> u64 {
    cursor().1
}

pub fn set_cursor(x: u64, y: u64) {
    set_cursor_raw(x, y);
}

/// Print `0x` followed by eight uppercase hex digits, matching the formatting
/// the original kernel used for its exception dumps.
pub fn printf_hex(value: u32) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut buf = [0u8; 10];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..8 {
        buf[2 + i] = HEX[((value >> (28 - i * 4)) & 0xF) as usize];
    }
    puts(core::str::from_utf8(&buf).unwrap_or("?"));
}

/// Stop the CPU forever without touching the terminal.
pub fn halt() -> ! {
    loop {
        crate::io::hlt();
    }
}

/// Print `args` and stop the CPU forever. Used for unrecoverable states where
/// continuing would silently corrupt memory (OOM, bogus page tables, ...).
pub fn panic(args: fmt::Arguments) -> ! {
    print(format_args!("PANIC: "));
    print(args);
    print(format_args!("\nSystem halted.\n"));
    halt();
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        $crate::terminal::print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! kpanic {
    ($($arg:tt)*) => {
        $crate::terminal::panic(format_args!($($arg)*))
    };
}
