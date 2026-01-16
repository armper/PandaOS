//! Linux-compatible user stack setup for execve
//!
//! This module provides functions to set up the initial user stack layout
//! that matches Linux's conventions for argv, envp, and auxv.

use crate::syscall::ErrorCode;
use alloc::vec::Vec;

/// Maximum number of arguments
pub const MAX_ARGC: usize = 128;

/// Maximum number of environment variables
pub const MAX_ENVC: usize = 128;

/// Maximum total size of argv + envp strings (1MB)
pub const MAX_ARG_STRLEN: usize = 1024 * 1024;

/// Auxiliary vector entry types
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum AuxvType {
    /// End of vector
    Null = 0,
    /// Entry point of program
    Entry = 9,
    /// Program headers pointer
    Phdr = 3,
    /// Number of program headers
    Phnum = 5,
    /// Page size
    Pagesz = 6,
}

/// Parse argv array from user space
///
/// # Safety
///
/// Caller must ensure argv_ptr points to valid user memory
pub unsafe fn parse_argv(argv_ptr: u64) -> Result<Vec<Vec<u8>>, ErrorCode> {
    if argv_ptr == 0 {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut i = 0;

    loop {
        if i >= MAX_ARGC {
            return Err(ErrorCode::E2BIG);
        }

        // Read pointer at argv[i]
        // SAFETY: Caller guarantees argv_ptr is valid user memory
        let arg_ptr = unsafe { *((argv_ptr + i as u64 * 8) as *const u64) };

        if arg_ptr == 0 {
            break;
        }

        // Read string at arg_ptr
        let arg = unsafe { read_user_string(arg_ptr, MAX_ARG_STRLEN)? };
        args.push(arg);
        i += 1;
    }

    Ok(args)
}

/// Parse envp array from user space
///
/// # Safety
///
/// Caller must ensure envp_ptr points to valid user memory
pub unsafe fn parse_envp(envp_ptr: u64) -> Result<Vec<Vec<u8>>, ErrorCode> {
    if envp_ptr == 0 {
        return Ok(Vec::new());
    }

    let mut envs = Vec::new();
    let mut i = 0;

    loop {
        if i >= MAX_ENVC {
            return Err(ErrorCode::E2BIG);
        }

        // Read pointer at envp[i]
        // SAFETY: Caller guarantees envp_ptr is valid user memory
        let env_ptr = unsafe { *((envp_ptr + i as u64 * 8) as *const u64) };

        if env_ptr == 0 {
            break;
        }

        // Read string at env_ptr
        let env = unsafe { read_user_string(env_ptr, MAX_ARG_STRLEN)? };
        envs.push(env);
        i += 1;
    }

    Ok(envs)
}

/// Read a NUL-terminated string from user space
///
/// # Safety
///
/// Caller must ensure ptr points to valid user memory
unsafe fn read_user_string(ptr: u64, max_len: usize) -> Result<Vec<u8>, ErrorCode> {
    if ptr == 0 {
        return Err(ErrorCode::EFAULT);
    }

    let mut bytes = Vec::new();

    for i in 0..max_len {
        // SAFETY: Caller guarantees ptr is valid user memory
        let byte = unsafe { *((ptr + i as u64) as *const u8) };

        if byte == 0 {
            return Ok(bytes);
        }

        bytes.push(byte);
    }

    // String too long
    Err(ErrorCode::E2BIG)
}

/// Set up Linux-compatible user stack for execve
///
/// Stack layout (from high to low address):
/// - argv strings
/// - envp strings
/// - auxv entries (AT_NULL terminated)
/// - envp pointers (NULL terminated)
/// - argv pointers (NULL terminated)
/// - argc
///
/// # Safety
///
/// Caller must ensure page_table_phys is valid and stack memory is allocated
pub unsafe fn setup_user_stack(
    page_table_phys: u64,
    stack_top: u64,
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
    entry_point: u64,
) -> Result<u64, ErrorCode> {
    // Calculate total size needed
    let mut total_size = 0usize;

    // Space for argc
    total_size += 8;

    // Space for argv pointers + NULL
    total_size += (argv.len() + 1) * 8;

    // Space for envp pointers + NULL
    total_size += (envp.len() + 1) * 8;

    // Space for auxv entries (we'll add minimal ones)
    // AT_ENTRY, AT_PAGESZ, AT_NULL (3 * 16 bytes)
    total_size += 48;

    // Space for argv strings
    for arg in argv {
        total_size += arg.len() + 1; // +1 for NUL terminator
    }

    // Space for envp strings
    for env in envp {
        total_size += env.len() + 1; // +1 for NUL terminator
    }

    // Align to 16 bytes
    total_size = (total_size + 15) & !15;

    // Check if it fits in stack
    if total_size > 16384 {
        // 4 pages = 16KB
        return Err(ErrorCode::E2BIG);
    }

    // Start writing from top of stack, going down
    let mut sp = stack_top;

    // Write strings first (at high addresses)
    let mut argv_ptrs = Vec::new();
    for arg in argv.iter().rev() {
        sp -= arg.len() as u64 + 1;
        argv_ptrs.push(sp);
        // SAFETY: Caller ensures stack memory is allocated and page table is valid
        unsafe {
            write_user_memory(page_table_phys, sp, arg)?;
            write_user_memory(page_table_phys, sp + arg.len() as u64, &[0u8])?;
        }
    }
    argv_ptrs.reverse();

    let mut envp_ptrs = Vec::new();
    for env in envp.iter().rev() {
        sp -= env.len() as u64 + 1;
        envp_ptrs.push(sp);
        // SAFETY: Caller ensures stack memory is allocated and page table is valid
        unsafe {
            write_user_memory(page_table_phys, sp, env)?;
            write_user_memory(page_table_phys, sp + env.len() as u64, &[0u8])?;
        }
    }
    envp_ptrs.reverse();

    // Align sp to 8 bytes
    sp &= !7;

    // Write auxv
    sp -= 16;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Null as u64)?;
        write_user_u64(page_table_phys, sp + 8, 0)?;
    }

    sp -= 16;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Pagesz as u64)?;
        write_user_u64(page_table_phys, sp + 8, 4096)?;
    }

    sp -= 16;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Entry as u64)?;
        write_user_u64(page_table_phys, sp + 8, entry_point)?;
    }

    // Write envp pointers
    sp -= 8;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_u64(page_table_phys, sp, 0)?; // NULL terminator
    }

    for &env_ptr in envp_ptrs.iter().rev() {
        sp -= 8;
        // SAFETY: Caller ensures stack memory is allocated and page table is valid
        unsafe {
            write_user_u64(page_table_phys, sp, env_ptr)?;
        }
    }

    // Write argv pointers
    sp -= 8;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_u64(page_table_phys, sp, 0)?; // NULL terminator
    }

    for &arg_ptr in argv_ptrs.iter().rev() {
        sp -= 8;
        // SAFETY: Caller ensures stack memory is allocated and page table is valid
        unsafe {
            write_user_u64(page_table_phys, sp, arg_ptr)?;
        }
    }

    // Write argc
    sp -= 8;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_u64(page_table_phys, sp, argv.len() as u64)?;
    }

    // Align to 16 bytes (required by x86_64 ABI)
    sp &= !15;

    Ok(sp)
}

