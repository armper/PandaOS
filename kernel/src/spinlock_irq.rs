//! IRQ-safe spinlock wrapper
//!
//! This module provides a spinlock that automatically disables interrupts
//! while the lock is held. This prevents deadlocks caused by interrupt
//! handlers trying to acquire the same lock.
//!
//! ## Usage
//!
//! ```rust,ignore
//! static COUNTER: SpinLockIrq<u32> = SpinLockIrq::new(0);
//!
//! // Lock automatically disables interrupts
//! let mut guard = COUNTER.lock();
//! *guard += 1;
//! // Dropping guard restores interrupt state
//! ```
//!
//! ## Safety
//!
//! - Lock acquisition disables interrupts and saves previous state
//! - Lock release restores previous interrupt state
//! - Nested locks are supported (interrupt state is properly tracked)

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// IRQ-safe spinlock
///
/// This wraps a value with a spinlock that automatically disables
/// interrupts while the lock is held.
pub struct SpinLockIrq<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

/// Guard for SpinLockIrq that restores interrupt state on drop
pub struct SpinLockIrqGuard<'a, T> {
    lock: &'a SpinLockIrq<T>,
    interrupts_were_enabled: bool,
}

// SAFETY: SpinLockIrq can be shared between threads because:
// - Access to data is protected by the atomic lock
// - Interrupts are disabled while lock is held
unsafe impl<T: Send> Sync for SpinLockIrq<T> {}
unsafe impl<T: Send> Send for SpinLockIrq<T> {}

impl<T> SpinLockIrq<T> {
    /// Create a new IRQ-safe spinlock
    pub const fn new(data: T) -> Self {
        Self { locked: AtomicBool::new(false), data: UnsafeCell::new(data) }
    }

    /// Acquire the lock, disabling interrupts
    ///
    /// This will spin until the lock is acquired, then disable interrupts
    /// and return a guard. The guard will restore interrupt state when dropped.
    pub fn lock(&self) -> SpinLockIrqGuard<T> {
        // Save current interrupt state
        let interrupts_were_enabled = x86_64::instructions::interrupts::are_enabled();

        // Disable interrupts
        x86_64::instructions::interrupts::disable();

        // Spin until we acquire the lock
        while self.locked.swap(true, Ordering::Acquire) {
            // Hint to CPU that we're spinning
            core::hint::spin_loop();
        }

        SpinLockIrqGuard { lock: self, interrupts_were_enabled }
    }

    /// Try to acquire the lock without blocking
    ///
    /// Returns None if the lock is already held.
    pub fn try_lock(&self) -> Option<SpinLockIrqGuard<T>> {
        let interrupts_were_enabled = x86_64::instructions::interrupts::are_enabled();
        x86_64::instructions::interrupts::disable();

        if self.locked.swap(true, Ordering::Acquire) {
            // Lock was already held, restore interrupts
            if interrupts_were_enabled {
                x86_64::instructions::interrupts::enable();
            }
            None
        } else {
            Some(SpinLockIrqGuard { lock: self, interrupts_were_enabled })
        }
    }

    /// Get a mutable reference to the inner data
    ///
    /// # Safety
    ///
    /// This is safe because we have exclusive access (&mut self)
    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: We have &mut self, so no other references can exist
        unsafe { &mut *self.data.get() }
    }
}

impl<T> Deref for SpinLockIrqGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: We hold the lock, so we have exclusive access
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockIrqGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: We hold the lock, so we have exclusive access
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockIrqGuard<'_, T> {
    fn drop(&mut self) {
        // Release the lock
        self.lock.locked.store(false, Ordering::Release);

        // Restore interrupt state
        if self.interrupts_were_enabled {
            x86_64::instructions::interrupts::enable();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinlock_irq_creation() {
        let lock = SpinLockIrq::new(42);
        let guard = lock.lock();
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_spinlock_irq_mutation() {
        let lock = SpinLockIrq::new(0);
        {
            let mut guard = lock.lock();
            *guard = 42;
        }
        let guard = lock.lock();
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_get_mut() {
        let mut lock = SpinLockIrq::new(0);
        *lock.get_mut() = 42;
        assert_eq!(*lock.lock(), 42);
    }
}
