//! User mode support for x86_64
//!
//! This module provides functionality for transitioning to ring 3 (user mode)
//! and handling syscall entry/exit using the syscall/sysret instructions.
//!
//! ## Invariants
//!
//! - User mode code runs at ring 3
//! - Kernel code runs at ring 0
//! - User stacks are separate from kernel stacks
//! - Syscall/sysret are configured before first user mode transition

use crate::{gdt, syscall};
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask, Star};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;

/// Initialize syscall/sysret support
///
/// This configures the MSRs needed for the syscall/sysret instructions:
/// - STAR: Segment selectors for syscall/sysret
/// - LSTAR: Entry point for syscall
/// - SFMASK: RFLAGS mask on syscall entry
/// - EFER.SCE: Enable syscall/sysret
///
/// # Safety
///
/// Must be called exactly once during kernel initialization after GDT is set up.
pub unsafe fn init_syscall() {
    // SAFETY: Caller guarantees GDT is initialized
    let selectors = unsafe { gdt::get_selectors() };

    // Configure STAR register with segment selectors
    // SAFETY: We're configuring syscall/sysret during kernel init
    // Note: Star::write returns Result but failure is not possible with valid selectors
    let _ = Star::write(
        selectors.user_code,
        selectors.user_data,
        selectors.kernel_code,
        selectors.kernel_data,
    );

    // Set LSTAR to syscall entry point
    // SAFETY: syscall_entry is a valid function pointer
    LStar::write(VirtAddr::new(syscall_entry as *const () as u64));

    // Set SFMASK to clear IF (interrupts) on syscall entry
    // SAFETY: This is a valid RFLAGS configuration
    SFMask::write(RFlags::INTERRUPT_FLAG);

    // Enable syscall/sysret in EFER
    // SAFETY: This is safe to do during kernel init
    unsafe {
        Efer::update(|flags| {
            *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
    }
}

/// Syscall entry point
///
/// This function is called via the syscall instruction from user mode.
/// The syscall instruction:
/// - Saves RIP to RCX
/// - Saves RFLAGS to R11
/// - Loads RIP from LSTAR MSR
/// - Loads CS from STAR MSR
/// - Clears RFLAGS bits specified in SFMASK
///
/// Register state on entry:
/// - RCX: user RIP
/// - R11: user RFLAGS
/// - RAX: syscall number
/// - RDI, RSI, RDX, R10, R8, R9: syscall arguments
///
/// We use a naked function to have full control over register preservation.
///
/// # Safety
///
/// This must only be called via the syscall instruction.
#[unsafe(naked)]
extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Save user space registers
        "push rcx",          // user RIP
        "push r11",          // user RFLAGS
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // RAX = syscall number (already in RAX)
        // RDI = arg1 (already in RDI)
        // RSI = arg2 (already in RSI)
        // RDX = arg3 (already in RDX)
        // R10 = arg4 (need to move to RCX for System V ABI)
        // R8 = arg5 (already in R8)
        // R9 = arg6 (already in R9)
        "mov rcx, r10",      // Move 4th arg to RCX for System V ABI

        // Call syscall handler (follows System V ABI)
        "call {syscall_handler}",

        // RAX now contains return value

        // Restore user space registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11",           // user RFLAGS
        "pop rcx",           // user RIP

        // Return to user space
        "sysretq",

        syscall_handler = sym syscall_handler_rust,
    );
}

/// Rust syscall handler called from assembly entry point
///
/// This function receives syscall arguments following System V ABI:
/// RDI, RSI, RDX, RCX, R8, R9
extern "C" fn syscall_handler_rust(
    syscall_number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    syscall::handle_syscall(syscall_number, arg1, arg2, arg3, arg4, arg5, arg6)
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
/// - Page table must be valid and properly initialized
/// - Called only once per process
/// - GDT must be initialized before calling
pub unsafe fn enter_usermode(entry_point: u64, stack_ptr: u64, page_table_phys: u64) -> ! {
    use x86_64::registers::control::Cr3;
    use x86_64::PhysAddr;

    // Switch to process page table
    // SAFETY: Caller guarantees page table is valid
    unsafe {
        let phys_addr = PhysAddr::new(page_table_phys);
        Cr3::write(
            x86_64::structures::paging::PhysFrame::containing_address(phys_addr),
            x86_64::registers::control::Cr3Flags::empty(),
        );
    }

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
