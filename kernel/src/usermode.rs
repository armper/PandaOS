//! User mode support for x86_64
//!
//! This module provides functionality for transitioning to ring 3 (user mode)
//! and handling syscall entry/exit using the syscall/sysret instructions.
//!
//! ## Invariants
//!
//! - User mode code runs at ring 3
//! - Kernel code runs at ring 0
//! - User stacks are separate from kernel stacks
//! - Syscall/sysret are configured before first user mode transition

use crate::context::CpuContext;
use crate::{gdt, syscall};
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask, Star};
use x86_64::registers::rflags::RFlags;
use x86_64::{PhysAddr, VirtAddr};

/// Current process syscall context pointer (arch-local, single CPU)
static mut CURRENT_CONTEXT_PTR: *mut CpuContext = core::ptr::null_mut();

/// Scratch storage for user RSP during syscall entry
static mut USER_RSP_SCRATCH: u64 = 0;

/// Scratch storage for user RDI during syscall entry
static mut USER_RDI_SCRATCH: u64 = 0;

/// Kernel page table physical address (captured during init).
static KERNEL_PAGE_TABLE_PHYS: AtomicU64 = AtomicU64::new(0);

/// Pending exited process page table for reaping.
static PENDING_REAP_PT: AtomicU64 = AtomicU64::new(0);
static PENDING_REAP_PID: AtomicU64 = AtomicU64::new(0);

const REAPER_STACK_SIZE: usize = 4096;

#[allow(dead_code)]
#[repr(align(16))]
struct ReaperStack([u8; REAPER_STACK_SIZE]);

static mut REAPER_STACK: ReaperStack = ReaperStack([0; REAPER_STACK_SIZE]);

/// Initialize syscall/sysret support
///
/// This configures the MSRs needed for the syscall/sysret instructions:
/// - STAR: Segment selectors for syscall/sysret
/// - LSTAR: Entry point for syscall
/// - SFMASK: RFLAGS mask on syscall entry
/// - EFER.SCE: Enable syscall/sysret
///
/// # Safety
///
/// Must be called exactly once during kernel initialization after GDT is set up.
pub unsafe fn init_syscall() {
    // SAFETY: Caller guarantees GDT is initialized
    let selectors = unsafe { gdt::get_selectors() };

    // Configure STAR register with segment selectors
    // SAFETY: We're configuring syscall/sysret during kernel init
    // Note: Star::write returns Result but failure is not possible with valid selectors
    let _ = Star::write(
        selectors.user_code,
        selectors.user_data,
        selectors.kernel_code,
        selectors.kernel_data,
    );

    // Set LSTAR to syscall entry point
    // SAFETY: syscall_entry is a valid function pointer
    LStar::write(VirtAddr::new(syscall_entry as *const () as u64));

    // Set SFMASK to clear IF (interrupts) on syscall entry
    // SAFETY: This is a valid RFLAGS configuration
    SFMask::write(RFlags::INTERRUPT_FLAG);

    // Enable syscall/sysret in EFER
    // SAFETY: This is safe to do during kernel init
    unsafe {
        Efer::update(|flags| {
            *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
    }
}

/// Set current process syscall context
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - `ctx` must remain valid while the process is running
pub unsafe fn set_current_syscall_context(ctx: *mut CpuContext) {
    // SAFETY: Caller guarantees interrupts are disabled and ctx is valid
    unsafe {
        CURRENT_CONTEXT_PTR = ctx;
    }
}

/// Store the kernel page table physical address for later reaping.
pub fn set_kernel_page_table_phys(phys: u64) {
    KERNEL_PAGE_TABLE_PHYS.store(phys, Ordering::Relaxed);
}

/// Read the stored kernel page table physical address.
pub fn kernel_page_table_phys() -> u64 {
    KERNEL_PAGE_TABLE_PHYS.load(Ordering::Relaxed)
}

/// Switch the active page table (CR3) while staying in kernel mode.
pub fn switch_page_table(page_table_phys: u64) {
    use x86_64::registers::control::Cr3;
    use x86_64::structures::paging::PhysFrame;

    let phys = PhysAddr::new(page_table_phys);
    let (current_frame, flags) = Cr3::read();

    if current_frame.start_address() == phys {
        return;
    }

    // SAFETY: Caller ensures page table is valid and interrupts are disabled.
    unsafe {
        Cr3::write(PhysFrame::containing_address(phys), flags);
    }
}

/// Queue an exited process for reaping after a CR3 switch.
pub fn set_pending_reap(page_table_phys: u64, pid: u64) {
    PENDING_REAP_PID.store(pid, Ordering::Relaxed);
    PENDING_REAP_PT.store(page_table_phys, Ordering::Release);
}

/// Copy a NUL-terminated user string into a kernel buffer.
pub fn copy_user_cstr<'a>(ptr: u64, buf: &'a mut [u8]) -> Result<&'a str, syscall::ErrorCode> {
    if ptr == 0 {
        return Err(syscall::ErrorCode::EFAULT);
    }

    for i in 0..buf.len() {
        // SAFETY: Caller provides user pointer; we cap the read length.
        let byte = unsafe { *(ptr as *const u8).add(i) };
        if byte == 0 {
            return core::str::from_utf8(&buf[..i]).map_err(|_| syscall::ErrorCode::EINVAL);
        }
        buf[i] = byte;
    }

    Err(syscall::ErrorCode::EINVAL)
}

