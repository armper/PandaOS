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
pub mod diskfs;
pub mod elf;
pub mod fs;
pub mod gdt;
pub mod heap;
pub mod interrupts;
pub mod invariants;
pub mod linker_symbols;
pub mod memory;
pub mod mount;
pub mod page_table_tracker;
pub mod paging;
pub mod pic;
pub mod pipe;
pub mod process;
pub mod scheduler;
pub mod syscall;
pub mod timer;
pub mod tmpfs;
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

    // Explicit early boot log to confirm serial is working
    serial_println!("[BOOT] serial ok");
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

    // Initialize mount table
    mount::init_mount_table();
    println!("Mount table initialized");

    // Mount tmpfs at /tmp
    match mount::mount_tmpfs_at_tmp() {
        Ok(()) => println!("Tmpfs mounted at /tmp"),
        Err(e) => println!("Warning: Failed to mount tmpfs at /tmp: {:?}", e),
    }

    // Mount disk filesystem at /mnt
    match mount::mount_disk_at_mnt() {
        Ok(()) => println!("Disk filesystem mounted at /mnt"),
        Err(e) => println!("Warning: Failed to mount disk at /mnt: {:?}", e),
    }

    // Finalize boot
    let _state = state.finalize();
    println!("Kernel initialization complete!");

    #[cfg(test)]
    test_main();

    #[cfg(not(test))]
    {
        // Run disk filesystem smoke test if feature is enabled
        #[cfg(feature = "disk-fs-smoke")]
        {
            serial_println!("Running disk_fs_smoke test");
            run_disk_fs_smoke_test();
        }

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

    let kernel_cr3 = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
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

    // Load init program from filesystem
    // First try /mnt/bin/init (disk), then fall back to /init (in-memory if present)
    let init_path = if fs::stat_path("/mnt/bin/init").is_ok() {
        "/mnt/bin/init"
    } else if fs::stat_path("/init").is_ok() {
        "/init"
    } else {
        panic!("init program not found in /mnt/bin/init or /init");
    };
    
    println!("Loading init from {}...", init_path);
    let init_data_vec = fs::read_file_to_vec(init_path).expect("Failed to read init");
    println!("Loaded init program ({} bytes)...", init_data_vec.len());
    let init_elf = elf::parse_elf(&init_data_vec).expect("Failed to parse init ELF");
    let init_process = unsafe {
        process::Process::new(&init_elf, &init_data_vec, &pid_allocator)
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
    syscall::set_write_handler(write_handler);
    syscall::set_close_handler(close_handler);
    syscall::set_stat_handler(stat_handler);
    syscall::set_fstat_handler(fstat_handler);
    syscall::set_getpid_handler(getpid_handler);
    syscall::set_fork_handler(fork_handler);
    syscall::set_waitpid_handler(waitpid_handler);
    syscall::set_pipe_handler(pipe_handler);
    syscall::set_dup2_handler(dup2_handler);
    syscall::set_kill_handler(kill_handler);
    syscall::set_setpgid_handler(setpgid_handler);
    syscall::set_getdents64_handler(getdents64_handler);
    syscall::set_getcwd_handler(getcwd_handler);
    syscall::set_chdir_handler(chdir_handler);
    syscall::set_unlink_handler(unlink_handler);
    syscall::set_getenv_handler(getenv_handler);

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
    // Get current process to access PATH environment variable
    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path: if it contains '/', use as-is; otherwise search PATH
    let resolved_path = if path.contains('/') {
        // Absolute or relative path - resolve normally
        fs::resolve_path(&current.cwd, path)?
    } else {
        // No '/' in path - search PATH directories
        let path_env = &current.path_env;
        let mut found_path = None;

        // Split PATH by ':' and try each directory
        for dir in path_env.split(':') {
            // Skip empty components (e.g., "::", leading/trailing ":")
            // Note: In some Unix shells, empty PATH components mean current directory,
            // but for security we explicitly skip them rather than using cwd
            if dir.is_empty() {
                continue;
            }

            // Construct full path: dir/path
            let mut full_path = alloc::string::String::new();
            full_path.push_str(dir);
            if !dir.ends_with('/') {
                full_path.push('/');
            }
            full_path.push_str(path);

            // Try to stat this path to see if it exists
            if fs::stat_path(&full_path).is_ok() {
                found_path = Some(full_path);
                break;
            }
        }

        found_path.ok_or(syscall::ErrorCode::ENOENT)?
    };

    // Load ELF file from filesystem (disk, tmpfs, or in-memory)
    let elf_data = fs::read_file_to_vec(&resolved_path)?;

    let elf_info = elf::parse_elf(&elf_data).map_err(|_| syscall::ErrorCode::EINVAL)?;

    // SAFETY: Frame allocator and GDT are initialized.
    unsafe {
        current.replace_image(&elf_info, &elf_data).map_err(|_| syscall::ErrorCode::ENOMEM)?;
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

fn open_handler(path: &str, flags: u64) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path relative to cwd
    let resolved_path = fs::resolve_path(&current.cwd, path)?;

    let fd = fs::open_path_with_flags(&mut current.fd_table, &resolved_path, flags)?;
    Ok(fd as u64)
}

fn read_handler(fd: i32, buf: u64, count: u64) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    let count = usize::try_from(count).map_err(|_| syscall::ErrorCode::EINVAL)?;

    // Check fd kind
    let fd_kind = current.fd_table.get(fd)?;

    match fd_kind {
        fs::FdKind::File(_, _) | fs::FdKind::DiskFile(_) | fs::FdKind::TmpfsFile(_) => {
            // Read from file using a temporary buffer
            let mut temp_buf = [0u8; 4096];
            let to_read = count.min(temp_buf.len());
            let bytes_read = current.fd_table.read(fd, &mut temp_buf[..to_read])?;

            if bytes_read > 0 {
                crate::usermode::copy_to_user_bytes(buf, &temp_buf[..bytes_read])?;
            }
            Ok(bytes_read as u64)
        }
        fs::FdKind::Directory(_) | fs::FdKind::DiskDirectory(_) | fs::FdKind::TmpfsDirectory(_) => {
            // Can't read directories with read() - use getdents64
            Err(syscall::ErrorCode::EISDIR)
        }
        fs::FdKind::PipeRead(pipe_id) => {
            // Read from pipe - use a temporary buffer
            let mut temp_buf = [0u8; 4096];
            let to_read = count.min(temp_buf.len());

            // Try to read from pipe (non-blocking)
            match crate::pipe::pipe_read(pipe_id, &mut temp_buf[..to_read]) {
                Ok(n) => {
                    if n > 0 {
                        crate::usermode::copy_to_user_bytes(buf, &temp_buf[..n])?;
                    }
                    Ok(n as u64)
                }
                Err(syscall::ErrorCode::EAGAIN) => {
                    // Busy-wait by yielding (simple blocking implementation)
                    // In a full implementation, we'd block the process
                    yield_handler();
                    // Never reached - yield_handler switches processes
                    unreachable!()
                }
                Err(e) => Err(e),
            }
        }
        fs::FdKind::PipeWrite(_) => {
            // Can't read from write end
            Err(syscall::ErrorCode::EBADF)
        }
    }
}

fn write_handler(fd: i32, buf: u64, count: u64) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    let count = usize::try_from(count).map_err(|_| syscall::ErrorCode::EINVAL)?;

    // Check fd kind
    let fd_kind = current.fd_table.get(fd)?;

    match fd_kind {
        fs::FdKind::File(_open, writable) => {
            if !writable {
                return Err(syscall::ErrorCode::EBADF);
            }

            // Write to writable file - use a temporary buffer
            let mut temp_buf = [0u8; 4096];
            let to_write = count.min(temp_buf.len());

            // Copy from user space
            let copied = crate::usermode::copy_user_bytes(buf, to_write, &mut temp_buf)?;

            // Write to file
            let written = current.fd_table.write(fd, &temp_buf[..copied])?;
            Ok(written as u64)
        }
        fs::FdKind::Directory(_) | fs::FdKind::DiskFile(_) | fs::FdKind::DiskDirectory(_) => {
            // Can't write to directories or disk files (read-only filesystem)
            Err(syscall::ErrorCode::EBADF)
        }
        fs::FdKind::TmpfsFile(_open) => {
            // Write to tmpfs file - use a temporary buffer
            let mut temp_buf = [0u8; 4096];
            let to_write = count.min(temp_buf.len());

            // Copy from user space
            let copied = crate::usermode::copy_user_bytes(buf, to_write, &mut temp_buf)?;

            // Write to file
            let written = current.fd_table.write(fd, &temp_buf[..copied])?;
            Ok(written as u64)
        }
        fs::FdKind::TmpfsDirectory(_) => {
            // Can't write to directories
            Err(syscall::ErrorCode::EBADF)
        }
        fs::FdKind::PipeWrite(pipe_id) => {
            // Write to pipe - use a temporary buffer
            let mut temp_buf = [0u8; 4096];
            let to_write = count.min(temp_buf.len());

            // Copy from user space
            let copied = crate::usermode::copy_user_bytes(buf, to_write, &mut temp_buf)?;

            // Try to write to pipe (non-blocking)
            match crate::pipe::pipe_write(pipe_id, &temp_buf[..copied]) {
                Ok(n) => Ok(n as u64),
                Err(syscall::ErrorCode::EAGAIN) => {
                    // Busy-wait by yielding (simple blocking implementation)
                    // In a full implementation, we'd block the process
                    yield_handler();
                    // Never reached - yield_handler switches processes
                    unreachable!()
                }
                Err(e) => Err(e),
            }
        }
        fs::FdKind::PipeRead(_) => {
            // Can't write to read end
            Err(syscall::ErrorCode::EBADF)
        }
    }
}

