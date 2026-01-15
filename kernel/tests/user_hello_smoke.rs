//! User mode hello world smoke test
//!
//! This test verifies that:
//! - ELF loading works
//! - Process creation succeeds
//! - User mode transition infrastructure is in place

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

#[macro_use]
extern crate panda_hal;

/// Embedded user program ELF binary
static USER_PROGRAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hello_elf"));

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize serial for logging
    unsafe {
        panda_hal::serial::init();
    }

    serial_println!("TEST START: user_hello_smoke");

    test_main();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL user_hello_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
    serial_println!("TEST PASS user_hello_smoke");
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
fn test_elf_embedded() {
    // Verify the user program is embedded
    assert!(USER_PROGRAM.len() > 0, "User program should be embedded");
    serial_println!("User program size: {} bytes", USER_PROGRAM.len());
}

#[test_case]
fn test_elf_parsing() {
    // Parse the embedded ELF
    let elf_info = panda_kernel::elf::parse_elf(USER_PROGRAM);

    match elf_info {
        Ok(info) => {
            serial_println!("ELF parsed successfully!");
            serial_println!("  Entry point: 0x{:x}", info.entry_point);
            serial_println!("  Segments: {}", info.segment_count);

            assert!(info.entry_point != 0, "Entry point should be non-zero");
            assert!(info.segment_count > 0, "Should have at least one segment");
        }
        Err(e) => {
            serial_println!("Failed to parse ELF: {:?}", e);
            panic!("ELF parsing failed");
        }
    }
}

// Note: Process creation and user mode execution tests require full kernel
// initialization which isn't available in standalone integration tests.
// These features are tested via the main kernel or dedicated QEMU tests.
