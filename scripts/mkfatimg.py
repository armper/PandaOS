#!/usr/bin/env python3
"""
Generate a FAT32 disk image for PandaOS.

This script creates a FAT32-formatted disk image containing userland binaries
and configuration files that can be mounted at /mnt in PandaOS.
"""

import subprocess
import sys
import tempfile
import shutil
from pathlib import Path


def check_mtools():
    """Check if mtools is installed."""
    try:
        subprocess.run(["mformat", "-h"], capture_output=True, check=False)
        return True
    except FileNotFoundError:
        return False


def create_fat32_image(output_path, size_mb=16):
    """
    Create a FAT32 disk image using mtools.
    
    Args:
        output_path: Path to the output image file
        size_mb: Size of the image in megabytes
    """
    # Calculate sectors (512 bytes each)
    sectors = (size_mb * 1024 * 1024) // 512
    
    print(f"Creating {size_mb}MB FAT32 image with {sectors} sectors...")
    
    # Create empty image file
    with open(output_path, "wb") as f:
        f.write(b"\x00" * (sectors * 512))
    
    # Format as FAT32
    # -F: FAT32, -C: create image, -i: volume ID
    subprocess.run(
        [
            "mformat",
            "-F",           # FAT32
            "-i", str(output_path),
            "-v", "PANDAOS",  # Volume label
            "-t", str(sectors // 63),  # Cylinders
            "-h", "16",     # Heads
            "-s", "63",     # Sectors per track
            "::",           # Drive (C:)
        ],
        check=True,
    )
    
    print(f"FAT32 image created: {output_path}")


def add_files_to_image(image_path, files_to_add):
    """
    Add files to the FAT32 image using mtools.
    
    Args:
        image_path: Path to the FAT32 image
        files_to_add: Dict mapping destination path to source path
    """
    for dest_path, source_path in files_to_add.items():
        if not Path(source_path).exists():
            print(f"Warning: Source file not found: {source_path}")
            continue
        
        # Get file size
        file_size = Path(source_path).stat().st_size
        print(f"  Adding {source_path} ({file_size} bytes) -> {dest_path}")
        
        # Create directory if needed
        dest_dir = Path(dest_path).parent
        if dest_dir != Path("."):
            # mmd creates directories
            try:
                subprocess.run(
                    ["mmd", "-i", str(image_path), f"::/{dest_dir}"],
                    check=False,
                    capture_output=True,
                    timeout=5,
                )
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                pass  # Directory might already exist
        
        # Copy file with timeout
        try:
            subprocess.run(
                ["mcopy", "-i", str(image_path), "-o", source_path, f"::/{dest_path}"],
                check=True,
                timeout=10,
            )
        except subprocess.TimeoutExpired:
            print(f"  Warning: Timeout copying {source_path}, retrying...")
            subprocess.run(
                ["mcopy", "-i", str(image_path), "-o", source_path, f"::/{dest_path}"],
                check=True,
                timeout=30,
            )


def main():
    """Main function to generate FAT32 disk image."""
    # Check if mtools is installed
    if not check_mtools():
        print("Error: mtools is not installed.")
        print("Please install mtools:")
        print("  Ubuntu/Debian: sudo apt-get install mtools")
        print("  Fedora/RHEL: sudo dnf install mtools")
        print("  macOS: brew install mtools")
        sys.exit(1)
    
    # Paths
    repo_root = Path(__file__).parent.parent
    userland_bin = repo_root / "userland" / "bin"
    output_image = repo_root / "fs.img"
    
    print(f"Generating FAT32 disk image: {output_image}")
    
    # Create image
    create_fat32_image(output_image, size_mb=16)
    
    # Create directories
    print("\nCreating directories...")
    for dir_path in ["bin", "etc"]:
        subprocess.run(
            ["mmd", "-i", str(output_image), f"::/{dir_path}"],
            check=False,
            capture_output=True,
        )
    
    # Prepare files to add
    files_to_add = {}
    
    # Add userland binaries
    print("\nAdding userland binaries...")
    for name in ["init", "sh", "ls", "cat", "echo", "wc", "true", "spin", "pingpong", "preempt_test"]:
        bin_path = userland_bin / name
        if bin_path.exists():
            files_to_add[f"bin/{name}"] = str(bin_path)
        else:
            print(f"  Warning: {name} not found, skipping")
    
    # Add configuration files
    print("\nAdding configuration files...")
    
    # Create a temporary version file
    with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".txt") as f:
        f.write("PandaOS 0.1.0 (FAT32)\n")
        version_file = f.name
    
    files_to_add["etc/version"] = version_file
    
    # Create a temporary README
    with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".txt") as f:
        f.write("PandaOS FAT32 Filesystem\n")
        f.write("========================\n")
        f.write("\n")
        f.write("This is a FAT32 filesystem mounted at /mnt\n")
        f.write("\n")
        f.write("Contents:\n")
        f.write("  /bin/ - User binaries (init, sh, ls, cat, etc.)\n")
        f.write("  /etc/ - Configuration files\n")
        readme_file = f.name
    
    files_to_add["README.TXT"] = readme_file
    
    # Add all files
    print("\nCopying files to image...")
    add_files_to_image(output_image, files_to_add)
    
    # Clean up temporary files
    Path(version_file).unlink()
    Path(readme_file).unlink()
    
    # List contents
    print("\nImage contents:")
    subprocess.run(["mdir", "-i", str(output_image), "::"], check=False)
    subprocess.run(["mdir", "-i", str(output_image), "::/bin"], check=False)
    subprocess.run(["mdir", "-i", str(output_image), "::/etc"], check=False)
    
    print(f"\nFAT32 image created successfully: {output_image}")
    print(f"Size: {output_image.stat().st_size / 1024 / 1024:.1f} MB")


if __name__ == "__main__":
    main()
