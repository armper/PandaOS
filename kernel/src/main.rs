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

use alloc::string::String;
use alloc::vec::Vec;
use core::panic::PanicInfo;

// Import VGA and serial macros
#[macro_use]
extern crate panda_hal;

pub mod boot_diagnostics;
pub mod boot_phases;
#[cfg(feature = "boot-watchdog")]
pub mod boot_watchdog;
pub mod console;
pub mod context;
pub mod context_switch;
pub mod diskfs;
pub mod elf;
pub mod exec_stack;
pub mod fs;
pub mod gdt;
pub mod heap;
pub mod homefs;
pub mod interrupt_frame;
pub mod interrupts;
pub mod invariants;
pub mod linker_symbols;
pub mod memory;
pub mod mount;
pub mod page_table_tracker;
pub mod paging;
pub mod percpu;
pub mod pic;
pub mod pipe;
pub mod process;
pub mod scheduler;
#[cfg(feature = "boot-selfcheck")]
pub mod selfcheck;
pub mod spinlock_irq;
pub mod syscall;
pub mod timer;
pub mod tmpfs;
pub mod tty;
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

    BOOT_STEP!(1);
    // Explicit early boot log to confirm serial is working
    serial_println!("[BOOT] serial ok");

    // Start boot watchdog if feature is enabled
    // Timeout after 30 seconds at 100Hz (3000 ticks)
    #[cfg(feature = "boot-watchdog")]
    boot_watchdog::start(3000);

    // Print boot banner to all consoles
    console::print_boot_banner();

    console_println!("Hardware abstraction layer initialized");

    BOOT_STEP!(2);
    // SAFETY: HAL is now initialized, safe to proceed
    let state = unsafe { state.init_memory() };

    // Initialize memory management with bootloader info (no bootloader types exposed)
    unsafe { memory::init_from_bootloader(boot_info) };

    BOOT_STEP!(3);
    // SAFETY: Memory is now initialized, safe to proceed
    let state = unsafe { state.init_interrupts() };

    // Initialize paging infrastructure
    unsafe {
        paging::init_identity_map_minimal().expect("Failed to initialize identity mapping");
        paging::init_higher_half_mapping().expect("Failed to initialize higher-half mapping");
    }
    console_println!("Paging infrastructure initialized");

    BOOT_STEP!(4);
    // Initialize GDT (must be before interrupts are enabled)
    unsafe { gdt::init() };
    console_println!("GDT initialized");

    BOOT_STEP!(5);
    // Initialize interrupts (after GDT)
    interrupts::init();
    console_println!("Interrupt handling initialized");

    BOOT_STEP!(6);
    // Initialize syscall/sysret support (after GDT and interrupts)
    unsafe { usermode::init_syscall() };
    console_println!("Syscall/sysret initialized");

    BOOT_STEP!(7);
    // Map heap region (allocate frames and map pages)
    // MUST happen before heap allocator init
    unsafe {
        heap::map_heap().expect("Failed to map heap");
    }
    console_println!("Heap region mapped");

    BOOT_STEP!(8);
    // Initialize heap allocator (after heap is mapped)
    unsafe { heap::init() };
    console_println!("Heap allocator initialized");

    // Test heap allocation
    {
        use alloc::vec::Vec;
        let mut test_vec = Vec::new();
        test_vec.push(1);
        test_vec.push(2);
        test_vec.push(3);
        console_println!("Heap test passed: {:?}", test_vec);
    }

    BOOT_STEP!(9);

    // If boot-selfcheck feature is enabled, run selfcheck instead of normal boot
    #[cfg(feature = "boot-selfcheck")]
    {
        serial_println!("=== Boot Selfcheck Mode ===");

        let _state = state.finalize();

        // Run selfcheck suite
        let passed = selfcheck::run();

        if passed {
            serial_println!("TEST PASS boot_selfcheck");
            exit_qemu(QemuExitCode::Success);
        } else {
            serial_println!("TEST FAIL boot_selfcheck");
            exit_qemu(QemuExitCode::Failed);
        }
    }

    // Normal boot continues here (only if boot-selfcheck is NOT enabled)
    #[cfg(not(feature = "boot-selfcheck"))]
    {
        // Initialize mount table
        mount::init_mount_table();
        console_println!("Mount table initialized");

        // Mount tmpfs at /tmp
        match mount::mount_tmpfs_at_tmp() {
            Ok(()) => console_println!("Tmpfs mounted at /tmp"),
            Err(e) => console_println!("Warning: Failed to mount tmpfs at /tmp: {:?}", e),
        }

        // Mount disk filesystem at /mnt
        match mount::mount_disk_at_mnt() {
            Ok(()) => console_println!("Disk filesystem mounted at /mnt"),
            Err(e) => console_println!("Warning: Failed to mount disk at /mnt: {:?}", e),
        }

        BOOT_STEP!(10);
        // Finalize boot
        let _state = state.finalize();
        console_println!("Kernel initialization complete!");

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

            BOOT_STEP!(11);
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
}

