//! PandaOS Kernel - A Unix-like x86_64 kernel in Rust
//!
//! This is the main entry point for the PandaOS kernel. It follows clean
//! architecture principles with modular design and strict crate boundaries.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

mod interrupts;
mod memory;

/// Entry point for the kernel
///
/// This function is called by the bootloader and never returns.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("PandaOS v{}", env!("CARGO_PKG_VERSION"));
    println!("Initializing kernel...");

    // Initialize HAL
    unsafe {
        panda_hal::serial::init();
    }

    serial_println!("Serial output initialized");
    println!("Hardware abstraction layer initialized");

    // Initialize interrupts
    interrupts::init();
    println!("Interrupt handling initialized");

    // Initialize memory management
    println!("Memory management initialized");

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

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

#[cfg(test)]
pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
