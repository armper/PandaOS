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
        if self.next_frame < self.available_frames.end {
            let frame = self.next_frame;
            self.next_frame += 1;
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
}
