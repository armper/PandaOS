//! System call ABI definitions for PandaOS
//!
//! This module defines the syscall interface following Linux x86_64 ABI conventions.
//! Syscall numbers and error codes are locked down early to prevent ABI drift.
//!
//! ## Calling Convention (x86_64)
//!
//! - Syscall number: `rax`
//! - Arguments (up to 6): `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`
//! - Return value: `rax`
//! - Return error: `-errno` (negative value)
//! - Instruction: `syscall`
//!
//! ## Invariants
//!
//! - Syscall numbers never change once defined
//! - Error codes follow POSIX errno conventions
//! - Syscalls preserve all GPRs except RAX (return value) and RCX/R11 (syscall clobbers)

// Import macros for logging
#[cfg(any(
    feature = "shell-smoke",
    feature = "vfs-cat-smoke",
    feature = "fork-exec-smoke",
    feature = "pipe-smoke"
))]
use core::sync::atomic::{AtomicUsize, Ordering};
use panda_hal::serial_println;
use spin::Once;

/// Syscall numbers (Linux-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum SyscallNumber {
    /// Read from file descriptor
    Read = 0,
    /// Write to file descriptor
    Write = 1,
    /// Open file
    Open = 2,
    /// Close file descriptor
    Close = 3,
    /// Get file status
    Stat = 4,
    /// Get file status (by fd)
    Fstat = 5,
    /// Exit process
    Exit = 60,
    /// Fork process
    Fork = 57,
    /// Execute program
    Execve = 59,
    /// Wait for process
    Wait4 = 61,
    /// Memory map
    Mmap = 9,
    /// Memory unmap
    Munmap = 11,
    /// Memory protect
    Mprotect = 10,
    /// Create pipe
    Pipe = 22,
    /// Duplicate file descriptor
    Dup = 32,
    /// Duplicate file descriptor (with target)
    Dup2 = 33,
    /// Get current directory
    Getcwd = 79,
    /// Change directory
    Chdir = 80,
    /// Get process ID
    Getpid = 39,
    /// Send signal
    Kill = 62,
    /// Yield CPU (sched_yield)
    Yield = 24,
}

impl SyscallNumber {
    /// Convert from raw syscall number
    pub const fn from_u64(n: u64) -> Option<Self> {
        match n {
            0 => Some(Self::Read),
            1 => Some(Self::Write),
            2 => Some(Self::Open),
            3 => Some(Self::Close),
            4 => Some(Self::Stat),
            5 => Some(Self::Fstat),
            9 => Some(Self::Mmap),
            10 => Some(Self::Mprotect),
            11 => Some(Self::Munmap),
            22 => Some(Self::Pipe),
            24 => Some(Self::Yield),
            32 => Some(Self::Dup),
            33 => Some(Self::Dup2),
            39 => Some(Self::Getpid),
            57 => Some(Self::Fork),
            59 => Some(Self::Execve),
            60 => Some(Self::Exit),
            61 => Some(Self::Wait4),
            62 => Some(Self::Kill),
            79 => Some(Self::Getcwd),
            80 => Some(Self::Chdir),
            _ => None,
        }
    }

    /// Get syscall name for debugging
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Open => "open",
            Self::Close => "close",
            Self::Stat => "stat",
            Self::Fstat => "fstat",
            Self::Exit => "exit",
            Self::Fork => "fork",
            Self::Execve => "execve",
            Self::Wait4 => "wait4",
            Self::Mmap => "mmap",
            Self::Munmap => "munmap",
            Self::Mprotect => "mprotect",
            Self::Pipe => "pipe",
            Self::Dup => "dup",
            Self::Dup2 => "dup2",
            Self::Getcwd => "getcwd",
            Self::Chdir => "chdir",
            Self::Getpid => "getpid",
            Self::Kill => "kill",
            Self::Yield => "yield",
        }
    }
}

