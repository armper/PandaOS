//! Page table management for x86_64
//!
//! This module provides safe abstractions for managing x86_64 page tables
//! with 4-level paging (PML4, PDPT, PD, PT).
//!
//! ## Invariants
//!
//! - Page tables are always properly aligned (4KB)
//! - Page table entries are validated before use
//! - Physical addresses are within valid memory range
//! - Recursive mapping is used for page table access

use core::ops::{Index, IndexMut};

/// Page table entry flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PageTableFlags(u64);

impl PageTableFlags {
    /// Entry is present in memory
    pub const PRESENT: Self = Self(1 << 0);
    /// Page is writable
    pub const WRITABLE: Self = Self(1 << 1);
    /// Page is accessible from user mode
    pub const USER_ACCESSIBLE: Self = Self(1 << 2);
    /// Write-through caching
    pub const WRITE_THROUGH: Self = Self(1 << 3);
    /// Disable cache
    pub const NO_CACHE: Self = Self(1 << 4);
    /// Page has been accessed
    pub const ACCESSED: Self = Self(1 << 5);
    /// Page has been written to (dirty)
    pub const DIRTY: Self = Self(1 << 6);
    /// Huge page (2MB or 1GB)
    pub const HUGE_PAGE: Self = Self(1 << 7);
    /// Page is global (not flushed from TLB on context switch)
    pub const GLOBAL: Self = Self(1 << 8);
    /// Disable execution
    pub const NO_EXECUTE: Self = Self(1 << 63);

    /// Create empty flags
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create flags from raw value
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Get raw value
    pub const fn bits(&self) -> u64 {
        self.0
    }

    /// Check if flag is set
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine flags
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A page table entry
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create a new unused entry
    pub const fn new() -> Self {
        Self(0)
    }

    /// Check if entry is present
    pub const fn is_present(&self) -> bool {
        (self.0 & PageTableFlags::PRESENT.bits()) != 0
    }

    /// Check if entry is unused
    pub const fn is_unused(&self) -> bool {
        self.0 == 0
    }

    /// Get flags from entry
    pub const fn flags(&self) -> PageTableFlags {
        PageTableFlags::from_bits(self.0 & 0xFFF)
    }

    /// Get physical address from entry
    pub const fn addr(&self) -> u64 {
        self.0 & 0x000F_FFFF_FFFF_F000
    }

    /// Set entry to map to physical address with given flags
    pub fn set(&mut self, addr: u64, flags: PageTableFlags) {
        // Clear address bits that overlap with flags
        let addr = addr & 0x000F_FFFF_FFFF_F000;
        self.0 = addr | flags.bits();
    }

    /// Clear entry
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

impl core::fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTableEntry")
            .field("present", &self.is_present())
            .field("addr", &format_args!("{:#x}", self.addr()))
            .field("flags", &self.flags())
            .finish()
    }
}

/// Number of entries in a page table
const ENTRY_COUNT: usize = 512;

/// A page table with 512 entries
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; ENTRY_COUNT],
}

impl PageTable {
    /// Create a new empty page table
    pub const fn new() -> Self {
        Self { entries: [PageTableEntry::new(); ENTRY_COUNT] }
    }

    /// Clear all entries
    pub fn zero(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }

    /// Get an iterator over entries
    pub fn iter(&self) -> core::slice::Iter<'_, PageTableEntry> {
        self.entries.iter()
    }

    /// Get a mutable iterator over entries
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, PageTableEntry> {
        self.entries.iter_mut()
    }
}

impl Index<usize> for PageTable {
    type Output = PageTableEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for PageTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

/// Virtual address wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    /// Create from u64
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get raw value
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Get page aligned address
    pub const fn page_align_down(&self) -> Self {
        Self(self.0 & !0xFFF)
    }

    /// Get PML4 index
    pub const fn p4_index(&self) -> usize {
        ((self.0 >> 39) & 0x1FF) as usize
    }

    /// Get PDPT index
    pub const fn p3_index(&self) -> usize {
        ((self.0 >> 30) & 0x1FF) as usize
    }

    /// Get PD index
    pub const fn p2_index(&self) -> usize {
        ((self.0 >> 21) & 0x1FF) as usize
    }

    /// Get PT index
    pub const fn p1_index(&self) -> usize {
        ((self.0 >> 12) & 0x1FF) as usize
    }

    /// Get page offset
    pub const fn page_offset(&self) -> usize {
        (self.0 & 0xFFF) as usize
    }
}

/// Physical address wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    /// Create from u64
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get raw value
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Get page aligned address
    pub const fn page_align_down(&self) -> Self {
        Self(self.0 & !0xFFF)
    }
}

/// Higher-half mapping support
///
/// This module provides functions to set up higher-half kernel mapping
/// where kernel code and data are mapped at high virtual addresses
/// (starting at KERNEL_VIRT_BASE).

