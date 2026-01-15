//! Frame allocator for physical memory management
//!
//! This module provides pure logic for managing physical memory frames.
//! It's designed to be testable on the host without hardware dependencies.

use core::ops::Range;

use crate::bitmap::Bitmap;

/// Size of a physical memory frame (4 KiB)
pub const FRAME_SIZE: usize = 4096;

/// Maximum number of reserved regions
const MAX_RESERVED_REGIONS: usize = 32;

/// A reserved region with a reason for debugging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedRegion {
    /// Start frame number (inclusive)
    pub start_frame: usize,
    /// End frame number (exclusive)
    pub end_frame: usize,
    /// Reason for reservation (for debugging)
    pub reason: ReservationReason,
}

/// Reason for frame reservation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationReason {
    /// Kernel image (code and data)
    KernelImage,
    /// Bootloader structures and data
    Bootloader,
    /// Page tables
    PageTables,
    /// Heap memory
    Heap,
    /// Initramfs or modules
    InitramfsModule,
    /// Frame 0 (BIOS/IVT data)
    NullFrame,
}

impl ReservedRegion {
    /// Create a new reserved region
    pub const fn new(start_frame: usize, end_frame: usize, reason: ReservationReason) -> Self {
        Self { start_frame, end_frame, reason }
    }

    /// Check if this region contains the given frame
    pub const fn contains(&self, frame: usize) -> bool {
        frame >= self.start_frame && frame < self.end_frame
    }

    /// Check if this region overlaps with another
    pub const fn overlaps(&self, other: &Self) -> bool {
        self.start_frame < other.end_frame && other.start_frame < self.end_frame
    }

    /// Try to merge with another region if they overlap or are adjacent
    pub fn try_merge(&self, other: &Self) -> Option<Self> {
        // Can merge if overlapping or adjacent
        let adjacent = self.end_frame == other.start_frame || other.end_frame == self.start_frame;
        let overlapping = self.overlaps(other);

        if adjacent || overlapping {
            Some(Self {
                start_frame: self.start_frame.min(other.start_frame),
                end_frame: self.end_frame.max(other.end_frame),
                // Keep the more specific reason (lower in enum order)
                reason: self.reason,
            })
        } else {
            None
        }
    }
}

/// A physical memory frame allocator
///
/// This uses a simple bump allocator with explicit frame reservations.
/// Reserved frames are never returned by the allocator.
#[derive(Debug)]
pub struct FrameAllocator {
    /// Range of available frames
    available_frames: Range<usize>,
    /// Next frame to allocate (simple bump allocator for now)
    next_frame: usize,
    /// Reserved regions that cannot be allocated
    reserved_regions: [Option<ReservedRegion>; MAX_RESERVED_REGIONS],
    /// Number of reserved regions
    reserved_count: usize,
    /// Allocation bitmap for tracking used frames
    allocation_bitmap: Bitmap,
}

impl FrameAllocator {
    /// Create a new frame allocator for the given memory range
    ///
    /// # Arguments
    ///
    /// * `start_frame` - First available frame number
    /// * `end_frame` - Last available frame number (exclusive)
    /// * `bitmap_storage` - Backing storage for allocation bitmap
    pub fn new(
        start_frame: usize,
        end_frame: usize,
        bitmap_storage: &'static mut [u8],
    ) -> Self {
        let total_frames = end_frame.saturating_sub(start_frame);
        let allocation_bitmap = Bitmap::new(bitmap_storage, total_frames);

        Self {
            available_frames: start_frame..end_frame,
            next_frame: start_frame,
            reserved_regions: [None; MAX_RESERVED_REGIONS],
            reserved_count: 0,
            allocation_bitmap,
        }
    }

