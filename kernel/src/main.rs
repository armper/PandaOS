//! PandaOS Kernel - A Unix-like x86_64 kernel in Rust
//!
//! This is the main entry point for the PandaOS kernel. It follows clean
//! architecture principles with modular design and strict crate boundaries.
//!
//! ## Invariants
//!
//! - No allocation before heap is initialized
//! - All unsafe code is in arch_x86_64 or driver modules
//! - Subsystems are initialized explicitly and passed by reference
//! - Hardware access goes through HAL only

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_panics_doc)]

use core::panic::PanicInfo;

// Import VGA and serial macros
#[macro_use]
extern crate panda_hal;

pub mod boot_phases;
pub mod elf;
pub mod interrupts;
pub mod invariants;
pub mod memory;
pub mod paging;
pub mod process;
pub mod syscall;
pub mod usermode;

/// Entry point for the kernel
///
/// This function is called by the bootloader and never returns.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Use boot phase state machine to enforce initialization order
    use boot_phases::KernelState;

    let state = KernelState::new();

    println!("PandaOS v{}", env!("CARGO_PKG_VERSION"));
    println!("Initializing kernel with boot phase enforcement...");

    // SAFETY: This is the first initialization call during boot
    let state = unsafe { state.init_hal() };

    serial_println!("Serial output initialized");
    println!("Hardware abstraction layer initialized");

    // SAFETY: HAL is now initialized, safe to proceed
    let state = unsafe { state.init_memory() };
    println!("Memory management initialized");

    // SAFETY: Memory is now initialized, safe to proceed
    let state = unsafe { state.init_interrupts() };

    // Initialize interrupts
    interrupts::init();
    println!("Interrupt handling initialized");

    // Finalize boot
    let _state = state.finalize();
    println!("Kernel initialization complete!");

    #[cfg(test)]
    test_main();

    println!("Kernel is running. Halting CPU.");

    loop {
        x86_64::instructions::hlt();
    }
}

/// Panic handler for the kernel
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    serial_println!("KERNEL PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    exit_qemu(QemuExitCode::Success);
}

/// QEMU exit codes for integration testing
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

/// Exit QEMU using isa-debug-exit device
#[cfg(test)]
pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }

    loop {
        x86_64::instructions::hlt();
    }
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
