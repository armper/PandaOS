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

/// Page size constant
const PAGE_SIZE: u64 = 4096;

/// Kernel stack top (virtual address, grows down)
pub const KERNEL_STACK_TOP: u64 = 0xFFFF_FFFF_8000_0000;

/// Default kernel stack size in pages
pub const KERNEL_STACK_PAGES: usize = 4;

/// Create a new user page table with kernel mappings
///
/// Creates a fresh L4 page table and copies kernel mappings from the current
/// page table. This ensures kernel code remains accessible after switching.
///
/// # Safety
///
/// Frame allocator must be initialized.
#[allow(clippy::cast_ptr_alignment)]
pub unsafe fn create_user_page_table() -> Result<u64, &'static str> {
    // Allocate a new L4 page table
    // SAFETY: Caller guarantees frame allocator is initialized
    let l4_frame = unsafe {
        crate::page_table_tracker::allocate_page_table_frame()
            .ok_or("Failed to allocate L4 page table")?
    };

    let l4_phys_addr = l4_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;

    // Zero out the new page table
    // NOTE: This assumes identity mapping of physical memory
    // TODO: Use proper virtual address translation
    // SAFETY: We just allocated this frame and assume identity mapping
    let l4_table = unsafe { &mut *(l4_phys_addr as *mut PageTable) };
    l4_table.zero();

    // Copy kernel mappings from current page table (upper half)
    // SAFETY: Reading CR3 is safe
    use x86_64::registers::control::Cr3;
    let (current_l4_frame, _) = Cr3::read();
    let current_l4_phys = current_l4_frame.start_address().as_u64();

    // NOTE: This assumes identity mapping of physical memory
    // TODO: Use proper virtual address translation
    // SAFETY: Current L4 is valid and we assume identity mapping
    let current_l4 = unsafe { &*(current_l4_phys as *const PageTable) };

    // Copy upper half entries (256-511) for kernel space
    for i in 256..512 {
        l4_table[i] = current_l4[i];
    }

    Ok(l4_phys_addr)
}