    /// Reserve a range of frames
    ///
    /// # Arguments
    ///
    /// * `start_frame` - First frame to reserve (inclusive)
    /// * `end_frame` - Last frame to reserve (exclusive)
    /// * `reason` - Reason for reservation (for debugging)
    ///
    /// # Panics
    ///
    /// Panics if too many regions are reserved (exceeds `MAX_RESERVED_REGIONS`)
    ///
    /// # Behavior with Overlaps
    ///
    /// If the new region overlaps with an existing reservation, they are merged
    /// into a single larger region. Adjacent regions are also merged.
    pub fn reserve_range(
        &mut self,
        start_frame: usize,
        end_frame: usize,
        reason: ReservationReason,
    ) {
        // Validate: non-empty range
        if start_frame >= end_frame {
            return; // Empty range, nothing to reserve
        }

        let new_region = ReservedRegion::new(start_frame, end_frame, reason);

        // Try to merge with existing regions
        let mut merged = false;
        for i in 0..self.reserved_count {
            if let Some(existing) = self.reserved_regions[i] {
                if let Some(merged_region) = existing.try_merge(&new_region) {
                    // Replace existing with merged region
                    self.reserved_regions[i] = Some(merged_region);
                    merged = true;

                    // Try to merge this with other regions (cascading merges)
                    self.consolidate_reservations();
                    break;
                }
            }
        }

        // If not merged, add as new region
        if !merged {
            assert!(
                self.reserved_count < MAX_RESERVED_REGIONS,
                "Too many reserved regions (max {MAX_RESERVED_REGIONS})"
            );
            self.reserved_regions[self.reserved_count] = Some(new_region);
            self.reserved_count += 1;
        }
    }

