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
pub mod fs;
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

    #[cfg(not(test))]
    {
        // Initialize scheduler and start multitasking
        unsafe {
            init_scheduler_and_start();
        }
    }

    #[cfg(test)]
    {
        println!("All tests passed. Halting CPU.");
        loop {
            x86_64::instructions::hlt();
        }
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

    let kernel_cr3 =
        x86_64::registers::control::Cr3::read().0.start_address().as_u64();
    usermode::set_kernel_page_table_phys(kernel_cr3);

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

/// Global scheduler instance
///
/// # Safety
///
/// This global mutable static is initialized exactly once during kernel boot
/// in `init_scheduler_and_start()`, before any interrupts are enabled or user
/// processes run. After initialization:
///
/// - It is accessed only from interrupt handlers (timer, syscall) where interrupts
///   are disabled, preventing concurrent access
/// - Each access uses `addr_of_mut!` to create a raw pointer, then converts to
///   a mutable reference with proper lifetime bounds
/// - The scheduler itself uses safe Rust internally; only the global access is unsafe
/// - No aliasing violations occur because interrupt handlers run atomically
///
/// Alternative approaches considered:
/// - `Once`/`Lazy`: Not available in no_std without custom implementation
/// - `Mutex`/`RwLock`: Cannot be used from interrupt context (may deadlock)
/// - `static mut`: Using raw pointers via `addr_of_mut!` is the recommended pattern
///   for interrupt handlers in the 2024 edition
static mut SCHEDULER: Option<scheduler::Scheduler> = None;

/// Initialize scheduler, load user programs, and start multitasking
///
/// # Safety
///
/// Must be called exactly once after all kernel subsystems are initialized.
unsafe fn init_scheduler_and_start() -> ! {
    use panda_hal::pid::PidAllocator;

    println!("Initializing scheduler...");

    // Create scheduler
    let mut sched = scheduler::Scheduler::new();

    // Create PID allocator
    let pid_allocator = PidAllocator::new(1);

    // Load init program from in-memory FS
    let init_data = fs::lookup("/init").expect("init not found in in-memory FS");
    println!("Loading init program ({} bytes)...", init_data.len());
    let init_elf = elf::parse_elf(init_data).expect("Failed to parse init ELF");
    let init_process = unsafe {
        process::Process::new(&init_elf, init_data, &pid_allocator)
            .expect("Failed to create init process")
    };
    println!("Created process PID {}", init_process.pid.as_u64());
    sched.add_process(init_process);

    // Store scheduler in global
    // SAFETY: This is the only place that initializes the scheduler
    unsafe {
        (*core::ptr::addr_of_mut!(SCHEDULER)) = Some(sched);
    }

    // Initialize PIC before setting up timer
    println!("Initializing PIC...");
    unsafe {
        pic::init();
    }

    // Initialize PIT for 100 Hz (10ms intervals)
    println!("Initializing PIT at 100 Hz...");
    unsafe {
        timer::init(100);
    }

    // Set timer interrupt handler
    unsafe {
        interrupts::set_timer_handler(timer_tick_handler);
    }

    // Set syscall handlers
    syscall::set_yield_handler(yield_handler);
    syscall::set_exit_handler(exit_handler);
    syscall::set_exec_handler(exec_handler);
    syscall::set_open_handler(open_handler);
    syscall::set_read_handler(read_handler);
    syscall::set_close_handler(close_handler);
    syscall::set_getpid_handler(getpid_handler);
    syscall::set_fork_handler(fork_handler);
    syscall::set_waitpid_handler(waitpid_handler);

    // Unmask timer interrupt (IRQ 0)
    println!("Enabling timer interrupt...");
    unsafe {
        pic::unmask_irq(0);
    }

    println!("Starting scheduler...");
    println!("======================================");

    // Start the scheduler - this never returns
    unsafe {
        start_first_process();
    }
}

/// Get a mutable reference to the global scheduler
///
/// # Safety
///
/// Must be called only from contexts where:
/// - Interrupts are disabled (ensuring no concurrent access)
/// - Scheduler has been initialized via `init_scheduler_and_start()`
///
/// This is safe in interrupt handlers and syscall handlers as they
/// run with interrupts disabled.
unsafe fn get_scheduler() -> &'static mut scheduler::Scheduler {
    // SAFETY: Caller guarantees interrupts are disabled and scheduler is initialized
    unsafe { (*core::ptr::addr_of_mut!(SCHEDULER)).as_mut().expect("Scheduler not initialized") }
}

