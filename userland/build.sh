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

# Build ls program
echo "Building ls..."
nasm -f elf64 ls.asm -o build/ls.o
$LINKER -o build/ls build/ls.o -static -nostdlib --entry=_start

# Build chmod program
echo "Building chmod..."
nasm -f elf64 chmod.asm -o build/chmod.o
$LINKER -o build/chmod build/chmod.o -static -nostdlib --entry=_start

# Build id program
echo "Building id..."
nasm -f elf64 id.asm -o build/id.o
$LINKER -o build/id build/id.o -static -nostdlib --entry=_start

# Build su program
echo "Building su..."
nasm -f elf64 su.asm -o build/su.o
$LINKER -o build/su build/su.o -static -nostdlib --entry=_start

# Build chown program
echo "Building chown..."
nasm -f elf64 chown.asm -o build/chown.o
$LINKER -o build/chown build/chown.o -static -nostdlib --entry=_start

# Build brk_test program
echo "Building brk_test..."
nasm -f elf64 brk_test.asm -o build/brk_test.o
$LINKER -o build/brk_test build/brk_test.o -static -nostdlib --entry=_start

# Build mmap_test program
echo "Building mmap_test..."
nasm -f elf64 mmap_test.asm -o build/mmap_test.o
$LINKER -o build/mmap_test build/mmap_test.o -static -nostdlib --entry=_start

# Build vm_test program
echo "Building vm_test..."
nasm -f elf64 vm_test.asm -o build/vm_test.o
$LINKER -o build/vm_test build/vm_test.o -static -nostdlib --entry=_start

# Build touch program
echo "Building touch..."
nasm -f elf64 touch.asm -o build/touch.o
$LINKER -o build/touch build/touch.o -static -nostdlib --entry=_start

# Build mkdir_cmd program
echo "Building mkdir..."
nasm -f elf64 mkdir_cmd.asm -o build/mkdir.o
$LINKER -o build/mkdir build/mkdir.o -static -nostdlib --entry=_start

# Build rm program
echo "Building rm..."
nasm -f elf64 rm.asm -o build/rm.o
$LINKER -o build/rm build/rm.o -static -nostdlib --entry=_start

# Build mv program
echo "Building mv..."
nasm -f elf64 mv.asm -o build/mv.o
$LINKER -o build/mv build/mv.o -static -nostdlib --entry=_start

# Build cp program
echo "Building cp..."
nasm -f elf64 cp.asm -o build/cp.o
$LINKER -o build/cp build/cp.o -static -nostdlib --entry=_start

# Build args program
echo "Building args..."
nasm -f elf64 args.asm -o build/args.o
$LINKER -o build/args build/args.o -static -nostdlib --entry=_start

# Build printenv program
echo "Building printenv..."
nasm -f elf64 printenv.asm -o build/printenv.o
$LINKER -o build/printenv build/printenv.o -static -nostdlib --entry=_start

# Build sleepy program
echo "Building sleepy..."
nasm -f elf64 sleepy.asm -o build/sleepy.o
$LINKER -o build/sleepy build/sleepy.o -static -nostdlib --entry=_start

# Build cowtest program
echo "Building cowtest..."
nasm -f elf64 cowtest.asm -o build/cowtest.o
$LINKER -o build/cowtest build/cowtest.o -static -nostdlib --entry=_start

# Build nslookup program
echo "Building nslookup..."
nasm -f elf64 nslookup.asm -o build/nslookup.o
$LINKER -o build/nslookup build/nslookup.o -static -nostdlib --entry=_start

cp build/hello build/hello1 build/hello2 build/init build/sh build/cat build/true build/echo build/wc build/ls build/chmod build/id build/su build/chown build/brk_test build/mmap_test build/vm_test build/touch build/mkdir build/rm build/mv build/cp build/args build/printenv build/sleepy build/cowtest build/nslookup bin/

echo "Userland programs built successfully!"
ls -lh build/
ls -lh bin/
