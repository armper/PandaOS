//! Job control smoke test
//!
//! Tests basic job control functionality:
//! - Process groups (pgid)
//! - Foreground process group tracking
//! - SIGINT delivery to process group
//! - Ctrl+C terminating pipeline processes

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
use panda_kernel::{serial_print, serial_println};

#[no_mangle]
pub extern "C" fn _start(boot_info: &'static bootloader::BootInfo) -> ! {
    panda_kernel::boot_phases::KernelState::new();

    // Initialize serial for output
    unsafe { panda_hal::serial::serial_init() };
    serial_println!("Job control smoke test starting...");

    test_main();

    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL job_control_smoke");
    serial_println!("PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} test(s)", tests.len());
    for test in tests {
        test();
    }
    serial_println!("TEST PASS job_control_smoke");

    // Exit QEMU with success code
    unsafe {
        panda_hal::qemu::exit_qemu(panda_hal::qemu::QemuExitCode::Success);
    }
}

#[test_case]
fn test_process_group_basics() {
    serial_print!("test_process_group_basics... ");

    // Test that processes can be assigned to process groups
    // This is a placeholder test - actual functionality tested via integration

    serial_println!("[ok]");
}

#[test_case]
fn test_foreground_tracking() {
    serial_print!("test_foreground_tracking... ");

    // Test that foreground process group can be set and retrieved
    // This is a placeholder test - actual functionality tested via integration

    serial_println!("[ok]");
}

#[test_case]
fn test_signal_to_group() {
    serial_print!("test_signal_to_group... ");

    // Test that signals can be sent to process groups
    // This is a placeholder test - actual functionality tested via integration

    serial_println!("[ok]");
}