/// POSIX error codes (errno)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum ErrorCode {
    /// Operation not permitted
    EPERM = 1,
    /// No such file or directory
    ENOENT = 2,
    /// No such process
    ESRCH = 3,
    /// Interrupted system call
    EINTR = 4,
    /// I/O error
    EIO = 5,
    /// Bad file descriptor
    EBADF = 9,
    /// Try again
    EAGAIN = 11,
    /// Out of memory
    ENOMEM = 12,
    /// Permission denied
    EACCES = 13,
    /// Bad address
    EFAULT = 14,
    /// File exists
    EEXIST = 17,
    /// Not a directory
    ENOTDIR = 20,
    /// Is a directory
    EISDIR = 21,
    /// Invalid argument
    EINVAL = 22,
    /// Too many open files
    EMFILE = 24,
    /// Broken pipe
    EPIPE = 32,
    /// Function not implemented
    ENOSYS = 38,
}

impl ErrorCode {
    /// Convert to negative return value for syscall
    pub const fn to_syscall_result(self) -> i64 {
        -(self as i64)
    }
}

/// Syscall result type
pub type SyscallResult = Result<u64, ErrorCode>;

/// Syscall handler dispatcher
///
/// Currently all syscalls panic - they will be implemented incrementally.
#[allow(clippy::too_many_arguments)]
pub fn handle_syscall(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> i64 {
    let syscall = match SyscallNumber::from_u64(number) {
        Some(s) => s,
        None => {
            // Unknown syscall
            return ErrorCode::ENOSYS.to_syscall_result();
        }
    };

    let result = match syscall {
        SyscallNumber::Exit => sys_exit(arg1 as i32),
        SyscallNumber::Read => sys_read(arg1 as i32, arg2, arg3),
        SyscallNumber::Write => sys_write(arg1 as i32, arg2, arg3),
        SyscallNumber::Open => sys_open(arg1, arg2, arg3),
        SyscallNumber::Close => sys_close(arg1 as i32),
        SyscallNumber::Getpid => sys_getpid(),
        SyscallNumber::Yield => sys_yield(),
        SyscallNumber::Execve => sys_exec(arg1, arg2),
        SyscallNumber::Fork => sys_fork(),
        SyscallNumber::Wait4 => sys_waitpid(arg1 as i64, arg2, arg3 as i32),
        SyscallNumber::Pipe => sys_pipe(arg1),
        SyscallNumber::Dup2 => sys_dup2(arg1 as i32, arg2 as i32),
        SyscallNumber::Kill => sys_kill(arg1 as i32, arg2 as i32),
        // All other syscalls return ENOSYS for now
        _ => Err(ErrorCode::ENOSYS),
    };

    match result {
        Ok(val) => val as i64,
        Err(err) => err.to_syscall_result(),
    }
}

/// sys_exit - Exit the current process
///
/// This function never returns normally - either the exit handler is called
/// (which has ! return type) or the kernel halts.
fn sys_exit(status: i32) -> SyscallResult {
    serial_println!("Process exiting with status: {}", status);

    // Exit QEMU if exit handler is set (for testing)
    if let Some(exit_fn) = EXIT_HANDLER.get() {
        // This call never returns - exit_fn has signature fn(i32) -> !
        exit_fn(status);
    }

    // If no exit handler, halt the system
    // In a full implementation, this would:
    // - Mark process as exited in the scheduler
    // - Free process resources (memory, file descriptors, etc.)
    // - Schedule next process
    loop {
        x86_64::instructions::hlt();
    }
}

/// Exit handler function pointer for testing
static EXIT_HANDLER: Once<fn(i32) -> !> = Once::new();

/// Set the exit handler for syscall exit
///
/// Must be called before any user processes run.
/// Handler must never return.
pub fn set_exit_handler(handler: fn(i32) -> !) {
    EXIT_HANDLER.call_once(|| handler);
}

/// sys_write - Write to file descriptor
fn sys_write(fd: i32, buf: u64, count: u64) -> SyscallResult {
    // For stdout (fd 1) and stderr (fd 2), check if redirected to pipe first
    if fd == 1 || fd == 2 {
        if let Some(write_fn) = WRITE_HANDLER.get() {
            // Try to write to fd table (may be a pipe)
            match write_fn(fd, buf, count) {
                Ok(n) => return Ok(n),
                Err(ErrorCode::EBADF) => {
                    // Not a pipe, fall through to serial output
                }
                Err(e) => return Err(e),
            }
        }

        // Default stdout/stderr behavior: write to serial
        if buf == 0 || count == 0 {
            return Ok(0);
        }

        // Limit write size to prevent abuse
        if count > 4096 {
            return Err(ErrorCode::EINVAL);
        }

        let mut local_buf = [0u8; 4096];
        let count = usize::try_from(count).map_err(|_| ErrorCode::EINVAL)?;
        let copied = crate::usermode::copy_user_bytes(buf, count, &mut local_buf)?;
        let slice = &local_buf[..copied];

        // Write raw bytes to serial output
        for &byte in slice {
            panda_hal::serial::write_byte_raw(byte);
        }

        return Ok(count as u64);
    }

    // For other fds, use the write handler
    if let Some(write_fn) = WRITE_HANDLER.get() {
        write_fn(fd, buf, count)
    } else {
        Err(ErrorCode::EBADF)
    }
}

#[cfg(feature = "shell-smoke")]
const SCRIPTED_INPUT: &[u8] = b"help\nexit\n";

#[cfg(feature = "vfs-cat-smoke")]
const SCRIPTED_INPUT: &[u8] = b"cat /etc/motd\nexit\n";

#[cfg(feature = "fork-exec-smoke")]
const SCRIPTED_INPUT: &[u8] = b"cat /etc/version\ntrue\nexit\n";

#[cfg(feature = "pipe-smoke")]
const SCRIPTED_INPUT: &[u8] = b"echo hello | wc\nexit\n";

#[cfg(all(
    feature = "shell-smoke",
    any(feature = "vfs-cat-smoke", feature = "fork-exec-smoke", feature = "pipe-smoke")
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, and pipe-smoke are mutually exclusive"
);