/// Map a page in the page table
///
/// Maps a virtual page to a physical frame with given flags.
/// Creates intermediate page tables as needed.
///
/// # Safety
///
/// - page_table_phys must point to a valid L4 page table
/// - Frame allocator must be initialized
/// - Physical frame must be valid
#[allow(clippy::cast_ptr_alignment)]
pub unsafe fn map_page(
    page_table_phys: u64,
    virt_addr: VirtAddr,
    phys_addr: PhysAddr,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    // SAFETY: Caller guarantees page table is valid
    // NOTE: All pointer casts assume identity mapping of physical memory
    // TODO: Use proper virtual address translation
    let l4_table = unsafe { &mut *(page_table_phys as *mut PageTable) };

    let p4_index = virt_addr.p4_index();
    let p3_index = virt_addr.p3_index();
    let p2_index = virt_addr.p2_index();
    let p1_index = virt_addr.p1_index();

    // Get or create L3 table
    if !l4_table[p4_index].is_present() {
        // SAFETY: Caller guarantees frame allocator is initialized
        let p3_frame = unsafe {
            crate::page_table_tracker::allocate_page_table_frame()
                .ok_or("Failed to allocate L3 page table")?
        };
        let p3_phys = p3_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;

        // SAFETY: Frame was just allocated
        let p3_table = unsafe { &mut *(p3_phys as *mut PageTable) };
        p3_table.zero();

        let p3_flags = PageTableFlags::PRESENT
            .or(PageTableFlags::WRITABLE)
            .or(PageTableFlags::USER_ACCESSIBLE);
        l4_table[p4_index].set(p3_phys, p3_flags);
    }

    // SAFETY: Entry is now present
    let l3_table = unsafe { &mut *(l4_table[p4_index].addr() as *mut PageTable) };

    // Get or create L2 table
    if !l3_table[p3_index].is_present() {
        // SAFETY: Caller guarantees frame allocator is initialized
        let p2_frame = unsafe {
            crate::page_table_tracker::allocate_page_table_frame()
                .ok_or("Failed to allocate L2 page table")?
        };
        let p2_phys = p2_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;

        // SAFETY: Frame was just allocated
        let p2_table = unsafe { &mut *(p2_phys as *mut PageTable) };
        p2_table.zero();

        let p2_flags = PageTableFlags::PRESENT
            .or(PageTableFlags::WRITABLE)
            .or(PageTableFlags::USER_ACCESSIBLE);
        l3_table[p3_index].set(p2_phys, p2_flags);
    }

    // SAFETY: Entry is now present
    let l2_table = unsafe { &mut *(l3_table[p3_index].addr() as *mut PageTable) };

    // Get or create L1 table
    if !l2_table[p2_index].is_present() {
        // SAFETY: Caller guarantees frame allocator is initialized
        let p1_frame = unsafe {
            crate::page_table_tracker::allocate_page_table_frame()
                .ok_or("Failed to allocate L1 page table")?
        };
        let p1_phys = p1_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;

        // SAFETY: Frame was just allocated
        let p1_table = unsafe { &mut *(p1_phys as *mut PageTable) };
        p1_table.zero();

        let p1_flags = PageTableFlags::PRESENT
            .or(PageTableFlags::WRITABLE)
            .or(PageTableFlags::USER_ACCESSIBLE);
        l2_table[p2_index].set(p1_phys, p1_flags);
    }

    // SAFETY: Entry is now present
    let l1_table = unsafe { &mut *(l2_table[p2_index].addr() as *mut PageTable) };

    // Check if the page is already mapped
    // This handles cases where:
    // 1. Bootloader pre-maps kernel pages and we try to map them again
    // 2. ELF loading maps the same page twice (e.g., adjacent segments sharing page boundaries)
    // 3. Kernel stack or heap initialization overlaps with existing mappings
    let already_mapped = l1_table[p1_index].is_present();
    
    // Debug logging for mapping operation
    println!(
        "map_page: VA={:#x}, phys_frame={:#x}, flags={:#x}, already_mapped={}",
        virt_addr.as_u64(),
        phys_addr.as_u64(),
        flags.bits(),
        already_mapped
    );

    if already_mapped {
        let existing_addr = l1_table[p1_index].addr();
        let new_addr = phys_addr.page_align_down().as_u64();

        // If already mapped to the same frame, this is an idempotent operation - allow it
        // This is safe and correct: we're just ensuring the mapping and potentially updating flags
        if existing_addr == new_addr {
            println!(
                "map_page: Idempotent mapping detected - page {:#x} already mapped to frame {:#x}, updating flags",
                virt_addr.as_u64(),
                existing_addr
            );
            // Update flags in case they changed
            l1_table[p1_index].set(phys_addr.as_u64(), flags);
            return Ok(());
        }

        // If mapped to a different frame, this is an error - we don't support remapping
        panic!(
            "Page already mapped to different frame: VA={:#x}, existing_frame={:#x}, requested_frame={:#x}",
            virt_addr.as_u64(),
            existing_addr,
            new_addr
        );
    }

    // Map the page for the first time
    l1_table[p1_index].set(phys_addr.as_u64(), flags);

    Ok(())
}

