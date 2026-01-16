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
use alloc::vec::Vec;
#[cfg(any(
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
    feature = "elf-exec-smoke",
    feature = "tty-smoke"
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
    /// Seek file position
    Lseek = 8,
    /// Memory map
    Mmap = 9,
    /// Memory protect
    Mprotect = 10,
    /// Memory unmap
    Munmap = 11,
    /// Change program break (heap management)
    Brk = 12,
    /// Create pipe
    Pipe = 22,
    /// Yield CPU (sched_yield)
    Yield = 24,
    /// Duplicate file descriptor
    Dup = 32,
    /// Duplicate file descriptor (with target)
    Dup2 = 33,
    /// Send signal
    Kill = 37,
    /// Get process ID
    Getpid = 39,
    /// Fork process
    Fork = 57,
    /// Execute program
    Execve = 59,
    /// Exit process
    Exit = 60,
    /// Wait for process
    Wait4 = 61,
    /// Get environment variable (custom syscall, not in Linux ABI)
    /// Uses 63 as it's unused in standard Linux x86_64 ABI
    Getenv = 63,
    /// Get current directory
    Getcwd = 79,
    /// Change directory
    Chdir = 80,
    /// Rename file
    Rename = 82,
    /// Create directory
    Mkdir = 83,
    /// Remove directory
    Rmdir = 84,
    /// Unlink (delete) file
    Unlink = 87,
    /// Change file mode (chmod)
    Chmod = 90,
    /// Change file ownership (chown)
    Chown = 92,
    /// Get real user ID
    Getuid = 102,
    /// Get real group ID
    Getgid = 104,
    /// Set user ID
    Setuid = 105,
    /// Set group ID
    Setgid = 106,
    /// Set process group ID
    Setpgid = 109,
    /// Get directory entries
    Getdents64 = 217,
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
            8 => Some(Self::Lseek),
            9 => Some(Self::Mmap),
            10 => Some(Self::Mprotect),
            11 => Some(Self::Munmap),
            12 => Some(Self::Brk),
            22 => Some(Self::Pipe),
            24 => Some(Self::Yield),
            32 => Some(Self::Dup),
            33 => Some(Self::Dup2),
            37 => Some(Self::Kill),
            39 => Some(Self::Getpid),
            57 => Some(Self::Fork),
            59 => Some(Self::Execve),
            60 => Some(Self::Exit),
            61 => Some(Self::Wait4),
            79 => Some(Self::Getcwd),
            80 => Some(Self::Chdir),
            82 => Some(Self::Rename),
            83 => Some(Self::Mkdir),
            84 => Some(Self::Rmdir),
            87 => Some(Self::Unlink),
            90 => Some(Self::Chmod),
            92 => Some(Self::Chown),
            102 => Some(Self::Getuid),
            104 => Some(Self::Getgid),
            105 => Some(Self::Setuid),
            106 => Some(Self::Setgid),
            109 => Some(Self::Setpgid),
            217 => Some(Self::Getdents64),
            63 => Some(Self::Getenv),
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
            Self::Lseek => "lseek",
            Self::Exit => "exit",
            Self::Fork => "fork",
            Self::Execve => "execve",
            Self::Wait4 => "wait4",
            Self::Brk => "brk",
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
            Self::Setpgid => "setpgid",
            Self::Rename => "rename",
            Self::Mkdir => "mkdir",
            Self::Rmdir => "rmdir",
            Self::Unlink => "unlink",
            Self::Yield => "yield",
            Self::Getdents64 => "getdents64",
            Self::Getenv => "getenv",
            Self::Chmod => "chmod",
            Self::Chown => "chown",
            Self::Getuid => "getuid",
            Self::Getgid => "getgid",
            Self::Setuid => "setuid",
            Self::Setgid => "setgid",
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
    /// Read-only filesystem
    EROFS = 30,
    /// Exec format error
    ENOEXEC = 8,
    /// Argument list too long
    E2BIG = 7,
    /// Broken pipe
    EPIPE = 32,
    /// Result too large
    ERANGE = 34,
    /// Function not implemented
    ENOSYS = 38,
    /// Directory not empty
    ENOTEMPTY = 39,
    /// Illegal seek (e.g., on a pipe)
    ESPIPE = 29,
    /// Cross-device link
    EXDEV = 18,
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
        SyscallNumber::Stat => sys_stat(arg1, arg2),
        SyscallNumber::Fstat => sys_fstat(arg1 as i32, arg2),
        SyscallNumber::Lseek => sys_lseek(arg1 as i32, arg2 as i64, arg3 as i32),
        SyscallNumber::Brk => sys_brk(arg1),
        SyscallNumber::Mmap => sys_mmap(arg1, arg2, arg3 as i32, _arg4 as i32, _arg5 as i32, _arg6),
        SyscallNumber::Getpid => sys_getpid(),
        SyscallNumber::Yield => sys_yield(),
        SyscallNumber::Execve => sys_execve(arg1, arg2, arg3),
        SyscallNumber::Fork => sys_fork(),
        SyscallNumber::Wait4 => sys_waitpid(arg1 as i64, arg2, arg3 as i32),
        SyscallNumber::Pipe => sys_pipe(arg1),
        SyscallNumber::Dup2 => sys_dup2(arg1 as i32, arg2 as i32),
        SyscallNumber::Kill => sys_kill(arg1 as i32, arg2 as i32),
        SyscallNumber::Setpgid => sys_setpgid(arg1 as i32, arg2 as i32),
        SyscallNumber::Getdents64 => sys_getdents64(arg1 as i32, arg2, arg3),
        SyscallNumber::Getcwd => sys_getcwd(arg1, arg2),
        SyscallNumber::Chdir => sys_chdir(arg1),
        SyscallNumber::Rename => sys_rename(arg1, arg2),
        SyscallNumber::Mkdir => sys_mkdir(arg1, arg2 as u16),
        SyscallNumber::Rmdir => sys_rmdir(arg1),
        SyscallNumber::Unlink => sys_unlink(arg1),
        SyscallNumber::Getenv => sys_getenv(arg1, arg2, arg3),
        SyscallNumber::Chmod => sys_chmod(arg1, arg2 as u16),
        SyscallNumber::Chown => sys_chown(arg1, arg2 as u32, arg3 as u32),
        SyscallNumber::Getuid => sys_getuid(),
        SyscallNumber::Getgid => sys_getgid(),
        SyscallNumber::Setuid => sys_setuid(arg1 as u32),
        SyscallNumber::Setgid => sys_setgid(arg1 as u32),
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

