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
use crate::fs::FdTable;
use panda_hal::pid::{Pid, PidAllocator};

/// Signal types supported by PandaOS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Signal {
    /// SIGINT - Interrupt signal (Ctrl+C)
    SIGINT = 2,
}

impl Signal {
    /// Convert from raw signal number
    pub const fn from_u32(n: u32) -> Option<Self> {
        match n {
            2 => Some(Self::SIGINT),
            _ => None,
        }
    }
}

/// Wait state for blocking operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitState {
    /// Process is not waiting
    NotWaiting,
    /// Process is waiting for any child to exit
    WaitingForAnyChild,
    /// Process is waiting for a specific child to exit
    WaitingForChild(Pid),
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run
    Ready,
    /// Process is currently running
    Running,
    /// Process has exited
    Exited(i32),
    /// Process is a zombie (exited but not yet reaped by parent)
    Zombie(i32),
}

/// Minimal process structure
pub struct Process {
    /// Process ID
    pub pid: Pid,
    /// Parent process ID (None for init)
    pub parent_pid: Option<Pid>,
    /// Process state
    pub state: ProcessState,
    /// Wait state (for blocking operations)
    pub wait_state: WaitState,
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
    /// Per-process file descriptor table
    pub fd_table: FdTable,
    /// Pending signals (bitmask)
    pub pending_signals: u32,
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

        // Allocate kernel stack in higher half (per-process mapping)
        let kernel_stack_top = crate::paging::KERNEL_STACK_TOP;
        // SAFETY: Caller guarantees frame allocator is initialized
        unsafe {
            crate::paging::allocate_kernel_stack(
                page_table_phys,
                kernel_stack_top,
                crate::paging::KERNEL_STACK_PAGES,
            )?;
        }
        let kernel_stack_ptr = kernel_stack_top;

        // Get GDT selectors for user mode
        // SAFETY: GDT must be initialized before creating processes
        let selectors = unsafe { crate::gdt::get_selectors() };
        let user_cs = selectors.user_code.0 as u64;
        let user_ss = selectors.user_data.0 as u64;

        // Initialize CPU context for user mode
        let context = CpuContext::new_user(elf_info.entry_point, user_stack_top, user_cs, user_ss);

