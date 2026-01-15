//! Higher-half smoke test
//!
//! This test verifies that the kernel can operate with higher-half mapping:
//! - Boot kernel with higher-half infrastructure
//! - Write/read a kernel static variable
//! - Allocate and verify heap memory
//! - Exit successfully

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::vec::Vec;
use core::panic::PanicInfo;
use panda_kernel::{exit_qemu, serial_print, serial_println, QemuExitCode};

/// Test global static variable
static mut TEST_STATIC: u64 = 0xDEADBEEF;

#[no_mangle]
pub extern "C" fn _start(boot_info: &'static bootloader::BootInfo) -> ! {
    // Initialize kernel subsystems
    unsafe {
        panda_kernel::init_for_test(boot_info);
    }

    serial_println!("TEST START: higher_half_smoke");

    test_main();

    serial_println!("TEST PASS higher_half_smoke");
    exit_qemu(QemuExitCode::Success);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL higher_half_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
}

fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} higher-half smoke tests", tests.len());
    for test in tests {
        test();
    }
}

#[test_case]
fn test_static_variable_access() {
    serial_print!("test_static_variable_access...");
    
    // Read initial value
    let initial = unsafe { TEST_STATIC };
    assert_eq!(initial, 0xDEADBEEF, "Initial value incorrect");
    
    // Write new value
    unsafe {
        TEST_STATIC = 0xCAFEBABE;
    }
    
    // Read back and verify
    let new_value = unsafe { TEST_STATIC };
    assert_eq!(new_value, 0xCAFEBABE, "Write/read failed");
    
    // Restore original value
    unsafe {
        TEST_STATIC = 0xDEADBEEF;
    }
    
    serial_println!("[ok]");
}

#[test_case]
fn test_heap_allocation() {
    serial_print!("test_heap_allocation...");
    
    // Allocate a vector on the heap
    let mut heap_vec = Vec::new();
    
    // Write pattern to heap
    for i in 0..100 {
        heap_vec.push(i * 2);
    }
    
    // Verify pattern
    for (idx, &val) in heap_vec.iter().enumerate() {
        assert_eq!(val, (idx * 2) as i32, "Heap data corrupted at index {}", idx);
    }
    
    serial_println!("[ok] - allocated and verified 100 elements");
}

#[test_case]
fn test_heap_multiple_allocations() {
    serial_print!("test_heap_multiple_allocations...");
    
    // Allocate multiple structures on heap
    let vec1 = (0..50).collect::<Vec<_>>();
    let vec2 = (50..100).collect::<Vec<_>>();
    let vec3 = (100..150).collect::<Vec<_>>();
    
    // Verify all allocations
    assert_eq!(vec1.len(), 50);
    assert_eq!(vec2.len(), 50);
    assert_eq!(vec3.len(), 50);
    
    // Verify data integrity
    assert_eq!(vec1[0], 0);
    assert_eq!(vec1[49], 49);
    assert_eq!(vec2[0], 50);
    assert_eq!(vec2[49], 99);
    assert_eq!(vec3[0], 100);
    assert_eq!(vec3[49], 149);
    
    serial_println!("[ok] - 3 vectors allocated successfully");
}

#[test_case]
fn test_function_pointer_execution() {
    serial_print!("test_function_pointer_execution...");
    
    // Define a simple function
    fn test_fn(a: u32, b: u32) -> u32 {
        a + b
    }
    
    // Get function pointer
    let fn_ptr: fn(u32, u32) -> u32 = test_fn;
    
    // Call through function pointer
    let result = fn_ptr(42, 58);
    assert_eq!(result, 100, "Function pointer call failed");
    
    serial_println!("[ok]");
}

#[test_case]
fn test_kernel_constants() {
    serial_print!("test_kernel_constants...");
    
    // Verify kernel constants are accessible
    let virt_base = panda_kernel::linker_symbols::KERNEL_VIRT_BASE;
    let phys_base = panda_kernel::linker_symbols::KERNEL_PHYS_BASE;
    
    // Verify constants are reasonable
    assert!(virt_base > 0xFFFF_0000_0000_0000, "KERNEL_VIRT_BASE not in higher-half");
    assert!(phys_base > 0, "KERNEL_PHYS_BASE is zero");
    assert!(phys_base < 0x1000_0000, "KERNEL_PHYS_BASE too high");
    
    serial_println!("[ok] - virt_base={:#x}, phys_base={:#x}", virt_base, phys_base);
}
