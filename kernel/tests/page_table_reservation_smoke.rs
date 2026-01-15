//! Page table reservation smoke test
//!
//! This test verifies that:
//! - Page table frames are tracked
//! - Allocated frames never equal any page table frame
//! - Page table reservation works correctly

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

    serial_println!("TEST START: page_table_reservation_smoke");

    test_main();

    serial_println!("TEST PASS page_table_reservation_smoke");
    exit_qemu(QemuExitCode::Success);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("TEST FAIL page_table_reservation_smoke: {}", info);
    exit_qemu(QemuExitCode::Failed);
}

fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} page table reservation tests", tests.len());
    for test in tests {
        test();
    }
}

#[test_case]
fn test_page_table_tracker_initialized() {
    serial_print!("test_page_table_tracker_initialized...");
    
    // Check that at least one page table frame is tracked (the L4 frame)
    let pt_count = panda_kernel::page_table_tracker::page_table_frame_count();
    assert!(pt_count > 0, "No page table frames tracked");
    
    serial_println!("[ok] - {} page table frames tracked", pt_count);
}

#[test_case]
fn test_allocated_frames_not_page_tables() {
    serial_print!("test_allocated_frames_not_page_tables...");
    
    // Get list of page table frames
    let pt_frames = panda_kernel::page_table_tracker::get_page_table_frames();
    assert!(!pt_frames.is_empty(), "No page table frames tracked");
    
    serial_println!("  Page table frames: {:?}", &pt_frames[..pt_frames.len().min(5)]);
    
    // Allocate many frames
    const TEST_ALLOC_COUNT: usize = 200;
    let mut allocated_frames = Vec::new();
    
    for _ in 0..TEST_ALLOC_COUNT {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            allocated_frames.push(frame);
        } else {
            break; // Out of memory
        }
    }
    
    assert!(!allocated_frames.is_empty(), "Should allocate at least some frames");
    serial_println!("  Allocated {} frames for testing", allocated_frames.len());
    
    // Verify no allocated frame is a page table frame
    for &frame in &allocated_frames {
        for &pt_frame in &pt_frames {
            assert_ne!(
                frame, pt_frame,
                "Frame {} was allocated but is a page table frame!",
                frame
            );
        }
    }
    
    serial_println!("[ok] - no overlap between allocated and page table frames");
}

#[test_case]
fn test_page_table_frames_reserved() {
    serial_print!("test_page_table_frames_reserved...");
    
    // Get page table frames
    let pt_frames = panda_kernel::page_table_tracker::get_page_table_frames();
    
    // Allocate many frames to test that page table frames are skipped
    const TEST_ALLOC_COUNT: usize = 150;
    let mut allocated_frames = Vec::new();
    
    for _ in 0..TEST_ALLOC_COUNT {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            // Verify this frame is not a page table frame
            assert!(
                !panda_kernel::page_table_tracker::is_page_table_frame(frame),
                "Allocated a page table frame: {}",
                frame
            );
            allocated_frames.push(frame);
        } else {
            break;
        }
    }
    
    assert!(!allocated_frames.is_empty(), "Should allocate some frames");
    
    serial_println!(
        "[ok] - {} frames allocated, none are page tables",
        allocated_frames.len()
    );
}

#[test_case]
fn test_no_double_allocation_with_page_tables() {
    serial_print!("test_no_double_allocation_with_page_tables...");
    
    // Allocate frames and track them
    const TEST_ALLOC_COUNT: usize = 100;
    let mut allocated_frames = Vec::new();
    
    for _ in 0..TEST_ALLOC_COUNT {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            // Check this frame hasn't been allocated before
            assert!(
                !allocated_frames.contains(&frame),
                "Frame {} allocated twice!",
                frame
            );
            
            // Check this frame is not a page table frame
            assert!(
                !panda_kernel::page_table_tracker::is_page_table_frame(frame),
                "Frame {} is a page table frame!",
                frame
            );
            
            allocated_frames.push(frame);
        } else {
            break;
        }
    }
    
    assert!(!allocated_frames.is_empty(), "Should allocate some frames");
    
    serial_println!(
        "[ok] - {} frames, no duplicates, no page table frames",
        allocated_frames.len()
    );
}

#[test_case]
fn test_heap_and_page_tables_coexist() {
    serial_print!("test_heap_and_page_tables_coexist...");
    
    // Get page table frames
    let pt_frames = panda_kernel::page_table_tracker::get_page_table_frames();
    
    // Allocate on heap (which uses physical frames internally)
    let mut heap_vec = Vec::new();
    for i in 0..200 {
        heap_vec.push(i);
    }
    
    // Verify heap still works
    assert_eq!(heap_vec.len(), 200);
    assert_eq!(heap_vec[100], 100);
    
    // Allocate more physical frames
    let mut frame_vec = Vec::new();
    for _ in 0..50 {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            // Verify not a page table frame
            assert!(
                !pt_frames.contains(&frame),
                "Allocated a page table frame: {}",
                frame
            );
            frame_vec.push(frame);
        }
    }
    
    assert!(!frame_vec.is_empty(), "Should allocate some frames");
    
    serial_println!(
        "[ok] - heap and frame allocator coexist, {} heap elements, {} frames",
        heap_vec.len(),
        frame_vec.len()
    );
}

#[test_case]
fn test_page_table_frame_count_stable() {
    serial_print!("test_page_table_frame_count_stable...");
    
    // Get initial count
    let initial_count = panda_kernel::page_table_tracker::page_table_frame_count();
    assert!(initial_count > 0, "No page table frames tracked");
    
    // Allocate some frames
    let mut allocated = Vec::new();
    for _ in 0..50 {
        if let Some(frame) = unsafe { panda_kernel::memory::allocate_frame() } {
            allocated.push(frame);
        }
    }
    
    // Count should remain stable (we're not allocating new page tables)
    let final_count = panda_kernel::page_table_tracker::page_table_frame_count();
    assert_eq!(
        initial_count, final_count,
        "Page table count changed unexpectedly"
    );
    
    serial_println!("[ok] - page table count stable at {}", final_count);
}
