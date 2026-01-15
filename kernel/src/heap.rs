//! Heap allocator for dynamic memory allocation
//!
//! This module provides a global heap allocator using the linked_list_allocator crate.
//! The heap is initialized once during boot and used for all dynamic allocations.
//!
//! ## Invariants
//!
//! - Heap must be initialized before any allocations
//! - Heap memory region must not overlap with kernel code/data
//! - All allocations are properly aligned

use linked_list_allocator::LockedHeap;

/// Heap start address (16 MiB into physical memory)
const HEAP_START: usize = 0x_4444_4444_0000;

/// Heap size (100 KiB to start)
const HEAP_SIZE: usize = 100 * 1024;

/// Global allocator instance
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initialize the heap allocator
///
/// # Safety
///
/// Must be called exactly once during kernel initialization.
/// The heap memory region must be valid and unused.
pub unsafe fn init() {
    // SAFETY: Caller guarantees this is called once during boot
    // and the heap region is valid
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_constants() {
        assert!(HEAP_SIZE > 0);
        assert!(HEAP_START > 0);
        assert!(HEAP_SIZE >= 1024); // At least 1 KiB
    }
}
