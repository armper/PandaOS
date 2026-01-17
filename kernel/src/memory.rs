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

use core::sync::atomic::{AtomicU64, Ordering};
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
const MAX_TRACKED_FRAMES: usize = 1_048_576;
const FRAME_BITMAP_BYTES: usize = (MAX_TRACKED_FRAMES + 7) / 8;
static mut FRAME_BITMAP_STORAGE: [u8; FRAME_BITMAP_BYTES] = [0; FRAME_BITMAP_BYTES];

/// Bootloader-provided offset where physical memory is mapped in the virtual address space.
///
/// With the `bootloader/map_physical_memory` feature enabled, the bootloader maps all physical
/// memory at this offset (typically in 2MiB pages). The kernel can then access an arbitrary
/// physical address `p` at virtual address `p + offset`.
static PHYSICAL_MEMORY_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Returns the physical memory mapping offset provided by the bootloader.
pub fn physical_memory_offset() -> u64 {
    PHYSICAL_MEMORY_OFFSET.load(Ordering::Relaxed)
}

/// Convert a physical address to a virtual address using the bootloader mapping.
pub fn phys_to_virt(phys_addr: u64) -> u64 {
    phys_addr + physical_memory_offset()
}

/// Initialize memory management from bootloader memory map
///
/// This function:
/// 1. Converts bootloader memory map to normalized abstraction
/// 2. Initializes HAL frame allocator with usable memory
/// 3. Reserves kernel, bootloader, page table, and critical frames
///
/// # Safety
///
/// Must be called exactly once during boot with valid memory map.
pub unsafe fn init_from_bootloader(boot_info: &'static bootloader::BootInfo) {
    use panda_hal::memory::ReservationReason;

    // Record the physical memory offset mapping provided by the bootloader.
    // This enables safe access to allocated frames before we have our own mapping layer.
    PHYSICAL_MEMORY_OFFSET.store(boot_info.physical_memory_offset, Ordering::SeqCst);

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

    // Calculate usable memory for frame allocator
    let usable_start_frame = find_first_usable_frame(&memory_map);
    let usable_end_frame = find_last_usable_frame(&memory_map);

    let total_frames = usable_end_frame.saturating_sub(usable_start_frame);
    assert!(
        total_frames <= MAX_TRACKED_FRAMES,
        "Frame allocator exceeds bitmap capacity: {} frames",
        total_frames
    );
    let bitmap_bytes = (total_frames + 7) / 8;

    // Initialize HAL frame allocator
    // SAFETY: FRAME_BITMAP_STORAGE is a global buffer used only during init.
    let bitmap_storage = unsafe { &mut FRAME_BITMAP_STORAGE[..bitmap_bytes] };
    let mut frame_allocator =
        FrameAllocator::new(usable_start_frame, usable_end_frame, bitmap_storage);

    // Reserve frame 0 (BIOS/IVT data - should never be used)
    frame_allocator.reserve_range(0, 1, ReservationReason::NullFrame);

    // Reserve kernel physical range using linker symbols
    // The bootloader loads the kernel at low physical memory
    let kernel_phys_start = crate::linker_symbols::kernel_phys_start();
    let kernel_phys_end = crate::linker_symbols::kernel_phys_end();

    let kernel_start_frame = (kernel_phys_start / panda_hal::memory::FRAME_SIZE as u64) as usize;
    let kernel_end_frame = ((kernel_phys_end + panda_hal::memory::FRAME_SIZE as u64 - 1)
        / panda_hal::memory::FRAME_SIZE as u64) as usize;

    frame_allocator.reserve_range(
        kernel_start_frame,
        kernel_end_frame,
        ReservationReason::KernelImage,
    );

    println!(
        "Kernel image: {:#x}..{:#x} ({} frames)",
        kernel_phys_start,
        kernel_phys_end,
        kernel_end_frame - kernel_start_frame
    );

    // Reserve bootloader structures
    // The bootloader memory map itself and boot info structure
    let boot_info_addr = boot_info as *const _ as u64;
    let boot_info_frame = (boot_info_addr / panda_hal::memory::FRAME_SIZE as u64) as usize;
    frame_allocator.reserve_range(
        boot_info_frame,
        boot_info_frame + 1,
        ReservationReason::Bootloader,
    );

    // Reserve page table frames currently in use
    // The bootloader sets up initial page tables - we need to preserve them
    // TODO: Walk page tables and reserve all PT frames
    // For now, we reserve the L4 page table frame
    use x86_64::registers::control::Cr3;
    let (level_4_table_frame, _) = Cr3::read();
    let l4_frame =
        level_4_table_frame.start_address().as_u64() / panda_hal::memory::FRAME_SIZE as u64;
    frame_allocator.reserve_range(
        l4_frame as usize,
        (l4_frame + 1) as usize,
        ReservationReason::PageTables,
    );

    // Log reservation statistics
    println!("Memory: {} KiB total", memory_map.usable_memory() / 1024);
    println!("Frame allocator: frames {}..{}", usable_start_frame, usable_end_frame);
    println!("  Total frames: {}", frame_allocator.total_frames());
    println!("  Reserved frames: {}", frame_allocator.reserved_frames());
    println!("  Usable frames: {}", frame_allocator.usable_frames());

    // Debug: log reserved regions
    #[cfg(debug_assertions)]
    {
        println!("Reserved regions:");
        for region in frame_allocator.reserved_regions() {
            println!(
                "  {}..{} ({} frames): {:?}",
                region.start_frame,
                region.end_frame,
                region.end_frame - region.start_frame,
                region.reason
            );
        }
    }

    *FRAME_ALLOCATOR.lock() = Some(frame_allocator);
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

/// Reserve a range of frames in the global allocator
///
/// This is used to mark frames as unavailable for allocation.
/// Typically called to reserve heap frames after heap mapping.
///
/// # Safety
///
/// Frame allocator must be initialized.
pub unsafe fn reserve_frames(
    start_frame: usize,
    end_frame: usize,
    reason: panda_hal::memory::ReservationReason,
) {
    if let Some(allocator) = FRAME_ALLOCATOR.lock().as_mut() {
        allocator.reserve_range(start_frame, end_frame, reason);
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
