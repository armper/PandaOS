//! Process Scheduler for PandaOS
//!
//! This module implements a minimal round-robin scheduler for preemptive multitasking.
//!
//! ## Design
//!
//! - **Single CPU**: SMP support explicitly out of scope
//! - **Round-robin**: Fair time-slicing between processes
//! - **No priorities**: All processes have equal scheduling weight
//! - **Simple states**: Ready, Running, Exited
//!
//! ## Invariants
//!
//! - At most one process is in Running state at any time
//! - Ready queue contains only Ready processes
//! - Exited processes are removed from the scheduler
//! - Scheduler operations are atomic (interrupts disabled during critical sections)
//! - schedule_next() always returns a valid process or None if queue is empty
//!
//! ## Safety
//!
//! The scheduler itself is safe Rust. However, it coordinates with unsafe
//! context switching code. Callers of scheduler functions that trigger
//! context switches must ensure:
//! - Interrupts are disabled during scheduler operations
//! - Process structures contain valid context and page table info
//! - No data races on process state

use crate::process::{Process, ProcessState};
use alloc::collections::VecDeque;

/// Round-robin process scheduler
///
/// The scheduler maintains a queue of runnable processes and selects
/// the next process to run in a fair, round-robin manner.
pub struct Scheduler {
    /// Queue of ready-to-run processes
    ready_queue: VecDeque<Process>,
    /// Currently running process (if any)
    current: Option<Process>,
}

impl Scheduler {
    /// Create a new empty scheduler
    pub fn new() -> Self {
        Self { ready_queue: VecDeque::new(), current: None }
    }

    /// Add a process to the scheduler
    ///
    /// The process is added to the ready queue and will be scheduled
    /// according to the round-robin policy.
    ///
    /// # Arguments
    ///
    /// * `process` - Process to add (must be in Ready state)
    ///
    /// # Panics
    ///
    /// Panics if the process is not in Ready state
    pub fn add_process(&mut self, mut process: Process) {
        assert_eq!(
            process.state,
            ProcessState::Ready,
            "Process must be in Ready state when added to scheduler"
        );
        process.state = ProcessState::Ready;
        self.ready_queue.push_back(process);
    }

    /// Get the next process to run
    ///
    /// Selects the next process from the ready queue in round-robin order.
    /// If a process is currently running, it is moved back to the ready queue
    /// (unless it has exited). Blocked processes are skipped.
    ///
    /// This also checks for pending signals and terminates processes that
    /// receive SIGINT. Signal delivery can also wake blocked processes.
    ///
    /// # Returns
    ///
    /// - `Some(&mut Process)` - The next process to run
    /// - `None` - No processes are available to run
    pub fn schedule_next(&mut self) -> Option<&mut Process> {
        // If there's a current process that's not exited, move it back to ready queue
        if let Some(mut proc) = self.current.take() {
            if !proc.is_exited() {
                // Check for pending signals before re-queueing
                if proc.deliver_signals() {
                    // Process was terminated by signal
                    // Exit code = 128 + signal number (standard Unix convention)
                    proc.set_exited(128 + crate::process::Signal::SIGINT as i32);
                } else {
                    // Wake up if there are pending signals (even if not terminating)
                    if proc.pending_signals != 0 {
                        proc.wake();
                    }
                    proc.state = ProcessState::Ready;
                    self.ready_queue.push_back(proc);
                }
            }
            // If exited, drop it (it won't be added back to queue)
        }

        // Find next runnable (not blocked) process from ready queue
        let ready_count = self.ready_queue.len();
        for _ in 0..ready_count {
            if let Some(mut next_proc) = self.ready_queue.pop_front() {
                if next_proc.is_blocked() {
                    // Process is blocked, put it back at the end and try next
                    self.ready_queue.push_back(next_proc);
                } else {
                    // Found a runnable process
                    next_proc.state = ProcessState::Running;
                    self.current = Some(next_proc);
                    return self.current.as_mut();
                }
            }
        }

        // No runnable processes found
        None
    }

    /// Get a reference to the currently running process
    ///
    /// # Returns
    ///
    /// - `Some(&Process)` - The currently running process
    /// - `None` - No process is currently running
    pub fn current_process(&self) -> Option<&Process> {
        self.current.as_ref()
    }

