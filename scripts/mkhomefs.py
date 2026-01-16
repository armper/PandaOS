#!/usr/bin/env python3
"""
Create a blank HomeFS disk image.
This creates an empty filesystem that can be mounted at /home.
"""

import struct
import sys

# Constants from homefs.rs
FS_MAGIC = 0x50414E44  # "PAND"
FS_VERSION = 2
SECTOR_SIZE = 512
TOTAL_SECTORS = 1024
MAX_INODES = 256
MAX_DATA_BLOCKS = TOTAL_SECTORS - 35  # 989
INODE_SIZE = 64
MAX_DIRECT_BLOCKS = 8
MAX_FILENAME_LEN = 248
DIR_ENTRY_SIZE = 256

# Inode types
INODE_FILE = 1
INODE_DIR = 2


def create_superblock():
    """Create the superblock (sector 0)."""
    sb = bytearray(SECTOR_SIZE)
    
    # Pack superblock structure
    struct.pack_into('<I', sb, 0, FS_MAGIC)  # magic
    struct.pack_into('<I', sb, 4, FS_VERSION)  # version
    struct.pack_into('<I', sb, 8, SECTOR_SIZE)  # block_size
    struct.pack_into('<I', sb, 12, TOTAL_SECTORS)  # total_blocks
    struct.pack_into('<I', sb, 16, MAX_INODES)  # inode_count
    struct.pack_into('<I', sb, 20, MAX_DATA_BLOCKS - 1)  # free_blocks (minus root dir)
    struct.pack_into('<I', sb, 24, MAX_INODES - 1)  # free_inodes (minus root)
    struct.pack_into('<I', sb, 28, 35)  # first_data_block
    struct.pack_into('<I', sb, 32, 1)  # inode_bitmap_sector
    struct.pack_into('<I', sb, 36, 2)  # block_bitmap_sector
    struct.pack_into('<I', sb, 40, 3)  # inode_table_sector
    struct.pack_into('<I', sb, 44, 1)  # root_inode
    
    return sb


def create_inode_bitmap():
    """Create inode bitmap (sector 1) with root inode marked."""
    bitmap = bytearray(SECTOR_SIZE)
    # Mark inode 1 as used (root directory)
    bitmap[0] = 0x02  # Bit 1
    return bitmap


def create_block_bitmap():
    """Create block bitmap (sector 2) with first block marked."""
    bitmap = bytearray(SECTOR_SIZE)
    # Mark block 0 as used (root directory data)
    bitmap[0] = 0x01  # Bit 0
    return bitmap


def create_root_inode():
    """Create root directory inode."""
    inode = bytearray(INODE_SIZE)
    
    struct.pack_into('<I', inode, 0, 1)  # inode_num
    struct.pack_into('<I', inode, 4, INODE_DIR)  # file_type
    struct.pack_into('<H', inode, 8, 0o755)  # mode
    struct.pack_into('<H', inode, 10, 0)  # uid
    struct.pack_into('<H', inode, 12, 0)  # gid
    struct.pack_into('<H', inode, 14, 1)  # link_count
    struct.pack_into('<I', inode, 16, SECTOR_SIZE)  # size (one block)
    struct.pack_into('<I', inode, 20, 1)  # blocks_used
    
    # direct_blocks[0] = 35 (first data block sector)
    struct.pack_into('<I', inode, 24, 35)
    # Rest are zeros
    
    return inode


def create_inode_table():
    """Create inode table (sectors 3-34) with root inode."""
    table = bytearray(SECTOR_SIZE * 32)  # 32 sectors
    
    # Write root inode at offset 0
    root_inode = create_root_inode()
    table[0:INODE_SIZE] = root_inode
    
    return table


def create_dir_entry(inode, name, file_type):
    """Create a directory entry."""
    entry = bytearray(DIR_ENTRY_SIZE)
    
    struct.pack_into('<I', entry, 0, inode)
    struct.pack_into('<B', entry, 4, len(name))
    struct.pack_into('<B', entry, 5, file_type)
    # padding at 6-7
    
    # Write name
    name_bytes = name.encode('utf-8')
    entry[8:8+len(name_bytes)] = name_bytes
    
    return entry


def create_root_directory():
    """Create root directory data block (sector 35)."""
    block = bytearray(SECTOR_SIZE)
    
    # . entry
    dot_entry = create_dir_entry(1, '.', INODE_DIR)
    block[0:DIR_ENTRY_SIZE] = dot_entry
    
    # .. entry (parent is also root)
    dotdot_entry = create_dir_entry(1, '..', INODE_DIR)
    block[DIR_ENTRY_SIZE:2*DIR_ENTRY_SIZE] = dotdot_entry
    
    return block


def main():
    """Create home.img disk image."""
    output_file = 'home.img'
    
    if len(sys.argv) > 1:
        output_file = sys.argv[1]
    
    print(f"Creating HomeFS disk image: {output_file}")
    print(f"Size: {TOTAL_SECTORS} sectors ({TOTAL_SECTORS * SECTOR_SIZE} bytes)")
    
    with open(output_file, 'wb') as f:
        # Sector 0: Superblock
        f.write(create_superblock())
        
        # Sector 1: Inode bitmap
        f.write(create_inode_bitmap())
        
        # Sector 2: Block bitmap
        f.write(create_block_bitmap())
        
        # Sectors 3-34: Inode table
        f.write(create_inode_table())
        
        # Sector 35: Root directory
        f.write(create_root_directory())
        
        # Sectors 36-1023: Zero-filled data blocks
        remaining = TOTAL_SECTORS - 36
        zero_block = bytearray(SECTOR_SIZE)
        for _ in range(remaining):
            f.write(zero_block)
    
    print(f"Created {output_file}")
    print(f"  Max files: {MAX_INODES}")
    print(f"  Max file size: {MAX_DIRECT_BLOCKS * SECTOR_SIZE} bytes")
    print(f"  Free blocks: {MAX_DATA_BLOCKS - 1}")


if __name__ == '__main__':
    main()