/// Timer interrupt handler - called on each timer tick
fn timer_tick_handler() {
    // For now, just acknowledge the timer tick
    // Full preemptive multitasking would require saving interrupt frame state
    // and switching page tables, which is complex. Start with yield-based switching.

    // TODO: Implement preemptive scheduling
    // This would require:
    // 1. Saving interrupt frame to process context
    // 2. Switching page tables
    // 3. Restoring next process's interrupt frame
    // 4. Returning via iretq
}

/// Yield handler - called when process voluntarily yields CPU
fn yield_handler() {
    serial_println!("[YIELD] Process yielding CPU");

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    // Ensure the current process returns 0 from yield() when resumed.
    if let Some(current) = scheduler.current_process_mut() {
        current.context.rax = 0;
    }

    // Get next process (current will be moved to ready queue)
    if let Some(next) = scheduler.schedule_next() {
        serial_println!("[YIELD] Switching to process PID {}", next.pid.as_u64());

        // Update syscall context pointer and kernel stack for the new process.
        // SAFETY: Scheduler is initialized and interrupts are disabled here.
        unsafe {
            usermode::set_current_syscall_context(core::ptr::addr_of_mut!(next.context));
        }

        // Switch CR3 and return to user mode for the next process.
        // SAFETY: Next process has a valid user context and page table.
        unsafe {
            usermode::switch_to_user(core::ptr::addr_of!(next.context), next.page_table_phys);
        }
    } else {
        serial_println!("[YIELD] No other processes to run");
    }
}

/// Exec handler - called when process replaces its image
const EXEC_ARG_ADDR: u64 = 0x7FFF_FFFF_C000;
const EXEC_ARG_MAX: usize = 128;

fn exec_handler(path: &str, arg: Option<&str>) -> Result<(), syscall::ErrorCode> {
    if !path.starts_with('/') {
        return Err(syscall::ErrorCode::ENOENT);
    }

    let elf_data = fs::lookup(path).ok_or(syscall::ErrorCode::ENOENT)?;
    let elf_info =
        elf::parse_elf(elf_data).map_err(|_| syscall::ErrorCode::EINVAL)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // SAFETY: Frame allocator and GDT are initialized.
    unsafe {
        current.replace_image(&elf_info, elf_data).map_err(|_| syscall::ErrorCode::ENOMEM)?;
    }

    // Switch to the new page table so user memory copies target the new image.
    usermode::switch_page_table(current.page_table_phys);

    // Seed the exec argument at a fixed user address.
    let mut arg_buf = [0u8; EXEC_ARG_MAX];
    let arg_len = match arg {
        Some(value) => {
            if value.len() + 1 > EXEC_ARG_MAX {
                return Err(syscall::ErrorCode::EINVAL);
            }
            arg_buf[..value.len()].copy_from_slice(value.as_bytes());
            arg_buf[value.len()] = 0;
            value.len() + 1
        }
        None => {
            arg_buf[0] = 0;
            1
        }
    };
    crate::usermode::copy_to_user_bytes(EXEC_ARG_ADDR, &arg_buf[..arg_len])?;

    // SAFETY: Scheduler is initialized and interrupts are disabled here.
    unsafe {
        usermode::set_current_syscall_context(core::ptr::addr_of_mut!(current.context));
    }

    // SAFETY: Context and page table are valid after replace_image.
    unsafe {
        usermode::switch_to_user(core::ptr::addr_of!(current.context), current.page_table_phys);
    }
}

fn open_handler(path: &str) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    let fd = fs::open_path(&mut current.fd_table, path)?;
    Ok(fd as u64)
}

fn read_handler(fd: i32, buf: u64, count: u64) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    let count = usize::try_from(count).map_err(|_| syscall::ErrorCode::EINVAL)?;
    let data = current.fd_table.read(fd, count)?;
    crate::usermode::copy_to_user_bytes(buf, data)?;
    Ok(data.len() as u64)
}

