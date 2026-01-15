//! Page table tracking for x86_64
//!
//! This module tracks all page table frames (L4, L3, L2, L1) to ensure
//! they are never returned by the frame allocator.
//!
//! ## Invariants
//!
//! - All page table frames are tracked and reserved
//! - Page table frames are allocated from the frame allocator
//! - Page table frames are immediately reserved with ReservationReason::PageTables
//! - No page table frame is ever returned by allocate_frame()

use alloc::vec::Vec;
use spin::Mutex;

/// Maximum number of page table frames to track
/// This supports up to 512 entries per level * 4 levels = lots of mappings
const MAX_PAGE_TABLE_FRAMES: usize = 512;

/// Page table frame tracker
///
/// Tracks all allocated page table frames to ensure they're never
/// returned by the frame allocator.
#[derive(Debug)]
pub struct PageTableTracker {
    /// List of page table frame numbers
    frames: Vec<usize>,
}

impl PageTableTracker {
    /// Create a new empty page table tracker
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Track a page table frame
    ///
    /// This should be called immediately after allocating a frame for page tables.
    /// The frame will be reserved in the frame allocator.
    ///
    /// # Safety
    ///
    /// Frame must be a valid page table frame that was allocated from the frame allocator.
    pub unsafe fn track_frame(&mut self, frame: usize) {
        // Reserve the frame in the frame allocator
        // SAFETY: Caller guarantees frame is valid and was allocated
        unsafe {
            crate::memory::reserve_frames(
                frame,
                frame + 1,
                panda_hal::memory::ReservationReason::PageTables,
            );
        }

        // Add to tracked list
        if !self.frames.contains(&frame) {
            self.frames.push(frame);
        }
    }

    /// Check if a frame is a tracked page table frame
    pub fn is_page_table_frame(&self, frame: usize) -> bool {
        self.frames.contains(&frame)
    }

    /// Get iterator over all tracked page table frames
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.frames.iter().copied()
    }

    /// Get count of tracked page table frames
    pub fn count(&self) -> usize {
        self.frames.len()
    }

    /// Get the list of tracked frames (for testing)
    pub fn frames(&self) -> &[usize] {
        &self.frames
    }
}

/// Global page table tracker
static PAGE_TABLE_TRACKER: Mutex<Option<PageTableTracker>> = Mutex::new(None);

/// Initialize the global page table tracker
pub fn init() {
    *PAGE_TABLE_TRACKER.lock() = Some(PageTableTracker::new());
}

/// Track a page table frame globally
///
/// # Safety
///
/// Frame must be a valid page table frame that was allocated from the frame allocator.
pub unsafe fn track_page_table_frame(frame: usize) {
    if let Some(tracker) = PAGE_TABLE_TRACKER.lock().as_mut() {
        // SAFETY: Caller guarantees frame is valid
        unsafe {
            tracker.track_frame(frame);
        }
    }
}

/// Check if a frame is a page table frame
pub fn is_page_table_frame(frame: usize) -> bool {
    PAGE_TABLE_TRACKER
        .lock()
        .as_ref()
        .map_or(false, |t| t.is_page_table_frame(frame))
}

/// Get count of tracked page table frames
pub fn page_table_frame_count() -> usize {
    PAGE_TABLE_TRACKER
        .lock()
        .as_ref()
        .map_or(0, |t| t.count())
}

/// Get list of all page table frames (for debugging and testing)
pub fn get_page_table_frames() -> Vec<usize> {
    PAGE_TABLE_TRACKER
        .lock()
        .as_ref()
        .map_or(Vec::new(), |t| t.frames().to_vec())
}

/// Allocate a frame for page tables
///
/// This allocates a frame from the global allocator and immediately
/// tracks it as a page table frame, reserving it to prevent re-allocation.
///
/// # Safety
///
/// Frame allocator must be initialized.
pub unsafe fn allocate_page_table_frame() -> Option<usize> {
    // SAFETY: Caller guarantees frame allocator is initialized
    if let Some(frame) = unsafe { crate::memory::allocate_frame() } {
        // Track and reserve the frame immediately
        // SAFETY: Frame was just allocated from the allocator
        unsafe {
            track_page_table_frame(frame);
        }
        Some(frame)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_table_tracker_creation() {
        let tracker = PageTableTracker::new();
        assert_eq!(tracker.count(), 0);
        assert!(tracker.frames().is_empty());
    }

    #[test]
    fn test_track_single_frame() {
        let mut tracker = PageTableTracker::new();
        
        // Note: track_frame() requires unsafe and reserves frames,
        // which we can't do in a unit test without the full kernel.
        // This test just verifies the data structure works.
        
        assert_eq!(tracker.count(), 0);
        assert!(!tracker.is_page_table_frame(100));
    }

    #[test]
    fn test_is_page_table_frame() {
        let tracker = PageTableTracker::new();
        assert!(!tracker.is_page_table_frame(0));
        assert!(!tracker.is_page_table_frame(100));
        assert!(!tracker.is_page_table_frame(1000));
    }

    #[test]
    fn test_frames_iterator() {
        let tracker = PageTableTracker::new();
        let frames: Vec<_> = tracker.iter().collect();
        assert!(frames.is_empty());
    }
}
