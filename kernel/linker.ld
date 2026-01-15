/* Linker script for PandaOS kernel
 * Defines memory layout and exports kernel boundary symbols
 * for use in frame reservation and memory management.
 */

ENTRY(_start)

/* Higher-half kernel base address */
KERNEL_VIRT_BASE = 0xFFFF800000000000;

/* Physical load address (bootloader loads kernel at low memory) */
KERNEL_PHYS_BASE = 0x400000; /* 4 MiB - typical multiboot location */

SECTIONS
{
    /* Start at higher-half virtual address, but load at physical address */
    . = KERNEL_VIRT_BASE + KERNEL_PHYS_BASE;
    
    /* Export kernel physical boundaries for frame reservation */
    __kernel_phys_start = . - KERNEL_VIRT_BASE;
    __kernel_virt_start = .;
    
    /* Text section (code) - Read + Execute, No Write */
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VIRT_BASE)
    {
        __text_start = .;
        KEEP(*(.text._start))
        *(.text .text.*)
        __text_end = .;
    }
    
    /* Read-only data - Read, No Write, No Execute */
    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_VIRT_BASE)
    {
        __rodata_start = .;
        *(.rodata .rodata.*)
        __rodata_end = .;
    }
    
    /* Data section (initialized data) - Read + Write, No Execute */
    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VIRT_BASE)
    {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    }
    
    /* BSS section (uninitialized data) - Read + Write, No Execute */
    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_VIRT_BASE)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }
    
    /* Export kernel end boundary */
    __kernel_virt_end = .;
    __kernel_phys_end = . - KERNEL_VIRT_BASE;
    
    /* Discard debug sections to reduce binary size */
    /DISCARD/ :
    {
        *(.eh_frame)
        *(.comment)
    }
}
