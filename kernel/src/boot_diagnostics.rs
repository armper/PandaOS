//! Boot diagnostics and crash reporting
//!
//! This module provides instrumentation for tracking boot progress and
//! diagnosing boot failures. It includes macros for logging boot steps
//! and asserting critical invariants during kernel initialization.
//!
//! ## Usage
//!
//! ```rust,ignore
//! BOOT_STEP!(1);  // Log boot step with CPU ID, CR3, RSP
//! BOOT_ASSERT!(condition, 0x100);  // Assert with failure code
//! ```

use core::sync::atomic::{AtomicU32, Ordering};
use x86_64::registers::control::Cr3;

/// Maximum number of boot steps to track
const MAX_BOOT_STEPS: usize = 32;

/// Current boot step counter
static CURRENT_STEP: AtomicU32 = AtomicU32::new(0);

/// Boot step history (circular buffer)
static mut BOOT_STEP_HISTORY: [u32; MAX_BOOT_STEPS] = [0; MAX_BOOT_STEPS];

/// Record a boot step in the history buffer
pub fn record_boot_step(step: u32) {
    let prev = CURRENT_STEP.fetch_add(1, Ordering::SeqCst);
    let idx = (prev as usize) % MAX_BOOT_STEPS;

    // SAFETY: Atomic access ensures no data races
    unsafe {
        BOOT_STEP_HISTORY[idx] = step;
    }
}

/// Get the last N boot steps for crash reporting
pub fn get_last_steps(out: &mut [u32]) -> usize {
    let current = CURRENT_STEP.load(Ordering::SeqCst) as usize;
    let count = out.len().min(current).min(MAX_BOOT_STEPS);

    // SAFETY: We're reading from a static buffer with proper bounds
    unsafe {
        for i in 0..count {
            let idx = (current.wrapping_sub(count - i)) % MAX_BOOT_STEPS;
            out[i] = BOOT_STEP_HISTORY[idx];
        }
    }

    count
}

/// Get current CPU ID (simplified - we only support single CPU for now)
#[inline]
pub fn get_cpu_id() -> u32 {
    0
}

/// Get current CR3 register value
#[inline]
pub fn get_cr3() -> u64 {
    Cr3::read().0.start_address().as_u64()
}

/// Get current RSP register value
#[inline]
pub fn get_rsp() -> u64 {
    let rsp: u64;
    // SAFETY: Reading RSP is safe
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack));
    }
    rsp
}

/// Log a boot step with diagnostics
///
/// Prints: "BOOT STEP {n} cpu={cpu} cr3={cr3:#x} rsp={rsp:#x}"
#[macro_export]
macro_rules! BOOT_STEP {
    ($step:expr) => {{
        $crate::boot_diagnostics::record_boot_step($step);
        let cpu = $crate::boot_diagnostics::get_cpu_id();
        let cr3 = $crate::boot_diagnostics::get_cr3();
        let rsp = $crate::boot_diagnostics::get_rsp();
        serial_println!("BOOT STEP {} cpu={} cr3={:#x} rsp={:#x}", $step, cpu, cr3, rsp);
    }};
}

/// Assert a boot condition with crash code
///
/// If condition is false, prints error and exits QEMU with failure
#[macro_export]
macro_rules! BOOT_ASSERT {
    ($cond:expr, $code:expr) => {{
        if !($cond) {
            let step =
                $crate::boot_diagnostics::CURRENT_STEP.load(core::sync::atomic::Ordering::SeqCst);
            serial_println!("BOOT ASSERT FAIL code={:#x} step={}", $code, step);
            $crate::exit_qemu($crate::QemuExitCode::Failed);
        }
    }};
}

/// Dump boot diagnostics on panic
pub fn dump_boot_diagnostics() {
    serial_println!("=== Boot Diagnostics ===");
    serial_println!("CPU: {}", get_cpu_id());
    serial_println!("CR3: {:#x}", get_cr3());
    serial_println!("RSP: {:#x}", get_rsp());

    let mut steps = [0u32; 16];
    let count = get_last_steps(&mut steps);

    if count > 0 {
        serial_println!("Last {} boot steps:", count);
        for i in 0..count {
            serial_println!("  [{}] step {}", i, steps[i]);
        }
    } else {
        serial_println!("No boot steps recorded");
    }
}
