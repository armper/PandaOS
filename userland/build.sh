#!/bin/bash
# Build userland programs for PandaOS

set -e

cd "$(dirname "$0")"

echo "Building userland programs..."

# Create build directory
mkdir -p build

# Build hello program
echo "Building hello..."
nasm -f elf64 hello.asm -o build/hello.o
ld -o build/hello build/hello.o -N --oformat binary

echo "Userland programs built successfully!"
echo "Output: build/hello"
ls -lh build/hello
