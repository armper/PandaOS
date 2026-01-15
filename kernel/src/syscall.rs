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
//! - All syscalls preserve callee-saved registers

// Import macros for logging
use panda_hal::{serial_print, serial_println};

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
        SyscallNumber::Write => sys_write(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpid => sys_getpid(),
        // All other syscalls return ENOSYS for now
        _ => Err(ErrorCode::ENOSYS),
    };

    match result {
        Ok(val) => val as i64,
        Err(err) => err.to_syscall_result(),
    }
}

/// sys_exit - Exit the current process
fn sys_exit(status: i32) -> SyscallResult {
    serial_println!("Process exiting with status: {}", status);

    // TODO: Actually terminate the process
    // For now, just halt the system
    loop {
        x86_64::instructions::hlt();
    }
}

/// sys_write - Write to file descriptor
fn sys_write(fd: i32, buf: u64, count: u64) -> SyscallResult {
    // Only support stdout (fd 1) and stderr (fd 2) for now
    if fd != 1 && fd != 2 {
        return Err(ErrorCode::EBADF);
    }

    // Validate buffer address (basic check - should be more thorough)
    if buf == 0 || count == 0 {
        return Ok(0);
    }

    // Limit write size to prevent abuse
    if count > 4096 {
        return Err(ErrorCode::EINVAL);
    }

    // SAFETY: We perform basic validation of the buffer
    // In a real kernel, this would check user memory permissions
    let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, count as usize) };

    // Write to serial output
    for &byte in slice {
        // Only print printable ASCII and newlines
        if (0x20..=0x7e).contains(&byte) || byte == b'\n' || byte == b'\r' || byte == b'\t' {
            serial_print!("{}", byte as char);
        }
    }

    Ok(count)
}

/// sys_getpid - Get process ID
fn sys_getpid() -> SyscallResult {
    // TODO: Implement getpid
    // For now, return fake PID of 1
    Ok(1)
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
}
