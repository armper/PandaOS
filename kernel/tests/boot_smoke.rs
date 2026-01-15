//! Boot smoke test - verifies kernel boots and initializes correctly

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize serial for logging
    unsafe {
        panda_hal::serial::init();
    }

    serial_println!("TEST START: boot_smoke");

    test_main();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL boot_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
    serial_println!("TEST PASS: boot_smoke");
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
fn test_serial_initialized() {
    serial_println!("Serial is working");
}

#[test_case]
fn test_basic_arithmetic() {
    assert_eq!(2 + 2, 4);
}

// Import serial_println macro
#[macro_use]
extern crate panda_hal;