/// Unmap a single page from a page table
///
/// Returns the physical address that was unmapped, or an error if the page was not mapped.
///
/// # Safety
///
/// - Page table must be valid and properly initialized
/// - TLB must be flushed after unmapping
#[allow(clippy::cast_ptr_alignment)]
pub unsafe fn unmap_page(
    page_table_phys: u64,
    virt_addr: VirtAddr,
) -> Result<PhysAddr, &'static str> {
    // SAFETY: Caller guarantees page table is valid
    // NOTE: All pointer casts assume identity mapping of physical memory
    let l4_table = unsafe { &mut *(page_table_phys as *mut PageTable) };

    let p4_index = virt_addr.p4_index();
    let p3_index = virt_addr.p3_index();
    let p2_index = virt_addr.p2_index();
    let p1_index = virt_addr.p1_index();

    // Walk the page table hierarchy
    if !l4_table[p4_index].is_present() {
        return Err("Page not mapped (L4 entry missing)");
    }

    // SAFETY: Entry is present
    let l3_table = unsafe { &mut *(l4_table[p4_index].addr() as *mut PageTable) };

    if !l3_table[p3_index].is_present() {
        return Err("Page not mapped (L3 entry missing)");
    }

    // SAFETY: Entry is present
    let l2_table = unsafe { &mut *(l3_table[p3_index].addr() as *mut PageTable) };

    if !l2_table[p2_index].is_present() {
        return Err("Page not mapped (L2 entry missing)");
    }

    // SAFETY: Entry is present
    let l1_table = unsafe { &mut *(l2_table[p2_index].addr() as *mut PageTable) };

    if !l1_table[p1_index].is_present() {
        return Err("Page not mapped (L1 entry missing)");
    }

    // Get the physical address before clearing
    let phys_addr = PhysAddr::new(l1_table[p1_index].addr());

    // Clear the entry
    l1_table[p1_index].clear();

    // Flush TLB for this address
    // SAFETY: We're unmapping a page we just verified exists
    use x86_64::instructions::tlb;
    tlb::flush(x86_64::VirtAddr::new(virt_addr.as_u64()));

    Ok(phys_addr)
}

/// Allocate and map user stack
///
/// Allocates physical frames and maps them to a user stack region.
/// Stack grows down from the specified top address.
///
/// # Safety
///
/// - page_table_phys must point to a valid L4 page table
/// - Frame allocator must be initialized
pub unsafe fn allocate_user_stack(
    page_table_phys: u64,
    stack_top: u64,
    num_pages: usize,
) -> Result<(), &'static str> {
    let flags = PageTableFlags::PRESENT
        .or(PageTableFlags::WRITABLE)
        .or(PageTableFlags::USER_ACCESSIBLE)
        .or(PageTableFlags::NO_EXECUTE);

    for i in 0..num_pages {
        // SAFETY: Caller guarantees frame allocator is initialized
        let frame =
            unsafe { crate::memory::allocate_frame().ok_or("Failed to allocate stack frame")? };
        let phys_addr = PhysAddr::new(frame as u64 * panda_hal::memory::FRAME_SIZE as u64);

        // Stack grows down, so subtract from top
        let virt_addr = VirtAddr::new(stack_top - ((i + 1) as u64 * PAGE_SIZE));

        // SAFETY: Caller guarantees page table is valid
        unsafe {
            map_page(page_table_phys, virt_addr, phys_addr, flags)?;
        }

        // Zero the stack page
        // NOTE: This assumes identity mapping of physical memory
        // TODO: Use proper virtual address translation
        // SAFETY: We just mapped this page and assume identity mapping
        let page_ptr = phys_addr.as_u64() as *mut u8;
        unsafe {
            core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE as usize);
        }
    }

    Ok(())
}

/// Allocate and map kernel stack
///
/// Allocates physical frames and maps them to a kernel stack region.
/// Stack grows down from the specified top address.
///
/// # Safety
///
/// - page_table_phys must point to a valid L4 page table
/// - Frame allocator must be initialized
pub unsafe fn allocate_kernel_stack(
    page_table_phys: u64,
    stack_top: u64,
    num_pages: usize,
) -> Result<(), &'static str> {
    let flags = PageTableFlags::PRESENT.or(PageTableFlags::WRITABLE).or(PageTableFlags::NO_EXECUTE);

    for i in 0..num_pages {
        // SAFETY: Caller guarantees frame allocator is initialized
        let frame =
            unsafe { crate::memory::allocate_frame().ok_or("Failed to allocate stack frame")? };
        let phys_addr = PhysAddr::new(frame as u64 * panda_hal::memory::FRAME_SIZE as u64);

        // Stack grows down, so subtract from top
        let virt_addr = VirtAddr::new(stack_top - ((i + 1) as u64 * PAGE_SIZE));

        // SAFETY: Caller guarantees page table is valid
        unsafe {
            map_page(page_table_phys, virt_addr, phys_addr, flags)?;
        }

        // Zero the stack page
        // NOTE: This assumes identity mapping of physical memory
        // TODO: Use proper virtual address translation
        // SAFETY: We just mapped this page and assume identity mapping
        let page_ptr = phys_addr.as_u64() as *mut u8;
        unsafe {
            core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE as usize);
        }
    }

    Ok(())
}