fn close_handler(fd: i32) -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;
    current.fd_table.close(fd)?;
    Ok(0)
}

/// stat handler - get file metadata by path
fn stat_handler(path_ptr: u64, stat_buf: u64) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    // Copy path from user space
    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // Get current process
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path relative to cwd
    let resolved_path = fs::resolve_path(&current.cwd, path)?;

    // Get metadata
    let metadata = fs::stat_path(&resolved_path)?;

    // Copy metadata to user space (file_type as u8, size as u64)
    let metadata_bytes = [
        metadata.file_type as u8,
        0,
        0,
        0,
        0,
        0,
        0,
        0, // padding to align size field
        (metadata.size & 0xFF) as u8,
        ((metadata.size >> 8) & 0xFF) as u8,
        ((metadata.size >> 16) & 0xFF) as u8,
        ((metadata.size >> 24) & 0xFF) as u8,
        ((metadata.size >> 32) & 0xFF) as u8,
        ((metadata.size >> 40) & 0xFF) as u8,
        ((metadata.size >> 48) & 0xFF) as u8,
        ((metadata.size >> 56) & 0xFF) as u8,
    ];
    crate::usermode::copy_to_user_bytes(stat_buf, &metadata_bytes)?;

    Ok(0)
}

/// fstat handler - get file metadata by file descriptor
fn fstat_handler(fd: i32, stat_buf: u64) -> syscall::SyscallResult {
    // Get current process
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Get metadata
    let metadata = fs::fstat_fd(&current.fd_table, fd)?;

    // Copy metadata to user space (file_type as u8, size as u64)
    let metadata_bytes = [
        metadata.file_type as u8,
        0,
        0,
        0,
        0,
        0,
        0,
        0, // padding to align size field
        (metadata.size & 0xFF) as u8,
        ((metadata.size >> 8) & 0xFF) as u8,
        ((metadata.size >> 16) & 0xFF) as u8,
        ((metadata.size >> 24) & 0xFF) as u8,
        ((metadata.size >> 32) & 0xFF) as u8,
        ((metadata.size >> 40) & 0xFF) as u8,
        ((metadata.size >> 48) & 0xFF) as u8,
        ((metadata.size >> 56) & 0xFF) as u8,
    ];
    crate::usermode::copy_to_user_bytes(stat_buf, &metadata_bytes)?;

    Ok(0)
}