        Ok(Self {
            pid,
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            entry_point: elf_info.entry_point,
            user_stack_ptr: user_stack_top,
            kernel_stack_ptr,
            page_table_phys,
            context,
            fd_table: FdTable::new(),
            pending_signals: 0,
        })
    }

    /// Replace the current process image with a new ELF.
    ///
    /// # Safety
    ///
    /// Frame allocator and GDT must be initialized.
    pub unsafe fn replace_image(
        &mut self,
        elf_info: &ElfInfo,
        elf_data: &[u8],
    ) -> Result<(), &'static str> {
        let old_page_table = self.page_table_phys;

        // SAFETY: Caller guarantees frame allocator is initialized
        let new_page_table = unsafe { crate::paging::create_user_page_table()? };

        let result = (|| {
            // SAFETY: Caller guarantees frame allocator is initialized
            unsafe {
                crate::elf::load_elf_segments(elf_info, elf_data, new_page_table)?;
            }

            let user_stack_top = 0x7FFF_FFFF_F000u64;
            // SAFETY: Caller guarantees frame allocator is initialized
            unsafe {
                crate::paging::allocate_user_stack(new_page_table, user_stack_top, 4)?;
            }

            // SAFETY: GDT must be initialized before creating processes
            let selectors = unsafe { crate::gdt::get_selectors() };
            let user_cs = selectors.user_code.0 as u64;
            let user_ss = selectors.user_data.0 as u64;

            self.entry_point = elf_info.entry_point;
            self.user_stack_ptr = user_stack_top;
            self.page_table_phys = new_page_table;
            self.context =
                CpuContext::new_user(elf_info.entry_point, user_stack_top, user_cs, user_ss);

            Ok(())
        })();

        if let Err(err) = result {
            // SAFETY: new_page_table is valid; keep kernel stack frames.
            unsafe {
                crate::paging::free_process_address_space(new_page_table, false)?;
            }
            return Err(err);
        }

        // SAFETY: old page table is valid; keep kernel stack frames.
        unsafe {
            crate::paging::free_process_address_space(old_page_table, false)?;
        }

        Ok(())
    }

    /// Fork the current process, creating a child copy
    ///
    /// # Safety
    ///
    /// Frame allocator and GDT must be initialized.
    pub unsafe fn fork_from(&self, child_pid: Pid) -> Result<Self, &'static str> {
        // Clone the parent's address space
        // SAFETY: Caller guarantees frame allocator is initialized
        let child_page_table =
            unsafe { crate::paging::clone_user_address_space(self.page_table_phys)? };

        // Allocate kernel stack for child (per-process mapping)
        let kernel_stack_top = crate::paging::KERNEL_STACK_TOP;
        // SAFETY: Caller guarantees frame allocator is initialized
        unsafe {
            crate::paging::allocate_kernel_stack(
                child_page_table,
                kernel_stack_top,
                crate::paging::KERNEL_STACK_PAGES,
            )?;
        }

        // Copy parent's CPU context - child will have rax=0 set by caller
        let child_context = self.context;

        // Duplicate FD table with proper refcounting
        let child_fd_table = self.fd_table.fork_copy().map_err(|_| "Failed to fork FD table")?;

        Ok(Self {
            pid: child_pid,
            parent_pid: Some(self.pid),
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            entry_point: self.entry_point,
            user_stack_ptr: self.user_stack_ptr,
            kernel_stack_ptr: kernel_stack_top,
            page_table_phys: child_page_table,
            context: child_context,
            fd_table: child_fd_table,
            pending_signals: 0,
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
        matches!(self.state, ProcessState::Exited(_) | ProcessState::Zombie(_))
    }

    /// Get exit code if process has exited
    pub const fn exit_code(&self) -> Option<i32> {
        match self.state {
            ProcessState::Exited(code) | ProcessState::Zombie(code) => Some(code),
            _ => None,
        }
    }

    /// Mark process as zombie (exited but awaiting parent's wait)
    pub fn set_zombie(&mut self, code: i32) {
        self.state = ProcessState::Zombie(code);
    }

    /// Check if process is a zombie
    pub const fn is_zombie(&self) -> bool {
        matches!(self.state, ProcessState::Zombie(_))
    }

    /// Send a signal to this process
    ///
    /// Signals are stored as a bitmask in pending_signals.
    ///
    /// # Panics
    ///
    /// Panics if signal number exceeds 31 (implementation limitation of u32 bitmask).
    /// Current implementation only supports SIGINT (signal #2), so this is not an issue.
    pub fn send_signal(&mut self, signal: Signal) {
        let signal_num = signal as u32;
        assert!(signal_num < 32, "Signal number must be < 32 for u32 bitmask storage");
        self.pending_signals |= 1 << signal_num;
    }

    /// Check if a signal is pending
    pub fn has_signal(&self, signal: Signal) -> bool {
        (self.pending_signals & (1 << (signal as u32))) != 0
    }

    /// Clear a pending signal
    pub fn clear_signal(&mut self, signal: Signal) {
        self.pending_signals &= !(1 << (signal as u32));
    }

    /// Deliver pending signals and return true if process should be terminated
    ///
    /// For SIGINT, the default action is to terminate the process.
    pub fn deliver_signals(&mut self) -> bool {
        if self.has_signal(Signal::SIGINT) {
            self.clear_signal(Signal::SIGINT);
            // Default action: terminate
            return true;
        }
        false
    }

    /// Check if process is blocked (waiting)
    pub const fn is_blocked(&self) -> bool {
        !matches!(self.wait_state, WaitState::NotWaiting)
    }

    /// Block the process waiting for any child
    pub fn block_on_any_child(&mut self) {
        self.wait_state = WaitState::WaitingForAnyChild;
    }

    /// Block the process waiting for a specific child
    pub fn block_on_child(&mut self, child_pid: Pid) {
        self.wait_state = WaitState::WaitingForChild(child_pid);
    }

    /// Wake the process (unblock)
    pub fn wake(&mut self) {
        self.wait_state = WaitState::NotWaiting;
    }

    /// Check if this process should be woken when a child exits
    ///
    /// Returns true if the process is waiting for the given child or any child
    pub fn should_wake_on_child_exit(&self, child_pid: Pid) -> bool {
        match self.wait_state {
            WaitState::WaitingForAnyChild => true,
            WaitState::WaitingForChild(pid) => pid == child_pid,
            WaitState::NotWaiting => false,
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
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: FdTable::new(),
            pending_signals: 0,
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

    #[test]
    fn test_parent_child_relationship() {
        let pid_allocator = PidAllocator::new(1);

        let parent = Process {
            pid: pid_allocator.allocate(),
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: FdTable::new(),
            pending_signals: 0,
        };

        // Parent has no parent
        assert_eq!(parent.parent_pid, None);
        assert_eq!(parent.pid.as_u64(), 1);
    }

    #[test]
    fn test_zombie_state() {
        let pid_allocator = PidAllocator::new(1);

        let mut process = Process {
            pid: pid_allocator.allocate(),
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: FdTable::new(),
            pending_signals: 0,
        };

        // Initially not a zombie
        assert!(!process.is_zombie());
        assert!(!process.is_exited());

        // Make it a zombie
        process.set_zombie(42);
        assert!(process.is_zombie());
        assert!(process.is_exited());
        assert_eq!(process.exit_code(), Some(42));
    }
}
