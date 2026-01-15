//! QEMU integration tests for PandaOS subsystems
//!
//! Each test boots the kernel, exercises one subsystem, and exits cleanly.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    test_main();
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
}

#[test_case]
fn test_frame_allocator_subsystem() {
    use panda_hal::memory::FrameAllocator;
    static mut BITMAP_STORAGE: [u8; 2] = [0; 2];
    // SAFETY: This test uses a single-threaded static buffer.
    let bitmap = unsafe { &mut BITMAP_STORAGE };
    let mut allocator = FrameAllocator::new(0, 10, bitmap);
    assert_eq!(allocator.allocate_frame(), Some(0));
}

#[test_case]
fn test_pid_allocator_subsystem() {
    use panda_hal::pid::PidAllocator;
    let allocator = PidAllocator::new(1);
    let pid1 = allocator.allocate();
    assert_eq!(pid1.as_u64(), 1);
}