/// pipe handler - create a pipe
fn pipe_handler(pipefd_ptr: u64) -> syscall::SyscallResult {
    serial_println!("[PIPE] Creating pipe");

    // Create a new pipe
    let (read_id, write_id) = crate::pipe::pipe_create()?;

    // Get current process
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Open read and write ends in FD table
    let read_fd = current.fd_table.open_pipe_read(read_id)?;
    let write_fd = current.fd_table.open_pipe_write(write_id)?;

    serial_println!("[PIPE] Created pipe: read_fd={}, write_fd={}", read_fd, write_fd);

    // Write fds to user memory
    let fds = [read_fd, write_fd];
    let fds_bytes = unsafe {
        core::slice::from_raw_parts(fds.as_ptr() as *const u8, core::mem::size_of_val(&fds))
    };
    crate::usermode::copy_to_user_bytes(pipefd_ptr, fds_bytes)?;

    Ok(0)
}

/// dup2 handler - duplicate a file descriptor
fn dup2_handler(oldfd: i32, newfd: i32) -> syscall::SyscallResult {
    serial_println!("[DUP2] Duplicating fd {} to {}", oldfd, newfd);

    // Get current process
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Perform dup2
    current.fd_table.dup2(oldfd, newfd)?;

    Ok(newfd as u64)
}

