//! Frame allocator for physical memory management
//!
//! This module provides pure logic for managing physical memory frames.
//! It's designed to be testable on the host without hardware dependencies.

use core::ops::Range;

/// Size of a physical memory frame (4 KiB)
pub const FRAME_SIZE: usize = 4096;

/// A physical memory frame allocator
///
/// This uses a simple bitmap-based allocation strategy.
#[derive(Debug)]
pub struct FrameAllocator {
    /// Range of available frames
    available_frames: Range<usize>,
    /// Next frame to allocate (simple bump allocator for now)
    next_frame: usize,
}

impl FrameAllocator {
    /// Create a new frame allocator for the given memory range
    ///
    /// # Arguments
    ///
    /// * `start_frame` - First available frame number
    /// * `end_frame` - Last available frame number (exclusive)
    pub const fn new(start_frame: usize, end_frame: usize) -> Self {
        Self { available_frames: start_frame..end_frame, next_frame: start_frame }
    }

    /// Allocate a single frame
    ///
    /// Returns the frame number or None if out of memory
    pub fn allocate_frame(&mut self) -> Option<usize> {
        // Invariant: next_frame should never exceed end
        #[cfg(debug_assertions)]
        {
            assert!(
                self.next_frame <= self.available_frames.end,
                "Frame allocator corrupted: next_frame {} > end {}",
                self.next_frame,
                self.available_frames.end
            );
        }

        if self.next_frame < self.available_frames.end {
            let frame = self.next_frame;
            self.next_frame += 1;

            // Invariant: allocated frame must be in valid range
            #[cfg(debug_assertions)]
            {
                assert!(
                    frame >= self.available_frames.start && frame < self.available_frames.end,
                    "Allocated frame {} out of range [{}..{})",
                    frame,
                    self.available_frames.start,
                    self.available_frames.end
                );
            }

            Some(frame)
        } else {
            None
        }
    }

    /// Deallocate a frame
    ///
    /// Note: Current implementation doesn't support deallocation
    /// This is a placeholder for future implementation
    pub fn deallocate_frame(&mut self, _frame: usize) {
        // TODO: Implement proper deallocation with bitmap
    }

    /// Get the number of available frames
    pub fn available_frames(&self) -> usize {
        self.available_frames.end.saturating_sub(self.next_frame)
    }

    /// Get the total number of frames managed by this allocator
    pub fn total_frames(&self) -> usize {
        self.available_frames.end - self.available_frames.start
    }

    /// Get the number of allocated frames
    pub fn allocated_frames(&self) -> usize {
        self.next_frame - self.available_frames.start
    }

    /// Convert frame number to physical address
    pub const fn frame_to_addr(frame: usize) -> usize {
        frame * FRAME_SIZE
    }