/// Write a u64 value to user memory
///
/// # Safety
///
/// Caller must ensure page_table_phys is valid and addr is mapped
unsafe fn write_user_u64(page_table_phys: u64, addr: u64, value: u64) -> Result<(), ErrorCode> {
    let bytes = value.to_le_bytes();
    // SAFETY: Caller guarantees page_table_phys is valid and addr is mapped
    unsafe { write_user_memory(page_table_phys, addr, &bytes) }
}

/// Write bytes to user memory
///
/// # Safety
///
/// Caller must ensure page_table_phys is valid and addr is mapped
unsafe fn write_user_memory(
    _page_table_phys: u64,
    addr: u64,
    data: &[u8],
) -> Result<(), ErrorCode> {
    // NOTE: This assumes identity mapping of physical memory
    // In a full implementation, this would use the page table to translate
    // virtual addresses to physical addresses

    if addr == 0 {
        return Err(ErrorCode::EFAULT);
    }

    // SAFETY: Caller guarantees addr is valid and mapped
    unsafe {
        let dst = core::slice::from_raw_parts_mut(addr as *mut u8, data.len());
        dst.copy_from_slice(data);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auxv_type_values() {
        assert_eq!(AuxvType::Null as u64, 0);
        assert_eq!(AuxvType::Entry as u64, 9);
        assert_eq!(AuxvType::Pagesz as u64, 6);
    }
}