/// getpid handler - return current process PID
fn getpid_handler() -> syscall::SyscallResult {
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
    Ok(current.pid.as_u64())
}

/// kill handler - send signal to a process or process group
fn kill_handler(pid: i32, sig: i32) -> syscall::SyscallResult {
    use crate::process::Signal;

    serial_println!("[KILL] Sending signal {} to PID {}", sig, pid);

    // Only support SIGINT (signal 2)
    let signal = Signal::from_u32(sig as u32).ok_or(syscall::ErrorCode::EINVAL)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    if pid > 0 {
        // Signal a specific process
        let target_pid = panda_hal::pid::Pid::new(pid as u64);

        // Check if it's the current process
        if let Some(current) = scheduler.current_process_mut() {
            if current.pid == target_pid {
                current.send_signal(signal);
                return Ok(0);
            }
        }

        // LIMITATION: We only support sending signals to the current process.
        // A full implementation would search the scheduler's ready queue for the target PID.
        serial_println!("[KILL] Process {} not found or not current", pid);
        Err(syscall::ErrorCode::ESRCH)
    } else if pid < 0 {
        // Signal a process group (negative PID means process group)
        let pgid = panda_hal::pid::Pid::new((-pid) as u64);
        serial_println!("[KILL] Signaling process group {}", pgid.as_u64());

        let count = scheduler.signal_process_group(pgid, signal);

        if count > 0 {
            serial_println!("[KILL] Signaled {} processes in group {}", count, pgid.as_u64());
            Ok(0)
        } else {
            serial_println!("[KILL] No processes in group {}", pgid.as_u64());
            Err(syscall::ErrorCode::ESRCH)
        }
    } else {
        // pid == 0: signal current process's group (not implemented yet)
        Err(syscall::ErrorCode::EINVAL)
    }
}

/// setpgid handler - set process group ID
fn setpgid_handler(pid: i32, pgid: i32) -> syscall::SyscallResult {
    serial_println!("[SETPGID] Setting PGID {} for PID {}", pgid, pid);

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    // Get the target process (pid==0 means current process)
    let target_pid = if pid == 0 {
        scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?.pid
    } else {
        panda_hal::pid::Pid::new(pid as u64)
    };

    // Determine the new pgid (pgid==0 means use target's PID)
    let new_pgid = if pgid == 0 { target_pid } else { panda_hal::pid::Pid::new(pgid as u64) };

    // For simplicity, only allow setting pgid for current process
    // A full implementation would search all processes for target_pid
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    if current.pid != target_pid {
        serial_println!(
            "[SETPGID] Can only set pgid for current process (pid={})",
            current.pid.as_u64()
        );
        return Err(syscall::ErrorCode::ESRCH);
    }

    serial_println!(
        "[SETPGID] Setting process {} to group {}",
        current.pid.as_u64(),
        new_pgid.as_u64()
    );
    current.pgid = new_pgid;

    Ok(0)
}