    /// Convert physical address to frame number
    pub const fn addr_to_frame(addr: usize) -> usize {
        addr / FRAME_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_allocator_creation() {
        let allocator = FrameAllocator::new(0, 100);
        assert_eq!(allocator.total_frames(), 100);
        assert_eq!(allocator.available_frames(), 100);
        assert_eq!(allocator.allocated_frames(), 0);
    }

    #[test]
    fn test_frame_allocation() {
        let mut allocator = FrameAllocator::new(0, 10);

        assert_eq!(allocator.allocate_frame(), Some(0));
        assert_eq!(allocator.allocate_frame(), Some(1));
        assert_eq!(allocator.allocate_frame(), Some(2));

        assert_eq!(allocator.allocated_frames(), 3);
        assert_eq!(allocator.available_frames(), 7);
    }

    #[test]
    fn test_frame_exhaustion() {
        let mut allocator = FrameAllocator::new(0, 3);

        assert_eq!(allocator.allocate_frame(), Some(0));
        assert_eq!(allocator.allocate_frame(), Some(1));
        assert_eq!(allocator.allocate_frame(), Some(2));
        assert_eq!(allocator.allocate_frame(), None);
        assert_eq!(allocator.allocate_frame(), None);
    }

    #[test]
    fn test_frame_address_conversion() {
        assert_eq!(FrameAllocator::frame_to_addr(0), 0);
        assert_eq!(FrameAllocator::frame_to_addr(1), 4096);
        assert_eq!(FrameAllocator::frame_to_addr(10), 40960);

        assert_eq!(FrameAllocator::addr_to_frame(0), 0);
        assert_eq!(FrameAllocator::addr_to_frame(4096), 1);
        assert_eq!(FrameAllocator::addr_to_frame(40960), 10);
    }

    #[test]
    fn test_frame_range() {
        let mut allocator = FrameAllocator::new(100, 200);

        assert_eq!(allocator.allocate_frame(), Some(100));
        assert_eq!(allocator.allocate_frame(), Some(101));
        assert_eq!(allocator.total_frames(), 100);
    }

    #[test]
    fn test_empty_allocator() {
        let mut allocator = FrameAllocator::new(0, 0);

        assert_eq!(allocator.total_frames(), 0);
        assert_eq!(allocator.allocate_frame(), None);
    }

    // Property-based tests
    mod proptests {
        use super::*;
        extern crate std;
        use proptest::prelude::*;

        proptest! {
            /// Property: Allocated frames are always within the valid range
            #[test]
            fn prop_allocated_frames_in_range(start in 0usize..1000, count in 1usize..100) {
                let mut allocator = FrameAllocator::new(start, start + count);
                let mut allocated = std::vec::Vec::new();

                // Allocate all frames
                for _ in 0..count {
                    if let Some(frame) = allocator.allocate_frame() {
                        allocated.push(frame);
                        // Check frame is in valid range
                        prop_assert!(frame >= start && frame < start + count);
                    }
                }

                // Should have allocated exactly 'count' frames
                prop_assert_eq!(allocated.len(), count);
            }

            /// Property: No frame is allocated twice
            #[test]
            fn prop_no_double_allocation(start in 0usize..1000, count in 1usize..100) {
                let mut allocator = FrameAllocator::new(start, start + count);
                let mut allocated = std::vec::Vec::new();

                // Allocate all frames
                for _ in 0..count {
                    if let Some(frame) = allocator.allocate_frame() {
                        // Check this frame hasn't been allocated before
                        prop_assert!(!allocated.contains(&frame), "Frame {} allocated twice", frame);
                        allocated.push(frame);
                    }
                }
            }

            /// Property: Total frames equals allocated + available
            #[test]
            fn prop_frame_count_invariant(start in 0usize..1000, count in 1usize..100, alloc_count in 0usize..100) {
                let mut allocator = FrameAllocator::new(start, start + count);
                let alloc_count = alloc_count.min(count);

                // Allocate some frames
                for _ in 0..alloc_count {
                    let _ = allocator.allocate_frame();
                }

                // Invariant: total = allocated + available
                prop_assert_eq!(
                    allocator.total_frames(),
                    allocator.allocated_frames() + allocator.available_frames()
                );
            }

            /// Property: Address conversion is bijective
            #[test]
            fn prop_address_conversion_bijective(frame_num in 0usize..10000) {
                let addr = FrameAllocator::frame_to_addr(frame_num);
                let back_to_frame = FrameAllocator::addr_to_frame(addr);
                prop_assert_eq!(frame_num, back_to_frame);
            }

            /// Property: Exhausted allocator always returns None
            #[test]
            fn prop_exhausted_allocator(start in 0usize..1000, count in 1usize..50) {
                let mut allocator = FrameAllocator::new(start, start + count);

                // Exhaust the allocator
                for _ in 0..count {
                    let _ = allocator.allocate_frame();
                }

                // Should always return None now
                for _ in 0..10 {
                    prop_assert_eq!(allocator.allocate_frame(), None);
                }
            }
        }
    }
}
