//! Linker symbols and kernel memory boundaries
//!
//! This module provides access to linker-defined symbols that mark
//! kernel section boundaries in physical and virtual memory.
//!
//! ## Invariants
//!
//! - Linker symbols are defined by the linker script or build process
//! - Physical addresses are used for frame reservation
//! - Virtual addresses are used for kernel mapping

/// Higher-half kernel virtual base address
/// All kernel code and data is mapped starting from this address
pub const KERNEL_VIRT_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Kernel physical load address (where bootloader loads the kernel)
/// This is typically around 1-4 MiB in physical memory
pub const KERNEL_PHYS_BASE: u64 = 0x0010_0000; // 1 MiB

// External linker symbols
// These are defined by the linker but we need to declare them as extern
// to access them from Rust code.
extern "C" {
    // Kernel text (code) section boundaries
    static __text_start: u8;
    static __text_end: u8;

    // Read-only data section boundaries
    static __rodata_start: u8;
    static __rodata_end: u8;

    // Initialized data section boundaries
    static __data_start: u8;
    static __data_end: u8;

    // Uninitialized data (BSS) section boundaries
    static __bss_start: u8;
    static __bss_end: u8;
}

/// Get kernel physical start address
///
/// This is the start of the kernel image in physical memory,
/// used for frame reservation to prevent allocating over kernel code/data.
///
/// Falls back to estimated address if linker symbols unavailable.
pub fn kernel_phys_start() -> u64 {
    // Try to use linker symbols if available
    // For now, use conservative estimate
    // The bootloader loads kernel around 1-4 MiB typically
    KERNEL_PHYS_BASE
}

/// Get kernel physical end address
///
/// This is the end of the kernel image in physical memory,
/// used for frame reservation to prevent allocating over kernel code/data.
///
/// Falls back to estimated address if linker symbols unavailable.
pub fn kernel_phys_end() -> u64 {
    // Conservative estimate: Reserve up to 8 MiB for kernel
    // This covers kernel code, data, BSS, and bootloader structures
    // TODO: Once linker symbols are properly wired, compute exact size
    //       from __bss_end - __text_start + KERNEL_PHYS_BASE
    8 * 1024 * 1024 // 8 MiB
}

/// Get text section start (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn text_start() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__text_start) as u64
}

/// Get text section end (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn text_end() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__text_end) as u64
}

/// Get rodata section start (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn rodata_start() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__rodata_start) as u64
}

/// Get rodata section end (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn rodata_end() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__rodata_end) as u64
}

/// Get data section start (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn data_start() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__data_start) as u64
}

/// Get data section end (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn data_end() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__data_end) as u64
}

/// Get BSS section start (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn bss_start() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__bss_start) as u64
}

/// Get BSS section end (virtual address)
///
/// # Safety
///
/// This accesses an extern static symbol defined by the linker.
/// The address is only valid if the linker script defines it.
#[allow(dead_code)]
pub unsafe fn bss_end() -> u64 {
    // SAFETY: Caller guarantees linker symbols are defined
    core::ptr::addr_of!(__bss_end) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_constants() {
        // Verify constants are reasonable
        assert!(KERNEL_VIRT_BASE > 0xFFFF_0000_0000_0000);
        assert!(KERNEL_PHYS_BASE > 0);
        assert!(KERNEL_PHYS_BASE < 0x1000_0000); // Less than 256 MiB
    }

    #[test]
    fn test_kernel_boundaries() {
        let start = kernel_phys_start();
        let end = kernel_phys_end();

        assert!(start < end, "Kernel start must be before end");
        assert!(start >= KERNEL_PHYS_BASE);
        assert!(end > start);
    }

    #[test]
    fn test_kernel_size_reasonable() {
        let start = kernel_phys_start();
        let end = kernel_phys_end();
        let size = end - start;

        // Kernel should be at least 1 page and less than 128 MiB
        assert!(size >= 4096);
        assert!(size < 128 * 1024 * 1024);
    }
}
