//! A very small interactive shell.
//!
//! It echoes what is typed, handles newline and backspace, and hands the line
//! to `execute_command` once there is a filesystem to execute against. The
//! line buffer lives on the task stack because that is what the stack is for.

use crate::{graphics, io, kprint, keyboard, terminal, kpanic};
#[cfg(feature = "ahci")]
use crate::{ext4::DirEnt, fs};
pub fn init_shell() {
    terminal::clear(0x0000);
    kprint!("Pinx Shell:/ ");
    terminal::draw_cursor();
}
extern "C" fn str_eq(s1: *const u8, s2: *const u8) -> i8 {
    let mut i = 0;

    unsafe {
        loop {
            let a = *s1.add(i);
            let b = *s2.add(i);

            if a != b {
                return 1;
            }

            if a == 0 {
                return 0;
            }

            i += 1;
        }
    }
}
#[cfg(feature = "ahci")]
fn list_root() {

    let mut entries = [DirEnt::empty(); 64];

    let Some(root) = fs::root() else {
        kprint!("\nno filesystem mounted");
        return;
    };


    let Some(result) = fs::read_dir(root, &mut entries) else {
        kprint!("\nread_dir returned None");
        return;
    };



    match result {
        Ok(n) => {
            kprint!("\nls: {} entries", n);

            for i in 0..core::cmp::min(n, entries.len()) {
                kprint!("\nls: entry {}", i);

                if let Some(name) = entries[i].name_str() {
                    kprint!(" {}", name);
                }
            }
        }
        Err(e) => {
            kprint!("\nls: filesystem error: {:?}", e);
        }
    }
}
extern "C" fn execute_command(command: *const u8) {
    if str_eq(command, b"hello\0".as_ptr()) == 0 {
        kprint!("\nHello command!");
    }
    else if str_eq(command, b"ls\0".as_ptr()) == 0 {
        list_root();
    }
    else if str_eq(command, b"panic\0".as_ptr()) == 0 {
        kpanic!("\nPanic Send From Shell");
    }
    else if str_eq(command, b"help\0".as_ptr()) == 0 {
        kprint!("\nComamnds:\nhello: Says hello back to user\nls: List's Directories and files\npanic: Triggers a kernel panic")
    }
    else {
        kprint!("\nUnknown command");
    }
}
pub extern "C" fn run_shell() {
    let mut buffer = [0u8; 4096];
    let mut pos: usize = 0;

    loop {
        // Halting is safe: the LAPIC timer keeps firing, so the CPU wakes on
        // the next tick even with nothing to read.
        while !keyboard::has_input() {
            io::hlt();
        }

        let c = keyboard::input();

        terminal::erase_cursor();

        if c == b'\n' {
            buffer[pos] = 0;


            execute_command(buffer.as_ptr()); // needs a filesystem first.

            pos = 0;
            kprint!("\nPinx Shell:/ ");
            terminal::draw_cursor();
            continue;
        }
        if c == 8 {
            if pos > 0 {
                pos -= 1;

                let x = terminal::get_cursor_x().saturating_sub(8);
                let y = terminal::get_cursor_y();

                // Clear the previous character's cell
                for dy in 0..8 {
                    for dx in 0..8 {
                        graphics::draw_pixel(x + dx, y + dy, 0x000000);
                    }
                }

                terminal::set_cursor(x, y);
                terminal::draw_cursor();
            } else {
                terminal::draw_cursor();
            }

            continue;
        }

        if pos < buffer.len() - 1 {
            buffer[pos] = c;
            pos += 1;
            terminal::putc(c);
            terminal::draw_cursor();
        } else {
            terminal::draw_cursor();
        }
    }
}