/// Panic handler for the kernel
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Print panic marker to both serial and console
    serial_println!("\n╔════════════════════════════════════════════════════════════════╗");
    serial_println!("║                      KERNEL PANIC                              ║");
    serial_println!("╚════════════════════════════════════════════════════════════════╝");
    serial_println!();
    serial_println!("Panic: {}", info);

    #[cfg(feature = "vga-console")]
    {
        console_println!("\n╔════════════════════════════════════════════════════════════════╗");
        console_println!("║                      KERNEL PANIC                              ║");
        console_println!("╚════════════════════════════════════════════════════════════════╝");
        console_println!();
        console_println!("Panic: {}", info);
    }

    // Print diagnostic information
    let cpu_id = boot_diagnostics::get_cpu_id();
    let cr3 = boot_diagnostics::get_cr3();
    let rsp = boot_diagnostics::get_rsp();

    serial_println!("CPU ID: {}", cpu_id);
    serial_println!("CR3:    {:#018x}", cr3);
    serial_println!("RSP:    {:#018x}", rsp);

    #[cfg(feature = "vga-console")]
    {
        console_println!("CPU ID: {}", cpu_id);
        console_println!("CR3:    {:#018x}", cr3);
        console_println!("RSP:    {:#018x}", rsp);
    }

    // Dump boot diagnostics to help debug
    boot_diagnostics::dump_boot_diagnostics();

    exit_qemu(QemuExitCode::Failed);
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

/// Timer tick counter for observability and testing
static mut TICK_COUNTER: u64 = 0;

/// Preemption flag - set when kernel-mode code should reschedule on syscall exit
static mut NEED_RESCHED: bool = false;

/// Context switch counter for observability and testing
static mut CONTEXT_SWITCH_COUNTER: u64 = 0;

