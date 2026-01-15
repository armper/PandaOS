//! Process/Task ID management
//!
//! Pure logic for managing process and task identifiers.

use core::sync::atomic::{AtomicU64, Ordering};

/// Process ID (PID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(u64);

impl Pid {
    /// Create a new PID from a raw value
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw PID value
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// The init process PID (always 1)
    pub const INIT: Self = Self(1);

    /// The kernel PID (always 0)
    pub const KERNEL: Self = Self(0);
}

/// PID allocator for generating unique process IDs
pub struct PidAllocator {
    next_pid: AtomicU64,
}

impl PidAllocator {
    /// Create a new PID allocator starting from the given PID
    pub const fn new(start_pid: u64) -> Self {
        Self { next_pid: AtomicU64::new(start_pid) }
    }

    /// Allocate a new unique PID
    pub fn allocate(&self) -> Pid {
        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
        Pid::new(pid)
    }

    /// Get the next PID that would be allocated (without allocating)
    pub fn peek(&self) -> Pid {
        Pid::new(self.next_pid.load(Ordering::SeqCst))
    }
}

impl Default for PidAllocator {
    fn default() -> Self {
        // Start from PID 2 (0 = kernel, 1 = init)
        Self::new(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_creation() {
        let pid = Pid::new(42);
        assert_eq!(pid.as_u64(), 42);
    }

    #[test]
    fn test_pid_constants() {
        assert_eq!(Pid::KERNEL.as_u64(), 0);
        assert_eq!(Pid::INIT.as_u64(), 1);
    }

    #[test]
    fn test_pid_comparison() {
        let pid1 = Pid::new(1);
        let pid2 = Pid::new(2);
        let pid3 = Pid::new(1);

        assert!(pid1 < pid2);
        assert_eq!(pid1, pid3);
        assert_ne!(pid1, pid2);
    }

    #[test]
    fn test_pid_allocator() {
        let allocator = PidAllocator::new(10);

        let pid1 = allocator.allocate();
        let pid2 = allocator.allocate();
        let pid3 = allocator.allocate();

        assert_eq!(pid1.as_u64(), 10);
        assert_eq!(pid2.as_u64(), 11);
        assert_eq!(pid3.as_u64(), 12);
    }

    #[test]
    fn test_pid_allocator_default() {
        let allocator = PidAllocator::default();
        let pid = allocator.allocate();

        assert_eq!(pid.as_u64(), 2);
    }

    #[test]
    fn test_pid_allocator_peek() {
        let allocator = PidAllocator::new(5);

        assert_eq!(allocator.peek().as_u64(), 5);
        assert_eq!(allocator.peek().as_u64(), 5); // Should not change

        let _pid = allocator.allocate();
        assert_eq!(allocator.peek().as_u64(), 6);
    }

    #[test]
    fn test_pid_allocator_concurrent() {
        extern crate std;
        use std::sync::Arc;
        use std::thread;
        use std::vec;
        use std::vec::Vec;

        let allocator = Arc::new(PidAllocator::new(0));
        let mut handles: Vec<_> = vec![];

        // Spawn 10 threads, each allocating 10 PIDs
        for _ in 0..10 {
            let alloc = Arc::clone(&allocator);
            let handle = thread::spawn(move || {
                let mut pids: Vec<Pid> = vec![];
                for _ in 0..10 {
                    pids.push(alloc.allocate());
                }
                pids
            });
            handles.push(handle);
        }

        // Collect all PIDs
        let mut all_pids: Vec<Pid> = vec![];
        for handle in handles {
            let pids: Vec<Pid> = handle.join().unwrap();
            all_pids.extend(pids);
        }

        // Check that all PIDs are unique
        all_pids.sort();
        for i in 0..all_pids.len() - 1 {
            assert_ne!(all_pids[i], all_pids[i + 1], "PIDs should be unique");
        }

        assert_eq!(all_pids.len(), 100);
    }
}
