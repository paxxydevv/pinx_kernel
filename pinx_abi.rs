//! Shared ABI between the PINX kernel and PINX userspace programs.
//!
//! This one file is meant to be compiled into both sides, so the layout can
//! never drift: the kernel pulls it in as the `user_abi` module, and a userspace
//! program includes it (or copies it verbatim).
//!
//! A program marks itself by embedding a header as a global, and the kernel
//! scans for that header's magic and checks the version *before* loading, so an
//! arbitrary ELF on the disk cannot be started by accident. There are two:
//!
//! * [`PinxInitHeader`] marks `/init`, the one program the kernel starts on its
//!   own at boot.
//! * [`PinxProgramHeader`] marks every other program, which can only be started
//!   by name via the `spawn` syscall.

// Each side only uses the header it needs; the other one is legitimately unused
// in that build.
#![allow(dead_code)]

/// ASCII tag that identifies the PINX init binary (`/init`). Twelve bytes so it
/// is unlikely to appear by chance in unrelated data.
pub const PINX_INIT_MAGIC: [u8; 12] = *b"PINX-64-INIT";

/// ABI version `/init` was built against. The kernel refuses to start it when
/// the header carries a different value.
pub const PINX_INIT_VERSION: u32 = 1;

/// Header `/init` embeds so the kernel will recognise it:
///
/// ```ignore
/// #[used]
/// #[no_mangle]
/// pub static PINX_INIT_HEADER: pinx_abi::PinxInitHeader =
///     pinx_abi::PinxInitHeader::new();
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PinxInitHeader {
    /// Always [`PINX_INIT_MAGIC`].
    pub magic: [u8; 12],
    /// Always [`PINX_INIT_VERSION`] for the kernel it was built for.
    pub version: u32,
}

impl PinxInitHeader {
    pub const fn new() -> PinxInitHeader {
        PinxInitHeader {
            magic: PINX_INIT_MAGIC,
            version: PINX_INIT_VERSION,
        }
    }
}

/// ASCII tag that identifies a spawnable PINX program. Twelve bytes so it is
/// unlikely to appear by chance in unrelated data.
pub const PINX_PROGRAM_MAGIC: [u8; 12] = *b"PINX-64-PROG";

/// ABI version a spawnable program was built against. The kernel refuses to
/// start a program whose header carries a different value.
pub const PINX_PROGRAM_VERSION: u32 = 1;

/// Header every non-init userspace program embeds so `spawn` will accept it:
///
/// ```ignore
/// #[used]
/// #[no_mangle]
/// pub static PINX_PROGRAM_HEADER: pinx_abi::PinxProgramHeader =
///     pinx_abi::PinxProgramHeader::new();
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PinxProgramHeader {
    /// Always [`PINX_PROGRAM_MAGIC`].
    pub magic: [u8; 12],
    /// Always [`PINX_PROGRAM_VERSION`] for the kernel it was built for.
    pub version: u32,
}

impl PinxProgramHeader {
    pub const fn new() -> PinxProgramHeader {
        PinxProgramHeader {
            magic: PINX_PROGRAM_MAGIC,
            version: PINX_PROGRAM_VERSION,
        }
    }
}

/// Event kinds delivered by the `read_events` syscall.
pub const PINX_EVENT_KEY: u32 = 1;

/// A single input event. For [`PINX_EVENT_KEY`], `code` is the ASCII character
/// and `value` is 1 for a press.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PinxEvent {
    pub kind: u32,
    pub code: u32,
    pub value: u32,
    pub _reserved: u32,
}

/// Geometry of the framebuffer mapped by the `map_framebuffer` syscall.
/// `address` is a user virtual address; pixels are `bpp / 8` bytes each and
/// rows are `pitch` bytes apart.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PinxFramebufferInfo {
    pub address: u64,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u64,
}

