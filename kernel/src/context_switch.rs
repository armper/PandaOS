//! Context switching implementation for x86_64
//!
//! This module provides the low-level context switching functionality needed
//! for preemptive multitasking. It includes assembly routines to save and
//! restore CPU state, as well as high-level functions to coordinate the switch.
//!
//! ## Safety
//!
//! Context switching is inherently unsafe as it manipulates CPU state directly.
//! All functions in this module must be called with:
//! - Interrupts disabled
//! - Valid process context pointers
//! - Valid page table addresses
//!
//! ## Invariants
//!
//! - Interrupts must be disabled during context switch
//! - Both processes must have valid contexts
//! - Page table switch must happen atomically with context switch
//! - Stack pointers must always point to valid memory

use crate::context::CpuContext;
use crate::process::Process;
use x86_64::registers::control::Cr3;
use x86_64::PhysAddr;

/// Switch from the current process to the next process
///
/// This function performs a complete context switch:
/// 1. Save the current process's CPU context
/// 2. Switch to the next process's page table (CR3)
/// 3. Restore the next process's CPU context
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Both current and next processes must have valid contexts
/// - Both processes must have valid page tables
/// - This function must not be called recursively
///
/// # Arguments
///
/// * `current` - The currently running process (whose context will be saved)
/// * `next` - The next process to run (whose context will be restored)
pub unsafe fn switch_to(current: &mut Process, next: &Process) {
    // SAFETY: Caller guarantees interrupts are disabled and processes are valid
    unsafe {
        // Save current process context
        save_context_to_process(current);

        // Switch page tables (CR3)
        switch_page_table(next.page_table_phys);

        // Restore next process context
        restore_context_from_process(next);
    }
}

/// Save the current CPU context to a process structure
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Process must have valid memory for context storage
/// - Current CPU state must be consistent
unsafe fn save_context_to_process(process: &mut Process) {
    // SAFETY: Caller guarantees process is valid and interrupts are disabled
    unsafe {
        save_context_asm(&mut process.context);
    }
}

/// Restore CPU context from a process structure
///
/// This function never returns - it jumps to the restored RIP.
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Process must have a valid, initialized context
/// - Context RSP must point to valid stack memory
/// - Context RIP must point to valid executable code
unsafe fn restore_context_from_process(process: &Process) -> ! {
    // SAFETY: Caller guarantees process has valid context and interrupts are disabled
    unsafe {
        restore_context_asm(&process.context);
    }
}

/// Switch the page table (CR3 register)
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Page table address must be a valid physical address
/// - Page table must be properly initialized with kernel mappings
unsafe fn switch_page_table(page_table_phys: u64) {
    // SAFETY: Caller guarantees page table address is valid
    let phys_addr = PhysAddr::new(page_table_phys);
    let (current_frame, _flags) = Cr3::read();

    // Only switch if different from current
    if current_frame.start_address() != phys_addr {
        unsafe {
            Cr3::write(
                x86_64::structures::paging::PhysFrame::containing_address(phys_addr),
                x86_64::registers::control::Cr3Flags::empty(),
            );
        }
    }
}

/// Save the current CPU context (assembly implementation)
///
/// This function saves all general-purpose registers, RIP, RSP, RFLAGS,
/// and segment selectors to the provided context structure.
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Context pointer must be valid and writable
#[inline(always)]
unsafe fn save_context_asm(ctx: *mut CpuContext) {
    // SAFETY: This assembly code saves registers to the context structure
    // The layout of CpuContext matches the order of saves/restores
    unsafe {
        core::arch::asm!(
            // Save general-purpose registers (in reverse order of CpuContext struct)
            "mov [rdi + 0x00], r15",
            "mov [rdi + 0x08], r14",
            "mov [rdi + 0x10], r13",
            "mov [rdi + 0x18], r12",
            "mov [rdi + 0x20], r11",
            "mov [rdi + 0x28], r10",
            "mov [rdi + 0x30], r9",
            "mov [rdi + 0x38], r8",
            "mov [rdi + 0x40], rbp",
            // Skip rdi (we're using it), save after
            "mov [rdi + 0x50], rsi",
            "mov [rdi + 0x58], rdx",
            "mov [rdi + 0x60], rcx",
            "mov [rdi + 0x68], rbx",
            "mov [rdi + 0x70], rax",

            // Save rdi
            "mov rax, rdi",
            "mov [rdi + 0x48], rax",

            // Save RIP (return address)
            "lea rax, [rip + 2f]",  // Address of the next instruction (label 2 forward)
            "mov [rdi + 0x78], rax",

            // Save RSP
            "mov rax, rsp",
            "mov [rdi + 0x80], rax",

            // Save RFLAGS
            "pushfq",
            "pop rax",
            "mov [rdi + 0x88], rax",

            // Save segment selectors
            "mov ax, cs",
            "movzx rax, ax",
            "mov [rdi + 0x90], rax",

            "mov ax, ss",
            "movzx rax, ax",
            "mov [rdi + 0x98], rax",

            "2:",  // Label for RIP to return to

            in("rdi") ctx,
            out("rax") _,
            options(nostack)
        );
    }
}