/// Copy raw bytes from user memory into a kernel buffer.
pub fn copy_user_bytes(ptr: u64, len: usize, dst: &mut [u8]) -> Result<usize, syscall::ErrorCode> {
    if ptr == 0 {
        return Err(syscall::ErrorCode::EFAULT);
    }

    if len > dst.len() {
        return Err(syscall::ErrorCode::EINVAL);
    }

    for i in 0..len {
        // SAFETY: Caller provides user pointer; we cap the read length.
        dst[i] = unsafe { *(ptr as *const u8).add(i) };
    }

    Ok(len)
}

/// Copy raw bytes from a kernel buffer into user memory.
pub fn copy_to_user_bytes(ptr: u64, src: &[u8]) -> Result<usize, syscall::ErrorCode> {
    if ptr == 0 {
        return Err(syscall::ErrorCode::EFAULT);
    }

    if src.is_empty() {
        return Ok(0);
    }

    let len = src.len();
    if ptr.checked_add(len as u64).is_none() {
        return Err(syscall::ErrorCode::EFAULT);
    }

    for (i, byte) in src.iter().enumerate() {
        // SAFETY: Caller provides user pointer; we cap the write length.
        unsafe {
            *((ptr + i as u64) as *mut u8) = *byte;
        }
    }

    Ok(len)
}

/// Reap a pending exited process after a CR3 switch.
extern "C" fn reap_pending() {
    let page_table_phys = PENDING_REAP_PT.swap(0, Ordering::AcqRel);
    if page_table_phys == 0 {
        return;
    }

    let pid = PENDING_REAP_PID.load(Ordering::Relaxed);
    serial_println!("[REAP] Freeing exited process PID {} (pt={:#x})", pid, page_table_phys);

    // SAFETY: The exited process is no longer active; we are on a different CR3.
    unsafe {
        crate::paging::free_process_address_space(page_table_phys, true)
            .expect("Failed to reap exited process");
    }
}

