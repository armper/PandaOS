#!/bin/bash
# Build userland programs for PandaOS

set -e

cd "$(dirname "$0")"

echo "Building userland programs..."

LINKER="ld"
if command -v ld.lld >/dev/null 2>&1; then
    LINKER="ld.lld"
elif command -v /usr/local/opt/lld/bin/ld.lld >/dev/null 2>&1; then
    LINKER="/usr/local/opt/lld/bin/ld.lld"
elif command -v x86_64-elf-ld >/dev/null 2>&1; then
    LINKER="x86_64-elf-ld"
fi

# Create build/output directories
mkdir -p build bin

# Build hello program as ELF executable
echo "Building hello..."
nasm -f elf64 hello.asm -o build/hello.o
$LINKER -o build/hello build/hello.o -static -nostdlib --entry=_start

# Build hello1 program
echo "Building hello1..."
nasm -f elf64 hello1.asm -o build/hello1.o
$LINKER -o build/hello1 build/hello1.o -static -nostdlib --entry=_start

# Build hello2 program
echo "Building hello2..."
nasm -f elf64 hello2.asm -o build/hello2.o
$LINKER -o build/hello2 build/hello2.o -static -nostdlib --entry=_start

# Build init program
echo "Building init..."
nasm -f elf64 init.asm -o build/init.o
$LINKER -o build/init build/init.o -static -nostdlib --entry=_start

# Build sh program
echo "Building sh..."
nasm -f elf64 sh.asm -o build/sh.o
$LINKER -o build/sh build/sh.o -static -nostdlib --entry=_start

# Build cat program
echo "Building cat..."
nasm -f elf64 cat.asm -o build/cat.o
$LINKER -o build/cat build/cat.o -static -nostdlib --entry=_start

# Build true program
echo "Building true..."
nasm -f elf64 true.asm -o build/true.o
$LINKER -o build/true build/true.o -static -nostdlib --entry=_start

# Build echo program
echo "Building echo..."
nasm -f elf64 echo.asm -o build/echo.o
$LINKER -o build/echo build/echo.o -static -nostdlib --entry=_start

# Build wc program
echo "Building wc..."
nasm -f elf64 wc.asm -o build/wc.o
$LINKER -o build/wc build/wc.o -static -nostdlib --entry=_start

cp build/hello build/hello1 build/hello2 build/init build/sh build/cat build/true build/echo build/wc bin/

echo "Userland programs built successfully!"
ls -lh build/
ls -lh bin/
