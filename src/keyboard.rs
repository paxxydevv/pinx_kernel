//! PS/2 keyboard driver.
//!
//! Scancode set 1, with shift, ctrl, alt and caps-lock tracking, the `0xE0`
//! extended prefix, and a small ring buffer the shell drains. The translation
//! tables are built at compile time by index, so a blank scancode is simply a
//! zero (`KEY_NONE`) rather than a separate initializer list to keep aligned.

use crate::io::{inb, io_wait, outb};

pub const DATA_PORT: u16 = 0x60;
pub const CMD_PORT: u16 = 0x64;
pub const IRQ_VECTOR: u8 = 33;
pub const BUFFER_SIZE: usize = 256;

pub const KEY_NONE: u8 = 0x00;
pub const KEY_BACKSPACE: u8 = 0x08;
pub const KEY_TAB: u8 = 0x09;
pub const KEY_ENTER: u8 = 0x0A;
pub const KEY_ESC: u8 = 0x1B;
pub const KEY_SPACE: u8 = 0x20;

pub const KEY_EXT_UP: u8 = 0x80;
pub const KEY_EXT_DOWN: u8 = 0x81;
pub const KEY_EXT_LEFT: u8 = 0x82;
pub const KEY_EXT_RIGHT: u8 = 0x83;
pub const KEY_EXT_HOME: u8 = 0x84;
pub const KEY_EXT_END: u8 = 0x85;
pub const KEY_EXT_PGUP: u8 = 0x86;
pub const KEY_EXT_PGDN: u8 = 0x87;
pub const KEY_EXT_INSERT: u8 = 0x88;
pub const KEY_EXT_DELETE: u8 = 0x89;
pub const KEY_EXT_F1: u8 = 0x8A;
pub const KEY_EXT_F2: u8 = 0x8B;
pub const KEY_EXT_F3: u8 = 0x8C;
pub const KEY_EXT_F4: u8 = 0x8D;
pub const KEY_EXT_F5: u8 = 0x8E;
pub const KEY_EXT_F6: u8 = 0x8F;
pub const KEY_EXT_F7: u8 = 0x90;
pub const KEY_EXT_F8: u8 = 0x91;
pub const KEY_EXT_F9: u8 = 0x92;
pub const KEY_EXT_F10: u8 = 0x93;
pub const KEY_EXT_F11: u8 = 0x94;
pub const KEY_EXT_F12: u8 = 0x95;

/// Build a 128-entry table from explicit `(scancode, value)` pairs. Every
/// scancode not named stays zero, which is `KEY_NONE`.
const fn table(entries: &[(usize, u8)]) -> [u8; 128] {
    let mut out = [KEY_NONE; 128];
    let mut i = 0;
    while i < entries.len() {
        out[entries[i].0] = entries[i].1;
        i += 1;
    }
    out
}

const SCANCODE_TO_ASCII: [u8; 128] = table(&[
    (0x01, KEY_ESC),
    (0x02, b'1'), (0x03, b'2'), (0x04, b'3'), (0x05, b'4'), (0x06, b'5'),
    (0x07, b'6'), (0x08, b'7'), (0x09, b'8'), (0x0A, b'9'), (0x0B, b'0'),
    (0x0C, b'-'), (0x0D, b'='), (0x0E, KEY_BACKSPACE), (0x0F, KEY_TAB),
    (0x10, b'q'), (0x11, b'w'), (0x12, b'e'), (0x13, b'r'), (0x14, b't'),
    (0x15, b'y'), (0x16, b'u'), (0x17, b'i'), (0x18, b'o'), (0x19, b'p'),
    (0x1A, b'['), (0x1B, b']'), (0x1C, KEY_ENTER),
    (0x1E, b'a'), (0x1F, b's'), (0x20, b'd'), (0x21, b'f'), (0x22, b'g'),
    (0x23, b'h'), (0x24, b'j'), (0x25, b'k'), (0x26, b'l'),
    (0x27, b';'), (0x28, b'\''), (0x29, b'`'),
    (0x2B, b'\\'),
    (0x2C, b'z'), (0x2D, b'x'), (0x2E, b'c'), (0x2F, b'v'), (0x30, b'b'),
    (0x31, b'n'), (0x32, b'm'),
    (0x33, b','), (0x34, b'.'), (0x35, b'/'),
    (0x37, b'*'), (0x39, KEY_SPACE),
    (0x47, b'7'), (0x48, b'8'), (0x49, b'9'), (0x4A, b'-'),
    (0x4B, b'4'), (0x4C, b'5'), (0x4D, b'6'), (0x4E, b'+'),
    (0x4F, b'1'), (0x50, b'2'), (0x51, b'3'), (0x52, b'0'), (0x53, b'.'),
]);

