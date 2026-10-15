//! A minimal test-and-set spinlock for SMP.
//!
//! Acquiring also disables interrupts and the returned guard restores the
//! previous interrupt flag on drop. That ordering matters: without it, an
//! interrupt that lands while the lock is held would call back into the
//! scheduler and deadlock trying to take the same lock.

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock {
    locked: AtomicBool,
}

/// The lock is the only shared state behind the atomic, so it is safe to share.
unsafe impl Sync for SpinLock {}

impl SpinLock {
    pub const fn new() -> SpinLock {
        SpinLock {
            locked: AtomicBool::new(false),
        }
    }

    /// Take the lock with interrupts disabled.
    pub fn lock(&self) -> SpinLockGuard<'_> {
        let flags: u64;
        unsafe {
            asm!("pushfq; pop {0}; cli", out(reg) flags, options(nomem, nostack));
        }
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self, flags }
    }
}

pub struct SpinLockGuard<'a> {
    lock: &'a SpinLock,
    flags: u64,
}

impl Drop for SpinLockGuard<'_> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        unsafe {
            asm!("push {0}; popfq", in(reg) self.flags, options(nomem, nostack));
        }
    }
}