#[cfg(feature = "ctrlc-smoke")]
const SCRIPTED_INPUT: &[u8] = b"echo test\x03\nhelp\nexit\n";

#[cfg(feature = "ls-smoke")]
const SCRIPTED_INPUT: &[u8] = b"ls\nexit\n";

#[cfg(feature = "ls-stat-smoke")]
const SCRIPTED_INPUT: &[u8] = b"ls\nexit\n";

#[cfg(feature = "ls-long-smoke")]
const SCRIPTED_INPUT: &[u8] = b"ls -l\ncd etc\nls -l\nexit\n";

#[cfg(feature = "cd-smoke")]
const SCRIPTED_INPUT: &[u8] = b"ls\ncd bin\nls\ncd ..\nls\nexit\n";

#[cfg(feature = "path-smoke")]
const SCRIPTED_INPUT: &[u8] = b"ls\ncat /etc/version\ncd bin\nls\nexit\n";

#[cfg(feature = "redir-smoke")]
const SCRIPTED_INPUT: &[u8] = b"echo hello > /tmp/x\ncat < /tmp/x\nls /tmp\nexit\n";

#[cfg(feature = "elf-exec-smoke")]
const SCRIPTED_INPUT: &[u8] = b"/mnt/bin/ls\n/mnt/bin/cat /mnt/version\nexit\n";

#[cfg(feature = "tty-smoke")]
const SCRIPTED_INPUT: &[u8] = b"echo hello\n\x03ls\nexit\n";

