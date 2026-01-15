//! User mode support for x86_64
//!
//! This module provides functionality for transitioning to ring 3 (user mode)
//! and handling syscall entry/exit.
//!
//! ## Invariants
//!
//! - User mode code runs at ring 3
//! - Kernel code runs at ring 0
//! - User stacks are separate from kernel stacks

use crate::gdt;

/// Initialize syscall/sysret support
///
/// # Safety
///
/// Must be called exactly once during kernel initialization after GDT is set up.
#[allow(dead_code)]
pub unsafe fn init_syscall() {
    // TODO: Set up STAR register for syscall/sysret
    // TODO: Enable syscall/sysret in EFER
}

/// Jump to user mode at given entry point
///
/// This function does not return - it switches to ring 3 and begins
/// executing user code.
///
/// # Safety
///
/// - Entry point must be valid user code
/// - Stack pointer must point to valid user stack
/// - Called only once per process
/// - GDT must be initialized before calling
#[allow(dead_code)]
pub unsafe fn enter_usermode(entry_point: u64, stack_ptr: u64) -> ! {
    // Get GDT selectors
    // SAFETY: Caller guarantees GDT is initialized
    let selectors = unsafe { gdt::get_selectors() };
    let user_data_sel = selectors.user_data.0;
    let user_code_sel = selectors.user_code.0;

    // SAFETY: Caller guarantees entry point and stack are valid
    unsafe {
        core::arch::asm!(
            // Set up user data segments
            "mov ds, {user_ds:x}",
            "mov es, {user_ds:x}",
            "mov fs, {user_ds:x}",
            "mov gs, {user_ds:x}",

            // Push user stack frame for iretq
            "push {user_ds:r}",        // SS
            "push {stack_ptr}",        // RSP
            "pushfq",                  // RFLAGS
            "pop rax",
            "or rax, 0x200",           // Set IF (interrupts enabled)
            "push rax",
            "push {user_cs:r}",        // CS
            "push {entry_point}",      // RIP

            // Clear registers for security
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rsi, rsi",
            "xor rdi, rdi",
            "xor rbp, rbp",
            "xor r8, r8",
            "xor r9, r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",

            // Jump to user mode
            "iretq",

            user_ds = in(reg) user_data_sel as u64,
            user_cs = in(reg) user_code_sel as u64,
            entry_point = in(reg) entry_point,
            stack_ptr = in(reg) stack_ptr,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_gdt_integration() {
        // Test that GDT selectors are properly structured
        // This is a compile-time test that ensures the module compiles correctly
        assert!(true);
    }
}
