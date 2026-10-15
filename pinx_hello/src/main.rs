#![no_std]
#![no_main]

use core::panic::PanicInfo;
use pinx_libc::{PinxEvent, Surface, exit, println};

#[path = "../../pinx_abi.rs"]
mod pinx_abi;
use pinx_abi::PinxProgramHeader;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    core::hint::black_box(&PINX_PROGRAM_HEADER);
    main()
}

const WIN_W: u64 = 320;
const WIN_H: u64 = 200;

fn main() -> ! {
    let mut screen = match Surface::framebuffer() {
        Some(s) => s,
        None => { println!("demo: no framebuffer"); exit(1) }
    };
    let mut back = match Surface::new(screen.width(), screen.height()) {
        Some(b) => b,
        None => { println!("demo: no back buffer"); exit(1) }
    };

    let x = screen.width().saturating_sub(WIN_W) / 2;
    let y = screen.height().saturating_sub(WIN_H) / 2;
    [PinxEvent { kind: 0, code: 0, value: 0, _reserved: 0 }; 16];

    loop {
        // Draw the frame into the back buffer.
        back.fill(0x20_2838);
        back.fill_rect(x, y, WIN_W, WIN_H, 0x30_4050);
        back.fill_rect(x, y, WIN_W, 16, 0x00_6080);   // title bar
        back.rect(x, y, WIN_W, WIN_H, 0x80_C0FF);
        back.draw_text(x + 6, y + 4, "Starting Pinx Os", 0xFF_FFFF);
        back.draw_text(x + 12, y + 34, "--------------", 0xC0_D0E0);
        screen.copy_from(&back, 0, 0);

            }
    }

#[panic_handler]
fn panic(info: &PanicInfo) -> ! { println!("demo: panic: {}", info); exit(1) }

#[used]
#[unsafe(no_mangle)]
pub static PINX_PROGRAM_HEADER: PinxProgramHeader = PinxProgramHeader::new();