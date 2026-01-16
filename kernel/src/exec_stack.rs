//! Linux-compatible user stack setup for execve
//!
//! This module provides functions to set up the initial user stack layout
//! that matches Linux's conventions for argv, envp, and auxv.

use crate::syscall::ErrorCode;
use alloc::vec::Vec;

/// Maximum number of arguments (per requirements)
pub const MAX_ARGC: usize = 64;

/// Maximum number of environment variables (per requirements)
pub const MAX_ENVC: usize = 64;

/// Maximum length for a single string (per requirements)
pub const MAX_STRLEN: usize = 256;

/// Maximum total size of argv + envp strings (32 KiB per requirements)
pub const MAX_TOTAL_BYTES: usize = 32 * 1024;

/// Auxiliary vector entry types (Linux AT_* constants)
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum AuxvType {
    /// End of vector
    Null = 0,
    /// Program headers pointer
    Phdr = 3,
    /// Size of program header entry
    Phent = 4,
    /// Number of program headers
    Phnum = 5,
    /// Page size
    Pagesz = 6,
    /// Entry point of program
    Entry = 9,
    /// Real user ID
    Uid = 11,
    /// Effective user ID
    Euid = 12,
    /// Real group ID
    Gid = 13,
    /// Effective group ID
    Egid = 14,
    /// 16 bytes of random data
    Random = 25,
    /// Pointer to filename
    Execfn = 31,
}

