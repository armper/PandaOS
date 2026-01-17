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
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use panda_hal::pid::{Pid, PidAllocator};

/// Memory region protection flags (for mmap)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtFlags(pub u32);

impl ProtFlags {
    pub const PROT_READ: Self = Self(0x1);
    pub const PROT_WRITE: Self = Self(0x2);
    pub const PROT_EXEC: Self = Self(0x4);
    pub const PROT_NONE: Self = Self(0x0);
}

/// Memory mapping flags (for mmap)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapFlags(pub u32);

impl MapFlags {
    pub const MAP_PRIVATE: Self = Self(0x02);
    pub const MAP_ANONYMOUS: Self = Self(0x20);
}

/// Type of VM region
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VMRegionType {
    /// Executable code segment
    Code,
    /// Data segment
    Data,
    /// Heap region
    Heap,
    /// Stack region
    Stack,
    /// Anonymous mapping
    Anonymous,
}

/// VM region tracking for process address space
#[derive(Debug, Clone)]
pub struct VMRegion {
    /// Start virtual address
    pub start_addr: u64,
    /// End virtual address (exclusive)
    pub end_addr: u64,
    /// Protection flags (PROT_READ, PROT_WRITE, PROT_EXEC)
    pub flags: u32,
    /// Type of region
    pub region_type: VMRegionType,
    /// True if backed by a file (ELF segment)
    pub file_backed: bool,
    /// File offset for file-backed regions
    pub file_offset: u64,
}

/// A single memory mapping created by mmap
#[derive(Debug, Clone)]
pub struct MemoryMapping {
    /// Starting virtual address
    pub addr: u64,
    /// Size in bytes
    pub length: u64,
    /// Protection flags
    pub prot: u32,
    /// Mapping flags
    pub flags: u32,
}

/// Heap management for a process
#[derive(Debug, Clone)]
pub struct HeapInfo {
    /// Start of heap (end of ELF data/bss)
    pub heap_start: u64,
    /// Current program break (end of allocated heap)
    pub heap_end: u64,
    /// Maximum allowed heap address
    pub heap_limit: u64,
}

impl HeapInfo {
    /// Create a new heap info with start address
    pub fn new(heap_start: u64) -> Self {
        // Align heap start to page boundary
        let aligned_start = (heap_start + 0xFFF) & !0xFFF;
        Self {
            heap_start: aligned_start,
            heap_end: aligned_start,
            // Allow 1 GB of heap by default
            heap_limit: aligned_start + 0x4000_0000,
        }
    }
}

/// Signal types supported by PandaOS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Signal {
    /// SIGINT - Interrupt signal (Ctrl+C)
    SIGINT = 2,
    /// SIGCONT - Continue stopped process
    SIGCONT = 18,
    /// SIGTSTP - Terminal stop signal (Ctrl+Z)
    SIGTSTP = 20,
}

impl Signal {
    /// Convert from raw signal number
    pub const fn from_u32(n: u32) -> Option<Self> {
        match n {
            2 => Some(Self::SIGINT),
            18 => Some(Self::SIGCONT),
            20 => Some(Self::SIGTSTP),
            _ => None,
        }
    }
}

/// Signal action to take after delivery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    /// No action needed
    None,
    /// Terminate the process
    Terminate,
    /// Stop (suspend) the process
    Stop,
    /// Continue a stopped process
    Continue,
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
    /// Process is stopped (suspended)
    Stopped,
    /// Process has exited
    Exited(i32),
    /// Process is a zombie (exited but not yet reaped by parent)
    Zombie(i32),
}

/// Minimal process structure
pub struct Process {
    /// Process ID
    pub pid: Pid,
    /// Process group ID (for job control)
    pub pgid: Pid,
    /// Parent process ID (None for init)
    pub parent_pid: Option<Pid>,
    /// Process state
    pub state: ProcessState,
    /// Wait state (for blocking operations)
    pub wait_state: WaitState,
    /// User ID (for permissions)
    pub uid: u32,
    /// Group ID (for permissions)
    pub gid: u32,
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
    /// Current working directory (absolute path)
    pub cwd: String,
    /// PATH environment variable for command lookup
    pub path_env: String,
    /// Environment variables (key=value map)
    pub environ: BTreeMap<String, String>,
    /// Heap management
    pub heap: HeapInfo,
    /// Memory mappings created by mmap
    pub mappings: Vec<MemoryMapping>,
    /// Base address for mmap allocations
    pub mmap_base: u64,
    /// VM regions for tracking address space layout
    pub vm_regions: Vec<VMRegion>,
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

        // Calculate heap start (after ELF segments)
        let heap_start = crate::elf::calculate_heap_start(elf_info);
        let heap = HeapInfo::new(heap_start);

        // mmap base: start from high address and grow downward
        // Place it below the stack, leaving room for stack growth
        let mmap_base = 0x7FFF_0000_0000u64;

        // Initialize default environment
        let mut environ = BTreeMap::new();
        environ.insert(String::from("PATH"), String::from("/mnt/bin:/bin"));
        environ.insert(String::from("USER"), String::from("root"));
        environ.insert(String::from("HOME"), String::from("/root"));