/// Restore CPU context (assembly implementation)
///
/// This function restores all general-purpose registers, RIP, RSP, RFLAGS,
/// and segment selectors from the provided context structure and jumps to
/// the restored RIP.
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Context must be properly initialized
/// - Context RSP must point to valid stack memory
/// - Context RIP must point to valid executable code
/// - All segment selectors must be valid
///
/// # Returns
///
/// This function never returns - it jumps to the restored RIP
#[inline(always)]
unsafe fn restore_context_asm(ctx: *const CpuContext) -> ! {
    // SAFETY: This assembly code restores registers from the context structure
    // and jumps to the saved RIP. It never returns.
    unsafe {
        core::arch::asm!(
            // Load context pointer into rdi
            "mov rdi, {ctx}",

            // Restore RFLAGS (must be done before other registers)
            "mov rax, [rdi + 0x88]",
            "push rax",
            "popfq",

            // Restore general-purpose registers
            "mov r15, [rdi + 0x00]",
            "mov r14, [rdi + 0x08]",
            "mov r13, [rdi + 0x10]",
            "mov r12, [rdi + 0x18]",
            "mov r11, [rdi + 0x20]",
            "mov r10, [rdi + 0x28]",
            "mov r9, [rdi + 0x30]",
            "mov r8, [rdi + 0x38]",
            "mov rbp, [rdi + 0x40]",
            // Skip rdi for now
            "mov rsi, [rdi + 0x50]",
            "mov rdx, [rdi + 0x58]",
            "mov rcx, [rdi + 0x60]",
            "mov rbx, [rdi + 0x68]",
            "mov rax, [rdi + 0x70]",

            // Restore RSP
            "mov rsp, [rdi + 0x80]",

            // Push RIP onto the new stack (for ret)
            "push qword ptr [rdi + 0x78]",

            // Finally restore rdi
            "mov rdi, [rdi + 0x48]",

            // Jump to restored RIP
            "ret",

            ctx = in(reg) ctx,
            options(noreturn)
        );
    }
}

/// Initialize context for first-time execution
///
/// This prepares a context for a process that has never been run before.
/// The context is set up so that when restored, it will jump to user mode
/// at the process's entry point.
///
/// # Arguments
///
/// * `process` - Process to initialize context for
///
/// # Returns
///
/// The process is updated with a properly initialized context
pub fn init_context_for_first_run(process: &mut Process) {
    // Get GDT selectors
    // SAFETY: GDT must be initialized before creating processes
    let selectors = unsafe { crate::gdt::get_selectors() };
    let user_cs = selectors.user_code.0 as u64;
    let user_ss = selectors.user_data.0 as u64;

    // Create a new user mode context
    process.context =
        CpuContext::new_user(process.entry_point, process.user_stack_ptr, user_cs, user_ss);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_offsets() {
        // Verify our manual offset calculations match the actual struct layout
        let ctx = CpuContext::zero();
        let ctx_ptr = &ctx as *const CpuContext as usize;

        // Check a few key offsets
        let r15_offset = &ctx.r15 as *const u64 as usize - ctx_ptr;
        let rax_offset = &ctx.rax as *const u64 as usize - ctx_ptr;
        let rip_offset = &ctx.rip as *const u64 as usize - ctx_ptr;
        let rsp_offset = &ctx.rsp as *const u64 as usize - ctx_ptr;

        assert_eq!(r15_offset, 0x00, "r15 offset should be 0x00");
        assert_eq!(rax_offset, 0x70, "rax offset should be 0x70");
        assert_eq!(rip_offset, 0x78, "rip offset should be 0x78");
        assert_eq!(rsp_offset, 0x80, "rsp offset should be 0x80");
    }
}
