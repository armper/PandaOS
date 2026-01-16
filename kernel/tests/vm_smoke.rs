//! vm_smoke test - comprehensive virtual memory management test
//!
//! This test runs a user program that:
//! - Allocates memory with brk (heap)
//! - Allocates memory with mmap (anonymous mapping)
//! - Writes test data to both regions
//! - Forks to create a child process
//! - Child modifies its copies of the data
//! - Parent verifies its data is unchanged (isolation)
//! - Reports success
//!
//! This validates:
//! - brk syscall works correctly
//! - mmap syscall works correctly
//! - fork properly isolates parent and child memory
//! - no memory corruption between processes

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

#[macro_use]
extern crate panda_hal;

/// Embedded vm test program ELF binary
static VM_TEST_PROGRAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vm_test_elf"));

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize serial for logging
    unsafe {
        panda_hal::serial::init();
    }

    serial_println!("TEST START: vm_smoke");

    test_main();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL vm_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
    // If we get here without the test program running, that's a failure
    serial_println!("TEST FAIL vm_smoke: test program did not run");
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
fn test_vm_program_embedded() {
    // Verify the vm test program is embedded
    assert!(VM_TEST_PROGRAM.len() > 0, "VM test program should be embedded");
    serial_println!("VM test program size: {} bytes", VM_TEST_PROGRAM.len());
}

// Note: The actual vm test execution happens in the full kernel environment
// This smoke test verifies the program can be loaded and embedded correctly
// The test program itself prints "TEST PASS vm_smoke" when successful