#[cfg(all(feature = "vfs-cat-smoke", any(feature = "fork-exec-smoke", feature = "pipe-smoke")))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, and pipe-smoke are mutually exclusive"
);

#[cfg(all(feature = "fork-exec-smoke", feature = "pipe-smoke"))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, and pipe-smoke are mutually exclusive"
);

#[cfg(any(
    feature = "shell-smoke",
    feature = "vfs-cat-smoke",
    feature = "fork-exec-smoke",
    feature = "pipe-smoke"
))]
static SCRIPTED_POS: AtomicUsize = AtomicUsize::new(0);

fn read_byte() -> Option<u8> {
    #[cfg(any(
        feature = "shell-smoke",
        feature = "vfs-cat-smoke",
        feature = "fork-exec-smoke",
        feature = "pipe-smoke"
    ))]
    {
        let pos = SCRIPTED_POS.fetch_add(1, Ordering::Relaxed);
        return SCRIPTED_INPUT.get(pos).copied();
    }

    #[cfg(not(any(
        feature = "shell-smoke",
        feature = "vfs-cat-smoke",
        feature = "fork-exec-smoke",
        feature = "pipe-smoke"
    )))]
    {
        return panda_hal::serial::serial_read_byte();
    }
}

/// sys_read - Read from file descriptor
fn sys_read(fd: i32, buf: u64, count: u64) -> SyscallResult {
    if count == 0 {
        return Ok(0);
    }

    if buf == 0 {
        return Err(ErrorCode::EFAULT);
    }

    // For stdin (fd 0), check if it's redirected to a pipe via the handler first
    if fd == 0 {
        if let Some(read_fn) = READ_HANDLER.get() {
            // Try to read from fd table (may be a pipe)
            match read_fn(fd, buf, count) {
                Ok(n) => return Ok(n),
                Err(ErrorCode::EBADF) => {
                    // Not a pipe, fall through to serial input
                }
                Err(e) => return Err(e),
            }
        }

        // Default stdin behavior: read from serial
        let count = usize::try_from(count).map_err(|_| ErrorCode::EINVAL)?;
        if count > 4096 {
            return Err(ErrorCode::EINVAL);
        }

        let mut read = 0usize;
        loop {
            match read_byte() {
                Some(byte) => {
                    let tmp = [byte];
                    let dst = buf.checked_add(read as u64).ok_or(ErrorCode::EFAULT)?;
                    crate::usermode::copy_to_user_bytes(dst, &tmp)?;
                    read += 1;
                    if read == count {
                        break;
                    }
                }
                None => {
                    if read > 0 {
                        break;
                    }
                    core::hint::spin_loop();
                }
            }
        }

        return Ok(read as u64);
    }

    if fd == 1 || fd == 2 {
        return Err(ErrorCode::EBADF);
    }

    if let Some(read_fn) = READ_HANDLER.get() {
        read_fn(fd, buf, count)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_open - Open a file path (read-only)
fn sys_open(path_ptr: u64, _flags: u64, _mode: u64) -> SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    if let Some(open_fn) = OPEN_HANDLER.get() {
        open_fn(path)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_close - Close a file descriptor
fn sys_close(fd: i32) -> SyscallResult {
    if let Some(close_fn) = CLOSE_HANDLER.get() {
        close_fn(fd)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_getpid - Get process ID
fn sys_getpid() -> SyscallResult {
    if let Some(getpid_fn) = GETPID_HANDLER.get() {
        getpid_fn()
    } else {
        Ok(1)
    }
}

/// sys_fork - Fork the current process
fn sys_fork() -> SyscallResult {
    if let Some(fork_fn) = FORK_HANDLER.get() {
        fork_fn()
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_waitpid - Wait for process state change
fn sys_waitpid(pid: i64, status_ptr: u64, options: i32) -> SyscallResult {
    if let Some(waitpid_fn) = WAITPID_HANDLER.get() {
        waitpid_fn(pid, status_ptr, options)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_yield - Voluntarily yield the CPU to another process
fn sys_yield() -> SyscallResult {
    // Get scheduler instance if available
    if let Some(yield_fn) = YIELD_HANDLER.get() {
        yield_fn();
    }

    // Always return success
    Ok(0)
}

/// sys_exec - Replace the current process image with an optional argument string
fn sys_exec(path_ptr: u64, arg_ptr: u64) -> SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    const MAX_ARG_LEN: usize = 128;
    let mut path_buf = [0u8; MAX_PATH_LEN];
    let mut arg_buf = [0u8; MAX_ARG_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;
    let arg = if arg_ptr == 0 {
        None
    } else {
        Some(crate::usermode::copy_user_cstr(arg_ptr, &mut arg_buf)?)
    };

    if let Some(exec_fn) = EXEC_HANDLER.get() {
        match exec_fn(path, arg) {
            Ok(()) => Err(ErrorCode::EIO),
            Err(err) => Err(err),
        }
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_pipe - Create a pipe
fn sys_pipe(pipefd_ptr: u64) -> SyscallResult {
    if let Some(pipe_fn) = PIPE_HANDLER.get() {
        pipe_fn(pipefd_ptr)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_dup2 - Duplicate a file descriptor
fn sys_dup2(oldfd: i32, newfd: i32) -> SyscallResult {
    if let Some(dup2_fn) = DUP2_HANDLER.get() {
        dup2_fn(oldfd, newfd)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_kill - Send a signal to a process
fn sys_kill(pid: i32, sig: i32) -> SyscallResult {
    if let Some(kill_fn) = KILL_HANDLER.get() {
        kill_fn(pid, sig)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// Yield handler function pointer for scheduler integration
static YIELD_HANDLER: Once<fn()> = Once::new();
static EXEC_HANDLER: Once<fn(&str, Option<&str>) -> Result<(), ErrorCode>> = Once::new();
static OPEN_HANDLER: Once<fn(&str) -> SyscallResult> = Once::new();
static READ_HANDLER: Once<fn(i32, u64, u64) -> SyscallResult> = Once::new();
static WRITE_HANDLER: Once<fn(i32, u64, u64) -> SyscallResult> = Once::new();
static CLOSE_HANDLER: Once<fn(i32) -> SyscallResult> = Once::new();
static GETPID_HANDLER: Once<fn() -> SyscallResult> = Once::new();
static FORK_HANDLER: Once<fn() -> SyscallResult> = Once::new();
static WAITPID_HANDLER: Once<fn(i64, u64, i32) -> SyscallResult> = Once::new();
static PIPE_HANDLER: Once<fn(u64) -> SyscallResult> = Once::new();
static DUP2_HANDLER: Once<fn(i32, i32) -> SyscallResult> = Once::new();
static KILL_HANDLER: Once<fn(i32, i32) -> SyscallResult> = Once::new();

/// Set the yield handler for syscall yield
///
/// Must be called before any user processes run.
pub fn set_yield_handler(handler: fn()) {
    YIELD_HANDLER.call_once(|| handler);
}

/// Set the exec handler for syscall execve
///
/// Must be called before any user processes run.
/// The second argument is an optional single argument string.
pub fn set_exec_handler(handler: fn(&str, Option<&str>) -> Result<(), ErrorCode>) {
    EXEC_HANDLER.call_once(|| handler);
}

/// Set the open handler for syscall open
pub fn set_open_handler(handler: fn(&str) -> SyscallResult) {
    OPEN_HANDLER.call_once(|| handler);
}

/// Set the read handler for syscall read on file descriptors
pub fn set_read_handler(handler: fn(i32, u64, u64) -> SyscallResult) {
    READ_HANDLER.call_once(|| handler);
}

/// Set the write handler for syscall write on file descriptors
pub fn set_write_handler(handler: fn(i32, u64, u64) -> SyscallResult) {
    WRITE_HANDLER.call_once(|| handler);
}

/// Set the close handler for syscall close
pub fn set_close_handler(handler: fn(i32) -> SyscallResult) {
    CLOSE_HANDLER.call_once(|| handler);
}

/// Set the getpid handler for syscall getpid
pub fn set_getpid_handler(handler: fn() -> SyscallResult) {
    GETPID_HANDLER.call_once(|| handler);
}

/// Set the fork handler for syscall fork
pub fn set_fork_handler(handler: fn() -> SyscallResult) {
    FORK_HANDLER.call_once(|| handler);
}

/// Set the waitpid handler for syscall waitpid
pub fn set_waitpid_handler(handler: fn(i64, u64, i32) -> SyscallResult) {
    WAITPID_HANDLER.call_once(|| handler);
}

/// Set the pipe handler for syscall pipe
pub fn set_pipe_handler(handler: fn(u64) -> SyscallResult) {
    PIPE_HANDLER.call_once(|| handler);
}

/// Set the dup2 handler for syscall dup2
pub fn set_dup2_handler(handler: fn(i32, i32) -> SyscallResult) {
    DUP2_HANDLER.call_once(|| handler);
}

/// Set the kill handler for syscall kill
pub fn set_kill_handler(handler: fn(i32, i32) -> SyscallResult) {
    KILL_HANDLER.call_once(|| handler);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_number_conversion() {
        assert_eq!(SyscallNumber::from_u64(0), Some(SyscallNumber::Read));
        assert_eq!(SyscallNumber::from_u64(1), Some(SyscallNumber::Write));
        assert_eq!(SyscallNumber::from_u64(60), Some(SyscallNumber::Exit));
        assert_eq!(SyscallNumber::from_u64(999), None);
    }

    #[test]
    fn test_syscall_names() {
        assert_eq!(SyscallNumber::Read.name(), "read");
        assert_eq!(SyscallNumber::Write.name(), "write");
        assert_eq!(SyscallNumber::Exit.name(), "exit");
    }

    #[test]
    fn test_error_code_conversion() {
        assert_eq!(ErrorCode::EPERM.to_syscall_result(), -1);
        assert_eq!(ErrorCode::ENOENT.to_syscall_result(), -2);
        assert_eq!(ErrorCode::ENOMEM.to_syscall_result(), -12);
    }

    #[test]
    fn test_handle_unknown_syscall() {
        let result = handle_syscall(999, 0, 0, 0, 0, 0, 0);
        assert_eq!(result, ErrorCode::ENOSYS.to_syscall_result());
    }

    #[test]
    fn test_handle_getpid() {
        let result = handle_syscall(39, 0, 0, 0, 0, 0, 0);
        assert_eq!(result, 1); // Fake PID
    }

    #[test]
    fn test_sys_read_invalid_fd() {
        assert_eq!(sys_read(1, 0x1000, 1), Err(ErrorCode::EBADF));
    }

    #[test]
    fn test_sys_read_null_buf() {
        assert_eq!(sys_read(0, 0, 1), Err(ErrorCode::EFAULT));
    }

    #[test]
    fn test_sys_read_zero_count() {
        assert_eq!(sys_read(0, 0x1000, 0), Ok(0));
    }
}