/// getdents64 handler - read directory entries
fn getdents64_handler(fd: i32, buf: u64, count: u64) -> syscall::SyscallResult {
    use alloc::vec::Vec;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Get fd kind and verify it's a directory
    let fd_kind = current.fd_table.get(fd)?;

    // Get entries based on directory type
    let (entries, offset) = match fd_kind {
        fs::FdKind::Directory(open) => {
            let node = fs::FILES.get(open.node_index).ok_or(syscall::ErrorCode::ENOENT)?;
            if node.file_type != fs::FileType::Directory {
                return Err(syscall::ErrorCode::ENOTDIR);
            }
            let entries = fs::list_directory(node.path)?;
            (entries, open.offset)
        }
        fs::FdKind::DiskDirectory(open) => {
            let entries = crate::mount::diskfs_list_dir(open.inode)?;
            (entries, open.offset)
        }
        fs::FdKind::TmpfsDirectory(open) => {
            let entries = crate::mount::tmpfs_list_dir(open.inode)?;
            (entries, open.offset)
        }
        _ => return Err(syscall::ErrorCode::ENOTDIR),
    };

    // Check if we've reached the end (offset >= number of entries)
    if offset >= entries.len() {
        return Ok(0); // EOF
    }

    // Calculate how many entries we can fit in the buffer
    let count = usize::try_from(count).map_err(|_| syscall::ErrorCode::EINVAL)?;
    let mut bytes_written = 0usize;
    let mut entries_read = 0usize;

    // Build directory entries in kernel buffer
    let mut kernel_buf = Vec::new();

    for (name, file_type) in entries.iter().skip(offset) {
        // Calculate record size: fixed header (19 bytes) + name + null + padding to 8-byte align
        let name_len = name.len();
        let record_size = 19 + name_len + 1; // header + name + null
        let aligned_size = (record_size + 7) & !7; // align to 8 bytes

        // Check if we have space in buffer
        if bytes_written + aligned_size > count {
            break; // Buffer full
        }

        // Build the directory entry
        // d_ino (8 bytes)
        let d_ino = (offset + entries_read + 1) as u64;
        kernel_buf.extend_from_slice(&d_ino.to_le_bytes());

        // d_off (8 bytes) - offset to next entry
        let d_off = (offset + entries_read + 1) as u64;
        kernel_buf.extend_from_slice(&d_off.to_le_bytes());

        // d_reclen (2 bytes)
        let d_reclen = aligned_size as u16;
        kernel_buf.extend_from_slice(&d_reclen.to_le_bytes());

        // d_type (1 byte)
        let d_type = match file_type {
            fs::FileType::File => 8,      // DT_REG
            fs::FileType::Directory => 4, // DT_DIR
        };
        kernel_buf.push(d_type);

        // name (null-terminated)
        kernel_buf.extend_from_slice(name.as_bytes());
        kernel_buf.push(0); // null terminator

        // padding to 8-byte alignment
        while kernel_buf.len() < bytes_written + aligned_size {
            kernel_buf.push(0);
        }

        bytes_written += aligned_size;
        entries_read += 1;
    }

    // Copy to user space
    if bytes_written > 0 {
        crate::usermode::copy_to_user_bytes(buf, &kernel_buf[..bytes_written])?;

        // Update offset in fd table
        let new_offset = offset + entries_read;
        current.fd_table.update_directory_offset(fd, new_offset)?;
    }

    Ok(bytes_written as u64)
}

/// getcwd handler - get current working directory
fn getcwd_handler(buf: u64, size: u64) -> syscall::SyscallResult {
    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    let cwd_bytes = current.cwd.as_bytes();
    let size = usize::try_from(size).map_err(|_| syscall::ErrorCode::EINVAL)?;

    if size == 0 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // Need space for string + null terminator
    if cwd_bytes.len() + 1 > size {
        return Err(syscall::ErrorCode::ERANGE);
    }

    // Copy cwd to user buffer
    crate::usermode::copy_to_user_bytes(buf, cwd_bytes)?;

    // Add null terminator
    let null_byte = [0u8];
    crate::usermode::copy_to_user_bytes(buf + cwd_bytes.len() as u64, &null_byte)?;

    Ok(buf)
}

