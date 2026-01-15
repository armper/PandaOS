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
}

impl Process {
    /// Create a new process from ELF information
    ///
    /// # Arguments
    ///
    /// * `elf_info` - Parsed ELF information
    /// * `pid_allocator` - PID allocator for generating unique IDs
    ///
    /// # Returns
    ///
    /// A new process ready to be executed
    pub fn new(elf_info: &ElfInfo, pid_allocator: &PidAllocator) -> Self {
        let pid = pid_allocator.allocate();

        // For now, use fixed stack addresses
        // TODO: Allocate actual memory for stacks
        let user_stack_ptr = 0x7FFF_FFFF_F000; // Top of user space
        let kernel_stack_ptr = 0xFFFF_FFFF_8000_0000; // Kernel stack

        Self {
            pid,
            state: ProcessState::Ready,
            entry_point: elf_info.entry_point,
            user_stack_ptr,
            kernel_stack_ptr,
        }
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
        let process = Process::new(&elf_info, &pid_allocator);

        assert_eq!(process.pid.as_u64(), 1);
        assert_eq!(process.state, ProcessState::Ready);
        assert_eq!(process.entry_point, 0x40_0000);
    }

    #[test]
    fn test_process_state_transitions() {
        let elf_info =
            ElfInfo { entry_point: 0x40_0000, load_segments: [None; 8], segment_count: 0 };

        let pid_allocator = PidAllocator::new(1);
        let mut process = Process::new(&elf_info, &pid_allocator);

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