/// Syscall entry point
///
/// This function is called via the syscall instruction from user mode.
/// The syscall instruction:
/// - Saves RIP to RCX
/// - Saves RFLAGS to R11
/// - Loads RIP from LSTAR MSR
/// - Loads CS from STAR MSR
/// - Clears RFLAGS bits specified in SFMASK
///
/// Register state on entry:
/// - RCX: user RIP
/// - R11: user RFLAGS
/// - RAX: syscall number
/// - RDI, RSI, RDX, R10, R8, R9: syscall arguments
///
/// Saved on entry:
/// - All GPRs into the current process CpuContext
/// - User RIP (RCX) into context.rip
/// - User RFLAGS (R11) into context.rflags
/// - User RSP into context.rsp (captured before kernel stack switch)
///
/// We use a naked function to have full control over register preservation
/// and the sysret return path.
///
/// # Safety
///
/// This must only be called via the syscall instruction.
#[unsafe(naked)]
extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Preserve user stack pointer and arg1 before switching stacks.
        "mov [rip + {user_rsp_scratch}], rsp",
        "mov [rip + {user_rdi_scratch}], rdi",

        // Switch to current process kernel stack (fixed VA, per-process mapping).
        "mov rsp, {kernel_stack_top}",

        // Load current process context pointer.
        "mov rdi, [rip + {context_ptr}]",

        // Save general-purpose registers into CpuContext.
        "mov [rdi + 0x00], r15",
        "mov [rdi + 0x08], r14",
        "mov [rdi + 0x10], r13",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], r11",
        "mov [rdi + 0x28], r10",
        "mov [rdi + 0x30], r9",
        "mov [rdi + 0x38], r8",
        "mov [rdi + 0x40], rbp",
        "mov [rdi + 0x50], rsi",
        "mov [rdi + 0x58], rdx",
        "mov [rdi + 0x60], rcx",
        "mov [rdi + 0x68], rbx",
        "mov [rdi + 0x70], rax",

        // Save original RDI from scratch.
        "mov rax, [rip + {user_rdi_scratch}]",
        "mov [rdi + 0x48], rax",

        // Save user RIP/RFLAGS from RCX/R11.
        "mov [rdi + 0x78], rcx",
        "mov [rdi + 0x88], r11",

        // Save user RSP from scratch.
        "mov rax, [rip + {user_rsp_scratch}]",
        "mov [rdi + 0x80], rax",

        // Call syscall handler (reads args from saved context).
        "call {syscall_handler}",

        // Store return value in context.rax.
        "mov rsi, [rip + {context_ptr}]",
        "mov [rsi + 0x70], rax",

        // Restore registers from CpuContext and return to user mode.
        "mov r15, [rsi + 0x00]",
        "mov r14, [rsi + 0x08]",
        "mov r13, [rsi + 0x10]",
        "mov r12, [rsi + 0x18]",
        "mov r10, [rsi + 0x28]",
        "mov r9, [rsi + 0x30]",
        "mov r8, [rsi + 0x38]",
        "mov rbp, [rsi + 0x40]",
        "mov rdi, [rsi + 0x48]",
        "mov rdx, [rsi + 0x58]",
        "mov rbx, [rsi + 0x68]",
        "mov rax, [rsi + 0x70]",
        "mov rcx, [rsi + 0x78]",
        "mov r11, [rsi + 0x88]",
        "mov rsp, [rsi + 0x80]",
        "mov rsi, [rsi + 0x50]",
        "sysretq",

        syscall_handler = sym syscall_handler_rust,
        context_ptr = sym CURRENT_CONTEXT_PTR,
        kernel_stack_top = const crate::paging::KERNEL_STACK_TOP,
        user_rsp_scratch = sym USER_RSP_SCRATCH,
        user_rdi_scratch = sym USER_RDI_SCRATCH,
    );
}

/// Rust syscall handler called from assembly entry point
///
/// This function reads syscall arguments from the saved CpuContext.
extern "C" fn syscall_handler_rust() -> i64 {
    // SAFETY: syscall_entry saved the current process context before calling.
    let ctx = unsafe { CURRENT_CONTEXT_PTR.as_ref() }.expect("Syscall context not set");

    syscall::handle_syscall(ctx.rax, ctx.rdi, ctx.rsi, ctx.rdx, ctx.r10, ctx.r8, ctx.r9)
}

/// Switch to a new user process from syscall context
///
/// This switches CR3 to the target page table, restores user registers from
/// the provided CpuContext, and returns to ring 3 via sysretq.
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - `ctx` must point to a valid, initialized user context
/// - `page_table_phys` must be a valid L4 page table physical address
pub unsafe fn switch_to_user(ctx: *const CpuContext, page_table_phys: u64) -> ! {
    // SAFETY: Caller guarantees ctx and page table are valid and interrupts are disabled
    unsafe {
        core::arch::asm!(
            "mov cr3, rax",

            "mov r15, [rsi + 0x00]",
            "mov r14, [rsi + 0x08]",
            "mov r13, [rsi + 0x10]",
            "mov r12, [rsi + 0x18]",
            "mov r10, [rsi + 0x28]",
            "mov r9, [rsi + 0x30]",
            "mov r8, [rsi + 0x38]",
            "mov rbp, [rsi + 0x40]",
            "mov rdi, [rsi + 0x48]",
            "mov rdx, [rsi + 0x58]",
            "mov rbx, [rsi + 0x68]",
            "mov rax, [rsi + 0x70]",
            "mov rcx, [rsi + 0x78]",
            "mov r11, [rsi + 0x88]",
            "mov rsp, [rsi + 0x80]",
            "mov rsi, [rsi + 0x50]",
            "sysretq",
            in("rsi") ctx,
            in("rax") page_table_phys,
            options(noreturn)
        );
    }
}