#[cfg(all(
    feature = "shell-smoke",
    any(
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
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "vfs-cat-smoke",
    any(
        feature = "fork-exec-smoke",
        feature = "pipe-smoke",
        feature = "ctrlc-smoke",
        feature = "ls-smoke",
        feature = "ls-stat-smoke",
        feature = "ls-long-smoke",
        feature = "cd-smoke",
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "fork-exec-smoke",
    any(
        feature = "pipe-smoke",
        feature = "ctrlc-smoke",
        feature = "ls-smoke",
        feature = "ls-stat-smoke",
        feature = "ls-long-smoke",
        feature = "cd-smoke",
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "pipe-smoke",
    any(
        feature = "ctrlc-smoke",
        feature = "ls-smoke",
        feature = "ls-stat-smoke",
        feature = "ls-long-smoke",
        feature = "cd-smoke",
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "ctrlc-smoke",
    any(
        feature = "ls-smoke",
        feature = "ls-stat-smoke",
        feature = "ls-long-smoke",
        feature = "cd-smoke",
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "ls-smoke",
    any(
        feature = "ls-stat-smoke",
        feature = "ls-long-smoke",
        feature = "cd-smoke",
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "ls-stat-smoke",
    feature = "ls-long-smoke",
    any(
        feature = "cd-smoke",
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "cd-smoke",
    any(
        feature = "path-smoke",
        feature = "redir-smoke",
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    )
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(
    feature = "path-smoke",
    any(feature = "redir-smoke", feature = "elf-exec-smoke", feature = "tty-smoke")
))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(all(feature = "redir-smoke", any(feature = "elf-exec-smoke", feature = "tty-smoke")))]
compile_error!(
    "shell-smoke, vfs-cat-smoke, fork-exec-smoke, pipe-smoke, ctrlc-smoke, ls-smoke, ls-stat-smoke, cd-smoke, path-smoke, redir-smoke, elf-exec-smoke, and tty-smoke are mutually exclusive"
);

#[cfg(any(
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
    feature = "elf-exec-smoke",
    feature = "tty-smoke"
))]
static SCRIPTED_POS: AtomicUsize = AtomicUsize::new(0);

fn read_byte() -> Option<u8> {
    #[cfg(any(
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
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
    ))]
    {
        let pos = SCRIPTED_POS.fetch_add(1, Ordering::Relaxed);
        return SCRIPTED_INPUT.get(pos).copied();
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
        feature = "elf-exec-smoke",
        feature = "tty-smoke"
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
                    // Not a pipe, fall through to TTY input
                }
                Err(e) => return Err(e),
            }
        }

        // Default stdin behavior: read from TTY
        let count = usize::try_from(count).map_err(|_| ErrorCode::EINVAL)?;
        if count > 4096 {
            return Err(ErrorCode::EINVAL);
        }

        // Allocate kernel buffer for TTY read
        let mut kernel_buf = [0u8; 4096];
        let buf_slice = &mut kernel_buf[..count];

        // Block until TTY has data, processing serial input
        loop {
            // Try to read from TTY
            if let Some(n) = crate::tty::tty_read(buf_slice) {
                // Copy to user space
                crate::usermode::copy_to_user_bytes(buf, &buf_slice[..n])?;
                return Ok(n as u64);
            }

            // No data available, poll serial and feed to TTY
            if let Some(byte) = read_byte() {
                let action = crate::tty::tty_input_byte(byte);

                // Handle TTY actions
                match action {
                    crate::tty::TtyAction::SendSignal => {
                        // Ctrl+C pressed - send SIGINT to foreground process group
                        if let Some(signal_fn) = SIGNAL_HANDLER.get() {
                            signal_fn();
                        }
                        // Continue waiting for input (shell will get SIGINT)
                    }
                    crate::tty::TtyAction::LineReady => {
                        // Line is ready, loop will read it on next iteration
                    }
                    crate::tty::TtyAction::None => {
                        // Continue waiting
                    }
                }
            } else {
                // No serial data, yield CPU
                core::hint::spin_loop();
            }
        }
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