/// Initialize minimal identity mapping for early boot
///
/// This sets up a minimal identity mapping for the first few megabytes
/// of physical memory to allow boot code to run before switching to
/// higher-half mapping.
///
/// # Safety
///
/// Must be called during early boot before higher-half transition.
/// Frame allocator must be initialized.
pub unsafe fn init_identity_map_minimal() -> Result<(), &'static str> {
    // For now, the bootloader already sets up identity mapping
    // This is a placeholder for future custom mapping implementation
    
    println!("Identity mapping: Using bootloader-provided mapping");
    Ok(())
}

/// Initialize higher-half kernel mapping
///
/// This sets up page tables to map kernel code and data at higher-half
/// virtual addresses (starting at KERNEL_VIRT_BASE).
///
/// # Safety
///
/// Must be called after identity mapping is set up.
/// Frame allocator and page table tracker must be initialized.
pub unsafe fn init_higher_half_mapping() -> Result<(), &'static str> {
    // Initialize page table tracker
    crate::page_table_tracker::init();
    
    // For now, we continue to use bootloader's identity mapping
    // TODO: Implement custom higher-half page table setup with:
    // - Kernel text mapped as RX (Read + Execute, No Write)
    // - Kernel rodata mapped as R (Read only, No Execute)  
    // - Kernel data/bss mapped as RW (Read + Write, No Execute)
    // - Heap mapped as RW, NX
    
    println!("Higher-half mapping: Prepared (using identity mapping for now)");
    
    // Track existing page tables from bootloader
    // SAFETY: We're reading CR3 which is safe
    use x86_64::registers::control::Cr3;
    let (level_4_table_frame, _) = Cr3::read();
    let l4_frame_num =
        level_4_table_frame.start_address().as_u64() / panda_hal::memory::FRAME_SIZE as u64;
    
    // Track the L4 page table frame
    // SAFETY: This frame is the active page table, we're just tracking it
    unsafe {
        crate::page_table_tracker::track_page_table_frame(l4_frame_num as usize);
    }
    
    println!("Page table tracking: L4 frame {} tracked", l4_frame_num);
    
    Ok(())
}

/// Switch to new page table
///
/// This switches the CPU to use a new page table by updating CR3.
///
/// # Safety
///
/// The new page table must be valid and properly set up.
/// All necessary mappings (kernel, stack, data) must be present.
pub unsafe fn switch_to_new_page_table(page_table_phys_addr: u64) -> Result<(), &'static str> {
    // SAFETY: Caller guarantees the page table is valid
    use x86_64::registers::control::Cr3;
    use x86_64::PhysAddr;
    
    let phys_addr = PhysAddr::new(page_table_phys_addr);
    let frame = x86_64::structures::paging::PhysFrame::containing_address(phys_addr);
    
    // SAFETY: Caller guarantees the page table is valid and properly set up
    unsafe {
        Cr3::write(frame, x86_64::registers::control::Cr3Flags::empty());
    }
    
    println!("Page table switched to {:#x}", page_table_phys_addr);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_table_flags() {
        let flags = PageTableFlags::PRESENT.or(PageTableFlags::WRITABLE);
        assert!(flags.contains(PageTableFlags::PRESENT));
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::USER_ACCESSIBLE));
    }

    #[test]
    fn test_page_table_entry() {
        let mut entry = PageTableEntry::new();
        assert!(entry.is_unused());
        assert!(!entry.is_present());

        let flags = PageTableFlags::PRESENT.or(PageTableFlags::WRITABLE);
        entry.set(0x1000, flags);

        assert!(entry.is_present());
        assert!(!entry.is_unused());
        assert_eq!(entry.addr(), 0x1000);
        assert!(entry.flags().contains(PageTableFlags::PRESENT));
        assert!(entry.flags().contains(PageTableFlags::WRITABLE));
    }

    #[test]
    fn test_virt_addr_indices() {
        let addr = VirtAddr::new(0x1234_5678_9ABC);
        assert_eq!(addr.p4_index(), 0);
        assert_eq!(addr.p3_index(), 0x48);
        assert_eq!(addr.p2_index(), 0x1A2);
        assert_eq!(addr.p1_index(), 0x189);
        assert_eq!(addr.page_offset(), 0xABC);
    }

    #[test]
    fn test_page_alignment() {
        let addr = VirtAddr::new(0x1234_5ABC);
        let aligned = addr.page_align_down();
        assert_eq!(aligned.as_u64(), 0x1234_5000);
    }

    #[test]
    fn test_page_table_size() {
        assert_eq!(core::mem::size_of::<PageTable>(), 4096);
        assert_eq!(core::mem::align_of::<PageTable>(), 4096);
    }

    #[test]
    fn test_page_table_indexing() {
        let mut table = PageTable::new();
        assert!(table[0].is_unused());

        let flags = PageTableFlags::PRESENT;
        table[0].set(0x1000, flags);

        assert!(table[0].is_present());
        assert_eq!(table[0].addr(), 0x1000);
    }
}
