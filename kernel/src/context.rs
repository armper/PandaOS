//! CPU Context for process context switching
//!
//! This module defines the CPU context structure that stores all registers
//! needed to resume execution of a process after a context switch.
//!
//! ## Invariants
//!
//! - Context must store ALL registers needed to resume execution
//! - Context save/restore must be atomic (interrupts disabled)
//! - Stack pointer (RSP) must always point to valid memory
//! - RIP must point to valid executable code
//! - Segment selectors must match GDT entries
//!
//! ## Safety
//!
//! All context switching operations are inherently unsafe as they manipulate
//! CPU state directly. Callers must ensure:
//! - Context is properly initialized before restoration
//! - Memory pointed to by RSP is valid and properly sized
//! - RIP points to valid code with correct permissions

/// CPU context structure containing all registers
///
/// This structure is used to save and restore the complete CPU state
/// during context switches. The layout must match the order in which
/// registers are pushed/popped in the assembly code.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuContext {
    // General purpose registers
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,

    // Instruction pointer and stack pointer
    pub rip: u64,
    pub rsp: u64,

    // RFLAGS register
    pub rflags: u64,

    // Segment selectors
    pub cs: u64,
    pub ss: u64,
}

impl CpuContext {
    /// Create a new context for a user mode process
    ///
    /// # Arguments
    ///
    /// * `entry_point` - Address of the first instruction to execute
    /// * `stack_ptr` - Top of the user stack
    /// * `user_cs` - User code segment selector
    /// * `user_ss` - User data/stack segment selector
    ///
    /// # Returns
    ///
    /// A new `CpuContext` initialized for user mode execution
    pub const fn new_user(entry_point: u64, stack_ptr: u64, user_cs: u64, user_ss: u64) -> Self {
        Self {
            // Zero out general purpose registers for security
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,

            // Set entry point and stack
            rip: entry_point,
            rsp: stack_ptr,

            // Set RFLAGS with interrupts enabled (IF = bit 9)
            rflags: 0x202, // IF | reserved bit 1 (always 1)

            // Set segment selectors
            cs: user_cs,
            ss: user_ss,
        }
    }

    /// Create a zeroed context (for initialization)
    pub const fn zero() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip: 0,
            rsp: 0,
            rflags: 0,
            cs: 0,
            ss: 0,
        }
    }
}

/// Save the current CPU context
///
/// This function saves all registers to the provided context structure.
/// It must be called from assembly as it needs to save the exact state
/// at the point of the call.
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - Context pointer must be valid
/// - Caller must ensure context memory is writable
///
/// # Returns
///
/// This function doesn't return normally - it's used in context switch paths
/// where the return address is part of the saved state
#[inline(never)]
pub unsafe fn save_context(_ctx: *mut CpuContext) {
    // This is a placeholder - actual implementation is in assembly
    // See switch_context in context_switch.rs
    unimplemented!("save_context should only be called from assembly")
}

/// Restore CPU context and resume execution
///
/// This function restores all registers from the provided context structure
/// and resumes execution at the saved RIP.
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
#[inline(never)]
pub unsafe fn restore_context(_ctx: *const CpuContext) -> ! {
    // This is a placeholder - actual implementation is in assembly
    // See switch_context in context_switch.rs
    unimplemented!("restore_context should only be called from assembly")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_size() {
        // Ensure context size is reasonable (should be 23 * 8 = 184 bytes)
        let size = core::mem::size_of::<CpuContext>();
        assert_eq!(size, 184, "CpuContext size should be 184 bytes");
    }

    #[test]
    fn test_context_alignment() {
        // Ensure proper alignment for performance
        let align = core::mem::align_of::<CpuContext>();
        assert!(align >= 8, "CpuContext should be at least 8-byte aligned");
    }

    #[test]
    fn test_new_user_context() {
        let ctx = CpuContext::new_user(0x400000, 0x7FFFFFFFF000, 0x28, 0x30);
        assert_eq!(ctx.rip, 0x400000);
        assert_eq!(ctx.rsp, 0x7FFFFFFFF000);
        assert_eq!(ctx.cs, 0x28);
        assert_eq!(ctx.ss, 0x30);
        assert_eq!(ctx.rflags & 0x200, 0x200, "IF flag should be set");
    }

    #[test]
    fn test_zero_context() {
        let ctx = CpuContext::zero();
        assert_eq!(ctx.rip, 0);
        assert_eq!(ctx.rsp, 0);
        assert_eq!(ctx.rax, 0);
        assert_eq!(ctx.rbx, 0);
    }
}