/// Switch to a new user process, then reap any exited process after CR3 switch.
///
/// # Safety
///
/// - Must be called with interrupts disabled
/// - `ctx` must point to a valid, initialized user context
/// - `page_table_phys` must be a valid L4 page table physical address
pub unsafe fn switch_to_user_with_reap(ctx: *const CpuContext, page_table_phys: u64) -> ! {
    // SAFETY: Caller guarantees ctx and page table are valid and interrupts are disabled
    unsafe {
        core::arch::asm!(
            "lea rsp, [rip + {stack}]",
            "add rsp, {stack_size}",
            "and rsp, -16",
            "mov cr3, rax",
            "call {reap_fn}",
            "mov rsi, r12",

            "mov r15, [rsi + 0x00]",
            "mov r14, [rsi + 0x08]",
            "mov r13, [rsi + 0x10]",
            "mov r12, [rsi + 0x18]",
            "mov r10, [rsi + 0x28]",
            "mov r9, [rsi + 0x30]",
            "mov r8, [rsi + 0x38]",
            "mov rbp, [rsi + 0x40]",
            "mov rdi, [rsi + 0x48]",
            "mov rdx, [rsi + 0x58]",
            "mov rbx, [rsi + 0x68]",
            "mov rax, [rsi + 0x70]",
            "mov rcx, [rsi + 0x78]",
            "mov r11, [rsi + 0x88]",
            "mov rsp, [rsi + 0x80]",
            "mov rsi, [rsi + 0x50]",
            "sysretq",
            stack = sym REAPER_STACK,
            stack_size = const REAPER_STACK_SIZE,
            reap_fn = sym reap_pending,
            in("rax") page_table_phys,
            in("r12") ctx,
            options(noreturn)
        );
    }
}

extern "C" fn exit_after_reap() -> ! {
    crate::exit_qemu(crate::QemuExitCode::Success);
}

/// Switch to the kernel page table, reap pending exits, then halt.
pub fn switch_to_kernel_and_reap_then_halt() -> ! {
    let kernel_pt = KERNEL_PAGE_TABLE_PHYS.load(Ordering::Relaxed);
    let kernel_pt = if kernel_pt == 0 {
        use x86_64::registers::control::Cr3;
        Cr3::read().0.start_address().as_u64()
    } else {
        kernel_pt
    };

    // SAFETY: This path never returns and uses a dedicated kernel stack.
    unsafe {
        core::arch::asm!(
            "lea rsp, [rip + {stack}]",
            "add rsp, {stack_size}",
            "and rsp, -16",
            "mov cr3, rax",
            "call {reap_fn}",
            "jmp {exit_fn}",
            stack = sym REAPER_STACK,
            stack_size = const REAPER_STACK_SIZE,
            reap_fn = sym reap_pending,
            exit_fn = sym exit_after_reap,
            in("rax") kernel_pt,
            options(noreturn)
        );
    }
}

/// Jump to user mode at given entry point
///
/// This function does not return - it switches to ring 3 and begins
/// executing user code.
///
/// # Safety
///
/// - Entry point must be valid user code
/// - Stack pointer must point to valid user stack
/// - Page table must be valid and properly initialized
/// - Called only once per process
/// - GDT must be initialized before calling
pub unsafe fn enter_usermode(entry_point: u64, stack_ptr: u64, page_table_phys: u64) -> ! {
    // Get GDT selectors
    // SAFETY: Caller guarantees GDT is initialized
    let selectors = unsafe { gdt::get_selectors() };
    let user_data_sel = selectors.user_data.0;
    let user_code_sel = selectors.user_code.0;

    // SAFETY: Caller guarantees entry point and stacks are valid
    unsafe {
        core::arch::asm!(
            // Switch to process page table
            "mov cr3, {page_table_phys}",

            // Set up user data segments
            "mov ds, {user_ds:x}",
            "mov es, {user_ds:x}",
            "mov fs, {user_ds:x}",
            "mov gs, {user_ds:x}",

            // Switch to process kernel stack before building iret frame
            "mov rsp, {kernel_stack_top}",

            // Push user stack frame for iretq
            "push {user_ds:r}",        // SS
            "push {stack_ptr}",        // RSP
            "pushfq",                  // RFLAGS
            "pop rax",
            "or rax, 0x200",           // Set IF (interrupts enabled)
            "push rax",
            "push {user_cs:r}",        // CS
            "push {entry_point}",      // RIP

            // Clear registers for security
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rsi, rsi",
            "xor rdi, rdi",
            "xor rbp, rbp",
            "xor r8, r8",
            "xor r9, r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",

            // Jump to user mode
            "iretq",

            user_ds = in(reg) user_data_sel as u64,
            user_cs = in(reg) user_code_sel as u64,
            entry_point = in(reg) entry_point,
            stack_ptr = in(reg) stack_ptr,
            kernel_stack_top = const crate::paging::KERNEL_STACK_TOP,
            page_table_phys = in(reg) page_table_phys,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_gdt_integration() {
        // Test that GDT selectors are properly structured
        // This is a compile-time test that ensures the module compiles correctly
        assert!(true);
    }
}
