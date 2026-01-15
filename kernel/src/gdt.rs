//! Global Descriptor Table (GDT) for x86_64
//!
//! The GDT defines segments and privilege levels for the x86_64 architecture.
//! While x86_64 uses paging primarily, the GDT is still required for:
//! - Task State Segment (TSS) for interrupt handling
//! - Switching between kernel and user mode
//! - Syscall/sysret instructions
//!
//! ## Invariants
//!
//! - GDT must be loaded before interrupts are enabled
//! - TSS must be loaded before using interrupt stack switching
//! - Segment selectors must match GDT layout

use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Index of the double fault interrupt stack in the TSS
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Size of the interrupt stack (16 KiB)
const STACK_SIZE: usize = 4096 * 4;

/// GDT and segment selectors
pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
    pub tss: SegmentSelector,
}

/// Global GDT instance (initialized once at boot)
static mut GDT: Option<GlobalDescriptorTable> = None;
static mut TSS: TaskStateSegment = TaskStateSegment::new();
static mut SELECTORS: Option<Selectors> = None;
static mut DOUBLE_FAULT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

/// Initialize the GDT and TSS
///
/// This must be called once during kernel initialization before interrupts
/// are enabled.
///
/// # Safety
///
/// Must be called exactly once during boot, before enabling interrupts.
pub unsafe fn init() {
    // Set up TSS with interrupt stack for double fault
    // SAFETY: We're the only ones accessing TSS during init
    let tss = unsafe { &mut *core::ptr::addr_of_mut!(TSS) };
    let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(DOUBLE_FAULT_STACK) as *const u8);
    let stack_end = stack_start + STACK_SIZE as u64;
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = stack_end;

    // Create GDT with kernel and user segments
    // Order matters: kernel segments must come before user segments for syscall/sysret
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code = gdt.append(Descriptor::kernel_code_segment());
    let kernel_data = gdt.append(Descriptor::kernel_data_segment());
    let user_code = gdt.append(Descriptor::user_code_segment());
    let user_data = gdt.append(Descriptor::user_data_segment());
    let tss = gdt.append(Descriptor::tss_segment(tss));

    // SAFETY: We're storing the GDT in a static, which is fine for this use case
    // The GDT must remain valid for the lifetime of the program
    unsafe {
        *core::ptr::addr_of_mut!(GDT) = Some(gdt);
        *core::ptr::addr_of_mut!(SELECTORS) =
            Some(Selectors { kernel_code, kernel_data, user_code, user_data, tss });
    }

    // Load GDT
    // SAFETY: We just initialized GDT above
    if let Some(gdt) = unsafe { &*core::ptr::addr_of!(GDT) } {
        gdt.load();
    }

    // Load TSS
    // SAFETY: We just initialized selectors and the TSS is valid
    if let Some(selectors) = unsafe { &*core::ptr::addr_of!(SELECTORS) } {
        unsafe {
            x86_64::instructions::tables::load_tss(selectors.tss);
        }
    }
}

/// Get the current GDT selectors
///
/// # Safety
///
/// Must be called after GDT has been initialized via `init()`
pub unsafe fn get_selectors() -> &'static Selectors {
    // SAFETY: Caller guarantees GDT is initialized
    unsafe {
        (*core::ptr::addr_of!(SELECTORS))
            .as_ref()
            .expect("GDT not initialized - call gdt::init() first")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdt_constants() {
        assert_eq!(DOUBLE_FAULT_IST_INDEX, 0);
        assert_eq!(STACK_SIZE, 16384);
    }

    #[test]
    fn test_stack_size() {
        // Verify the double fault stack is properly sized
        assert!(STACK_SIZE >= 4096, "Stack too small");
        assert!(STACK_SIZE % 4096 == 0, "Stack not page-aligned");
    }
}
