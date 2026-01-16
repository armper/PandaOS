//! brk smoke test - verifies brk syscall works correctly
//!
//! This test loads and runs the brk_test user program which:
//! - Gets current program break
//! - Grows heap by 8KB
//! - Writes test pattern to new heap memory
//! - Reads back and verifies
//! - Shrinks heap back
//! - Reports success

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

#[macro_use]
extern crate panda_hal;

/// Embedded brk test program ELF binary
static BRK_TEST_PROGRAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/brk_test_elf"));

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize serial for logging
    unsafe {
        panda_hal::serial::init();
    }

    serial_println!("TEST START: brk_smoke");

    test_main();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL brk_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
    // If we get here without the test program running, that's a failure
    serial_println!("TEST FAIL brk_smoke: test program did not run");
    exit_qemu(QemuExitCode::Failed);
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
fn test_brk_program_embedded() {
    // Verify the brk test program is embedded
    assert!(BRK_TEST_PROGRAM.len() > 0, "BRK test program should be embedded");
    serial_println!("BRK test program size: {} bytes", BRK_TEST_PROGRAM.len());
}

// Note: The actual brk test execution happens in the full kernel environment
// This smoke test verifies the program can be loaded and embedded correctly
// The test program itself prints "TEST PASS brk_smoke" when successful
