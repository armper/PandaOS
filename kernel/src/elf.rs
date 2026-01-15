//! ELF64 loader for static binaries
//!
//! This module provides safe ELF parsing and loading functionality.
//! It validates headers defensively and maps segments with correct permissions.
//!
//! ## Invariants
//!
//! - ELF headers are validated before use
//! - Program headers are bounds-checked
//! - Only PT_LOAD segments are processed
//! - Memory is not allocated until validation succeeds

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class - 64-bit
const ELFCLASS64: u8 = 2;

/// ELF data encoding - little endian
const ELFDATA2LSB: u8 = 1;

/// ELF version
const EV_CURRENT: u8 = 1;

/// ELF type - executable
const ET_EXEC: u16 = 2;

/// ELF machine - x86-64
const EM_X86_64: u16 = 62;

/// Program header type - loadable segment
const PT_LOAD: u32 = 1;

/// Program header flags
const PF_X: u32 = 1; // Execute
const PF_W: u32 = 2; // Write
const PF_R: u32 = 4; // Read

/// ELF64 header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Ehdr {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

/// ELF64 program header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// ELF parsing errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// Invalid magic number
    InvalidMagic,
    /// Unsupported ELF class (not 64-bit)
    InvalidClass,
    /// Unsupported endianness (not little-endian)
    InvalidEndian,
    /// Unsupported version
    InvalidVersion,
    /// Not an executable
    NotExecutable,
    /// Wrong machine type (not x86-64)
    WrongMachine,
    /// File too small
    FileTooSmall,
    /// Invalid program header offset
    InvalidPhdrOffset,
    /// Invalid program header count
    InvalidPhdrCount,
    /// Invalid segment alignment
    InvalidAlignment,
    /// Segment size mismatch
    InvalidSize,
}

/// Parsed ELF information
#[derive(Debug)]
pub struct ElfInfo {
    pub entry_point: u64,
    pub load_segments: [Option<LoadSegment>; 8],
    pub segment_count: usize,
}

/// A loadable segment
#[derive(Debug, Clone, Copy)]
pub struct LoadSegment {
    pub vaddr: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub flags: u32,
}

impl LoadSegment {
    /// Check if segment is readable
    pub const fn is_readable(&self) -> bool {
        (self.flags & PF_R) != 0
    }

    /// Check if segment is writable
    pub const fn is_writable(&self) -> bool {
        (self.flags & PF_W) != 0
    }

    /// Check if segment is executable
    pub const fn is_executable(&self) -> bool {
        (self.flags & PF_X) != 0
    }
}

/// Parse an ELF64 executable
///
/// This function performs defensive validation of all headers before
/// returning parsed information.
pub fn parse_elf(data: &[u8]) -> Result<ElfInfo, ElfError> {
    // Validate minimum size
    if data.len() < core::mem::size_of::<Elf64Ehdr>() {
        return Err(ElfError::FileTooSmall);
    }

    // Parse ELF header
    // SAFETY: We verified the buffer is large enough for Elf64Ehdr
    let ehdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };

    // Validate magic number
    if ehdr.e_ident[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }

    // Validate class (64-bit)
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::InvalidClass);
    }

    // Validate endianness (little-endian)
    if ehdr.e_ident[5] != ELFDATA2LSB {
        return Err(ElfError::InvalidEndian);
    }

    // Validate version
    if ehdr.e_ident[6] != EV_CURRENT || ehdr.e_version != 1 {
        return Err(ElfError::InvalidVersion);
    }

    // Validate type (executable)
    if ehdr.e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }

    // Validate machine (x86-64)
    if ehdr.e_machine != EM_X86_64 {
        return Err(ElfError::WrongMachine);
    }

    // Validate program header info
    if ehdr.e_phentsize != core::mem::size_of::<Elf64Phdr>() as u16 {
        return Err(ElfError::InvalidPhdrOffset);
    }

    if ehdr.e_phnum > 8 {
        return Err(ElfError::InvalidPhdrCount);
    }

    // Parse program headers
    let mut segments = [None; 8];
    let mut segment_count = 0;

    for i in 0..ehdr.e_phnum {
        let phdr_offset = ehdr.e_phoff as usize + (i as usize * ehdr.e_phentsize as usize);

        // Bounds check
        if phdr_offset + core::mem::size_of::<Elf64Phdr>() > data.len() {
            return Err(ElfError::InvalidPhdrOffset);
        }

        // SAFETY: We bounds-checked the offset
        let phdr = unsafe { &*(data.as_ptr().add(phdr_offset) as *const Elf64Phdr) };

        // Only process PT_LOAD segments
        if phdr.p_type == PT_LOAD {
            // Validate sizes
            if phdr.p_filesz > phdr.p_memsz {
                return Err(ElfError::InvalidSize);
            }

            // Validate file bounds
            let segment_end =
                phdr.p_offset.checked_add(phdr.p_filesz).ok_or(ElfError::InvalidSize)?;
            if segment_end as usize > data.len() {
                return Err(ElfError::InvalidSize);
            }

            segments[segment_count] = Some(LoadSegment {
                vaddr: phdr.p_vaddr,
                file_offset: phdr.p_offset,
                file_size: phdr.p_filesz,
                mem_size: phdr.p_memsz,
                flags: phdr.p_flags,
            });
            segment_count += 1;
        }
    }

    Ok(ElfInfo { entry_point: ehdr.e_entry, load_segments: segments, segment_count })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_magic() {
        let data = [0u8; 64];
        assert_eq!(parse_elf(&data), Err(ElfError::InvalidMagic));
    }

    #[test]
    fn test_file_too_small() {
        let data = [0u8; 10];
        assert_eq!(parse_elf(&data), Err(ElfError::FileTooSmall));
    }

    #[test]
    fn test_valid_elf_header() {
        let mut data = vec![0u8; 1024];

        // Create minimal valid ELF header
        data[0..4].copy_from_slice(&ELF_MAGIC);
        data[4] = ELFCLASS64;
        data[5] = ELFDATA2LSB;
        data[6] = EV_CURRENT;

        // e_type = ET_EXEC (offset 16, little-endian)
        data[16] = ET_EXEC as u8;
        data[17] = (ET_EXEC >> 8) as u8;

        // e_machine = EM_X86_64 (offset 18, little-endian)
        data[18] = EM_X86_64 as u8;
        data[19] = (EM_X86_64 >> 8) as u8;

        // e_version (offset 20, 4 bytes little-endian)
        data[20] = 1;

        // e_phentsize (offset 54, 2 bytes)
        let phsize = core::mem::size_of::<Elf64Phdr>() as u16;
        data[54] = phsize as u8;
        data[55] = (phsize >> 8) as u8;

        // e_phnum = 0 (offset 56)
        data[56] = 0;

        let result = parse_elf(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_segment_flags() {
        let seg = LoadSegment {
            vaddr: 0x1000,
            file_offset: 0,
            file_size: 100,
            mem_size: 100,
            flags: PF_R | PF_X,
        };

        assert!(seg.is_readable());
        assert!(!seg.is_writable());
        assert!(seg.is_executable());
    }
}
