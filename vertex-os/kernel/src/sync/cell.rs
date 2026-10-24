//! Kernel-wide shared-state cell.
//!
//! On a single-CPU kernel, interrupts are the only source of concurrency, so a
//! structure is safe to share as long as every access happens inside
//! [`crate::arch::x86_64::cpu::without_interrupts`]. Using an explicit
//! `UnsafeCell` wrapper (instead of `static mut`) keeps that requirement
//! visible and avoids `static_mut_refs` hazards.

use core::cell::UnsafeCell;

/// A `static`-friendly cell guarded by interrupt masking.
#[repr(transparent)]
pub struct SyncCell<T> {
    inner: UnsafeCell<T>,
}

unsafe impl<T> Sync for SyncCell<T> {}

impl<T> SyncCell<T> {
    /// Create a new cell. `const` so it can be used in statics.
    pub const fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(value),
        }
    }

    /// Get a raw pointer to the contents.
    #[inline]
    pub const fn as_ptr(&self) -> *mut T {
        self.inner.get()
    }

    /// Borrow the contents immutably.
    ///
    /// # Safety
    /// Caller must ensure no other borrow is live (interrupts disabled).
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get(&self) -> &mut T {
        &mut *self.inner.get()
    }
}
