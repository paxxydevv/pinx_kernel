//! Log-level plumbing.
//!
//! Deliberately free of graphics and console dependencies: nothing here touches
//! the framebuffer, so the threshold and parsing rules could be tested on a
//! host without booting the kernel. The half that does need the console stays
//! in `terminal`.
//!
//! Two independent filters sit in front of the output:
//!
//!   1. Compile time, via Cargo features. A disabled macro expands to nothing,
//!      so the call, the format string and the argument expressions all vanish
//!      from the binary. This is what `--no-default-features` removes.
//!
//!   2. Run time, via `THRESHOLD`. Every enabled level is compared against a
//!      variable that can be changed at any point, so raising the threshold
//!      costs one branch and needs no rebuild.

use core::fmt;

/// Ordered, so a threshold is a single `>=` comparison.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    None = 5,
}

/// The active threshold. `volatile` through raw pointers because it is meant
/// to be poked from a debugger or a future shell, and the read in `enabled`
/// must not be hoisted and then go stale.
static mut THRESHOLD: i32 = Level::Info as i32;

const NAMES: [(&str, Level); 6] = [
    ("trace", Level::Trace),
    ("debug", Level::Debug),
    ("info", Level::Info),
    ("warn", Level::Warn),
    ("error", Level::Error),
    ("none", Level::None),
];

#[inline]
pub fn enabled(level: Level) -> bool {
    let threshold = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(THRESHOLD)) };
    // `none` silences everything, errors included, so it is checked first.
    if threshold >= Level::None as i32 {
        return false;
    }
    (level as i32) >= threshold
}

pub fn level_name(level: Level) -> &'static str {
    for (name, value) in NAMES {
        if value == level {
            return name;
        }
    }
    "unknown"
}

pub fn level_parse(name: &str) -> Option<Level> {
    for (candidate, value) in NAMES {
        if candidate == name {
            return Some(value);
        }
    }
    None
}

pub fn set_level(level: Level) {
    unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(THRESHOLD), level as i32) }
}

pub fn get_level() -> Level {
    let value = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(THRESHOLD)) };
    match value {
        0 => Level::Trace,
        1 => Level::Debug,
        2 => Level::Info,
        3 => Level::Warn,
        4 => Level::Error,
        _ => Level::None,
    }
}

/// Apply the build-time `PINX_LOG_LEVEL` from `build.rs`. Call once, early.
pub fn init() {
    match level_parse(env!("PINX_LOG_LEVEL")) {
        Some(level) => set_level(level),
        // A bad value cannot happen through build.rs, which validates it, but
        // fall back to info rather than dumping every trace line.
        None => set_level(Level::Info),
    }
}

/// Threshold a level against the runtime filter, then emit.
pub fn at(level: Level, args: fmt::Arguments) {
    if enabled(level) {
        emit(level, args);
    }
}

/// Emit one line: optional level tag, then the formatted message.
pub fn emit(level: Level, args: fmt::Arguments) {
    // `level` only feeds the tag, which `log-tags` can compile out.
    let _ = level;

    #[cfg(feature = "log-tags")]
    crate::terminal::print(format_args!("[{}] ", level_name(level)));

    crate::terminal::print(args);
}

/// Unconditional and unfiltered. Used by the panic path.
pub fn error(args: fmt::Arguments) {
    emit(Level::Error, args);
}

// ---------------------------------------------------------------- macros

#[macro_export]
macro_rules! klog_at {
    ($level:expr, $($arg:tt)*) => {
        $crate::klog::at($level, format_args!($($arg)*))
    };
}

/* Compile-time gated by CONFIG_DEBUG. */
#[cfg(feature = "debug")]
#[macro_export]
macro_rules! ktrace {
    ($($arg:tt)*) => { $crate::klog_at!($crate::klog::Level::Trace, $($arg)*) };
}
#[cfg(not(feature = "debug"))]
#[macro_export]
macro_rules! ktrace {
    ($($arg:tt)*) => {{}};
}

#[cfg(feature = "debug")]
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => { $crate::klog_at!($crate::klog::Level::Debug, $($arg)*) };
}
#[cfg(not(feature = "debug"))]
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => {{}};
}

#[cfg(feature = "debug")]
#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => { $crate::klog_at!($crate::klog::Level::Info, $($arg)*) };
}
#[cfg(not(feature = "debug"))]
#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => {{}};
}

/* Errors are never compiled out; only the `none` threshold silences them. */
#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => { $crate::klog_at!($crate::klog::Level::Error, $($arg)*) };
}

/* PCI and heap output sit on their own options, both requiring `debug`. */
#[cfg(all(feature = "debug", feature = "debug-pci"))]
#[macro_export]
macro_rules! kpci {
    ($($arg:tt)*) => { $crate::klog_at!($crate::klog::Level::Info, $($arg)*) };
}
#[cfg(not(all(feature = "debug", feature = "debug-pci")))]
#[macro_export]
macro_rules! kpci {
    ($($arg:tt)*) => {{}};
}

#[cfg(all(feature = "debug", feature = "debug-heap"))]
#[macro_export]
macro_rules! kheap {
    ($($arg:tt)*) => { $crate::klog_at!($crate::klog::Level::Warn, $($arg)*) };
}
#[cfg(not(all(feature = "debug", feature = "debug-heap")))]
#[macro_export]
macro_rules! kheap {
    ($($arg:tt)*) => {{}};
}