/// Free a process address space and optionally its kernel stack frames.
///
/// # Safety
///
/// - page_table_phys must point to a valid L4 page table.
/// - Caller must ensure no further allocations occur before switching CR3.
pub unsafe fn free_process_address_space(
    page_table_phys: u64,
    free_kernel_stack: bool,
) -> Result<(), &'static str> {
    // SAFETY: Caller guarantees the L4 page table is valid.
    let l4_table = unsafe { &mut *(page_table_phys as *mut PageTable) };

    for p4_index in 0..256 {
        if !l4_table[p4_index].is_present() {
            continue;
        }

        let l3_phys = l4_table[p4_index].addr();
        // SAFETY: L3 table address is from a present L4 entry.
        let l3_table = unsafe { &mut *(l3_phys as *mut PageTable) };

        for p3_index in 0..ENTRY_COUNT {
            if !l3_table[p3_index].is_present() {
                continue;
            }

            let l2_phys = l3_table[p3_index].addr();
            // SAFETY: L2 table address is from a present L3 entry.
            let l2_table = unsafe { &mut *(l2_phys as *mut PageTable) };

            for p2_index in 0..ENTRY_COUNT {
                if !l2_table[p2_index].is_present() {
                    continue;
                }

                if l2_table[p2_index].flags().contains(PageTableFlags::HUGE_PAGE) {
                    return Err("Huge pages not supported in cleanup");
                }

                let l1_phys = l2_table[p2_index].addr();
                // SAFETY: L1 table address is from a present L2 entry.
                let l1_table = unsafe { &mut *(l1_phys as *mut PageTable) };

                for p1_index in 0..ENTRY_COUNT {
                    if !l1_table[p1_index].is_present() {
                        continue;
                    }

                    let frame =
                        (l1_table[p1_index].addr() / panda_hal::memory::FRAME_SIZE as u64) as usize;
                    // SAFETY: Frame was allocated via the global allocator.
                    unsafe {
                        crate::memory::deallocate_frame(frame);
                    }
                    l1_table[p1_index].clear();
                }

                let l1_frame = (l1_phys / panda_hal::memory::FRAME_SIZE as u64) as usize;
                // SAFETY: L1 table frame was allocated via the global allocator.
                unsafe {
                    crate::memory::deallocate_frame(l1_frame);
                }
                l2_table[p2_index].clear();
            }

            let l2_frame = (l2_phys / panda_hal::memory::FRAME_SIZE as u64) as usize;
            // SAFETY: L2 table frame was allocated via the global allocator.
            unsafe {
                crate::memory::deallocate_frame(l2_frame);
            }
            l3_table[p3_index].clear();
        }

        let l3_frame = (l3_phys / panda_hal::memory::FRAME_SIZE as u64) as usize;
        // SAFETY: L3 table frame was allocated via the global allocator.
        unsafe {
            crate::memory::deallocate_frame(l3_frame);
        }
        l4_table[p4_index].clear();
    }

    if free_kernel_stack {
        // SAFETY: Caller guarantees the page table is valid.
        unsafe {
            free_kernel_stack_pages(page_table_phys)?;
        }
    }

    let l4_frame = (page_table_phys / panda_hal::memory::FRAME_SIZE as u64) as usize;
    // SAFETY: L4 table frame was allocated via the global allocator.
    unsafe {
        crate::memory::deallocate_frame(l4_frame);
    }

    Ok(())
}

