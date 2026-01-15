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

/// GDT selector for kernel code segment
const KERNEL_CS: u16 = 0x08;

/// GDT selector for kernel data segment  
const KERNEL_DS: u16 = 0x10;

/// GDT selector for user code segment (ring 3)
const USER_CS: u16 = 0x18 | 3; // RPL = 3

/// GDT selector for user data segment (ring 3)
const USER_DS: u16 = 0x20 | 3; // RPL = 3

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
#[allow(dead_code)]
pub unsafe fn enter_usermode(entry_point: u64, stack_ptr: u64) -> ! {
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

            user_ds = in(reg) USER_DS as u64,
            user_cs = in(reg) USER_CS as u64,
            entry_point = in(reg) entry_point,
            stack_ptr = in(reg) stack_ptr,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_constants() {
        // Verify selectors have correct privilege levels
        assert_eq!(USER_CS & 3, 3); // Ring 3
        assert_eq!(USER_DS & 3, 3); // Ring 3
        assert_eq!(KERNEL_CS & 3, 0); // Ring 0
        assert_eq!(KERNEL_DS & 3, 0); // Ring 0
    }
}
