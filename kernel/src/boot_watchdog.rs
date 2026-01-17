//! Boot watchdog for detecting boot hangs
//!
//! This module provides a simple watchdog that can detect if the kernel
//! fails to complete boot within a reasonable time. It's feature-gated
//! and only active when the `boot-watchdog` feature is enabled.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Early in boot
//! boot_watchdog::start(timeout_ticks);
//!
//! // When boot completes successfully
//! boot_watchdog::stop();
//!
//! // In timer interrupt handler
//! boot_watchdog::tick();
//! ```

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Maximum number of timer ticks before timeout
static WATCHDOG_TIMEOUT: AtomicU32 = AtomicU32::new(0);

/// Current tick count
static WATCHDOG_TICKS: AtomicU32 = AtomicU32::new(0);

/// Whether watchdog is active
static WATCHDOG_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Start the boot watchdog
///
/// # Arguments
///
/// * `timeout_ticks` - Number of timer ticks before timeout (e.g., 3000 for 30s at 100Hz)
pub fn start(timeout_ticks: u32) {
    WATCHDOG_TIMEOUT.store(timeout_ticks, Ordering::SeqCst);
    WATCHDOG_TICKS.store(0, Ordering::SeqCst);
    WATCHDOG_ACTIVE.store(true, Ordering::SeqCst);

    serial_println!("[WATCHDOG] Boot watchdog started (timeout: {} ticks)", timeout_ticks);
}

/// Stop the boot watchdog (boot completed successfully)
pub fn stop() {
    if WATCHDOG_ACTIVE.load(Ordering::SeqCst) {
        WATCHDOG_ACTIVE.store(false, Ordering::SeqCst);
        let ticks = WATCHDOG_TICKS.load(Ordering::SeqCst);
        serial_println!("[WATCHDOG] Boot watchdog stopped after {} ticks", ticks);
    }
}

/// Tick the watchdog (called from timer interrupt)
///
/// Returns `true` if timeout occurred, `false` otherwise.
pub fn tick() -> bool {
    if !WATCHDOG_ACTIVE.load(Ordering::SeqCst) {
        return false;
    }

    let ticks = WATCHDOG_TICKS.fetch_add(1, Ordering::SeqCst) + 1;
    let timeout = WATCHDOG_TIMEOUT.load(Ordering::SeqCst);

    if ticks >= timeout {
        serial_println!("\n╔════════════════════════════════════════════════════════════════╗");
        serial_println!("║                      BOOT TIMEOUT                              ║");
        serial_println!("╚════════════════════════════════════════════════════════════════╝");
        serial_println!();
        serial_println!("Boot failed to complete within {} ticks", timeout);
        serial_println!("Last boot step: {}", crate::boot_diagnostics::get_current_step());

        // Dump boot diagnostics
        crate::boot_diagnostics::dump_boot_diagnostics();

        return true;
    }

    false
}

/// Get current watchdog tick count
pub fn get_ticks() -> u32 {
    WATCHDOG_TICKS.load(Ordering::SeqCst)
}

/// Check if watchdog is active
pub fn is_active() -> bool {
    WATCHDOG_ACTIVE.load(Ordering::SeqCst)
}