        Ok(Self {
            pid,
            pgid: pid, // Initially, process is its own group leader
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            uid: 0, // Default to root
            gid: 0, // Default to root
            entry_point: elf_info.entry_point,
            user_stack_ptr: user_stack_top,
            kernel_stack_ptr,
            page_table_phys,
            context,
            fd_table: FdTable::new(),
            pending_signals: 0,
            cwd: String::from("/"),
            path_env: String::from("/mnt/bin:/bin"),
            environ,
            heap,
            mappings: Vec::new(),
            mmap_base,
            vm_regions: Vec::new(),
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

            // Calculate new heap start
            let heap_start = crate::elf::calculate_heap_start(elf_info);
            let heap = HeapInfo::new(heap_start);

            self.entry_point = elf_info.entry_point;
            self.user_stack_ptr = user_stack_top;
            self.page_table_phys = new_page_table;
            self.context =
                CpuContext::new_user(elf_info.entry_point, user_stack_top, user_cs, user_ss);
            self.heap = heap;
            self.mappings.clear();
            self.vm_regions.clear();

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
            pgid: child_pid, // Child gets its own process group by default
            parent_pid: Some(self.pid),
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            uid: self.uid, // Inherit uid from parent
            gid: self.gid, // Inherit gid from parent
            entry_point: self.entry_point,
            user_stack_ptr: self.user_stack_ptr,
            kernel_stack_ptr: kernel_stack_top,
            page_table_phys: child_page_table,
            context: child_context,
            fd_table: child_fd_table,
            pending_signals: 0,
            cwd: self.cwd.clone(),
            path_env: self.path_env.clone(),
            environ: self.environ.clone(), // Clone environment
            heap: self.heap.clone(),
            mappings: self.mappings.clone(),
            mmap_base: self.mmap_base,
            vm_regions: self.vm_regions.clone(),
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

    /// Mark process as stopped (suspended)
    pub fn set_stopped(&mut self) {
        self.state = ProcessState::Stopped;
    }

    /// Check if process is stopped
    pub const fn is_stopped(&self) -> bool {
        matches!(self.state, ProcessState::Stopped)
    }

    /// Resume a stopped process (transition to Ready state)
    pub fn resume(&mut self) {
        if self.is_stopped() {
            self.state = ProcessState::Ready;
        }
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

    /// Deliver pending signals and return signal action
    ///
    /// Returns:
    /// - `SignalAction::Terminate` - Process should be terminated (SIGINT)
    /// - `SignalAction::Stop` - Process should be stopped (SIGTSTP)
    /// - `SignalAction::Continue` - Process should be continued (SIGCONT)
    /// - `SignalAction::None` - No action needed
    pub fn deliver_signals(&mut self) -> SignalAction {
        // SIGCONT resumes stopped processes
        if self.has_signal(Signal::SIGCONT) {
            self.clear_signal(Signal::SIGCONT);
            if self.is_stopped() {
                self.resume();
            }
            return SignalAction::Continue;
        }

        // SIGTSTP stops the process
        if self.has_signal(Signal::SIGTSTP) {
            self.clear_signal(Signal::SIGTSTP);
            return SignalAction::Stop;
        }

        // SIGINT terminates the process
        if self.has_signal(Signal::SIGINT) {
            self.clear_signal(Signal::SIGINT);
            return SignalAction::Terminate;
        }

        SignalAction::None
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
        let elf_info = ElfInfo {
            entry_point: 0x40_0000,
            load_segments: [None; 8],
            segment_count: 0,
            phdr_addr: 0,
            phnum: 0,
        };

        let pid_allocator = PidAllocator::new(1);

        // Note: Process::new now requires unsafe and ELF data
        // We can't test it in a unit test without the full kernel
        // This test is kept as a placeholder

        assert_eq!(pid_allocator.allocate().as_u64(), 1);
    }

    #[test]
    fn test_process_state_transitions() {
        let elf_info = ElfInfo {
            entry_point: 0x40_0000,
            load_segments: [None; 8],
            segment_count: 0,
            phdr_addr: 0,
            phnum: 0,
        };

        let pid_allocator = PidAllocator::new(1);

        // Create a mock process for state testing
        let mut process = Process {
            pid: pid_allocator.allocate(),
            pgid: panda_hal::pid::Pid::new(1),
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            uid: 0,
            gid: 0,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: FdTable::new(),
            pending_signals: 0,
            cwd: String::from("/"),
            path_env: String::from("/bin"),
            environ: BTreeMap::new(),
            heap: HeapInfo::new(0x1000000),
            mappings: Vec::new(),
            mmap_base: 0x7FFF_0000_0000,
            vm_regions: Vec::new(),
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
            pgid: panda_hal::pid::Pid::new(1),
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            uid: 0,
            gid: 0,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: FdTable::new(),
            pending_signals: 0,
            cwd: String::from("/"),
            path_env: String::from("/bin"),
            environ: BTreeMap::new(),
            heap: HeapInfo::new(0x1000000),
            mappings: Vec::new(),
            mmap_base: 0x7FFF_0000_0000,
            vm_regions: Vec::new(),
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
            pgid: panda_hal::pid::Pid::new(1),
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: WaitState::NotWaiting,
            uid: 0,
            gid: 0,
            entry_point: 0x40_0000,
            user_stack_ptr: 0x7FFF_FFFF_F000,
            kernel_stack_ptr: 0xFFFF_FFFF_8000_0000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: FdTable::new(),
            pending_signals: 0,
            cwd: String::from("/"),
            path_env: String::from("/bin"),
            environ: BTreeMap::new(),
            heap: HeapInfo::new(0x1000000),
            mappings: Vec::new(),
            mmap_base: 0x7FFF_0000_0000,
            vm_regions: Vec::new(),
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