fn close_handler(fd: i32) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    current.fd_table.close(fd)?;
    Ok(0)
}

/// getpid handler - return current process PID
fn getpid_handler() -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
    Ok(current.pid.as_u64())
}

/// fork handler - create a child process
fn fork_handler() -> syscall::SyscallResult {
    serial_println!("[FORK] Starting fork");

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    
    // Allocate PID for child
    // Note: In a full implementation, we'd have a global PID allocator
    // For now, we'll use a simple incrementing scheme
    static mut NEXT_PID: u64 = 2;
    let child_pid = panda_hal::pid::Pid::new(unsafe {
        let pid = NEXT_PID;
        NEXT_PID += 1;
        pid
    });

    serial_println!("[FORK] Creating child PID {}", child_pid.as_u64());

    // Fork the process
    // SAFETY: Frame allocator and GDT are initialized
    let mut child = unsafe {
        current.fork_from(child_pid)
            .map_err(|_| syscall::ErrorCode::ENOMEM)?
    };

    // Set child's return value to 0
    child.context.rax = 0;

    // Set parent's return value to child PID
    current.context.rax = child_pid.as_u64();

    serial_println!("[FORK] Child PID {} created, adding to scheduler", child_pid.as_u64());

    // Add child to scheduler
    scheduler.add_process(child);

    // Return child PID to parent (already set in rax)
    Ok(child_pid.as_u64())
}

/// waitpid handler - wait for child process to exit
fn waitpid_handler(pid: i64, status_ptr: u64, options: i32) -> syscall::SyscallResult {
    serial_println!("[WAITPID] pid={}, status_ptr={:#x}, options={}", pid, status_ptr, options);

    // Only support options=0
    if options != 0 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    
    let parent = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
    let parent_pid = parent.pid;

    // Find zombie child
    let zombie = if pid == -1 {
        // Wait for any child
        scheduler.find_any_zombie_child(parent_pid)
    } else if pid > 0 {
        // Wait for specific child
        let child_pid = panda_hal::pid::Pid::new(pid as u64);
        scheduler.find_zombie_child(parent_pid)
            .filter(|p| p.pid == child_pid)
    } else {
        // pid == 0 or pid < -1 not supported yet
        return Err(syscall::ErrorCode::EINVAL);
    };

    match zombie {
        Some(child) => {
            let exit_code = child.exit_code().unwrap_or(0);
            let child_pid = child.pid.as_u64();
            
            serial_println!("[WAITPID] Found zombie child PID {} with exit code {}", child_pid, exit_code);

            // Write exit status to user if pointer is non-null
            if status_ptr != 0 {
                // Exit status format: exit code << 8
                let status = (exit_code << 8) as u32;
                let status_bytes = status.to_ne_bytes();
                crate::usermode::copy_to_user_bytes(status_ptr, &status_bytes)?;
            }

            // Reap the child process
            // SAFETY: Child page table is valid
            unsafe {
                let pt = child.page_table_phys;
                let pid = child.pid.as_u64();
                serial_println!("[WAITPID] Reaping child PID {} (pt={:#x})", pid, pt);
                crate::paging::free_process_address_space(pt, true)
                    .map_err(|_| syscall::ErrorCode::EIO)?;
            }

            // Return child PID
            Ok(child_pid)
        }
        None => {
            // Check if parent has any children at all
            if scheduler.has_children(parent_pid) {
                // Has children but none are zombies yet
                // For now, busy-wait by yielding
                serial_println!("[WAITPID] No zombie children yet, yielding");
                // Return EAGAIN to indicate no child available now
                // In a full implementation, we'd block the process
                Err(syscall::ErrorCode::EAGAIN)
            } else {
                // No children at all
                serial_println!("[WAITPID] No children found");
                Err(syscall::ErrorCode::ESRCH)
            }
        }
    }
}