/// Initialize scheduler, load user programs, and start multitasking
///
/// # Safety
///
/// Must be called exactly once after all kernel subsystems are initialized.
unsafe fn init_scheduler_and_start() -> ! {
    use panda_hal::pid::PidAllocator;

    serial_println!("[sched] Initializing scheduler...");

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

    // For preempt-smoke test, use init_preempt if available
    #[cfg(feature = "preempt-smoke")]
    let init_path = "/init_preempt";

    serial_println!("[sched] Loading init from {}...", init_path);
    let init_data_vec = fs::read_file_to_vec(init_path).expect("Failed to read init");
    serial_println!("[sched] Loaded init program ({} bytes)...", init_data_vec.len());
    let init_elf = elf::parse_elf(&init_data_vec).expect("Failed to parse init ELF");
    serial_println!("[sched] Parsed init ELF OK");
    let init_process = unsafe {
        process::Process::new(&init_elf, &init_data_vec, &pid_allocator)
            .expect("Failed to create init process")
    };
    serial_println!("[sched] Created init PID {}", init_process.pid.as_u64());
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
    syscall::set_execve_handler(execve_handler);
    syscall::set_open_handler(open_handler);
    syscall::set_read_handler(read_handler);
    syscall::set_write_handler(write_handler);
    syscall::set_close_handler(close_handler);
    syscall::set_stat_handler(stat_handler);
    syscall::set_fstat_handler(fstat_handler);
    syscall::set_getpid_handler(getpid_handler);
    syscall::set_fork_handler(fork_handler);
    syscall::set_brk_handler(brk_handler);
    syscall::set_mmap_handler(mmap_handler);
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
    syscall::set_chmod_handler(chmod_handler);
    syscall::set_chown_handler(chown_handler);
    syscall::set_getuid_handler(getuid_handler);
    syscall::set_getgid_handler(getgid_handler);
    syscall::set_setuid_handler(setuid_handler);
    syscall::set_setgid_handler(setgid_handler);
    syscall::set_signal_handler(signal_handler);
    syscall::set_stop_signal_handler(stop_signal_handler);
    syscall::set_lseek_handler(lseek_handler);
    syscall::set_mkdir_handler(mkdir_handler);
    syscall::set_rmdir_handler(rmdir_handler);
    syscall::set_rename_handler(rename_handler);

    // Unmask timer interrupt (IRQ 0)
    println!("Enabling timer interrupt...");
    unsafe {
        pic::unmask_irq(0);
    }

    // Print ready marker before starting scheduler
    console::print_ready_marker();

    // Stop boot watchdog - boot completed successfully
    #[cfg(feature = "boot-watchdog")]
    boot_watchdog::stop();

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

/// Get the need_resched flag
///
/// # Safety
///
/// Safe to call from any context, returns the current value atomically
pub unsafe fn get_need_resched() -> bool {
    // SAFETY: Reading a bool is atomic
    unsafe { NEED_RESCHED }
}

/// Clear the need_resched flag
///
/// # Safety
///
/// Must be called with interrupts disabled to prevent races
pub unsafe fn clear_need_resched() {
    // SAFETY: Caller guarantees interrupts are disabled
    unsafe { NEED_RESCHED = false }
}

/// Get the current tick counter
///
/// # Safety
///
/// Safe to call from any context, returns the current value
pub unsafe fn get_tick_counter() -> u64 {
    // SAFETY: Reading a u64 is atomic on x86_64
    unsafe { TICK_COUNTER }
}

/// Get the context switch counter
///
/// # Safety
///
/// Safe to call from any context, returns the current value
pub unsafe fn get_context_switch_counter() -> u64 {
    // SAFETY: Reading a u64 is atomic on x86_64
    unsafe { CONTEXT_SWITCH_COUNTER }
}

/// Increment the context switch counter
///
/// # Safety
///
/// Must be called with interrupts disabled to prevent races
unsafe fn increment_context_switch_counter() {
    // SAFETY: Caller guarantees interrupts are disabled
    unsafe { CONTEXT_SWITCH_COUNTER += 1 }
}

/// Timer interrupt handler - called on each timer tick
///
/// This handler implements preemptive multitasking by:
/// 1. Incrementing the tick counter
/// 2. Checking if we're in user mode (by examining the interrupt frame)
/// 3. If in user mode, preempting the current process and switching to the next
/// 4. If in kernel mode, setting need_resched flag for syscall exit
///
/// # Safety
///
/// This runs with interrupts disabled (IRQ handler context).
/// We can safely access the scheduler and perform context switches.
fn timer_tick_handler() {
    // Tick the boot watchdog if enabled
    #[cfg(feature = "boot-watchdog")]
    {
        if boot_watchdog::tick() {
            // Boot timeout occurred
            serial_println!("Boot watchdog timeout - exiting QEMU");
            exit_qemu(QemuExitCode::Failed);
        }
    }

    // Increment tick counter
    // SAFETY: Called from interrupt handler with interrupts disabled
    unsafe {
        TICK_COUNTER += 1;
    }

    // Set need_resched flag - syscall exit path will check this
    // For now, we always set it on every tick. In the future, we could
    // add timeslice accounting per process to be more sophisticated.
    // SAFETY: Called from interrupt handler with interrupts disabled
    unsafe {
        NEED_RESCHED = true;
    }

    // Note: We cannot perform context switches directly from the timer interrupt
    // handler because:
    // 1. The current implementation uses syscall/sysret for user transitions
    // 2. We'd need to use iretq for interrupt returns
    // 3. Mixing these mechanisms is complex and error-prone
    //
    // Instead, we use a hybrid approach:
    // - Set need_resched flag on every timer tick
    // - Syscall handlers check need_resched before returning to user mode
    // - If set, perform a context switch before returning
    //
    // This provides preemption at syscall boundaries, which is sufficient
    // for most practical purposes and maintains correctness.

    // Log preemption events if feature is enabled
    #[cfg(feature = "preempt-log")]
    {
        let tick = unsafe { TICK_COUNTER };
        if tick % 100 == 0 {
            serial_println!("[PREEMPT] tick={} need_resched=true", tick);
        }
    }
}

/// Yield handler - called when process voluntarily yields CPU
fn yield_handler() {
    // Log with rate limiting or feature flag
    #[cfg(feature = "preempt-log")]
    serial_println!("[YIELD] Process yielding CPU");

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    // Ensure the current process returns 0 from yield() when resumed.
    if let Some(current) = scheduler.current_process_mut() {
        current.context.rax = 0;
    }

    // Get next process (current will be moved to ready queue)
    if let Some(next) = scheduler.schedule_next() {
        // Increment context switch counter
        // SAFETY: Called with interrupts disabled
        unsafe { increment_context_switch_counter() };
        
        // Log with rate limiting or feature flag
        #[cfg(feature = "preempt-log")]
        {
            let switch_count = unsafe { get_context_switch_counter() };
            serial_println!("[YIELD] Switching to process PID {} (switch #{})", next.pid.as_u64(), switch_count);
        }

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
fn execve_handler(
    path: &str,
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
) -> Result<(), syscall::ErrorCode> {
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

    // Check file metadata and permissions before loading
    let metadata = fs::stat_path(&resolved_path)?;

    // Ensure it's a regular file, not a directory
    if metadata.is_dir() {
        return Err(syscall::ErrorCode::EISDIR);
    }

    // Check execute permission (requirement: enforce x bit)
    if !fs::can_exec(current.uid, current.gid, metadata.uid, metadata.gid, metadata.mode) {
        return Err(syscall::ErrorCode::EACCES);
    }

    // Load ELF file from filesystem (disk, tmpfs, or in-memory)
    let elf_data = fs::read_file_to_vec(&resolved_path)?;

    let elf_info = elf::parse_elf(&elf_data).map_err(|e| match e {
        elf::ElfError::InvalidMagic
        | elf::ElfError::InvalidClass
        | elf::ElfError::InvalidEndian
        | elf::ElfError::InvalidVersion
        | elf::ElfError::NotExecutable
        | elf::ElfError::WrongMachine => syscall::ErrorCode::ENOEXEC,
        _ => syscall::ErrorCode::EINVAL,
    })?;

    // Save process info before replacing image
    let uid = current.uid;
    let gid = current.gid;

    // SAFETY: Frame allocator and GDT are initialized.
    unsafe {
        current.replace_image(&elf_info, &elf_data).map_err(|_| syscall::ErrorCode::ENOMEM)?;
    }

    // Switch to the new page table so user memory copies target the new image.
    usermode::switch_page_table(current.page_table_phys);

    // Update environment from envp (replace old environment)
    current.environ.clear();
    for env in envp {
        // Parse KEY=VALUE format
        if let Ok(env_str) = core::str::from_utf8(env) {
            if let Some(eq_pos) = env_str.find('=') {
                let key = &env_str[..eq_pos];
                let value = &env_str[eq_pos + 1..];
                current.environ.insert(String::from(key), String::from(value));
            }
        }
    }

    // Set up Linux-compatible user stack with argc/argv/envp/auxv
    // SAFETY: Stack memory is allocated and page table is valid after replace_image
    let new_sp = unsafe {
        exec_stack::setup_user_stack(
            current.page_table_phys,
            current.user_stack_ptr,
            &resolved_path,
            argv,
            envp,
            elf_info.entry_point,
            elf_info.phdr_addr,
            elf_info.phnum,
            uid,
            gid,
        )?
    };

    // Update process stack pointer and context for new program
    current.user_stack_ptr = new_sp;
    current.context.rsp = new_sp;

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

    let fd = fs::open_path_with_flags(
        &mut current.fd_table,
        &resolved_path,
        flags,
        current.uid,
        current.gid,
    )?;
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

    // Copy metadata to user space
    // struct stat {
    //     st_mode: u16,   // offset 0, 2 bytes
    //     padding: u16,   // offset 2, 2 bytes (alignment)
    //     st_nlink: u32,  // offset 4, 4 bytes
    //     st_uid: u32,    // offset 8, 4 bytes
    //     st_gid: u32,    // offset 12, 4 bytes
    //     st_size: u64,   // offset 16, 8 bytes
    //     st_ino: u64,    // offset 24, 8 bytes
    // }
    // Total: 32 bytes
    let metadata_bytes = [
        // st_mode (u16, little-endian)
        (metadata.mode & 0xFF) as u8,
        ((metadata.mode >> 8) & 0xFF) as u8,
        // padding (u16)
        0,
        0,
        // st_nlink (u32, always 1)
        1,
        0,
        0,
        0,
        // st_uid (u32, always 0)
        0,
        0,
        0,
        0,
        // st_gid (u32, always 0)
        0,
        0,
        0,
        0,
        // st_size (u64, little-endian)
        (metadata.size & 0xFF) as u8,
        ((metadata.size >> 8) & 0xFF) as u8,
        ((metadata.size >> 16) & 0xFF) as u8,
        ((metadata.size >> 24) & 0xFF) as u8,
        ((metadata.size >> 32) & 0xFF) as u8,
        ((metadata.size >> 40) & 0xFF) as u8,
        ((metadata.size >> 48) & 0xFF) as u8,
        ((metadata.size >> 56) & 0xFF) as u8,
        // st_ino (u64, fake inode = 0)
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
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

    // Copy metadata to user space (same format as stat)
    // struct stat {
    //     st_mode: u16,   // offset 0, 2 bytes
    //     padding: u16,   // offset 2, 2 bytes (alignment)
    //     st_nlink: u32,  // offset 4, 4 bytes
    //     st_uid: u32,    // offset 8, 4 bytes
    //     st_gid: u32,    // offset 12, 4 bytes
    //     st_size: u64,   // offset 16, 8 bytes
    //     st_ino: u64,    // offset 24, 8 bytes
    // }
    // Total: 32 bytes
    let metadata_bytes = [
        // st_mode (u16, little-endian)
        (metadata.mode & 0xFF) as u8,
        ((metadata.mode >> 8) & 0xFF) as u8,
        // padding (u16)
        0,
        0,
        // st_nlink (u32, always 1)
        1,
        0,
        0,
        0,
        // st_uid (u32, always 0)
        0,
        0,
        0,
        0,
        // st_gid (u32, always 0)
        0,
        0,
        0,
        0,
        // st_size (u64, little-endian)
        (metadata.size & 0xFF) as u8,
        ((metadata.size >> 8) & 0xFF) as u8,
        ((metadata.size >> 16) & 0xFF) as u8,
        ((metadata.size >> 24) & 0xFF) as u8,
        ((metadata.size >> 32) & 0xFF) as u8,
        ((metadata.size >> 40) & 0xFF) as u8,
        ((metadata.size >> 48) & 0xFF) as u8,
        ((metadata.size >> 56) & 0xFF) as u8,
        // st_ino (u64, fake inode = 0)
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
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

/// Signal handler for TTY Ctrl+C
///
/// Sends SIGINT to the foreground process group
fn signal_handler() {
    use crate::process::Signal;

    // SAFETY: Called from syscall context with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    if let Some(pgid) = scheduler.foreground_pgid() {
        serial_println!("[TTY] Ctrl+C: sending SIGINT to foreground pgid {}", pgid.as_u64());
        let count = scheduler.signal_process_group(pgid, Signal::SIGINT);
        if count > 0 {
            serial_println!("[TTY] Signaled {} processes", count);
        }
    } else {
        serial_println!("[TTY] Ctrl+C: no foreground process group");
    }
}

/// Handle Ctrl+Z by sending SIGTSTP to foreground process group
fn stop_signal_handler() {
    use crate::process::Signal;

    // SAFETY: Called from syscall context with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    if let Some(pgid) = scheduler.foreground_pgid() {
        serial_println!("[TTY] Ctrl+Z: sending SIGTSTP to foreground pgid {}", pgid.as_u64());
        let count = scheduler.signal_process_group(pgid, Signal::SIGTSTP);
        if count > 0 {
            serial_println!("[TTY] Stopped {} processes", count);
        }
    } else {
        serial_println!("[TTY] Ctrl+Z: no foreground process group");
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

/// chmod handler - change file mode
fn chmod_handler(path_ptr: u64, mode: u16) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path relative to cwd
    let resolved_path = fs::resolve_path(&current.cwd, path)?;

    // Validate mode (only permission bits 0-0777)
    if mode > 0o777 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // Get file metadata to check ownership
    let metadata = fs::stat_path(&resolved_path)?;

    // Only owner or root can chmod
    if current.uid != 0 && current.uid != metadata.uid {
        return Err(syscall::ErrorCode::EPERM);
    }

    // Change the file mode
    fs::chmod_path(&resolved_path, mode)?;

    Ok(0)
}

/// chown handler - change file ownership
fn chown_handler(path_ptr: u64, uid: u32, gid: u32) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Only root can chown
    if current.uid != 0 {
        return Err(syscall::ErrorCode::EPERM);
    }

    // Resolve path relative to cwd
    let resolved_path = fs::resolve_path(&current.cwd, path)?;

    // Change the file ownership
    // Linux allows -1 (u32::MAX) to mean "don't change"
    fs::chown_path(&resolved_path, uid, gid)?;

    Ok(0)
}

/// getuid handler - get real user ID
fn getuid_handler() -> syscall::SyscallResult {
    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
    Ok(current.uid as u64)
}

/// getgid handler - get real group ID
fn getgid_handler() -> syscall::SyscallResult {
    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
    Ok(current.gid as u64)
}

/// setuid handler - set user ID
fn setuid_handler(uid: u32) -> syscall::SyscallResult {
    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Only root can setuid
    if current.uid != 0 {
        return Err(syscall::ErrorCode::EPERM);
    }

    // Only allow changing to root (0) or user (1000)
    if uid != 0 && uid != 1000 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    current.uid = uid;
    Ok(0)
}

/// setgid handler - set group ID
fn setgid_handler(gid: u32) -> syscall::SyscallResult {
    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Only root can setgid
    if current.uid != 0 {
        return Err(syscall::ErrorCode::EPERM);
    }

    // Only allow changing to root (0) or user (1000)
    if gid != 0 && gid != 1000 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    current.gid = gid;
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

/// lseek handler - reposition read/write file offset
fn lseek_handler(fd: i32, offset: i64, whence: i32) -> syscall::SyscallResult {
    const SEEK_SET: i32 = 0;
    const SEEK_CUR: i32 = 1;
    const SEEK_END: i32 = 2;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Calculate new offset based on whence
    let new_offset = match whence {
        SEEK_SET => {
            // Absolute position
            if offset < 0 {
                return Err(syscall::ErrorCode::EINVAL);
            }
            offset
        }
        SEEK_CUR => {
            // Relative to current position
            let current_offset = current.fd_table.get_offset(fd)?;
            let result = current_offset.checked_add(offset).ok_or(syscall::ErrorCode::EINVAL)?;
            if result < 0 {
                return Err(syscall::ErrorCode::EINVAL);
            }
            result
        }
        SEEK_END => {
            // Relative to end of file
            let file_size = current.fd_table.get_file_size(fd)?;
            let result = file_size.checked_add(offset).ok_or(syscall::ErrorCode::EINVAL)?;
            if result < 0 {
                return Err(syscall::ErrorCode::EINVAL);
            }
            result
        }
        _ => return Err(syscall::ErrorCode::EINVAL),
    };

    // Set the new offset
    let final_offset = current.fd_table.set_offset(fd, new_offset)?;
    Ok(final_offset as u64)
}

/// mkdir handler - create a directory
fn mkdir_handler(path_ptr: u64, _mode: u16) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 256;
    let mut path_buf = [0u8; MAX_PATH_LEN];
    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path against cwd
    let abs_path = fs::resolve_path(&current.cwd, path)?;

    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = mount::resolve_mount_path(&abs_path) {
        match fs_type {
            mount::FsType::Disk => {
                // Disk filesystem is read-only
                return Err(syscall::ErrorCode::EROFS);
            }
            mount::FsType::Tmpfs => {
                // Extract parent path and directory name
                let (parent, name) = if let Some(pos) = rel_path.rfind('/') {
                    let parent = &rel_path[..pos];
                    let name = &rel_path[pos + 1..];
                    (if parent.is_empty() { "/" } else { parent }, name)
                } else {
                    ("/", rel_path.as_str())
                };

                // Create the directory in tmpfs
                mount::tmpfs_mkdir(parent, name)?;
                return Ok(0);
            }
        }
    }

    // In-memory filesystem doesn't support mkdir
    Err(syscall::ErrorCode::EACCES)
}

/// rmdir handler - remove an empty directory
fn rmdir_handler(path_ptr: u64) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 256;
    let mut path_buf = [0u8; MAX_PATH_LEN];
    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve path against cwd
    let abs_path = fs::resolve_path(&current.cwd, path)?;

    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = mount::resolve_mount_path(&abs_path) {
        match fs_type {
            mount::FsType::Disk => {
                // Disk filesystem is read-only
                return Err(syscall::ErrorCode::EROFS);
            }
            mount::FsType::Tmpfs => {
                // Extract parent path and directory name
                let (parent, name) = if let Some(pos) = rel_path.rfind('/') {
                    let parent = &rel_path[..pos];
                    let name = &rel_path[pos + 1..];
                    (if parent.is_empty() { "/" } else { parent }, name)
                } else {
                    ("/", rel_path.as_str())
                };

                // Remove the directory from tmpfs
                mount::tmpfs_rmdir(parent, name)?;
                return Ok(0);
            }
        }
    }

    // In-memory filesystem doesn't support rmdir
    Err(syscall::ErrorCode::EACCES)
}

/// rename handler - rename/move a file or directory
fn rename_handler(oldpath_ptr: u64, newpath_ptr: u64) -> syscall::SyscallResult {
    const MAX_PATH_LEN: usize = 256;
    let mut oldpath_buf = [0u8; MAX_PATH_LEN];
    let mut newpath_buf = [0u8; MAX_PATH_LEN];
    let oldpath = crate::usermode::copy_user_cstr(oldpath_ptr, &mut oldpath_buf)?;
    let newpath = crate::usermode::copy_user_cstr(newpath_ptr, &mut newpath_buf)?;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;

    // Resolve paths against cwd
    let abs_oldpath = fs::resolve_path(&current.cwd, oldpath)?;
    let abs_newpath = fs::resolve_path(&current.cwd, newpath)?;

    // Check if paths are on mounted filesystems
    let old_mount = mount::resolve_mount_path(&abs_oldpath);
    let new_mount = mount::resolve_mount_path(&abs_newpath);

    match (old_mount, new_mount) {
        (None, None) => {
            // Both in in-memory filesystem - not supported
            Err(syscall::ErrorCode::EACCES)
        }
        (Some((_, _, old_fs)), Some((_, _, new_fs))) if old_fs != new_fs => {
            // Different filesystems
            Err(syscall::ErrorCode::EXDEV)
        }
        (Some((_, old_rel, mount::FsType::Disk)), Some((_, _, mount::FsType::Disk))) => {
            // Both on disk - read-only
            let _ = old_rel;
            Err(syscall::ErrorCode::EROFS)
        }
        (Some((_, old_rel, mount::FsType::Tmpfs)), Some((_, new_rel, mount::FsType::Tmpfs))) => {
            // Both on tmpfs - perform rename
            // Extract parent paths and names
            let (old_parent, old_name) = if let Some(pos) = old_rel.rfind('/') {
                let parent = &old_rel[..pos];
                let name = &old_rel[pos + 1..];
                (if parent.is_empty() { "/" } else { parent }, name)
            } else {
                ("/", old_rel.as_str())
            };

            let (new_parent, new_name) = if let Some(pos) = new_rel.rfind('/') {
                let parent = &new_rel[..pos];
                let name = &new_rel[pos + 1..];
                (if parent.is_empty() { "/" } else { parent }, name)
            } else {
                ("/", new_rel.as_str())
            };

            // Perform the rename
            mount::tmpfs_rename(old_parent, old_name, new_parent, new_name)?;
            Ok(0)
        }
        _ => {
            // One path is mounted, the other is not - cross-device
            Err(syscall::ErrorCode::EXDEV)
        }
    }
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

/// brk handler - manage program break (heap allocation)
fn brk_handler(addr: u64) -> syscall::SyscallResult {
    // Constants for page alignment
    const PAGE_SIZE: u64 = 4096;
    const PAGE_MASK: u64 = 0xFFF;

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // If addr is 0, return current break
    if addr == 0 {
        return Ok(current.heap.heap_end);
    }

    // Validate address range
    if addr < current.heap.heap_start || addr > current.heap.heap_limit {
        serial_println!(
            "[BRK] Invalid address {:#x} (start={:#x}, limit={:#x})",
            addr,
            current.heap.heap_start,
            current.heap.heap_limit
        );
        return Ok(current.heap.heap_end); // Return current break on invalid request
    }

    // Check for collision with mmap region
    if addr > current.mmap_base {
        serial_println!("[BRK] Would collide with mmap region at {:#x}", current.mmap_base);
        return Err(syscall::ErrorCode::ENOMEM);
    }

    let old_break = current.heap.heap_end;
    let new_break = (addr + PAGE_MASK) & !PAGE_MASK; // Page-align upward

    serial_println!(
        "[BRK] Change break from {:#x} to {:#x} (requested {:#x})",
        old_break,
        new_break,
        addr
    );

    if new_break > old_break {
        // Growing heap - map new pages
        let num_pages = ((new_break - old_break) / PAGE_SIZE) as usize;
        serial_println!("[BRK] Growing heap by {} pages", num_pages);

        for i in 0..num_pages {
            let page_addr = old_break + (i as u64 * PAGE_SIZE);

            // Allocate physical frame
            // SAFETY: Frame allocator is initialized
            let frame = unsafe { memory::allocate_frame().ok_or(syscall::ErrorCode::ENOMEM)? };

            let phys_addr =
                paging::PhysAddr::new(frame as u64 * panda_hal::memory::FRAME_SIZE as u64);
            let virt_addr = paging::VirtAddr::new(page_addr);

            // Map page with RW, NX, USER flags
            let flags = paging::PageTableFlags::PRESENT
                .or(paging::PageTableFlags::WRITABLE)
                .or(paging::PageTableFlags::USER_ACCESSIBLE)
                .or(paging::PageTableFlags::NO_EXECUTE);

            // SAFETY: Page table is valid, frame is allocated
            unsafe {
                paging::map_page(current.page_table_phys, virt_addr, phys_addr, flags)
                    .map_err(|_| syscall::ErrorCode::ENOMEM)?;
            }

            // Zero the page
            // SAFETY: We just mapped this page, assuming identity mapping
            unsafe {
                core::ptr::write_bytes(phys_addr.as_u64() as *mut u8, 0, 4096);
            }
        }

        current.heap.heap_end = new_break;
    } else if new_break < old_break {
        // Shrinking heap - unmap pages
        let num_pages = ((old_break - new_break) / PAGE_SIZE) as usize;
        serial_println!("[BRK] Shrinking heap by {} pages", num_pages);

        for i in 0..num_pages {
            let page_addr = new_break + (i as u64 * PAGE_SIZE);
            let virt_addr = paging::VirtAddr::new(page_addr);

            // Unmap and deallocate
            // SAFETY: Page table is valid
            unsafe {
                if let Ok(phys_addr) = paging::unmap_page(current.page_table_phys, virt_addr) {
                    let frame = phys_addr.as_u64() / panda_hal::memory::FRAME_SIZE as u64;
                    memory::deallocate_frame(frame as usize);
                }
            }
        }

        current.heap.heap_end = new_break;
    }

    // Return new break
    Ok(new_break)
}

/// mmap handler - map anonymous memory
fn mmap_handler(
    addr: u64,
    length: u64,
    prot: i32,
    flags: i32,
    fd: i32,
    _offset: u64,
) -> syscall::SyscallResult {
    // Constants for page alignment and memory layout
    const PAGE_SIZE: u64 = 4096;
    const PAGE_MASK: u64 = 0xFFF;
    const MAX_MMAP_SIZE: u64 = 0x4000_0000; // 1GB
    const KERNEL_SPACE_START: u64 = 0x8000_0000_0000;

    serial_println!(
        "[MMAP] addr={:#x}, length={}, prot={}, flags={:#x}, fd={}",
        addr,
        length,
        prot,
        flags,
        fd
    );

    // Validate length
    if length == 0 || length > MAX_MMAP_SIZE {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // Round up to page size
    let size = (length + PAGE_MASK) & !PAGE_MASK;
    let num_pages = (size / PAGE_SIZE) as usize;

    // Only support MAP_PRIVATE | MAP_ANONYMOUS
    const MAP_PRIVATE: i32 = 0x02;
    const MAP_ANONYMOUS: i32 = 0x20;

    if (flags & MAP_ANONYMOUS) == 0 || (flags & MAP_PRIVATE) == 0 {
        serial_println!("[MMAP] Only MAP_PRIVATE|MAP_ANONYMOUS supported");
        return Err(syscall::ErrorCode::EINVAL);
    }

    // fd must be -1 for anonymous mappings
    if fd != -1 {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // Parse protection flags
    const PROT_READ: i32 = 0x1;
    const PROT_WRITE: i32 = 0x2;
    const PROT_EXEC: i32 = 0x4;

    // Enforce W^X: reject PROT_WRITE | PROT_EXEC
    if (prot & PROT_WRITE) != 0 && (prot & PROT_EXEC) != 0 {
        serial_println!("[MMAP] W^X violation: PROT_WRITE and PROT_EXEC both set");
        return Err(syscall::ErrorCode::EINVAL);
    }

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };
    let current = scheduler.current_process_mut().ok_or(syscall::ErrorCode::ESRCH)?;

    // Choose address if addr == 0
    let map_addr = if addr == 0 {
        // Allocate from mmap_base, growing downward
        current.mmap_base = current.mmap_base.saturating_sub(size);
        current.mmap_base
    } else {
        // User specified address - validate it's page-aligned
        if (addr & PAGE_MASK) != 0 {
            return Err(syscall::ErrorCode::EINVAL);
        }
        addr
    };

    // Validate address doesn't overlap kernel space
    if map_addr >= KERNEL_SPACE_START {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // Check for collision with heap
    if map_addr < current.heap.heap_end {
        serial_println!(
            "[MMAP] Would collide with heap at {:#x} (heap_end={:#x})",
            map_addr,
            current.heap.heap_end
        );
        return Err(syscall::ErrorCode::ENOMEM);
    }

    serial_println!("[MMAP] Mapping {} pages at {:#x}", num_pages, map_addr);

    // Map pages
    for i in 0..num_pages {
        let page_addr = map_addr + (i as u64 * PAGE_SIZE);

        // Allocate physical frame
        // SAFETY: Frame allocator is initialized
        let frame = unsafe { memory::allocate_frame().ok_or(syscall::ErrorCode::ENOMEM)? };

        let phys_addr = paging::PhysAddr::new(frame as u64 * panda_hal::memory::FRAME_SIZE as u64);
        let virt_addr = paging::VirtAddr::new(page_addr);

        // Build page table flags
        let mut page_flags =
            paging::PageTableFlags::PRESENT.or(paging::PageTableFlags::USER_ACCESSIBLE);

        if (prot & PROT_WRITE) != 0 {
            page_flags = page_flags.or(paging::PageTableFlags::WRITABLE);
        }

        if (prot & PROT_EXEC) == 0 {
            page_flags = page_flags.or(paging::PageTableFlags::NO_EXECUTE);
        }

        // SAFETY: Page table is valid, frame is allocated
        unsafe {
            paging::map_page(current.page_table_phys, virt_addr, phys_addr, page_flags)
                .map_err(|_| syscall::ErrorCode::ENOMEM)?;
        }

        // Zero the page
        // SAFETY: We just mapped this page, assuming identity mapping
        unsafe {
            core::ptr::write_bytes(phys_addr.as_u64() as *mut u8, 0, 4096);
        }
    }

    // Track mapping
    current.mappings.push(process::MemoryMapping {
        addr: map_addr,
        length: size,
        prot: prot as u32,
        flags: flags as u32,
    });

    serial_println!("[MMAP] Successfully mapped at {:#x}", map_addr);
    Ok(map_addr)
}

/// waitpid handler - wait for child process to exit
fn waitpid_handler(pid: i64, status_ptr: u64, options: i32) -> syscall::SyscallResult {
    serial_println!("[WAITPID] pid={}, status_ptr={:#x}, options={}", pid, status_ptr, options);

    // Support WUNTRACED option (0x2)
    const WUNTRACED: i32 = 0x2;
    let wuntraced = (options & WUNTRACED) != 0;

    // Only support options=0 or WUNTRACED
    if options != 0 && options != WUNTRACED {
        return Err(syscall::ErrorCode::EINVAL);
    }

    // SAFETY: Called from syscall handler with interrupts disabled
    let scheduler = unsafe { get_scheduler() };

    let parent = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
    let parent_pid = parent.pid;

    // First, check for stopped children if WUNTRACED is set
    if wuntraced {
        if let Some(stopped_pid) = scheduler.find_stopped_child(parent_pid) {
            serial_println!("[WAITPID] Found stopped child PID {}", stopped_pid.as_u64());

            // Write stop status to user if pointer is non-null
            // Status format for stopped: 0x7f (127) in low byte, signal in next byte
            // For SIGTSTP (20): (20 << 8) | 0x7f = 0x147f
            if status_ptr != 0 {
                let status = ((crate::process::Signal::SIGTSTP as u32) << 8) | 0x7f;
                let status_bytes = status.to_ne_bytes();
                crate::usermode::copy_to_user_bytes(status_ptr, &status_bytes)?;
            }

            // Return child PID (don't reap stopped processes)
            return Ok(stopped_pid.as_u64());
        }
    }

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

                // After returning from yield (when woken), retry to find zombie or stopped
                // Get fresh scheduler reference after context switch
                let scheduler = unsafe { get_scheduler() };
                let parent = scheduler.current_process().ok_or(syscall::ErrorCode::ESRCH)?;
                let parent_pid = parent.pid;

                // Check for stopped children again if WUNTRACED is set
                if wuntraced {
                    if let Some(stopped_pid) = scheduler.find_stopped_child(parent_pid) {
                        serial_println!(
                            "[WAITPID] After wake, found stopped child PID {}",
                            stopped_pid.as_u64()
                        );

                        // Write stop status to user if pointer is non-null
                        if status_ptr != 0 {
                            let status = ((crate::process::Signal::SIGTSTP as u32) << 8) | 0x7f;
                            let status_bytes = status.to_ne_bytes();
                            crate::usermode::copy_to_user_bytes(status_ptr, &status_bytes)?;
                        }

                        return Ok(stopped_pid.as_u64());
                    }
                }

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
        #[cfg(feature = "ls-long-smoke")]
        serial_println!("TEST PASS ls_long_smoke");
        #[cfg(feature = "cd-smoke")]
        serial_println!("TEST PASS cd_smoke");
        #[cfg(feature = "path-smoke")]
        serial_println!("TEST PASS path_smoke");
        #[cfg(feature = "redir-smoke")]
        serial_println!("TEST PASS redir_smoke");
        #[cfg(feature = "tmpfs-redir-smoke")]
        serial_println!("TEST PASS tmpfs_redir_smoke");
        #[cfg(feature = "elf-exec-smoke")]
        serial_println!("TEST PASS elf_exec_smoke");
        #[cfg(feature = "tty-smoke")]
        serial_println!("TEST PASS tty_smoke");
        #[cfg(feature = "preempt-smoke")]
        {
            // Print observability data for preemption test
            let tick = unsafe { get_tick_counter() };
            let switches = unsafe { get_context_switch_counter() };
            serial_println!("[PREEMPT] Final stats: ticks={} switches={}", tick, switches);
            serial_println!("TEST PASS preempt_smoke");
        }
        #[cfg(not(any(
            feature = "shell-smoke",
            feature = "vfs-cat-smoke",
            feature = "fork-exec-smoke",
            feature = "pipe-smoke",
            feature = "ctrlc-smoke",
            feature = "ls-smoke",
            feature = "ls-stat-smoke",
            feature = "ls-long-smoke",
            feature = "cd-smoke",
            feature = "path-smoke",
            feature = "redir-smoke",
            feature = "tmpfs-redir-smoke",
            feature = "elf-exec-smoke",
            feature = "tty-smoke",
            feature = "preempt-smoke"
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
    match fs::open_path_with_flags(&mut fd_table, "/mnt/hello.txt", fs::O_RDONLY, 0, 0) {
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