/// sys_open - Open a file path with flags
fn sys_open(path_ptr: u64, flags: u64, _mode: u64) -> SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    if let Some(open_fn) = OPEN_HANDLER.get() {
        open_fn(path, flags)
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

/// sys_stat - Get file metadata by path
fn sys_stat(path_ptr: u64, stat_buf: u64) -> SyscallResult {
    if let Some(stat_fn) = STAT_HANDLER.get() {
        stat_fn(path_ptr, stat_buf)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_fstat - Get file metadata by file descriptor
fn sys_fstat(fd: i32, stat_buf: u64) -> SyscallResult {
    if let Some(fstat_fn) = FSTAT_HANDLER.get() {
        fstat_fn(fd, stat_buf)
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

/// sys_execve - Replace the current process image with a new program
///
/// Linux-compatible execve syscall:
/// - arg1: path to executable
/// - arg2: argv array (NULL-terminated array of string pointers)
/// - arg3: envp array (NULL-terminated array of string pointers)
///
/// For backward compatibility, if argv is 0 or points to a single string,
/// the old simplified interface is used.
fn sys_execve(path_ptr: u64, argv_ptr: u64, envp_ptr: u64) -> SyscallResult {
    const MAX_PATH_LEN: usize = 256;
    let mut path_buf = [0u8; MAX_PATH_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // Check if we're using the new argv/envp interface or old single-arg interface
    if argv_ptr == 0 {
        // Old interface: no arguments
        if let Some(exec_fn) = EXECVE_HANDLER.get() {
            match exec_fn(path, &[], &[]) {
                Ok(()) => Err(ErrorCode::EIO), // exec never returns on success
                Err(err) => Err(err),
            }
        } else {
            Err(ErrorCode::ENOSYS)
        }
    } else {
        // New interface: parse argv and envp arrays
        // SAFETY: User provides argv_ptr and envp_ptr, we validate in parse functions
        let argv = unsafe { crate::exec_stack::parse_argv(argv_ptr)? };
        let envp = unsafe { crate::exec_stack::parse_envp(envp_ptr)? };

        if let Some(exec_fn) = EXECVE_HANDLER.get() {
            match exec_fn(path, &argv, &envp) {
                Ok(()) => Err(ErrorCode::EIO), // exec never returns on success
                Err(err) => Err(err),
            }
        } else {
            Err(ErrorCode::ENOSYS)
        }
    }
}

/// sys_exec - DEPRECATED: Old simplified exec interface
///
/// This is kept for backward compatibility with existing code.
/// Use sys_execve instead.
#[allow(dead_code)]
fn sys_exec(path_ptr: u64, arg_ptr: u64) -> SyscallResult {
    const MAX_PATH_LEN: usize = 64;
    const MAX_ARG_LEN: usize = 128;
    let mut path_buf = [0u8; MAX_PATH_LEN];
    let mut arg_buf = [0u8; MAX_ARG_LEN];

    let path = crate::usermode::copy_user_cstr(path_ptr, &mut path_buf)?;

    // Convert old-style arg to new argv format
    let argv: Vec<Vec<u8>> = if arg_ptr == 0 {
        Vec::new()
    } else {
        let arg_str = crate::usermode::copy_user_cstr(arg_ptr, &mut arg_buf)?;
        alloc::vec![arg_str.as_bytes().to_vec()]
    };

    if let Some(exec_fn) = EXECVE_HANDLER.get() {
        match exec_fn(path, &argv, &[]) {
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

/// sys_setpgid - Set process group ID
fn sys_setpgid(pid: i32, pgid: i32) -> SyscallResult {
    if let Some(setpgid_fn) = SETPGID_HANDLER.get() {
        setpgid_fn(pid, pgid)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_getdents64 - Get directory entries
fn sys_getdents64(fd: i32, buf: u64, count: u64) -> SyscallResult {
    if let Some(getdents_fn) = GETDENTS64_HANDLER.get() {
        getdents_fn(fd, buf, count)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_getcwd - Get current working directory
fn sys_getcwd(buf: u64, size: u64) -> SyscallResult {
    if let Some(getcwd_fn) = GETCWD_HANDLER.get() {
        getcwd_fn(buf, size)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_chdir - Change current working directory
fn sys_chdir(path: u64) -> SyscallResult {
    if let Some(chdir_fn) = CHDIR_HANDLER.get() {
        chdir_fn(path)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_unlink - Delete a file or empty directory
fn sys_unlink(path: u64) -> SyscallResult {
    if let Some(unlink_fn) = UNLINK_HANDLER.get() {
        unlink_fn(path)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_getenv - Get environment variable value
fn sys_getenv(name_ptr: u64, buf_ptr: u64, size: u64) -> SyscallResult {
    if let Some(getenv_fn) = GETENV_HANDLER.get() {
        getenv_fn(name_ptr, buf_ptr, size)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_chmod - Change file mode
fn sys_chmod(path_ptr: u64, mode: u16) -> SyscallResult {
    if let Some(chmod_fn) = CHMOD_HANDLER.get() {
        chmod_fn(path_ptr, mode)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_chown - Change file ownership
///
/// Linux-compatible chown syscall:
/// - path_ptr: pointer to file path string in user space
/// - uid: new owner user ID (u32::MAX or -1 to leave unchanged)
/// - gid: new owner group ID (u32::MAX or -1 to leave unchanged)
///
/// Returns 0 on success, or -errno on failure.
fn sys_chown(path_ptr: u64, uid: u32, gid: u32) -> SyscallResult {
    if let Some(chown_fn) = CHOWN_HANDLER.get() {
        chown_fn(path_ptr, uid, gid)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_getuid - Get real user ID
///
/// Returns the real user ID of the calling process.
fn sys_getuid() -> SyscallResult {
    if let Some(getuid_fn) = GETUID_HANDLER.get() {
        getuid_fn()
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_getgid - Get real group ID
///
/// Returns the real group ID of the calling process.
fn sys_getgid() -> SyscallResult {
    if let Some(getgid_fn) = GETGID_HANDLER.get() {
        getgid_fn()
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_setuid - Set user ID
///
/// Linux-compatible setuid syscall:
/// - uid: new user ID
///
/// Returns 0 on success, or -EPERM if not privileged.
fn sys_setuid(uid: u32) -> SyscallResult {
    if let Some(setuid_fn) = SETUID_HANDLER.get() {
        setuid_fn(uid)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_setgid - Set group ID
///
/// Linux-compatible setgid syscall:
/// - gid: new group ID
///
/// Returns 0 on success, or -EPERM if not privileged.
fn sys_setgid(gid: u32) -> SyscallResult {
    if let Some(setgid_fn) = SETGID_HANDLER.get() {
        setgid_fn(gid)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_lseek - Reposition read/write file offset
///
/// Linux-compatible lseek syscall:
/// - fd: file descriptor
/// - offset: new offset (relative or absolute)
/// - whence: SEEK_SET (0), SEEK_CUR (1), or SEEK_END (2)
///
/// Returns new offset on success, or -errno on error.
fn sys_lseek(fd: i32, offset: i64, whence: i32) -> SyscallResult {
    if let Some(lseek_fn) = LSEEK_HANDLER.get() {
        lseek_fn(fd, offset, whence)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_mkdir - Create a directory
///
/// Linux-compatible mkdir syscall:
/// - path_ptr: pointer to path string in user space
/// - mode: permission mode (ignored for now)
///
/// Returns 0 on success, or -errno on error.
fn sys_mkdir(path_ptr: u64, mode: u16) -> SyscallResult {
    if let Some(mkdir_fn) = MKDIR_HANDLER.get() {
        mkdir_fn(path_ptr, mode)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_rmdir - Remove an empty directory
///
/// Linux-compatible rmdir syscall:
/// - path_ptr: pointer to path string in user space
///
/// Returns 0 on success, or -errno on error.
fn sys_rmdir(path_ptr: u64) -> SyscallResult {
    if let Some(rmdir_fn) = RMDIR_HANDLER.get() {
        rmdir_fn(path_ptr)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_rename - Rename/move a file or directory
///
/// Linux-compatible rename syscall:
/// - oldpath_ptr: pointer to old path string in user space
/// - newpath_ptr: pointer to new path string in user space
///
/// Returns 0 on success, or -errno on error.
fn sys_rename(oldpath_ptr: u64, newpath_ptr: u64) -> SyscallResult {
    if let Some(rename_fn) = RENAME_HANDLER.get() {
        rename_fn(oldpath_ptr, newpath_ptr)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_brk - Change the program break (heap management)
///
/// Linux-compatible brk syscall:
/// - arg: new program break address (0 to query current break)
///
/// Returns current or new program break on success, or -ENOMEM on failure.
fn sys_brk(addr: u64) -> SyscallResult {
    if let Some(brk_fn) = BRK_HANDLER.get() {
        brk_fn(addr)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// sys_mmap - Map memory
///
/// Linux-compatible mmap syscall (minimal implementation):
/// - addr: requested address (0 = kernel chooses)
/// - length: size in bytes
/// - prot: protection flags (PROT_READ|PROT_WRITE|PROT_EXEC)
/// - flags: mapping flags (MAP_PRIVATE|MAP_ANONYMOUS only)
/// - fd: file descriptor (-1 for anonymous)
/// - offset: file offset (ignored for anonymous)
///
/// Returns mapped address on success, or -errno on failure.
#[allow(clippy::too_many_arguments)]
fn sys_mmap(addr: u64, length: u64, prot: i32, flags: i32, fd: i32, offset: u64) -> SyscallResult {
    if let Some(mmap_fn) = MMAP_HANDLER.get() {
        mmap_fn(addr, length, prot, flags, fd, offset)
    } else {
        Err(ErrorCode::ENOSYS)
    }
}

/// Yield handler function pointer for scheduler integration
static YIELD_HANDLER: Once<fn()> = Once::new();
static EXECVE_HANDLER: Once<fn(&str, &[Vec<u8>], &[Vec<u8>]) -> Result<(), ErrorCode>> =
    Once::new();
static OPEN_HANDLER: Once<fn(&str, u64) -> SyscallResult> = Once::new();
static READ_HANDLER: Once<fn(i32, u64, u64) -> SyscallResult> = Once::new();
static WRITE_HANDLER: Once<fn(i32, u64, u64) -> SyscallResult> = Once::new();
static CLOSE_HANDLER: Once<fn(i32) -> SyscallResult> = Once::new();
static GETPID_HANDLER: Once<fn() -> SyscallResult> = Once::new();
static FORK_HANDLER: Once<fn() -> SyscallResult> = Once::new();
static WAITPID_HANDLER: Once<fn(i64, u64, i32) -> SyscallResult> = Once::new();
static PIPE_HANDLER: Once<fn(u64) -> SyscallResult> = Once::new();
static DUP2_HANDLER: Once<fn(i32, i32) -> SyscallResult> = Once::new();
static KILL_HANDLER: Once<fn(i32, i32) -> SyscallResult> = Once::new();
static SIGNAL_HANDLER: Once<fn()> = Once::new();
static SETPGID_HANDLER: Once<fn(i32, i32) -> SyscallResult> = Once::new();
static GETDENTS64_HANDLER: Once<fn(i32, u64, u64) -> SyscallResult> = Once::new();
static GETCWD_HANDLER: Once<fn(u64, u64) -> SyscallResult> = Once::new();
static CHDIR_HANDLER: Once<fn(u64) -> SyscallResult> = Once::new();
static UNLINK_HANDLER: Once<fn(u64) -> SyscallResult> = Once::new();
static GETENV_HANDLER: Once<fn(u64, u64, u64) -> SyscallResult> = Once::new();
static STAT_HANDLER: Once<fn(u64, u64) -> SyscallResult> = Once::new();
static FSTAT_HANDLER: Once<fn(i32, u64) -> SyscallResult> = Once::new();
static CHMOD_HANDLER: Once<fn(u64, u16) -> SyscallResult> = Once::new();
static CHOWN_HANDLER: Once<fn(u64, u32, u32) -> SyscallResult> = Once::new();
static GETUID_HANDLER: Once<fn() -> SyscallResult> = Once::new();
static GETGID_HANDLER: Once<fn() -> SyscallResult> = Once::new();
static SETUID_HANDLER: Once<fn(u32) -> SyscallResult> = Once::new();
static SETGID_HANDLER: Once<fn(u32) -> SyscallResult> = Once::new();
static BRK_HANDLER: Once<fn(u64) -> SyscallResult> = Once::new();
static MMAP_HANDLER: Once<fn(u64, u64, i32, i32, i32, u64) -> SyscallResult> = Once::new();
static LSEEK_HANDLER: Once<fn(i32, i64, i32) -> SyscallResult> = Once::new();
static MKDIR_HANDLER: Once<fn(u64, u16) -> SyscallResult> = Once::new();
static RMDIR_HANDLER: Once<fn(u64) -> SyscallResult> = Once::new();
static RENAME_HANDLER: Once<fn(u64, u64) -> SyscallResult> = Once::new();

/// Set the yield handler for syscall yield
///
/// Must be called before any user processes run.
pub fn set_yield_handler(handler: fn()) {
    YIELD_HANDLER.call_once(|| handler);
}

/// Set the execve handler for syscall execve
///
/// Must be called before any user processes run.
/// Handler receives path, argv array, and envp array.
pub fn set_execve_handler(handler: fn(&str, &[Vec<u8>], &[Vec<u8>]) -> Result<(), ErrorCode>) {
    EXECVE_HANDLER.call_once(|| handler);
}

/// Set the open handler for syscall open
pub fn set_open_handler(handler: fn(&str, u64) -> SyscallResult) {
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

/// Set the setpgid handler for syscall setpgid
pub fn set_setpgid_handler(handler: fn(i32, i32) -> SyscallResult) {
    SETPGID_HANDLER.call_once(|| handler);
}

/// Set the getdents64 handler for syscall getdents64
pub fn set_getdents64_handler(handler: fn(i32, u64, u64) -> SyscallResult) {
    GETDENTS64_HANDLER.call_once(|| handler);
}

/// Set the getcwd handler for syscall getcwd
pub fn set_getcwd_handler(handler: fn(u64, u64) -> SyscallResult) {
    GETCWD_HANDLER.call_once(|| handler);
}

/// Set the chdir handler for syscall chdir
pub fn set_chdir_handler(handler: fn(u64) -> SyscallResult) {
    CHDIR_HANDLER.call_once(|| handler);
}

/// Set the unlink handler for syscall unlink
pub fn set_unlink_handler(handler: fn(u64) -> SyscallResult) {
    UNLINK_HANDLER.call_once(|| handler);
}

/// Set the getenv handler for syscall getenv
pub fn set_getenv_handler(handler: fn(u64, u64, u64) -> SyscallResult) {
    GETENV_HANDLER.call_once(|| handler);
}

/// Set the stat handler for syscall stat
pub fn set_stat_handler(handler: fn(u64, u64) -> SyscallResult) {
    STAT_HANDLER.call_once(|| handler);
}

/// Set the fstat handler for syscall fstat
pub fn set_fstat_handler(handler: fn(i32, u64) -> SyscallResult) {
    FSTAT_HANDLER.call_once(|| handler);
}

/// Set the chmod handler for syscall chmod
pub fn set_chmod_handler(handler: fn(u64, u16) -> SyscallResult) {
    CHMOD_HANDLER.call_once(|| handler);
}

/// Set the chown handler for syscall chown
pub fn set_chown_handler(handler: fn(u64, u32, u32) -> SyscallResult) {
    CHOWN_HANDLER.call_once(|| handler);
}

/// Set the getuid handler for syscall getuid
pub fn set_getuid_handler(handler: fn() -> SyscallResult) {
    GETUID_HANDLER.call_once(|| handler);
}

/// Set the getgid handler for syscall getgid
pub fn set_getgid_handler(handler: fn() -> SyscallResult) {
    GETGID_HANDLER.call_once(|| handler);
}

/// Set the setuid handler for syscall setuid
pub fn set_setuid_handler(handler: fn(u32) -> SyscallResult) {
    SETUID_HANDLER.call_once(|| handler);
}

/// Set the setgid handler for syscall setgid
pub fn set_setgid_handler(handler: fn(u32) -> SyscallResult) {
    SETGID_HANDLER.call_once(|| handler);
}

/// Set the brk handler for syscall brk
pub fn set_brk_handler(handler: fn(u64) -> SyscallResult) {
    BRK_HANDLER.call_once(|| handler);
}

/// Set the mmap handler for syscall mmap
pub fn set_mmap_handler(handler: fn(u64, u64, i32, i32, i32, u64) -> SyscallResult) {
    MMAP_HANDLER.call_once(|| handler);
}

/// Set the signal handler for TTY Ctrl+C
///
/// This handler is called when Ctrl+C is pressed in the TTY.
/// It should send SIGINT to the foreground process group.
pub fn set_signal_handler(handler: fn()) {
    SIGNAL_HANDLER.call_once(|| handler);
}

/// Set the lseek handler for syscall lseek
pub fn set_lseek_handler(handler: fn(i32, i64, i32) -> SyscallResult) {
    LSEEK_HANDLER.call_once(|| handler);
}

/// Set the mkdir handler for syscall mkdir
pub fn set_mkdir_handler(handler: fn(u64, u16) -> SyscallResult) {
    MKDIR_HANDLER.call_once(|| handler);
}

/// Set the rmdir handler for syscall rmdir
pub fn set_rmdir_handler(handler: fn(u64) -> SyscallResult) {
    RMDIR_HANDLER.call_once(|| handler);
}

/// Set the rename handler for syscall rename
pub fn set_rename_handler(handler: fn(u64, u64) -> SyscallResult) {
    RENAME_HANDLER.call_once(|| handler);
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
