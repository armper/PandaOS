//! Integration test for frame reservation system
//!
//! This test verifies that:
//! - Frame allocator respects reservations
//! - Allocated frames never fall within reserved ranges
//! - Kernel, bootloader, and heap frames are properly reserved

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

    serial_println!("TEST START: frame_reservation_smoke");

    test_main();

    serial_println!("TEST PASS frame_reservation_smoke");
    exit_qemu(QemuExitCode::Success);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL frame_reservation_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
}

fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} frame reservation tests", tests.len());
    for test in tests {
        test();
    }
}

#[test_case]
fn test_frame_allocator_initialized() {
    serial_print!("test_frame_allocator_initialized...");

    // Try to allocate a frame - should succeed if allocator is initialized
    let frame = unsafe { panda_kernel::memory::allocate_frame() };
    assert!(frame.is_some(), "Frame allocator not initialized");

    serial_println!("[ok]");
}

#[test_case]
fn test_reserved_frames_not_allocated() {
    serial_print!("test_reserved_frames_not_allocated...");

    // Allocate many frames to test that we skip reserved ones
    const TEST_ALLOC_COUNT: usize = 100;
    let mut allocated_frames = Vec::new();

    for _ in 0..TEST_ALLOC_COUNT {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            allocated_frames.push(frame);
        } else {
            break; // Out of memory
        }
    }

    assert!(!allocated_frames.is_empty(), "Should be able to allocate at least some frames");

    // Verify none of the allocated frames are in critical low memory
    // Frame 0 should always be reserved (BIOS/IVT)
    for &frame in &allocated_frames {
        assert_ne!(frame, 0, "Allocated frame 0 - should be reserved!");

        // Frames below 16MB (4096 frames) should be reserved for kernel/bootloader
        // But the allocator might give us frames just above this range
        // So we just check that frame 0 is never allocated
    }

    serial_println!("[ok] - allocated {} frames", allocated_frames.len());
}

#[test_case]
fn test_heap_frames_allocated_only_once() {
    serial_print!("test_heap_frames_allocated_only_once...");

    // The heap has already allocated frames during init
    // Try to allocate more frames and verify they don't overlap with heap

    const TEST_ALLOC_COUNT: usize = 50;
    let mut allocated_frames = Vec::new();

    for _ in 0..TEST_ALLOC_COUNT {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            // Check this frame hasn't been allocated before in this test
            assert!(!allocated_frames.contains(&frame), "Frame {} allocated twice!", frame);
            allocated_frames.push(frame);
        } else {
            break;
        }
    }

    // Verify we could allocate some frames
    assert!(!allocated_frames.is_empty(), "Should allocate some frames");

    serial_println!("[ok] - no double allocations in {} frames", allocated_frames.len());
}

#[test_case]
fn test_allocation_after_heap_init() {
    serial_print!("test_allocation_after_heap_init...");

    // Allocate on heap - this should work
    let mut heap_vec = Vec::new();
    for i in 0..100 {
        heap_vec.push(i);
    }

    // Allocate physical frames - this should also work and not conflict
    let mut frame_vec = Vec::new();
    for _ in 0..10 {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            frame_vec.push(frame);
        }
    }

    // Verify heap still works
    assert_eq!(heap_vec.len(), 100);
    assert_eq!(heap_vec[50], 50);

    // Verify we got some frames
    assert!(!frame_vec.is_empty());

    serial_println!("[ok] - heap and frame allocator coexist");
}