const SCANCODE_TO_ASCII_SHIFT: [u8; 128] = table(&[
    (0x01, KEY_ESC),
    (0x02, b'!'), (0x03, b'@'), (0x04, b'#'), (0x05, b'$'), (0x06, b'%'),
    (0x07, b'^'), (0x08, b'&'), (0x09, b'*'), (0x0A, b'('), (0x0B, b')'),
    (0x0C, b'_'), (0x0D, b'+'), (0x0E, KEY_BACKSPACE), (0x0F, KEY_TAB),
    (0x10, b'Q'), (0x11, b'W'), (0x12, b'E'), (0x13, b'R'), (0x14, b'T'),
    (0x15, b'Y'), (0x16, b'U'), (0x17, b'I'), (0x18, b'O'), (0x19, b'P'),
    (0x1A, b'{'), (0x1B, b'}'), (0x1C, KEY_ENTER),
    (0x1E, b'A'), (0x1F, b'S'), (0x20, b'D'), (0x21, b'F'), (0x22, b'G'),
    (0x23, b'H'), (0x24, b'J'), (0x25, b'K'), (0x26, b'L'),
    (0x27, b':'), (0x28, b'"'), (0x29, b'~'),
    (0x2B, b'|'),
    (0x2C, b'Z'), (0x2D, b'X'), (0x2E, b'C'), (0x2F, b'V'), (0x30, b'B'),
    (0x31, b'N'), (0x32, b'M'),
    (0x33, b'<'), (0x34, b'>'), (0x35, b'?'),
    (0x37, b'*'), (0x39, KEY_SPACE),
    (0x47, b'7'), (0x48, b'8'), (0x49, b'9'), (0x4A, b'-'),
    (0x4B, b'4'), (0x4C, b'5'), (0x4D, b'6'), (0x4E, b'+'),
    (0x4F, b'1'), (0x50, b'2'), (0x51, b'3'), (0x52, b'0'), (0x53, b'.'),
]);

const EXTENDED_TO_KEY: [u8; 128] = table(&[
    (0x48, KEY_EXT_UP), (0x50, KEY_EXT_DOWN), (0x4B, KEY_EXT_LEFT), (0x4D, KEY_EXT_RIGHT),
    (0x47, KEY_EXT_HOME), (0x4F, KEY_EXT_END), (0x49, KEY_EXT_PGUP), (0x51, KEY_EXT_PGDN),
    (0x52, KEY_EXT_INSERT), (0x53, KEY_EXT_DELETE),
    (0x3B, KEY_EXT_F1), (0x3C, KEY_EXT_F2), (0x3D, KEY_EXT_F3), (0x3E, KEY_EXT_F4),
    (0x3F, KEY_EXT_F5), (0x40, KEY_EXT_F6), (0x41, KEY_EXT_F7), (0x42, KEY_EXT_F8),
    (0x43, KEY_EXT_F9), (0x44, KEY_EXT_F10), (0x57, KEY_EXT_F11), (0x5D, KEY_EXT_F12),
    (0x1D, KEY_NONE), (0x38, KEY_NONE), (0x5B, KEY_NONE),
]);

static mut KB_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut KB_HEAD: u32 = 0;
static mut KB_TAIL: u32 = 0;

static mut SHIFT_HELD: u8 = 0;
static mut CTRL_HELD: u8 = 0;
static mut ALT_HELD: u8 = 0;
static mut CAPS_LOCK: u8 = 0;
static mut EXTENDED_PREFIX: u8 = 0;

fn buffer_push(key: u8) {
    unsafe {
        let next = (KB_HEAD + 1) % BUFFER_SIZE as u32;
        if next != KB_TAIL {
            KB_BUFFER[KB_HEAD as usize] = key;
            KB_HEAD = next;
        }
    }
}

fn apply_caps_lock(c: u8, is_alpha: bool) -> u8 {
    unsafe {
        if CAPS_LOCK == 0 || !is_alpha {
            return c;
        }
    }
    if c.is_ascii_lowercase() {
        c - 32
    } else if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}

