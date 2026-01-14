//! Hardware Abstraction Layer (HAL) for PandaOS
//!
//! This crate provides hardware-independent abstractions for interacting with
//! x86_64 hardware. It follows a clean architecture pattern where hardware-specific
//! code is isolated behind trait boundaries.

#![no_std]
#![warn(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

// Pure logic modules (always available, testable on host)
pub mod bitmap;
pub mod memory;
pub mod pid;
pub mod ringbuffer;

// Hardware-specific modules (only with hardware feature)
#[cfg(feature = "hardware")]
pub mod serial;
#[cfg(feature = "hardware")]
pub mod vga;

/// Initialize the HAL subsystems
///
/// # Safety
///
/// This function must be called only once during kernel initialization.
/// It performs hardware initialization that affects global state.
#[cfg(feature = "hardware")]
pub unsafe fn init() {
    // HAL initialization logic will go here
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hal_module_exists() {
        // Placeholder test to ensure the module compiles
    }
}
