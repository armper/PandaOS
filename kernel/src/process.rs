//! Process model for PandaOS
//!
//! This module provides a minimal process abstraction for user mode execution.
//!
//! ## Invariants
//!
//! - Each process has an isolated address space
//! - Processes have separate user and kernel stacks
//! - Entry point is validated before execution
//! - No process can access another's memory

use crate::context::CpuContext;
use crate::elf::ElfInfo;
use panda_hal::pid::{Pid, PidAllocator};

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run
    Ready,
    /// Process is currently running
    Running,
    /// Process has exited
    Exited(i32),
}

/// Minimal process structure
pub struct Process {
    /// Process ID
    pub pid: Pid,
    /// Process state
    pub state: ProcessState,
    /// Entry point address
    pub entry_point: u64,
    /// User stack pointer
    pub user_stack_ptr: u64,
    /// Kernel stack pointer (for syscalls)
    pub kernel_stack_ptr: u64,
    /// Page table physical address
    pub page_table_phys: u64,
    /// Saved CPU context for context switching
    pub context: CpuContext,
}

impl Process {
    /// Create a new process from ELF information
    ///
    /// # Arguments
    ///
    /// * `elf_info` - Parsed ELF information
    /// * `elf_data` - Raw ELF binary data
    /// * `pid_allocator` - PID allocator for generating unique IDs
    ///
    /// # Safety
    ///
    /// Frame allocator must be initialized.
    ///
    /// # Returns
    ///
    /// A new process with memory mapped and ready to execute
    pub unsafe fn new(
        elf_info: &ElfInfo,
        elf_data: &[u8],
        pid_allocator: &PidAllocator,
    ) -> Result<Self, &'static str> {
        let pid = pid_allocator.allocate();

        // Create user page table
        // SAFETY: Caller guarantees frame allocator is initialized
        let page_table_phys = unsafe { crate::paging::create_user_page_table()? };

        // Load ELF segments into user address space
        // SAFETY: Caller guarantees frame allocator is initialized
        unsafe {
            crate::elf::load_elf_segments(elf_info, elf_data, page_table_phys)?;
        }

        // Allocate user stack (4 pages at top of user space)
        let user_stack_top = 0x7FFF_FFFF_F000u64;
        // SAFETY: Caller guarantees frame allocator is initialized
        unsafe {
            crate::paging::allocate_user_stack(page_table_phys, user_stack_top, 4)?;
        }

        // For now, use fixed kernel stack address
        // TODO: Allocate actual kernel stack
        let kernel_stack_ptr = 0xFFFF_FFFF_8000_0000;

        // Get GDT selectors for user mode
        // SAFETY: GDT must be initialized before creating processes
        let selectors = unsafe { crate::gdt::get_selectors() };
        let user_cs = selectors.user_code.0 as u64;
        let user_ss = selectors.user_data.0 as u64;

        // Initialize CPU context for user mode
        let context = CpuContext::new_user(elf_info.entry_point, user_stack_top, user_cs, user_ss);

        Ok(Self {
            pid,
            state: ProcessState::Ready,
            entry_point: elf_info.entry_point,
            user_stack_ptr: user_stack_top,
            kernel_stack_ptr,
            page_table_phys,
            context,
        })
    }

    /// Mark process as running
    pub fn set_running(&mut self) {
        self.state = ProcessState::Running;
    }

    /// Mark process as exited with given code
    pub fn set_exited(&mut self, code: i32) {
        self.state = ProcessState::Exited(code);
    }

    /// Check if process has exited
    pub const fn is_exited(&self) -> bool {
        matches!(self.state, ProcessState::Exited(_))
    }

    /// Get exit code if process has exited
    pub const fn exit_code(&self) -> Option<i32> {
        match self.state {
            ProcessState::Exited(code) => Some(code),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::ElfInfo;

    #[test]
    fn test_process_creation() {
        let elf_info =
            ElfInfo { entry_point: 0x40_0000, load_segments: [None; 8], segment_count: 0 };

        let pid_allocator = PidAllocator::new(1);

        // Note: Process::new now requires unsafe and ELF data
        // We can't test it in a unit test without the full kernel
        // This test is kept as a placeholder

        assert_eq!(pid_allocator.allocate().as_u64(), 1);
    }

    #[test]
    fn test_process_state_transitions() {
        let elf_info =
            ElfInfo { entry_point: 0x40_0000, load_segments: [None; 8], segment_count: 0 };

        let pid_allocator = PidAllocator::new(1);

        // Create a mock process for state testing
        let mut process = Process {
            pid: pid_allocator.allocate(),
            state: ProcessState::Ready,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
        };

        assert_eq!(process.state, ProcessState::Ready);
        assert!(!process.is_exited());

        process.set_running();
        assert_eq!(process.state, ProcessState::Running);
        assert!(!process.is_exited());

        process.set_exited(42);
        assert!(process.is_exited());
        assert_eq!(process.exit_code(), Some(42));
    }
}
