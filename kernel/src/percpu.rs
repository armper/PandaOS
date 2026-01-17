//! Per-CPU data structures for SMP support
//!
//! This module provides per-CPU data structures that store CPU-local state
//! such as the current process, CPU ID, and scheduler state.
//!
//! ## Design
//!
//! Each CPU has its own instance of `CpuLocal` that stores:
//! - CPU ID
//! - Current running process PID
//! - Preemption disable count (for critical sections)
//! - CPU-local scratch space
//!
//! ## Safety
//!
//! Per-CPU data can only be accessed with interrupts disabled to prevent
//! race conditions from interrupt handlers or context switches.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use panda_hal::pid::Pid;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 8;

/// Per-CPU data structure
#[repr(C, align(64))] // Cache line aligned to avoid false sharing
pub struct CpuLocal {
    /// CPU ID (0 = BSP, 1+ = APs)
    pub cpu_id: u32,

    /// Current running process PID (0 if idle)
    pub current_pid: AtomicU64,

    /// Preemption disable count (>0 means preemption disabled)
    pub preempt_count: AtomicU32,

    /// CPU online flag
    pub online: AtomicU32,

    /// Padding to ensure cache line size
    _padding: [u8; 32],
}

impl CpuLocal {
    /// Create a new per-CPU data structure
    pub const fn new(cpu_id: u32) -> Self {
        Self {
            cpu_id,
            current_pid: AtomicU64::new(0),
            preempt_count: AtomicU32::new(0),
            online: AtomicU32::new(0),
            _padding: [0; 32],
        }
    }

    /// Get the current process PID
    pub fn get_current_pid(&self) -> Option<Pid> {
        let pid = self.current_pid.load(Ordering::Acquire);
        if pid == 0 {
            None
        } else {
            Some(Pid::new(pid))
        }
    }

    /// Set the current process PID
    pub fn set_current_pid(&self, pid: Option<Pid>) {
        let val = pid.map_or(0, |p| p.as_u64());
        self.current_pid.store(val, Ordering::Release);
    }

    /// Disable preemption for this CPU
    pub fn preempt_disable(&self) {
        self.preempt_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Enable preemption for this CPU
    pub fn preempt_enable(&self) {
        let old = self.preempt_count.fetch_sub(1, Ordering::SeqCst);
        if old == 0 {
            panic!("Preemption enable count underflow");
        }
    }

    /// Check if preemption is disabled
    pub fn is_preempt_disabled(&self) -> bool {
        self.preempt_count.load(Ordering::SeqCst) > 0
    }

    /// Mark this CPU as online
    pub fn mark_online(&self) {
        self.online.store(1, Ordering::Release);
    }

    /// Check if this CPU is online
    pub fn is_online(&self) -> bool {
        self.online.load(Ordering::Acquire) != 0
    }
}

/// Global array of per-CPU data
static mut CPU_DATA: [CpuLocal; MAX_CPUS] = [
    CpuLocal::new(0),
    CpuLocal::new(1),
    CpuLocal::new(2),
    CpuLocal::new(3),
    CpuLocal::new(4),
    CpuLocal::new(5),
    CpuLocal::new(6),
    CpuLocal::new(7),
];

/// Number of online CPUs
static CPU_COUNT: AtomicU32 = AtomicU32::new(0);

/// Initialize per-CPU data for the BSP (Bootstrap Processor)
///
/// # Safety
///
/// Must be called exactly once during kernel initialization on the BSP.
pub unsafe fn init_bsp() {
    // SAFETY: Called once during kernel init
    unsafe {
        CPU_DATA[0].mark_online();
    }
    CPU_COUNT.store(1, Ordering::Release);
}

/// Initialize per-CPU data for an AP (Application Processor)
///
/// # Safety
///
/// Must be called exactly once per AP during SMP initialization.
pub unsafe fn init_ap(cpu_id: u32) {
    if cpu_id as usize >= MAX_CPUS {
        panic!("CPU ID {} exceeds MAX_CPUS", cpu_id);
    }

    // SAFETY: Called once per AP
    unsafe {
        CPU_DATA[cpu_id as usize].mark_online();
    }
    CPU_COUNT.fetch_add(1, Ordering::Release);
}

/// Get the current CPU ID
///
/// # Safety
///
/// Must be called with interrupts disabled to ensure CPU doesn't change.
pub unsafe fn current_cpu_id() -> u32 {
    // For now, we use CPU 0 (BSP) as default
    // TODO: Read from APIC ID or similar
    0
}

/// Get per-CPU data for the current CPU
///
/// # Safety
///
/// Must be called with interrupts disabled to ensure CPU doesn't change.
pub unsafe fn current_cpu() -> &'static CpuLocal {
    let cpu_id = unsafe { current_cpu_id() };
    unsafe { &CPU_DATA[cpu_id as usize] }
}

/// Get per-CPU data for a specific CPU
///
/// # Safety
///
/// Must ensure cpu_id is valid (< MAX_CPUS and CPU is online).
pub unsafe fn get_cpu(cpu_id: u32) -> &'static CpuLocal {
    if cpu_id as usize >= MAX_CPUS {
        panic!("Invalid CPU ID: {}", cpu_id);
    }
    unsafe { &CPU_DATA[cpu_id as usize] }
}

/// Get the number of online CPUs
pub fn online_cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_local_creation() {
        let cpu = CpuLocal::new(0);
        assert_eq!(cpu.cpu_id, 0);
        assert_eq!(cpu.get_current_pid(), None);
        assert!(!cpu.is_preempt_disabled());
        assert!(!cpu.is_online());
    }

    #[test]
    fn test_preempt_count() {
        let cpu = CpuLocal::new(0);
        assert!(!cpu.is_preempt_disabled());

        cpu.preempt_disable();
        assert!(cpu.is_preempt_disabled());

        cpu.preempt_enable();
        assert!(!cpu.is_preempt_disabled());
    }

    #[test]
    #[should_panic(expected = "underflow")]
    fn test_preempt_underflow() {
        let cpu = CpuLocal::new(0);
        cpu.preempt_enable(); // Should panic
    }

    #[test]
    fn test_current_pid() {
        let cpu = CpuLocal::new(0);
        assert_eq!(cpu.get_current_pid(), None);

        let pid = Pid::from_u64(42);
        cpu.set_current_pid(Some(pid));
        assert_eq!(cpu.get_current_pid(), Some(pid));

        cpu.set_current_pid(None);
        assert_eq!(cpu.get_current_pid(), None);
    }

    #[test]
    fn test_online_status() {
        let cpu = CpuLocal::new(0);
        assert!(!cpu.is_online());

        cpu.mark_online();
        assert!(cpu.is_online());
    }

    #[test]
    fn test_cpu_count() {
        // This test reads the global CPU_COUNT, which may be modified by other tests
        // or kernel initialization, so we just check it's reasonable
        let count = online_cpu_count();
        assert!(count <= MAX_CPUS as u32);
    }
}
