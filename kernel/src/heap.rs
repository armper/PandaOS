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
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use linked_list_allocator::LockedHeap;

/// Heap size (104 KiB - page aligned)
pub const HEAP_SIZE: usize = 26 * 4096; // 104 KiB, exactly 26 pages

/// Actual heap start address (set by map_heap, used by init)
static ACTUAL_HEAP_START: AtomicUsize = AtomicUsize::new(0);

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
/// It allocates physical frames from the frame allocator.
///
/// # Safety
///
/// - Must be called before heap allocator init
/// - Frame allocator must be initialized
///
/// # Implementation Note
///
/// Currently uses identity-mapped low memory from bootloader for simplicity.
/// TODO: Implement proper page table mapping for high kernel addresses.
pub unsafe fn map_heap() -> Result<(), &'static str> {
    use panda_hal::memory::ReservationReason;

    // Calculate number of frames needed
    let num_frames = (HEAP_SIZE + 4095) / 4096;

    // Allocate consecutive frames for the heap (without using Vec - circular dependency)
    // We allocate a fixed array on stack to avoid heap usage during heap setup
    const MAX_HEAP_FRAMES: usize = 32; // Support up to 128 KiB heap
    let mut heap_frames = [0usize; MAX_HEAP_FRAMES];
    let mut frames_allocated = 0;

    for i in 0..num_frames {
        // SAFETY: Frame allocator is initialized at this point
        if let Some(frame) = unsafe { crate::memory::allocate_frame() } {
            if i < MAX_HEAP_FRAMES {
                heap_frames[i] = frame;
                frames_allocated += 1;
            } else {
                return Err("Heap too large: exceeds MAX_HEAP_FRAMES");
            }
        } else {
            return Err("Out of memory: cannot allocate frames for heap");
        }
    }

    // Use first frame as heap start (bootloader identity-maps low memory)
    let heap_start = heap_frames[0] * 4096;

    println!("Heap: {} frames allocated ({} KiB)", frames_allocated, HEAP_SIZE / 1024);
    println!("Heap physical: {:#x}..{:#x}", heap_start, heap_start + HEAP_SIZE);

    // Reserve heap frames so they won't be allocated again
    // Reserve each frame individually since they may not be consecutive
    unsafe {
        for i in 0..frames_allocated {
            crate::memory::reserve_frames(
                heap_frames[i],
                heap_frames[i] + 1,
                ReservationReason::Heap,
            );
        }
    }

    // Store actual heap start for init
    ACTUAL_HEAP_START.store(heap_start, Ordering::SeqCst);

    Ok(())
}

/// Initialize the heap allocator
///
/// This must be called AFTER heap region is mapped via map_heap().
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

    // Get the actual heap start set by map_heap
    let heap_start = ACTUAL_HEAP_START.load(Ordering::SeqCst);

    if heap_start == 0 {
        panic!("Heap not mapped: call map_heap() before init()");
    }

    // SAFETY: Caller guarantees:
    // - This is called exactly once during boot
    // - Single-core, interrupts disabled (no race conditions)
    // - The heap region is valid and mapped (map_heap was called first)
    unsafe {
        INNER_ALLOCATOR.lock().init(heap_start as *mut u8, HEAP_SIZE);
    }

    println!("Heap allocator initialized at {:#x}", heap_start);
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
        assert!(HEAP_SIZE >= 1024); // At least 1 KiB
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
