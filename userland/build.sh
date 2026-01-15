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
ld -o build/hello build/hello.o -static -nostdlib --entry=_start

# Build hello1 program
echo "Building hello1..."
nasm -f elf64 hello1.asm -o build/hello1.o
ld -o build/hello1 build/hello1.o -static -nostdlib --entry=_start

# Build hello2 program
echo "Building hello2..."
nasm -f elf64 hello2.asm -o build/hello2.o
ld -o build/hello2 build/hello2.o -static -nostdlib --entry=_start

# Build init program
echo "Building init..."
nasm -f elf64 init.asm -o build/init.o
ld -o build/init build/init.o -static -nostdlib --entry=_start

# Build sh program
echo "Building sh..."
nasm -f elf64 sh.asm -o build/sh.o
ld -o build/sh build/sh.o -static -nostdlib --entry=_start

echo "Userland programs built successfully!"
ls -lh build/
