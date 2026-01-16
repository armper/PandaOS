//! Serial smoke test - verifies serial output works reliably
//!
//! This is the most minimal test: just boot, print, and exit.
//! If serial output isn't working, this test will fail.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

#[macro_use]
extern crate panda_hal;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize serial IMMEDIATELY - this is the first and only requirement
    // SAFETY: This is the first and only call to serial::init during boot
    unsafe {
        panda_hal::serial::init();
    }

    // Print marker that serial is working
    serial_println!("[BOOT] serial ok");

    // Run test framework
    test_main();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Even panics should go to serial
    serial_println!("TEST FAIL serial_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} test(s)", tests.len());
    for test in tests {
        test();
    }
    serial_println!("TEST PASS serial_smoke");
    exit_qemu(QemuExitCode::Success);
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

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
fn test_serial_write() {
    serial_println!("Serial output is working");
}

#[test_case]
fn test_early_boot_marker() {
    // This verifies we can print during early boot
    serial_println!("Early boot marker visible");
}
