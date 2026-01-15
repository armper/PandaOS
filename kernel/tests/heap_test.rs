//! Integration tests for heap allocator
//!
//! These tests run in QEMU and verify that heap allocation works correctly.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::vec::Vec;
use core::panic::PanicInfo;
use panda_kernel::{exit_qemu, serial_print, serial_println, QemuExitCode};

#[no_mangle]
pub extern "C" fn _start(boot_info: &'static bootloader::BootInfo) -> ! {
    // Initialize kernel subsystems
    unsafe {
        panda_kernel::init_for_test(boot_info);
    }

    test_main();

    exit_qemu(QemuExitCode::Success);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[failed]");
    serial_println!("Error: {}", info);
    exit_qemu(QemuExitCode::Failed);
}

fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    serial_println!("All tests passed!");
}

#[test_case]
fn heap_smoke_test() {
    serial_print!("heap_smoke_test...");

    // Allocate a vector
    let mut vec = Vec::new();

    // Write pattern
    for i in 0..100 {
        vec.push(i);
    }

    // Verify pattern
    for (i, &val) in vec.iter().enumerate() {
        assert_eq!(val, i, "Heap corruption detected at index {}", i);
    }

    // Free (implicit when vec goes out of scope)
    drop(vec);

    serial_println!("[ok]");
    serial_println!("TEST PASS heap_smoke");
}

#[test_case]
fn heap_multiple_allocations() {
    serial_print!("heap_multiple_allocations...");

    // Allocate multiple vectors
    let vec1: Vec<u8> = (0..50).collect();
    let vec2: Vec<u16> = (0..50).collect();
    let vec3: Vec<u32> = (0..50).collect();

    // Verify all
    assert_eq!(vec1.len(), 50);
    assert_eq!(vec2.len(), 50);
    assert_eq!(vec3.len(), 50);

    serial_println!("[ok]");
}