    /// Consolidate overlapping or adjacent reservations
    fn consolidate_reservations(&mut self) {
        // Simple O(n^2) consolidation - fine for small number of regions
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..self.reserved_count {
                for j in (i + 1)..self.reserved_count {
                    if let (Some(region_i), Some(region_j)) =
                        (self.reserved_regions[i], self.reserved_regions[j])
                    {
                        if let Some(merged) = region_i.try_merge(&region_j) {
                            // Merge into i, remove j
                            self.reserved_regions[i] = Some(merged);
                            // Shift remaining regions down
                            for k in j..self.reserved_count - 1 {
                                self.reserved_regions[k] = self.reserved_regions[k + 1];
                            }
                            self.reserved_regions[self.reserved_count - 1] = None;
                            self.reserved_count -= 1;
                            changed = true;
                            break;
                        }
                    }
                }
                if changed {
                    break;
                }
            }
        }
    }

    /// Check if a frame is reserved
    fn is_reserved(&self, frame: usize) -> bool {
        for i in 0..self.reserved_count {
            if let Some(region) = self.reserved_regions[i] {
                if region.contains(frame) {
                    return true;
                }
            }
        }
        false
    }

    /// Convert a frame number to a bitmap index
    fn frame_index(&self, frame: usize) -> Option<usize> {
        if frame < self.available_frames.start || frame >= self.available_frames.end {
            None
        } else {
            Some(frame - self.available_frames.start)
        }
    }

    /// Check if a frame is allocated
    fn is_allocated(&self, frame: usize) -> bool {
        self.frame_index(frame)
            .map(|index| self.allocation_bitmap.is_set(index))
            .unwrap_or(false)
    }

    /// Mark a frame as allocated
    fn set_allocated(&mut self, frame: usize) {
        if let Some(index) = self.frame_index(frame) {
            self.allocation_bitmap.set(index);
        }
    }

    /// Mark a frame as free
    fn clear_allocated(&mut self, frame: usize) {
        if let Some(index) = self.frame_index(frame) {
            self.allocation_bitmap.clear(index);
        }
    }

    /// Get iterator over reserved regions (for debugging)
    pub fn reserved_regions(&self) -> impl Iterator<Item = &ReservedRegion> {
        self.reserved_regions[..self.reserved_count].iter().filter_map(|r| r.as_ref())
    }

    /// Get total number of reserved frames
    pub fn reserved_frames(&self) -> usize {
        self.reserved_regions().map(|r| r.end_frame - r.start_frame).sum()
    }

    /// Get total number of usable frames (total - reserved)
    pub fn usable_frames(&self) -> usize {
        self.total_frames().saturating_sub(self.reserved_frames())
    }

    /// Allocate a single frame
    ///
    /// Returns the frame number or None if out of memory.
    /// This method skips any reserved frames.
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

        let total_frames = self.total_frames();
        if total_frames == 0 {
            return None;
        }

        let mut checked = 0;
        let mut frame = self.next_frame;
        if frame < self.available_frames.start || frame >= self.available_frames.end {
            frame = self.available_frames.start;
        }

        while checked < total_frames {
            if frame >= self.available_frames.end {
                frame = self.available_frames.start;
            }

            if !self.is_reserved(frame) && !self.is_allocated(frame) {
                self.set_allocated(frame);
                self.next_frame = frame + 1;
                return Some(frame);
            }

            frame += 1;
            checked += 1;
        }

        None
    }

    /// Deallocate a frame
    ///
    /// Clears the allocation bitmap so the frame can be reused.
    pub fn deallocate_frame(&mut self, frame: usize) {
        if self.is_reserved(frame) {
            return;
        }

        if !self.is_allocated(frame) {
            debug_assert!(false, "Deallocating unallocated frame {frame}");
            return;
        }

        self.clear_allocated(frame);

        if frame < self.next_frame {
            self.next_frame = frame;
        }
    }

    /// Get the number of available frames
    pub fn available_frames(&self) -> usize {
        self.usable_frames().saturating_sub(self.allocated_frames())
    }

    /// Get the total number of frames managed by this allocator
    pub fn total_frames(&self) -> usize {
        self.available_frames.end - self.available_frames.start
    }

    /// Get the number of allocated frames
    pub fn allocated_frames(&self) -> usize {
        self.allocation_bitmap.count_set()
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
    extern crate std;
    use std::boxed::Box;
    use std::vec;

    fn create_allocator(start: usize, end: usize) -> FrameAllocator {
        let total_frames = end.saturating_sub(start);
        let bytes = total_frames.div_ceil(8).max(1);
        let storage = vec![0u8; bytes];
        let static_storage: &'static mut [u8] = Box::leak(storage.into_boxed_slice());
        FrameAllocator::new(start, end, static_storage)
    }

    #[test]
    fn test_frame_allocator_creation() {
        let allocator = create_allocator(0, 100);
        assert_eq!(allocator.total_frames(), 100);
        assert_eq!(allocator.available_frames(), 100);
        assert_eq!(allocator.allocated_frames(), 0);
        assert_eq!(allocator.reserved_frames(), 0);
        assert_eq!(allocator.usable_frames(), 100);
    }

    #[test]
    fn test_frame_allocation() {
        let mut allocator = create_allocator(0, 10);

        assert_eq!(allocator.allocate_frame(), Some(0));
        assert_eq!(allocator.allocate_frame(), Some(1));
        assert_eq!(allocator.allocate_frame(), Some(2));

        assert_eq!(allocator.allocated_frames(), 3);
        assert_eq!(allocator.available_frames(), 7);
    }

    #[test]
    fn test_frame_exhaustion() {
        let mut allocator = create_allocator(0, 3);

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
        let mut allocator = create_allocator(100, 200);

        assert_eq!(allocator.allocate_frame(), Some(100));
        assert_eq!(allocator.allocate_frame(), Some(101));
        assert_eq!(allocator.total_frames(), 100);
    }

    #[test]
    fn test_empty_allocator() {
        let mut allocator = create_allocator(0, 0);

        assert_eq!(allocator.total_frames(), 0);
        assert_eq!(allocator.allocate_frame(), None);
    }

    // Reservation tests
    #[test]
    fn test_reserve_range_basic() {
        let mut allocator = create_allocator(0, 100);
        allocator.reserve_range(10, 20, ReservationReason::KernelImage);

        assert_eq!(allocator.reserved_frames(), 10);
        assert_eq!(allocator.usable_frames(), 90);
        assert_eq!(allocator.reserved_regions().count(), 1);
    }

    #[test]
    fn test_reserve_range_multiple() {
        let mut allocator = create_allocator(0, 100);
        allocator.reserve_range(10, 20, ReservationReason::KernelImage);
        allocator.reserve_range(30, 40, ReservationReason::Bootloader);

        assert_eq!(allocator.reserved_frames(), 20);
        assert_eq!(allocator.usable_frames(), 80);
        assert_eq!(allocator.reserved_regions().count(), 2);
    }

    #[test]
    fn test_reserve_empty_range() {
        let mut allocator = create_allocator(0, 100);
        allocator.reserve_range(10, 10, ReservationReason::KernelImage);

        assert_eq!(allocator.reserved_frames(), 0);
        assert_eq!(allocator.reserved_regions().count(), 0);
    }

    #[test]
    fn test_reserve_overlapping_ranges_merge() {
        let mut allocator = create_allocator(0, 100);
        allocator.reserve_range(10, 20, ReservationReason::KernelImage);
        allocator.reserve_range(15, 25, ReservationReason::Bootloader);

        // Should merge into one region [10, 25)
        assert_eq!(allocator.reserved_frames(), 15);
        assert_eq!(allocator.reserved_regions().count(), 1);

        let region = allocator.reserved_regions().next().unwrap();
        assert_eq!(region.start_frame, 10);
        assert_eq!(region.end_frame, 25);
    }

    #[test]
    fn test_reserve_adjacent_ranges_merge() {
        let mut allocator = create_allocator(0, 100);
        allocator.reserve_range(10, 20, ReservationReason::KernelImage);
        allocator.reserve_range(20, 30, ReservationReason::Bootloader);

        // Should merge into one region [10, 30)
        assert_eq!(allocator.reserved_frames(), 20);
        assert_eq!(allocator.reserved_regions().count(), 1);

        let region = allocator.reserved_regions().next().unwrap();
        assert_eq!(region.start_frame, 10);
        assert_eq!(region.end_frame, 30);
    }

    #[test]
    fn test_allocate_skips_reserved() {
        let mut allocator = create_allocator(0, 20);

        // Reserve frames 5-10
        allocator.reserve_range(5, 10, ReservationReason::KernelImage);

        // Allocate frames - should skip 5-10
        assert_eq!(allocator.allocate_frame(), Some(0));
        assert_eq!(allocator.allocate_frame(), Some(1));
        assert_eq!(allocator.allocate_frame(), Some(2));
        assert_eq!(allocator.allocate_frame(), Some(3));
        assert_eq!(allocator.allocate_frame(), Some(4));
        assert_eq!(allocator.allocate_frame(), Some(10)); // Skip 5-9
        assert_eq!(allocator.allocate_frame(), Some(11));
    }

    #[test]
    fn test_allocate_never_returns_reserved() {
        extern crate std;
        let mut allocator = create_allocator(0, 100);

        // Reserve multiple ranges
        allocator.reserve_range(10, 20, ReservationReason::KernelImage);
        allocator.reserve_range(50, 60, ReservationReason::Heap);
        allocator.reserve_range(80, 90, ReservationReason::PageTables);

        // Allocate all frames
        let mut allocated = std::vec::Vec::new();
        while let Some(frame) = allocator.allocate_frame() {
            allocated.push(frame);
        }

        // Verify no reserved frames were allocated
        for frame in &allocated {
            assert!(!allocator.is_reserved(*frame), "Allocated reserved frame {}", frame);
        }

        // Should have allocated 70 frames (100 - 30 reserved)
        assert_eq!(allocated.len(), 70);
    }

    #[test]
    fn test_reserved_region_contains() {
        let region = ReservedRegion::new(10, 20, ReservationReason::KernelImage);

        assert!(!region.contains(9));
        assert!(region.contains(10));
        assert!(region.contains(15));
        assert!(region.contains(19));
        assert!(!region.contains(20));
    }

    #[test]
    fn test_reserved_region_overlaps() {
        let region1 = ReservedRegion::new(10, 20, ReservationReason::KernelImage);
        let region2 = ReservedRegion::new(15, 25, ReservationReason::Bootloader);
        let region3 = ReservedRegion::new(30, 40, ReservationReason::Heap);

        assert!(region1.overlaps(&region2));
        assert!(region2.overlaps(&region1));
        assert!(!region1.overlaps(&region3));
        assert!(!region3.overlaps(&region1));
    }

    #[test]
    fn test_reserved_region_merge() {
        let region1 = ReservedRegion::new(10, 20, ReservationReason::KernelImage);
        let region2 = ReservedRegion::new(15, 25, ReservationReason::Bootloader);

        let merged = region1.try_merge(&region2).unwrap();
        assert_eq!(merged.start_frame, 10);
        assert_eq!(merged.end_frame, 25);
    }

    #[test]
    fn test_reserved_region_merge_adjacent() {
        let region1 = ReservedRegion::new(10, 20, ReservationReason::KernelImage);
        let region2 = ReservedRegion::new(20, 30, ReservationReason::Bootloader);

        let merged = region1.try_merge(&region2).unwrap();
        assert_eq!(merged.start_frame, 10);
        assert_eq!(merged.end_frame, 30);
    }

    #[test]
    fn test_reserved_region_no_merge_disjoint() {
        let region1 = ReservedRegion::new(10, 20, ReservationReason::KernelImage);
        let region2 = ReservedRegion::new(30, 40, ReservationReason::Bootloader);

        assert!(region1.try_merge(&region2).is_none());
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
                let mut allocator = super::create_allocator(start, start + count);
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
                let mut allocator = super::create_allocator(start, start + count);
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
                let mut allocator = super::create_allocator(start, start + count);
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
                let mut allocator = super::create_allocator(start, start + count);

                // Exhaust the allocator
                for _ in 0..count {
                    let _ = allocator.allocate_frame();
                }

                // Should always return None now
                for _ in 0..10 {
                    prop_assert_eq!(allocator.allocate_frame(), None);
                }
            }

            /// Property: Allocated frames never intersect with reserved frames
            #[test]
            fn prop_allocated_never_reserved(
                start in 0usize..100,
                count in 50usize..100,
                reserve_start in 0usize..50,
                reserve_count in 10usize..30
            ) {
                let mut allocator = super::create_allocator(start, start + count);

                // Reserve some frames
                let reserve_end = (reserve_start + reserve_count).min(start + count);
                if reserve_start < reserve_end {
                    allocator.reserve_range(
                        start + reserve_start,
                        start + reserve_end,
                        ReservationReason::KernelImage
                    );
                }

                // Allocate all available frames
                let mut allocated = std::vec::Vec::new();
                while let Some(frame) = allocator.allocate_frame() {
                    allocated.push(frame);
                }

                // Verify no allocated frame is reserved
                for frame in allocated {
                    prop_assert!(!allocator.is_reserved(frame));
                }
            }

            /// Property: Usable frames = total frames - reserved frames
            #[test]
            fn prop_usable_frames_invariant(
                start in 0usize..100,
                count in 50usize..100,
                reserve_start in 0usize..50,
                reserve_count in 10usize..30
            ) {
                let mut allocator = super::create_allocator(start, start + count);

                let reserve_end = (reserve_start + reserve_count).min(start + count);
                if reserve_start < reserve_end {
                    allocator.reserve_range(
                        start + reserve_start,
                        start + reserve_end,
                        ReservationReason::KernelImage
                    );
                }

                prop_assert_eq!(
                    allocator.usable_frames(),
                    allocator.total_frames() - allocator.reserved_frames()
                );
            }
        }
    }
}