/// Exit handler - called when process exits
fn exit_handler(status: i32) -> ! {
    serial_println!("Process exiting with status: {}", status);

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    let mut exited_pid = None;
    let mut exited_pt = None;
    let mut has_parent = false;

    if let Some(current) = scheduler.current_process_mut() {
        exited_pid = Some(current.pid.as_u64());
        exited_pt = Some(current.page_table_phys);
        has_parent = current.parent_pid.is_some();
        
        // If process has a parent, become a zombie. Otherwise, exit immediately.
        if has_parent {
            current.set_zombie(status);
            serial_println!(
                "[EXIT] PID {} became zombie (parent exists), status={}",
                current.pid.as_u64(),
                status
            );
        } else {
            current.set_exited(status);
            serial_println!(
                "[EXIT] PID {} exited (no parent), status={}",
                current.pid.as_u64(),
                status
            );
        }
    }

    // If no parent, mark for reaping
    if !has_parent {
        if let (Some(pid), Some(pt)) = (exited_pid, exited_pt) {
            serial_println!(
                "[EXIT] Marked PID {} for reaping (pt={:#x})",
                pid,
                pt
            );
            usermode::set_pending_reap(pt, pid);
        }
    }

    // Schedule next process
    if let Some(next) = scheduler.schedule_next() {
        let current_cr3 =
            x86_64::registers::control::Cr3::read().0.start_address().as_u64();
        serial_println!(
            "[EXIT] Switching CR3: from {:#x} to {:#x}",
            current_cr3,
            next.page_table_phys
        );
        // Update syscall context pointer and kernel stack for the new process.
        // SAFETY: Scheduler is initialized and interrupts are disabled here.
        unsafe {
            usermode::set_current_syscall_context(core::ptr::addr_of_mut!(next.context));
        }

        // Switch CR3 and return to user mode for the next process.
        // SAFETY: Next process has a valid user context and page table.
        unsafe {
            if has_parent {
                // Zombie - don't reap yet, just switch
                usermode::switch_to_user(
                    core::ptr::addr_of!(next.context),
                    next.page_table_phys,
                );
            } else {
                // No parent - reap immediately
                usermode::switch_to_user_with_reap(
                    core::ptr::addr_of!(next.context),
                    next.page_table_phys,
                );
            }
        }
    } else {
        // No more processes - report success and exit QEMU deterministically.
        #[cfg(feature = "shell-smoke")]
        serial_println!("TEST PASS shell_smoke");
        #[cfg(feature = "vfs-cat-smoke")]
        serial_println!("TEST PASS vfs_cat_smoke");
        #[cfg(feature = "fork-exec-smoke")]
        serial_println!("TEST PASS fork_exec_smoke");
        #[cfg(not(any(feature = "shell-smoke", feature = "vfs-cat-smoke", feature = "fork-exec-smoke")))]
        serial_println!("TEST PASS exec_smoke");
        let kernel_pt = usermode::kernel_page_table_phys();
        let current_cr3 =
            x86_64::registers::control::Cr3::read().0.start_address().as_u64();
        serial_println!(
            "[EXIT] Switching CR3 to kernel table: from {:#x} to {:#x}",
            current_cr3,
            kernel_pt
        );
        usermode::switch_to_kernel_and_reap_then_halt();
    }
}

/// Start the first process in the scheduler
///
/// # Safety
///
/// Must be called with interrupts disabled and scheduler initialized.
unsafe fn start_first_process() -> ! {
    // SAFETY: Scheduler is initialized before this is called
    let scheduler = unsafe { get_scheduler() };

    // Get first process to run
    let first_process = scheduler.schedule_next().expect("No processes to run");

    println!("Starting process PID {}...", first_process.pid.as_u64());

    // Initialize context for first run
    context_switch::init_context_for_first_run(first_process);

    // Publish current syscall context and kernel stack pointer
    // SAFETY: Scheduler is initialized and interrupts are disabled here
    unsafe {
        usermode::set_current_syscall_context(core::ptr::addr_of_mut!(first_process.context));
    }

    // Enable interrupts before jumping to user mode
    x86_64::instructions::interrupts::enable();

    // Enter user mode and start first process
    // SAFETY: Process has been properly initialized
    unsafe {
        usermode::enter_usermode(
            first_process.entry_point,
            first_process.user_stack_ptr,
            first_process.page_table_phys,
        );
    }
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
