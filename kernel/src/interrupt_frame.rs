//! Interrupt frame for saving/restoring CPU state on interrupts
//!
//! This module defines the interrupt frame structure that is used to
//! save and restore the complete CPU state when entering/exiting
//! interrupt handlers and performing context switches from interrupt context.
//!
//! ## Design
//!
//! The interrupt frame contains:
//! - All general-purpose registers
//! - Interrupt number and error code
//! - Hardware-pushed interrupt frame (RIP, CS, RFLAGS, RSP, SS)
//!
//! This allows the kernel to:
//! - Preempt user mode processes
//! - Perform context switches from timer interrupts
//! - Properly restore all state after interrupt handling
//!
//! ## Safety
//!
//! The interrupt frame must be properly aligned and sized to match
//! the CPU's expectations for interrupt handling.

/// Complete interrupt frame with all saved state
///
/// This structure is pushed onto the stack by interrupt handlers
/// to save the complete CPU state. It matches the x86_64 interrupt
/// calling convention.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    // Saved general-purpose registers (pushed by software)
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

    // Interrupt information
    pub interrupt_number: u64,
    pub error_code: u64,

    // Hardware-pushed interrupt frame
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl InterruptFrame {
    /// Create a new interrupt frame for user mode
    ///
    /// This is used when creating a new process or forking.
    pub const fn new_user(entry_point: u64, stack_ptr: u64, user_cs: u64, user_ss: u64) -> Self {
        Self {
            // Zero out all GPRs
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

            // No interrupt/error initially
            interrupt_number: 0,
            error_code: 0,

            // Set up for user mode execution
            rip: entry_point,
            cs: user_cs,
            rflags: 0x202, // IF | reserved bit 1
            rsp: stack_ptr,
            ss: user_ss,
        }
    }

    /// Create a zeroed interrupt frame
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
            interrupt_number: 0,
            error_code: 0,
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        }
    }

    /// Check if this frame represents user mode execution
    pub fn is_user_mode(&self) -> bool {
        // User mode has DPL=3 in CS (bits 0-1 of selector)
        (self.cs & 0x3) == 3
    }
}

/// Convert InterruptFrame to CpuContext for context switching
impl From<InterruptFrame> for crate::context::CpuContext {
    fn from(frame: InterruptFrame) -> Self {
        Self {
            r15: frame.r15,
            r14: frame.r14,
            r13: frame.r13,
            r12: frame.r12,
            r11: frame.r11,
            r10: frame.r10,
            r9: frame.r9,
            r8: frame.r8,
            rbp: frame.rbp,
            rdi: frame.rdi,
            rsi: frame.rsi,
            rdx: frame.rdx,
            rcx: frame.rcx,
            rbx: frame.rbx,
            rax: frame.rax,
            rip: frame.rip,
            rsp: frame.rsp,
            rflags: frame.rflags,
            cs: frame.cs,
            ss: frame.ss,
        }
    }
}

/// Convert CpuContext to InterruptFrame for interrupt return
impl From<crate::context::CpuContext> for InterruptFrame {
    fn from(ctx: crate::context::CpuContext) -> Self {
        Self {
            r15: ctx.r15,
            r14: ctx.r14,
            r13: ctx.r13,
            r12: ctx.r12,
            r11: ctx.r11,
            r10: ctx.r10,
            r9: ctx.r9,
            r8: ctx.r8,
            rbp: ctx.rbp,
            rdi: ctx.rdi,
            rsi: ctx.rsi,
            rdx: ctx.rdx,
            rcx: ctx.rcx,
            rbx: ctx.rbx,
            rax: ctx.rax,
            interrupt_number: 0,
            error_code: 0,
            rip: ctx.rip,
            cs: ctx.cs,
            rflags: ctx.rflags,
            rsp: ctx.rsp,
            ss: ctx.ss,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupt_frame_size() {
        // Interrupt frame should be properly sized for stack alignment
        let size = core::mem::size_of::<InterruptFrame>();
        // 15 GPRs + 2 interrupt fields + 5 hardware fields = 22 * 8 = 176 bytes
        assert_eq!(size, 176, "InterruptFrame size should be 176 bytes");
    }

    #[test]
    fn test_interrupt_frame_alignment() {
        let align = core::mem::align_of::<InterruptFrame>();
        assert!(align >= 8, "InterruptFrame should be at least 8-byte aligned");
    }

    #[test]
    fn test_user_mode_detection() {
        let mut frame = InterruptFrame::zero();
        frame.cs = 0x1B; // User code selector (DPL=3)
        assert!(frame.is_user_mode());

        frame.cs = 0x08; // Kernel code selector (DPL=0)
        assert!(!frame.is_user_mode());
    }

    #[test]
    fn test_new_user_frame() {
        let frame = InterruptFrame::new_user(0x400000, 0x7FFFFFFFF000, 0x1B, 0x23);
        assert_eq!(frame.rip, 0x400000);
        assert_eq!(frame.rsp, 0x7FFFFFFFF000);
        assert_eq!(frame.cs, 0x1B);
        assert_eq!(frame.ss, 0x23);
        assert!(frame.is_user_mode());
        assert_eq!(frame.rflags & 0x200, 0x200); // IF flag set
    }
}