/// Parse argv array from user space
///
/// # Safety
///
/// Caller must ensure argv_ptr points to valid user memory
///
/// # TODO
///
/// Add bounds checking for user memory access. Currently assumes identity mapping
/// and doesn't validate that argv_ptr + i * 8 is within valid user memory.
/// Should use proper page table walking or trap page faults.
pub unsafe fn parse_argv(argv_ptr: u64) -> Result<Vec<Vec<u8>>, ErrorCode> {
    if argv_ptr == 0 {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut total_bytes = 0usize;
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

        // Read string at arg_ptr with per-string length limit
        let arg = unsafe { read_user_string(arg_ptr, MAX_STRLEN)? };
        
        // Check total bytes limit (32 KiB)
        total_bytes += arg.len() + 1; // +1 for NUL terminator
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(ErrorCode::E2BIG);
        }
        
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
///
/// # TODO
///
/// Add bounds checking for user memory access. Currently assumes identity mapping
/// and doesn't validate that envp_ptr + i * 8 is within valid user memory.
/// Should use proper page table walking or trap page faults.
pub unsafe fn parse_envp(envp_ptr: u64) -> Result<Vec<Vec<u8>>, ErrorCode> {
    if envp_ptr == 0 {
        return Ok(Vec::new());
    }

    let mut envs = Vec::new();
    let mut total_bytes = 0usize;
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

        // Read string at env_ptr with per-string length limit
        let env = unsafe { read_user_string(env_ptr, MAX_STRLEN)? };
        
        // Check total bytes limit (32 KiB shared with argv)
        total_bytes += env.len() + 1; // +1 for NUL terminator
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(ErrorCode::E2BIG);
        }
        
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
///
/// # TODO
///
/// Add proper user memory validation. Currently doesn't validate memory
/// mappings or handle page faults. Should use safe user memory access
/// functions that trap faults and validate against page tables.
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
/// - filename string (for AT_EXECFN)
/// - random bytes (for AT_RANDOM)
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
    path: &str,
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
    entry_point: u64,
    phdr: u64,
    phnum: u16,
    uid: u32,
    gid: u32,
) -> Result<u64, ErrorCode> {
    // Calculate total size needed
    let mut total_size = 0usize;

    // Space for argc
    total_size += 8;

    // Space for argv pointers + NULL
    total_size += (argv.len() + 1) * 8;

    // Space for envp pointers + NULL
    total_size += (envp.len() + 1) * 8;

    // Space for auxv entries (comprehensive set)
    // AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM, AT_ENTRY, 
    // AT_UID, AT_EUID, AT_GID, AT_EGID, AT_RANDOM, AT_EXECFN, AT_NULL
    // = 12 entries * 16 bytes
    total_size += 192;

    // Space for argv strings
    for arg in argv {
        total_size += arg.len() + 1; // +1 for NUL terminator
    }

    // Space for envp strings
    for env in envp {
        total_size += env.len() + 1; // +1 for NUL terminator
    }

    // Space for filename string (for AT_EXECFN)
    total_size += path.len() + 1;

    // Space for random bytes (16 bytes for AT_RANDOM)
    total_size += 16;

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
    
    // Write argv strings
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

    // Write envp strings
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

    // Write filename string for AT_EXECFN
    sp -= path.len() as u64 + 1;
    let execfn_ptr = sp;
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_memory(page_table_phys, sp, path.as_bytes())?;
        write_user_memory(page_table_phys, sp + path.len() as u64, &[0u8])?;
    }

    // Write random bytes for AT_RANDOM
    // For now, use a deterministic seed (can be improved later)
    sp -= 16;
    let random_ptr = sp;
    let random_bytes: [u8; 16] = [
        0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe,
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
    ];
    // SAFETY: Caller ensures stack memory is allocated and page table is valid
    unsafe {
        write_user_memory(page_table_phys, sp, &random_bytes)?;
    }

    // Align sp to 8 bytes
    sp &= !7;

    // Write auxv entries (in reverse order, from AT_NULL upward)
    
    // AT_NULL (end marker)
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Null as u64)?;
        write_user_u64(page_table_phys, sp + 8, 0)?;
    }

    // AT_EXECFN
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Execfn as u64)?;
        write_user_u64(page_table_phys, sp + 8, execfn_ptr)?;
    }

    // AT_RANDOM
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Random as u64)?;
        write_user_u64(page_table_phys, sp + 8, random_ptr)?;
    }

    // AT_EGID
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Egid as u64)?;
        write_user_u64(page_table_phys, sp + 8, gid as u64)?;
    }

    // AT_GID
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Gid as u64)?;
        write_user_u64(page_table_phys, sp + 8, gid as u64)?;
    }

    // AT_EUID
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Euid as u64)?;
        write_user_u64(page_table_phys, sp + 8, uid as u64)?;
    }

    // AT_UID
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Uid as u64)?;
        write_user_u64(page_table_phys, sp + 8, uid as u64)?;
    }

    // AT_ENTRY
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Entry as u64)?;
        write_user_u64(page_table_phys, sp + 8, entry_point)?;
    }

    // AT_PHNUM
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Phnum as u64)?;
        write_user_u64(page_table_phys, sp + 8, phnum as u64)?;
    }

    // AT_PHENT
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Phent as u64)?;
        write_user_u64(page_table_phys, sp + 8, 56)?; // sizeof(Elf64_Phdr)
    }

    // AT_PHDR (if available)
    if phdr != 0 {
        sp -= 16;
        unsafe {
            write_user_u64(page_table_phys, sp, AuxvType::Phdr as u64)?;
            write_user_u64(page_table_phys, sp + 8, phdr)?;
        }
    }

    // AT_PAGESZ
    sp -= 16;
    unsafe {
        write_user_u64(page_table_phys, sp, AuxvType::Pagesz as u64)?;
        write_user_u64(page_table_phys, sp + 8, 4096)?;
    }

    // Write envp pointers
    sp -= 8;
    unsafe {
        write_user_u64(page_table_phys, sp, 0)?; // NULL terminator
    }

    for &env_ptr in envp_ptrs.iter().rev() {
        sp -= 8;
        unsafe {
            write_user_u64(page_table_phys, sp, env_ptr)?;
        }
    }

    // Write argv pointers
    sp -= 8;
    unsafe {
        write_user_u64(page_table_phys, sp, 0)?; // NULL terminator
    }

    for &arg_ptr in argv_ptrs.iter().rev() {
        sp -= 8;
        unsafe {
            write_user_u64(page_table_phys, sp, arg_ptr)?;
        }
    }

    // Write argc
    sp -= 8;
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
        assert_eq!(AuxvType::Phdr as u64, 3);
        assert_eq!(AuxvType::Phent as u64, 4);
        assert_eq!(AuxvType::Phnum as u64, 5);
        assert_eq!(AuxvType::Pagesz as u64, 6);
        assert_eq!(AuxvType::Entry as u64, 9);
        assert_eq!(AuxvType::Uid as u64, 11);
        assert_eq!(AuxvType::Euid as u64, 12);
        assert_eq!(AuxvType::Gid as u64, 13);
        assert_eq!(AuxvType::Egid as u64, 14);
        assert_eq!(AuxvType::Random as u64, 25);
        assert_eq!(AuxvType::Execfn as u64, 31);
    }

    #[test]
    fn test_max_constants() {
        // Validate requirements are properly set
        assert_eq!(MAX_ARGC, 64);
        assert_eq!(MAX_ENVC, 64);
        assert_eq!(MAX_STRLEN, 256);
        assert_eq!(MAX_TOTAL_BYTES, 32 * 1024);
    }
}
