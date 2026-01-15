#!/bin/bash
# Build userland programs for PandaOS

set -e

cd "$(dirname "$0")"

echo "Building userland programs..."

# Create build directory
mkdir -p build

# Build hello program as ELF executable
echo "Building hello..."
nasm -f elf64 hello.asm -o build/hello.o

# Link as static ELF executable
# -static: create static executable
# -nostdlib: don't link with standard library
# --entry=_start: set entry point
ld -o build/hello build/hello.o -static -nostdlib --entry=_start

echo "Userland programs built successfully!"
echo "Output: build/hello"
ls -lh build/hello
file build/hello