    /// Get a mutable reference to the currently running process
    ///
    /// # Returns
    ///
    /// - `Some(&mut Process)` - The currently running process
    /// - `None` - No process is currently running
    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current.as_mut()
    }

    /// Mark the current process as exited
    ///
    /// The process will be removed from the scheduler and will not be
    /// scheduled again. The next call to `schedule_next()` will select
    /// a different process.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - Process exit code
    ///
    /// # Returns
    ///
    /// `true` if a process was marked as exited, `false` if no process was running
    pub fn exit_current(&mut self, exit_code: i32) -> bool {
        if let Some(proc) = self.current.as_mut() {
            proc.set_exited(exit_code);
            true
        } else {
            false
        }
    }

    /// Remove all exited processes from the scheduler
    ///
    /// This performs garbage collection of processes that have exited.
    /// In our current implementation, exited processes are already removed
    /// by schedule_next(), so this is mainly for explicit cleanup.
    ///
    /// # Returns
    ///
    /// Number of processes removed
    pub fn remove_exited(&mut self) -> usize {
        let initial_len = self.ready_queue.len();
        self.ready_queue.retain(|proc| !proc.is_exited());
        let removed = initial_len - self.ready_queue.len();

        // Also check current process
        if let Some(proc) = &self.current {
            if proc.is_exited() {
                self.current = None;
                removed + 1
            } else {
                removed
            }
        } else {
            removed
        }
    }

    /// Get the number of processes in the ready queue
    pub fn ready_count(&self) -> usize {
        self.ready_queue.len()
    }

    /// Get the total number of processes (ready + running)
    pub fn total_count(&self) -> usize {
        self.ready_queue.len() + if self.current.is_some() { 1 } else { 0 }
    }

    /// Check if the scheduler has any runnable processes
    pub fn has_runnable(&self) -> bool {
        !self.ready_queue.is_empty() || self.current.is_some()
    }

    /// Yield the current process voluntarily
    ///
    /// The current process is moved to the back of the ready queue,
    /// and the next process is selected. This is used for the yield()
    /// syscall.
    ///
    /// # Returns
    ///
    /// `true` if a yield occurred, `false` if no process was running
    pub fn yield_current(&mut self) -> bool {
        if self.current.is_some() {
            // schedule_next() will automatically move current to ready queue
            self.schedule_next();
            true
        } else {
            false
        }
    }

    /// Find and return a zombie child of the given parent PID
    ///
    /// # Returns
    ///
    /// - `Some(Process)` - A zombie child process
    /// - `None` - No zombie children found
    pub fn find_zombie_child(&mut self, parent_pid: panda_hal::pid::Pid) -> Option<Process> {
        // Check current process first
        if let Some(proc) = &self.current {
            if proc.is_zombie() && proc.parent_pid == Some(parent_pid) {
                return self.current.take();
            }
        }

        // Check ready queue
        if let Some(pos) =
            self.ready_queue.iter().position(|p| p.is_zombie() && p.parent_pid == Some(parent_pid))
        {
            return Some(self.ready_queue.remove(pos).expect("Position exists"));
        }

        None
    }

    /// Find any zombie child of the given parent PID (for waitpid(-1))
    ///
    /// # Returns
    ///
    /// - `Some(Process)` - Any zombie child process
    /// - `None` - No zombie children found
    pub fn find_any_zombie_child(&mut self, parent_pid: panda_hal::pid::Pid) -> Option<Process> {
        self.find_zombie_child(parent_pid)
    }

    /// Check if a process has any children (zombie or alive)
    pub fn has_children(&self, parent_pid: panda_hal::pid::Pid) -> bool {
        // Check current process
        if let Some(proc) = &self.current {
            if proc.parent_pid == Some(parent_pid) {
                return true;
            }
        }

        // Check ready queue
        self.ready_queue.iter().any(|p| p.parent_pid == Some(parent_pid))
    }

    /// Wake processes waiting for a specific child to exit
    ///
    /// Called when a child process exits to wake its parent if it's waiting
    pub fn wake_waiters_for_child(&mut self, child_pid: panda_hal::pid::Pid) {
        // Wake the current process if waiting
        if let Some(proc) = &mut self.current {
            if proc.should_wake_on_child_exit(child_pid) {
                proc.wake();
            }
        }

        // Wake processes in ready queue if waiting
        for proc in &mut self.ready_queue {
            if proc.should_wake_on_child_exit(child_pid) {
                proc.wake();
            }
        }
    }

    /// Get all processes (for debugging/testing)
    pub fn all_processes(&self) -> impl Iterator<Item = &Process> {
        self.current.iter().chain(self.ready_queue.iter())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::ElfInfo;
    use panda_hal::pid::PidAllocator;

    fn create_mock_process(pid: u64) -> Process {
        Process {
            pid: panda_hal::pid::Pid::new(pid),
            parent_pid: None,
            state: ProcessState::Ready,
            wait_state: crate::process::WaitState::NotWaiting,
            entry_point: 0x400000,
            user_stack_ptr: 0x7FFFFFFFF000,
            kernel_stack_ptr: 0xFFFFFFFF80000000,
            page_table_phys: 0x1000,
            context: crate::context::CpuContext::zero(),
            fd_table: crate::fs::FdTable::new(),
            pending_signals: 0,
        }
    }

    #[test]
    fn test_new_scheduler() {
        let scheduler = Scheduler::new();
        assert_eq!(scheduler.ready_count(), 0);
        assert_eq!(scheduler.total_count(), 0);
        assert!(!scheduler.has_runnable());
    }

    #[test]
    fn test_add_process() {
        let mut scheduler = Scheduler::new();
        let proc = create_mock_process(1);

        scheduler.add_process(proc);
        assert_eq!(scheduler.ready_count(), 1);
        assert_eq!(scheduler.total_count(), 1);
        assert!(scheduler.has_runnable());
    }

    #[test]
    fn test_schedule_next() {
        let mut scheduler = Scheduler::new();
        let proc1 = create_mock_process(1);
        let proc2 = create_mock_process(2);

        scheduler.add_process(proc1);
        scheduler.add_process(proc2);

        // Schedule first process
        let next = scheduler.schedule_next();
        assert!(next.is_some());
        assert_eq!(next.unwrap().pid.as_u64(), 1);
        assert_eq!(scheduler.ready_count(), 1);

        // Schedule second process (first goes back to ready)
        let next = scheduler.schedule_next();
        assert!(next.is_some());
        assert_eq!(next.unwrap().pid.as_u64(), 2);
        assert_eq!(scheduler.ready_count(), 1);

        // Schedule first process again (round-robin)
        let next = scheduler.schedule_next();
        assert!(next.is_some());
        assert_eq!(next.unwrap().pid.as_u64(), 1);
    }

    #[test]
    fn test_exit_current() {
        let mut scheduler = Scheduler::new();
        let proc = create_mock_process(1);

        scheduler.add_process(proc);
        scheduler.schedule_next();

        // Exit the current process
        let exited = scheduler.exit_current(0);
        assert!(exited);

        // Schedule next should find no processes
        let next = scheduler.schedule_next();
        assert!(next.is_none());
        assert_eq!(scheduler.total_count(), 0);
    }

    #[test]
    fn test_remove_exited() {
        let mut scheduler = Scheduler::new();
        let proc = create_mock_process(1);

        scheduler.add_process(proc);
        scheduler.schedule_next();
        scheduler.exit_current(0);

        let removed = scheduler.remove_exited();
        assert_eq!(removed, 1);
        assert_eq!(scheduler.total_count(), 0);
    }

    #[test]
    fn test_yield_current() {
        let mut scheduler = Scheduler::new();
        let proc1 = create_mock_process(1);
        let proc2 = create_mock_process(2);

        scheduler.add_process(proc1);
        scheduler.add_process(proc2);
        scheduler.schedule_next(); // PID 1 running

        // Yield current process
        let yielded = scheduler.yield_current();
        assert!(yielded);

        // Should now have PID 2 running
        let current = scheduler.current_process();
        assert!(current.is_some());
        assert_eq!(current.unwrap().pid.as_u64(), 2);
    }

    #[test]
    fn test_current_process() {
        let mut scheduler = Scheduler::new();
        let proc = create_mock_process(1);

        assert!(scheduler.current_process().is_none());

        scheduler.add_process(proc);
        scheduler.schedule_next();

        let current = scheduler.current_process();
        assert!(current.is_some());
        assert_eq!(current.unwrap().pid.as_u64(), 1);
        assert_eq!(current.unwrap().state, ProcessState::Running);
    }

    #[test]
    fn test_parent_child_tracking() {
        let mut scheduler = Scheduler::new();
        let parent_pid = panda_hal::pid::Pid::new(1);
        let child1_pid = panda_hal::pid::Pid::new(2);
        let child2_pid = panda_hal::pid::Pid::new(3);

        let mut parent = create_mock_process(1);
        parent.parent_pid = None;

        let mut child1 = create_mock_process(2);
        child1.parent_pid = Some(parent_pid);

        let mut child2 = create_mock_process(3);
        child2.parent_pid = Some(parent_pid);

        scheduler.add_process(parent);
        scheduler.add_process(child1);
        scheduler.add_process(child2);

        // Parent has children
        assert!(scheduler.has_children(parent_pid));

        // Children have no children
        assert!(!scheduler.has_children(child1_pid));
        assert!(!scheduler.has_children(child2_pid));
    }

    #[test]
    fn test_zombie_child_retrieval() {
        let mut scheduler = Scheduler::new();
        let parent_pid = panda_hal::pid::Pid::new(1);
        let child_pid = panda_hal::pid::Pid::new(2);

        let mut parent = create_mock_process(1);
        parent.parent_pid = None;

        let mut child = create_mock_process(2);
        child.parent_pid = Some(parent_pid);
        child.state = ProcessState::Zombie(0);

        scheduler.add_process(parent);
        scheduler.add_process(child);

        // Find zombie child
        let zombie = scheduler.find_zombie_child(parent_pid);
        assert!(zombie.is_some());
        assert_eq!(zombie.unwrap().pid.as_u64(), 2);

        // After removal, no more zombies
        let zombie = scheduler.find_zombie_child(parent_pid);
        assert!(zombie.is_none());
    }
}
