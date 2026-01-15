//! PandaOS Kernel - A Unix-like x86_64 kernel in Rust
//!
//! This is the main entry point for the PandaOS kernel. It follows clean
//! architecture principles with modular design and strict crate boundaries.
//!
//! ## Invariants
//!
//! - No allocation before heap is initialized
//! - All unsafe code is in arch_x86_64 or driver modules
//! - Subsystems are initialized explicitly and passed by reference
//! - Hardware access goes through HAL only

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_panics_doc)]
#![allow(stable_features)]

extern crate alloc;

use core::panic::PanicInfo;

// Import VGA and serial macros
#[macro_use]
extern crate panda_hal;

pub mod boot_phases;
pub mod context;
pub mod context_switch;
pub mod elf;
pub mod gdt;
pub mod heap;
pub mod interrupts;
pub mod invariants;
pub mod linker_symbols;
pub mod memory;
pub mod page_table_tracker;
pub mod paging;
pub mod pic;
pub mod process;
pub mod scheduler;
pub mod syscall;
pub mod timer;
pub mod usermode;

/// Entry point for the kernel
///
/// This function is called by the bootloader and never returns.
/// The bootloader passes a BootInfo structure with memory map and other info.
#[no_mangle]
pub extern "C" fn _start(boot_info: &'static bootloader::BootInfo) -> ! {
    // Use boot phase state machine to enforce initialization order
    use boot_phases::KernelState;

    let state = KernelState::new();

    // SAFETY: This is the first initialization call during boot
    let state = unsafe { state.init_hal() };

    serial_println!("Serial output initialized");
    println!("PandaOS v{}", env!("CARGO_PKG_VERSION"));
    println!("Hardware abstraction layer initialized");

    // SAFETY: HAL is now initialized, safe to proceed
    let state = unsafe { state.init_memory() };

    // Initialize memory management with bootloader info (no bootloader types exposed)
    unsafe { memory::init_from_bootloader(boot_info) };

    // SAFETY: Memory is now initialized, safe to proceed
    let state = unsafe { state.init_interrupts() };

    // Initialize paging infrastructure
    unsafe {
        paging::init_identity_map_minimal().expect("Failed to initialize identity mapping");
        paging::init_higher_half_mapping().expect("Failed to initialize higher-half mapping");
    }
    println!("Paging infrastructure initialized");

    // Initialize GDT (must be before interrupts are enabled)
    unsafe { gdt::init() };
    println!("GDT initialized");

    // Initialize interrupts (after GDT)
    interrupts::init();
    println!("Interrupt handling initialized");

    // Initialize syscall/sysret support (after GDT and interrupts)
    unsafe { usermode::init_syscall() };
    println!("Syscall/sysret initialized");

    // Map heap region (allocate frames and map pages)
    // MUST happen before heap allocator init
    unsafe {
        heap::map_heap().expect("Failed to map heap");
    }
    println!("Heap region mapped");

    // Initialize heap allocator (after heap is mapped)
    unsafe { heap::init() };
    println!("Heap allocator initialized");

    // Test heap allocation
    {
        use alloc::vec::Vec;
        let mut test_vec = Vec::new();
        test_vec.push(1);
        test_vec.push(2);
        test_vec.push(3);
        println!("Heap test passed: {:?}", test_vec);
    }

    // Finalize boot
    let _state = state.finalize();
    println!("Kernel initialization complete!");

    #[cfg(test)]
    test_main();

    println!("Kernel is running. Halting CPU.");

    loop {
        x86_64::instructions::hlt();
    }
}

/// Panic handler for the kernel
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    serial_println!("KERNEL PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}

/// Allocation error handler
#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("Allocation error: {:?}", layout)
}

/// Initialize kernel for testing
///
/// # Safety
///
/// Must be called once at test start with valid boot info
pub unsafe fn init_for_test(boot_info: &'static bootloader::BootInfo) {
    // Use boot phase state machine
    use boot_phases::KernelState;

    let state = KernelState::new();

    // Initialize HAL
    let state = unsafe { state.init_hal() };

    // Initialize memory
    let state = unsafe { state.init_memory() };
    unsafe { memory::init_from_bootloader(boot_info) };

    // Initialize GDT and interrupts
    let state = unsafe { state.init_interrupts() };
    unsafe { gdt::init() };
    interrupts::init();

    // Initialize paging
    unsafe {
        paging::init_identity_map_minimal().expect("Failed to initialize identity mapping");
        paging::init_higher_half_mapping().expect("Failed to initialize higher-half mapping");
    }

    // Map and initialize heap
    unsafe {
        heap::map_heap().expect("Failed to map heap");
        heap::init();
    }

    let _state = state.finalize();
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    exit_qemu(QemuExitCode::Success);
}

/// QEMU exit codes for integration testing

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

/// Exit QEMU using isa-debug-exit device

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
fn trivial_assertion() {
    assert_eq!(1, 1);
}