#[allow(clippy::cast_ptr_alignment)]
unsafe fn lookup_phys_addr(page_table_phys: u64, virt: VirtAddr) -> Option<u64> {
    // SAFETY: Caller guarantees the L4 page table is valid.
    let l4_table = unsafe { &*(page_table_phys as *const PageTable) };
    let l4_entry = l4_table[virt.p4_index()];
    if !l4_entry.is_present() {
        return None;
    }

    // SAFETY: L3 table address is from a present L4 entry.
    let l3_table = unsafe { &*(l4_entry.addr() as *const PageTable) };
    let l3_entry = l3_table[virt.p3_index()];
    if !l3_entry.is_present() {
        return None;
    }

    // SAFETY: L2 table address is from a present L3 entry.
    let l2_table = unsafe { &*(l3_entry.addr() as *const PageTable) };
    let l2_entry = l2_table[virt.p2_index()];
    if !l2_entry.is_present() || l2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return None;
    }

    // SAFETY: L1 table address is from a present L2 entry.
    let l1_table = unsafe { &*(l2_entry.addr() as *const PageTable) };
    let l1_entry = l1_table[virt.p1_index()];
    if !l1_entry.is_present() {
        return None;
    }

    Some(l1_entry.addr())
}

#[allow(clippy::cast_ptr_alignment)]
unsafe fn free_kernel_stack_pages(page_table_phys: u64) -> Result<(), &'static str> {
    for i in 0..KERNEL_STACK_PAGES {
        let vaddr = KERNEL_STACK_TOP - ((i + 1) as u64 * PAGE_SIZE);
        let virt = VirtAddr::new(vaddr);

        // SAFETY: Caller guarantees the page table is valid.
        let phys = unsafe {
            lookup_phys_addr(page_table_phys, virt).ok_or("Kernel stack page not mapped")?
        };

        let frame = (phys / panda_hal::memory::FRAME_SIZE as u64) as usize;
        // SAFETY: Frame was allocated via the global allocator.
        unsafe {
            crate::memory::deallocate_frame(frame);
        }
    }

    Ok(())
}

