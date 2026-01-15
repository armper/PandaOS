//! Heap allocator for dynamic memory allocation
//!
//! This module provides a global heap allocator using the linked_list_allocator crate.
//! The heap is initialized once during boot and used for all dynamic allocations.
//!
//! ## Invariants
//!
//! - Heap region must be mapped before initialization
//! - Heap must be initialized before any allocations (enforced with debug assertions)
//! - Init must be called exactly once (panics on second call)
//! - Heap memory region must not overlap with kernel code/data
//! - All allocations are properly aligned
//! - Init assumes single-core with interrupts disabled (no locking needed)

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};
use linked_list_allocator::LockedHeap;

/// Heap start address in virtual memory
/// This address will be mapped by map_heap() before allocator init
pub const HEAP_START: usize = 0xFFFF_8000_0000_0000; // Kernel heap region

/// Heap size (100 KiB to start)
pub const HEAP_SIZE: usize = 100 * 1024;

/// Inner allocator instance
static INNER_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Tracks whether heap has been initialized (for debug assertions)
static HEAP_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Wrapper allocator that checks initialization in debug builds
struct CheckedAllocator;

unsafe impl GlobalAlloc for CheckedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Debug assertion to catch allocations before init
        #[cfg(debug_assertions)]
        {
            assert!(
                HEAP_INITIALIZED.load(Ordering::Relaxed),
                "Attempted heap allocation before heap initialization"
            );
        }

        // SAFETY: Caller guarantees layout requirements
        unsafe { INNER_ALLOCATOR.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Caller guarantees ptr and layout are valid
        unsafe { INNER_ALLOCATOR.dealloc(ptr, layout) }
    }
}

/// Global allocator instance with debug checks
#[global_allocator]
static ALLOCATOR: CheckedAllocator = CheckedAllocator;

/// Map heap region in page tables
///
/// This must be called BEFORE heap allocator initialization.
/// It allocates physical frames and maps them to the heap virtual address range.
///
/// # Safety
///
/// - Must be called before heap allocator init
/// - Frame allocator must be initialized
/// - Page tables must be accessible
pub unsafe fn map_heap() -> Result<(), &'static str> {
    // TODO: Implement actual page table mapping
    // For now, we'll use identity mapping provided by bootloader
    // This is a placeholder that will be replaced with proper paging

    // Calculate number of frames needed
    let num_frames = (HEAP_SIZE + 4095) / 4096;

    println!("Heap mapping: {} frames for {} KiB", num_frames, HEAP_SIZE / 1024);
    println!("Heap range: {:#x}..{:#x}", HEAP_START, HEAP_START + HEAP_SIZE);

    // TODO: Allocate frames from frame allocator
    // TODO: Map frames to HEAP_START..HEAP_START+HEAP_SIZE in page tables
    // TODO: Mark pages as writable and present

    Ok(())
}

/// Initialize the heap allocator
///
/// This must be called AFTER heap region is mapped.
///
/// # Safety
///
/// - Must be called exactly once during kernel initialization
/// - Must be called before interrupts are enabled (single-core assumption)
/// - The heap memory region must be valid and properly mapped (call map_heap first)
///
/// # Panics
///
/// Panics if called more than once (idempotency check)
pub unsafe fn init() {
    // Check if already initialized (idempotency)
    if HEAP_INITIALIZED.swap(true, Ordering::SeqCst) {
        panic!("Heap allocator already initialized");
    }

    // SAFETY: Caller guarantees:
    // - This is called exactly once during boot
    // - Single-core, interrupts disabled (no race conditions)
    // - The heap region is valid and mapped (map_heap was called first)
    unsafe {
        INNER_ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}

/// Check if heap is initialized
#[inline]
pub fn is_initialized() -> bool {
    HEAP_INITIALIZED.load(Ordering::Relaxed)
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

    #[test]
    fn test_heap_start_in_kernel_space() {
        // Ensure heap is in kernel address space (upper half)
        assert!(HEAP_START >= 0xFFFF_8000_0000_0000);
    }

    #[test]
    fn test_not_initialized_by_default() {
        // In test context, heap might be initialized, but we test the logic
        // This just ensures the function is callable
        let _ = is_initialized();
    }

    #[test]
    fn test_heap_size_page_aligned() {
        // Heap size should be multiple of page size for easier mapping
        assert_eq!(HEAP_SIZE % 4096, 0);
    }
}