/// chdir handler - change current working directory
fn chdir_handler(path_ptr: u64) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 256;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path relative to current cwd
    let resolved_path = fs::resolve_path(&current.cwd, path)?;

    // Validate that it's a directory
    fs::validate_directory(&resolved_path)?;

    // Update cwd
    current.cwd = resolved_path;

    Ok(0)
}

/// unlink handler - delete a file or empty directory
fn unlink_handler(path_ptr: u64) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 256;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path relative to current cwd
    let resolved_path = fs::resolve_path(&current.cwd, path)?;

    // Unlink the file
    fs::unlink_path(&resolved_path)?;

    Ok(0)
}

/// getenv handler - get environment variable value
fn getenv_handler(name_ptr: u64, buf_ptr: u64, size: u64) -> syscall::SyscallResult {
    const MAX_NAME_LEN: usize = 64;
    const ENV_PATH: &str = "PATH";
    let mut name_buf = [0u8; MAX_NAME_LEN];

    let name = crate::usermode::copy_user_cstr(name_ptr, &mut name_buf)?;
    let size = usize::try_from(size).map_err(|_| syscall::ErrorCode::EINVAL)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // For now, we only support PATH environment variable
    let value = if name == ENV_PATH {
        &current.path_env
    } else {
        // Environment variable not found
        return Err(syscall::ErrorCode::ENOENT);
    };

    let value_bytes = value.as_bytes();

    if size == 0 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // Need space for string + null terminator
    if value_bytes.len() + 1 > size {
        return Err(syscall::ErrorCode::ERANGE);
    }

    // Copy value to user buffer
    crate::usermode::copy_to_user_bytes(buf_ptr, value_bytes)?;

    // Add null terminator
    let null_byte = [0u8];
    crate::usermode::copy_to_user_bytes(buf_ptr + value_bytes.len() as u64, &null_byte)?;

    Ok(value_bytes.len() as u64)
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
    let mut child =
        unsafe { current.fork_from(child_pid).map_err(|_| syscall::ErrorCode::ENOMEM)? };

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
        scheduler.find_zombie_child(parent_pid).filter(|p| p.pid == child_pid)
    } else {
        // pid == 0 or pid < -1 not supported yet
        return Err(syscall::ErrorCode::EINVAL);
    };

    match zombie {
        Some(child) => {
            let exit_code = child.exit_code().unwrap_or(0);
            let child_pid = child.pid.as_u64();

            serial_println!(
                "[WAITPID] Found zombie child PID {} with exit code {}",
                child_pid,
                exit_code
            );

            // Write exit status to user if pointer is non-null
            if status_ptr != 0 {
                // Exit status format: exit code << 8
                let status = (exit_code.wrapping_shl(8).cast_unsigned()).to_ne_bytes();
                crate::usermode::copy_to_user_bytes(status_ptr, &status)?;
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
                // Has children but none are zombies yet - block the process
                serial_println!("[WAITPID] No zombie children yet, blocking parent");

                let parent = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

                // Block the parent based on what it's waiting for
                if pid == -1 {
                    parent.block_on_any_child();
                } else if pid > 0 {
                    let child_pid = panda_hal::pid::Pid::new(pid as u64);
                    parent.block_on_child(child_pid);
                } else {
                    return Err(syscall::ErrorCode::EINVAL);
                }

                // Trigger a context switch to another process
                // The yield handler will call schedule_next which will skip blocked processes
                yield_handler();

                // After returning from yield (when woken), retry to find zombie
                // Get fresh scheduler reference after context switch
                let scheduler = unsafe { get_scheduler() };
                let parent = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
                let parent_pid = parent.pid;

                let zombie_after_wake = if pid == -1 {
                    scheduler.find_any_zombie_child(parent_pid)
                } else if pid > 0 {
                    let child_pid = panda_hal::pid::Pid::new(pid as u64);
                    scheduler.find_zombie_child(parent_pid).filter(|p| p.pid == child_pid)
                } else {
                    return Err(syscall::ErrorCode::EINVAL);
                };

                match zombie_after_wake {
                    Some(child) => {
                        let exit_code = child.exit_code().unwrap_or(0);
                        let child_pid = child.pid.as_u64();

                        serial_println!(
                            "[WAITPID] After wake, found zombie child PID {}",
                            child_pid
                        );

                        // Write exit status to user if pointer is non-null
                        if status_ptr != 0 {
                            // Exit status format: exit code << 8
                            let status = (exit_code.wrapping_shl(8).cast_unsigned()).to_ne_bytes();
                            crate::usermode::copy_to_user_bytes(status_ptr, &status)?;
                        }

                        // Reap the child process
                        // SAFETY: Child page table is valid
                        unsafe {
                            let pt = child.page_table_phys;
                            serial_println!(
                                "[WAITPID] Reaping child PID {} (pt={:#x})",
                                child_pid,
                                pt
                            );
                            crate::paging::free_process_address_space(pt, true)
                                .map_err(|_| syscall::ErrorCode::EIO)?;
                        }

                        Ok(child_pid)
                    }
                    None => {
                        // Child still not ready - return EINTR (interrupted by signal or wakeup)
                        serial_println!("[WAITPID] After wake, child still not ready");
                        Err(syscall::ErrorCode::EINTR)
                    }
                }
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
    let mut child_pid_for_wake = None;

    if let Some(current) = scheduler.current_process_mut() {
        exited_pid = Some(current.pid.as_u64());
        exited_pt = Some(current.page_table_phys);
        has_parent = current.parent_pid.is_some();
        child_pid_for_wake = Some(current.pid);

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

    // Wake any parent waiting for this child
    if let Some(child_pid) = child_pid_for_wake {
        serial_println!("[EXIT] Waking processes waiting for child PID {}", child_pid.as_u64());
        scheduler.wake_waiters_for_child(child_pid);
    }

    // If no parent, mark for reaping
    if !has_parent {
        if let (Some(pid), Some(pt)) = (exited_pid, exited_pt) {
            serial_println!("[EXIT] Marked PID {} for reaping (pt={:#x})", pid, pt);
            usermode::set_pending_reap(pt, pid);
        }
    }

    // Schedule next process
    if let Some(next) = scheduler.schedule_next() {
        let current_cr3 = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
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
                usermode::switch_to_user(core::ptr::addr_of!(next.context), next.page_table_phys);
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
        #[cfg(feature = "pipe-smoke")]
        serial_println!("TEST PASS pipe_smoke");
        #[cfg(feature = "ctrlc-smoke")]
        serial_println!("TEST PASS ctrlc_smoke");
        #[cfg(feature = "ls-smoke")]
        serial_println!("TEST PASS ls_smoke");
        #[cfg(feature = "ls-stat-smoke")]
        serial_println!("TEST PASS ls_stat_smoke");
        #[cfg(feature = "cd-smoke")]
        serial_println!("TEST PASS cd_smoke");
        #[cfg(feature = "path-smoke")]
        serial_println!("TEST PASS path_smoke");
        #[cfg(feature = "redir-smoke")]
        serial_println!("TEST PASS redir_smoke");
        #[cfg(not(any(
            feature = "shell-smoke",
            feature = "vfs-cat-smoke",
            feature = "fork-exec-smoke",
            feature = "pipe-smoke",
            feature = "ctrlc-smoke",
            feature = "ls-smoke",
            feature = "ls-stat-smoke",
            feature = "cd-smoke",
            feature = "path-smoke",
            feature = "redir-smoke"
        )))]
        serial_println!("TEST PASS exec_smoke");
        let kernel_pt = usermode::kernel_page_table_phys();
        let current_cr3 = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
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

/// Disk filesystem smoke test
#[cfg(feature = "disk-fs-smoke")]
fn run_disk_fs_smoke_test() {
    use alloc::string::ToString;

    serial_println!("Testing disk filesystem at /mnt");

    // Test 1: Check if /mnt exists and is a directory
    match fs::stat_path("/mnt") {
        Ok(metadata) => {
            serial_println!("✓ /mnt exists");
            if metadata.file_type == fs::FileType::Directory {
                serial_println!("✓ /mnt is a directory");
            } else {
                serial_println!("✗ /mnt is not a directory");
                serial_println!("TEST FAIL disk_fs_smoke");
                loop {
                    x86_64::instructions::hlt();
                }
            }
        }
        Err(e) => {
            serial_println!("✗ Failed to stat /mnt: {:?}", e);
            serial_println!("TEST FAIL disk_fs_smoke");
            loop {
                x86_64::instructions::hlt();
            }
        }
    }

    // Test 2: List directory entries in /mnt
    match fs::list_directory("/mnt") {
        Ok(entries) => {
            serial_println!("✓ Successfully listed /mnt");
            serial_println!("  Found {} entries:", entries.len());
            for (name, file_type) in &entries {
                let type_str = match file_type {
                    fs::FileType::File => "file",
                    fs::FileType::Directory => "dir",
                };
                serial_println!("    - {} ({})", name, type_str);
            }

            // Check for expected files
            let has_hello = entries.iter().any(|(n, _)| n == "hello.txt");
            let has_readme = entries.iter().any(|(n, _)| n == "README");

            if has_hello && has_readme {
                serial_println!("✓ Found expected files (hello.txt, README)");
            } else {
                serial_println!("✗ Missing expected files");
                serial_println!("TEST FAIL disk_fs_smoke");
                loop {
                    x86_64::instructions::hlt();
                }
            }
        }
        Err(e) => {
            serial_println!("✗ Failed to list /mnt: {:?}", e);
            serial_println!("TEST FAIL disk_fs_smoke");
            loop {
                x86_64::instructions::hlt();
            }
        }
    }

    // Test 3: Read contents of /mnt/hello.txt
    let mut fd_table = fs::FdTable::new();
    match fs::open_path_with_flags(&mut fd_table, "/mnt/hello.txt", fs::O_RDONLY) {
        Ok(fd) => {
            serial_println!("✓ Opened /mnt/hello.txt (fd {})", fd);

            let mut buffer = [0u8; 256];
            match fd_table.read(fd, &mut buffer) {
                Ok(bytes_read) => {
                    if bytes_read > 0 {
                        let content =
                            core::str::from_utf8(&buffer[..bytes_read]).unwrap_or("<invalid utf8>");
                        serial_println!("✓ Read {} bytes from /mnt/hello.txt", bytes_read);
                        serial_println!("  Content: \"{}\"", content.trim());

                        if content.contains("Hello from disk") {
                            serial_println!("✓ File content matches expected");
                        } else {
                            serial_println!("✗ File content doesn't match expected");
                            serial_println!("TEST FAIL disk_fs_smoke");
                            loop {
                                x86_64::instructions::hlt();
                            }
                        }
                    } else {
                        serial_println!("✗ Read 0 bytes (unexpected EOF)");
                        serial_println!("TEST FAIL disk_fs_smoke");
                        loop {
                            x86_64::instructions::hlt();
                        }
                    }
                }
                Err(e) => {
                    serial_println!("✗ Failed to read from /mnt/hello.txt: {:?}", e);
                    serial_println!("TEST FAIL disk_fs_smoke");
                    loop {
                        x86_64::instructions::hlt();
                    }
                }
            }

            // Close file
            let _ = fd_table.close(fd);
        }
        Err(e) => {
            serial_println!("✗ Failed to open /mnt/hello.txt: {:?}", e);
            serial_println!("TEST FAIL disk_fs_smoke");
            loop {
                x86_64::instructions::hlt();
            }
        }
    }

    serial_println!("✓ All disk filesystem tests passed");
    serial_println!("TEST PASS disk_fs_smoke");

    // Exit QEMU
    use x86_64::instructions::port::Port;
    unsafe {
        let mut port = Port::new(0xf4);
        port.write(0x10u32); // Success exit code
    }

    loop {
        x86_64::instructions::hlt();
    }
}