fn kbd_cmd(cmd: u8) {
    for _ in 0..1000 {
        if unsafe { inb(CMD_PORT) } & 0x02 == 0 {
            unsafe { outb(CMD_PORT, cmd) };
            return;
        }
        io_wait();
    }
}

fn kbd_write(val: u8) {
    for _ in 0..1000 {
        if unsafe { inb(CMD_PORT) } & 0x02 == 0 {
            unsafe { outb(DATA_PORT, val) };
            return;
        }
        io_wait();
    }
}

fn kbd_read() -> u8 {
    for _ in 0..1000 {
        if unsafe { inb(CMD_PORT) } & 0x01 != 0 {
            return unsafe { inb(DATA_PORT) };
        }
        io_wait();
    }
    0
}

fn kbd_flush() {
    while unsafe { inb(CMD_PORT) } & 0x01 != 0 {
        unsafe { inb(DATA_PORT) };
        io_wait();
    }
}

pub fn init() {
    kbd_cmd(0xAD);
    kbd_flush();

    kbd_cmd(0x20);
    let mut config = kbd_read();
    config |= 0x01;
    config |= 0x40;
    config &= !0x10;
    kbd_cmd(0x60);
    kbd_write(config);

    kbd_cmd(0xAE);

    kbd_write(0xF4);
    kbd_read();

    unsafe {
        SHIFT_HELD = 0;
        CTRL_HELD = 0;
        ALT_HELD = 0;
        CAPS_LOCK = 0;
        EXTENDED_PREFIX = 0;
        KB_HEAD = 0;
        KB_TAIL = 0;
    }
}

pub fn irq() {
    let scancode = unsafe { inb(DATA_PORT) };

    unsafe {
        if scancode == 0xE0 {
            EXTENDED_PREFIX = 1;
            return;
        }

        if EXTENDED_PREFIX != 0 {
            EXTENDED_PREFIX = 0;

            if scancode == 0x2A || scancode == 0x36 {
                SHIFT_HELD = 0;
                return;
            }
            if scancode & 0x80 != 0 {
                return;
            }

            let key = EXTENDED_TO_KEY[scancode as usize];
            if key != KEY_NONE {
                buffer_push(key);
            }
            return;
        }

        if scancode == 0x2A || scancode == 0x36 {
            SHIFT_HELD = 1;
            return;
        }
        if scancode == 0x1D {
            CTRL_HELD = 1;
            return;
        }
        if scancode == 0x38 {
            ALT_HELD = 1;
            return;
        }
        if scancode == 0x3A {
            CAPS_LOCK ^= 1;
            return;
        }

        if scancode & 0x80 != 0 {
            let released = scancode & 0x7F;
            if released == 0x2A || released == 0x36 {
                SHIFT_HELD = 0;
            } else if released == 0x1D {
                CTRL_HELD = 0;
            } else if released == 0x38 {
                ALT_HELD = 0;
            }
            return;
        }

        if scancode >= 128 {
            return;
        }

        let is_shift = SHIFT_HELD != 0;
        let is_alpha = (0x10..=0x19).contains(&scancode)
            || (0x1E..=0x26).contains(&scancode)
            || (0x2C..=0x32).contains(&scancode);

        let mut c = if is_shift {
            SCANCODE_TO_ASCII_SHIFT[scancode as usize]
        } else {
            SCANCODE_TO_ASCII[scancode as usize]
        };

        if c == KEY_NONE {
            return;
        }

        if is_alpha {
            c = apply_caps_lock(c, true);
        } else if CAPS_LOCK != 0 && is_shift {
            c = apply_caps_lock(c, false);
        }

        if CTRL_HELD != 0 {
            if c.is_ascii_lowercase() {
                c = c - b'a' + 1;
            } else if c.is_ascii_uppercase() {
                c = c - b'A' + 1;
            }
        }

        buffer_push(c);
    }
}

pub fn input() -> u8 {
    unsafe {
        if KB_HEAD == KB_TAIL {
            return KEY_NONE;
        }
        let key = KB_BUFFER[KB_TAIL as usize];
        KB_TAIL = (KB_TAIL + 1) % BUFFER_SIZE as u32;
        key
    }
}

pub fn has_input() -> bool {
    unsafe { KB_HEAD != KB_TAIL }
}
