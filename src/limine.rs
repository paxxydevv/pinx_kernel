//! Limine boot protocol bindings.
//!
//! This is the Rust replacement for the third-party `limine.h`. Only the parts
//! the kernel actually consumes are modelled: the framebuffer, the memory map,
//! the HHDM offset and the TSC frequency. The request structures and the
//! surrounding markers are placed in their own sections so the bootloader can
//! find them by scanning the loaded image.
//!
//! Layout matters more than usual here: the bootloader interprets these bytes
//! directly, so every struct is `#[repr(C)]` and every field is spelled in the
//! order the protocol defines.

#![allow(dead_code)]

pub const BASE_REVISION: u64 = 6;

const COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineUuid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineVideoMode {
    pub pitch: u64,
    pub width: u64,
    pub height: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineFramebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 7],
    pub edid_size: u64,
    pub edid: *mut u8,
    pub mode_count: u64,
    pub modes: *mut *mut LimineVideoMode,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineFramebufferResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *mut *mut LimineFramebuffer,
}

#[repr(C)]
pub struct LimineFramebufferRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut LimineFramebufferResponse,
}

pub const MEMMAP_USABLE: u64 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineMemmapEntry {
    pub base: u64,
    pub length: u64,
    pub type_: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineMemmapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *mut *mut LimineMemmapEntry,
}

#[repr(C)]
pub struct LimineMemmapRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut LimineMemmapResponse,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineHhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[repr(C)]
pub struct LimineHhdmRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut LimineHhdmResponse,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineTscFrequencyResponse {
    pub revision: u64,
    pub frequency: u64,
}

#[repr(C)]
pub struct LimineTscFrequencyRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut LimineTscFrequencyResponse,
}

/// One processor the bootloader knows about. The bootloader fills in
/// `processor_id`, `lapic_id` and `reserved`; the kernel writes `goto_address`
/// (and optionally `extra_argument`) to have the AP started there.
#[repr(C)]
pub struct LimineSmpInfo {
    pub processor_id: u32,
    pub lapic_id: u32,
    pub reserved: u64,
    pub goto_address: Option<extern "C" fn(*mut LimineSmpInfo)>,
    pub extra_argument: u64,
}

#[repr(C)]
pub struct LimineSmpResponse {
    pub revision: u64,
    /// Bit 0 set means the LAPIC ids are x2APIC ids.
    pub flags: u32,
    pub bsp_lapic_id: u32,
    pub cpu_count: u64,
    pub cpus: *mut *mut LimineSmpInfo,
}

#[repr(C)]
pub struct LimineSmpRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut LimineSmpResponse,
    pub flags: u64,
}

/// Ask the bootloader to bring up the other processors in x2APIC mode.
pub const LIMINE_SMP_X2APIC: u64 = 1 << 0;

// ------------------------------------------------------------------ markers

#[used]
#[link_section = ".limine_requests_start"]
static LIMINE_REQUESTS_START_MARKER: [u64; 4] = [
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
];

#[used]
#[link_section = ".limine_requests"]
static LIMINE_BASE_REVISION: [u64; 3] = [
    0xf9562b2d5c95a6c8,
    0x6a7b384944536bdc,
    BASE_REVISION,
];

#[used]
#[link_section = ".limine_requests"]
static mut FRAMEBUFFER_REQUEST: LimineFramebufferRequest = LimineFramebufferRequest {
    id: [
        COMMON_MAGIC[0],
        COMMON_MAGIC[1],
        0x9d5827dcd881dd75,
        0xa3148604f6fab11b,
    ],
    revision: 0,
    response: core::ptr::null_mut(),
};

#[used]
#[link_section = ".limine_requests"]
static mut MEMMAP_REQUEST: LimineMemmapRequest = LimineMemmapRequest {
    id: [
        COMMON_MAGIC[0],
        COMMON_MAGIC[1],
        0x67cf3d9d378a806f,
        0xe304acdfc50c3c62,
    ],
    revision: 0,
    response: core::ptr::null_mut(),
};

#[used]
#[link_section = ".limine_requests"]
static mut HHDM_REQUEST: LimineHhdmRequest = LimineHhdmRequest {
    id: [
        COMMON_MAGIC[0],
        COMMON_MAGIC[1],
        0x48dcf1cb8ad2b852,
        0x63984e959a98244b,
    ],
    revision: 0,
    response: core::ptr::null_mut(),
};

#[used]
#[link_section = ".limine_requests"]
static mut TSC_FREQUENCY_REQUEST: LimineTscFrequencyRequest = LimineTscFrequencyRequest {
    id: [
        COMMON_MAGIC[0],
        COMMON_MAGIC[1],
        0x10f2ee1d87d195e4,
        0xf747a2b78f6ddb31,
    ],
    revision: 0,
    response: core::ptr::null_mut(),
};

#[used]
#[link_section = ".limine_requests"]
static mut SMP_REQUEST: LimineSmpRequest = LimineSmpRequest {
    id: [
        COMMON_MAGIC[0],
        COMMON_MAGIC[1],
        0x95a67b819a1b857e,
        0xa0b61b723b6a73e0,
    ],
    revision: 0,
    response: core::ptr::null_mut(),
    flags: LIMINE_SMP_X2APIC,
};

#[used]
#[link_section = ".limine_requests_end"]
static LIMINE_REQUESTS_END_MARKER: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];

// ----------------------------------------------------------------- accessors

#[inline]
pub fn framebuffer_response() -> *mut LimineFramebufferResponse {
    unsafe { core::ptr::read(core::ptr::addr_of!(FRAMEBUFFER_REQUEST.response)) }
}

#[inline]
pub fn memmap_response() -> *mut LimineMemmapResponse {
    unsafe { core::ptr::read(core::ptr::addr_of!(MEMMAP_REQUEST.response)) }
}

#[inline]
pub fn hhdm_response() -> *mut LimineHhdmResponse {
    unsafe { core::ptr::read(core::ptr::addr_of!(HHDM_REQUEST.response)) }
}

#[inline]
pub fn tsc_frequency_response() -> *mut LimineTscFrequencyResponse {
    unsafe { core::ptr::read(core::ptr::addr_of!(TSC_FREQUENCY_REQUEST.response)) }
}

#[inline]
pub fn smp_response() -> *mut LimineSmpResponse {
    unsafe { core::ptr::read(core::ptr::addr_of!(SMP_REQUEST.response)) }
}