/// Clone a user address space for fork()
///
/// Creates a new page table with copied user mappings (lower half).
/// Kernel mappings (upper half) are shared (copied from source).
///
/// # Safety
///
/// - parent_page_table_phys must point to a valid L4 page table
/// - Frame allocator must be initialized
#[allow(clippy::cast_ptr_alignment)]
pub unsafe fn clone_user_address_space(parent_page_table_phys: u64) -> Result<u64, &'static str> {
    // Create a new user page table (copies kernel mappings)
    // SAFETY: Caller guarantees frame allocator is initialized
    let child_pt = unsafe { create_user_page_table()? };

    // SAFETY: Both page tables are valid
    let parent_l4 = unsafe { &*(parent_page_table_phys as *const PageTable) };
    let child_l4 = unsafe { &mut *(child_pt as *mut PageTable) };

    // Walk the parent's user space (lower half, entries 0-255)
    for p4_index in 0..256 {
        if !parent_l4[p4_index].is_present() {
            continue;
        }

        let parent_l3_phys = parent_l4[p4_index].addr();
        // SAFETY: L3 address is from a present L4 entry
        let parent_l3 = unsafe { &*(parent_l3_phys as *const PageTable) };

        // Create or get child L3 table
        if !child_l4[p4_index].is_present() {
            // SAFETY: Caller guarantees frame allocator is initialized
            let l3_frame = unsafe {
                crate::page_table_tracker::allocate_page_table_frame()
                    .ok_or("Failed to allocate L3 for clone")?
            };
            let l3_phys = l3_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;
            // SAFETY: Frame was just allocated
            let l3_table = unsafe { &mut *(l3_phys as *mut PageTable) };
            l3_table.zero();

            let flags = PageTableFlags::PRESENT
                .or(PageTableFlags::WRITABLE)
                .or(PageTableFlags::USER_ACCESSIBLE);
            child_l4[p4_index].set(l3_phys, flags);
        }

        let child_l3_phys = child_l4[p4_index].addr();
        // SAFETY: L3 is now present
        let child_l3 = unsafe { &mut *(child_l3_phys as *mut PageTable) };

        for p3_index in 0..ENTRY_COUNT {
            if !parent_l3[p3_index].is_present() {
                continue;
            }

            let parent_l2_phys = parent_l3[p3_index].addr();
            // SAFETY: L2 address is from a present L3 entry
            let parent_l2 = unsafe { &*(parent_l2_phys as *const PageTable) };

            // Create child L2 table
            if !child_l3[p3_index].is_present() {
                // SAFETY: Caller guarantees frame allocator is initialized
                let l2_frame = unsafe {
                    crate::page_table_tracker::allocate_page_table_frame()
                        .ok_or("Failed to allocate L2 for clone")?
                };
                let l2_phys = l2_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;
                // SAFETY: Frame was just allocated
                let l2_table = unsafe { &mut *(l2_phys as *mut PageTable) };
                l2_table.zero();

                let flags = PageTableFlags::PRESENT
                    .or(PageTableFlags::WRITABLE)
                    .or(PageTableFlags::USER_ACCESSIBLE);
                child_l3[p3_index].set(l2_phys, flags);
            }

            let child_l2_phys = child_l3[p3_index].addr();
            // SAFETY: L2 is now present
            let child_l2 = unsafe { &mut *(child_l2_phys as *mut PageTable) };

            for p2_index in 0..ENTRY_COUNT {
                if !parent_l2[p2_index].is_present() {
                    continue;
                }

                if parent_l2[p2_index].flags().contains(PageTableFlags::HUGE_PAGE) {
                    return Err("Huge pages not supported in fork");
                }

                let parent_l1_phys = parent_l2[p2_index].addr();
                // SAFETY: L1 address is from a present L2 entry
                let parent_l1 = unsafe { &*(parent_l1_phys as *const PageTable) };

                // Create child L1 table
                if !child_l2[p2_index].is_present() {
                    // SAFETY: Caller guarantees frame allocator is initialized
                    let l1_frame = unsafe {
                        crate::page_table_tracker::allocate_page_table_frame()
                            .ok_or("Failed to allocate L1 for clone")?
                    };
                    let l1_phys = l1_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;
                    // SAFETY: Frame was just allocated
                    let l1_table = unsafe { &mut *(l1_phys as *mut PageTable) };
                    l1_table.zero();

                    let flags = PageTableFlags::PRESENT
                        .or(PageTableFlags::WRITABLE)
                        .or(PageTableFlags::USER_ACCESSIBLE);
                    child_l2[p2_index].set(l1_phys, flags);
                }

                let child_l1_phys = child_l2[p2_index].addr();
                // SAFETY: L1 is now present
                let child_l1 = unsafe { &mut *(child_l1_phys as *mut PageTable) };

                // Copy each mapped page
                for p1_index in 0..ENTRY_COUNT {
                    if !parent_l1[p1_index].is_present() {
                        continue;
                    }

                    let parent_phys = parent_l1[p1_index].addr();
                    let flags = parent_l1[p1_index].flags();

                    // Allocate a new physical frame for the child
                    // SAFETY: Caller guarantees frame allocator is initialized
                    let child_frame = unsafe {
                        crate::memory::allocate_frame()
                            .ok_or("Failed to allocate frame for page copy")?
                    };
                    let child_phys = child_frame as u64 * panda_hal::memory::FRAME_SIZE as u64;

                    // Copy page contents from parent to child
                    // SAFETY: Both frames are valid and identity-mapped
                    unsafe {
                        let src = parent_phys as *const u8;
                        let dst = child_phys as *mut u8;
                        core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE as usize);
                    }

                    // Map the new frame in child's page table
                    child_l1[p1_index].set(child_phys, flags);
                }
            }
        }
    }

    Ok(child_pt)
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
