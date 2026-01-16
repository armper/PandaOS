#!/bin/bash
# Build musl-based userland programs for PandaOS
#
# Requirements:
# - musl-gcc or x86_64-linux-musl-gcc
# - gcc

set -e

cd "$(dirname "$0")"

echo "Building musl-based userland programs..."

# Detect musl compiler
MUSL_GCC=""
if command -v x86_64-linux-musl-gcc >/dev/null 2>&1; then
    MUSL_GCC="x86_64-linux-musl-gcc"
elif command -v musl-gcc >/dev/null 2>&1; then
    MUSL_GCC="musl-gcc"
else
    echo "Warning: musl-gcc not found. Install musl-tools or musl-cross."
    echo "On Ubuntu/Debian: sudo apt-get install musl-tools"
    echo "Skipping musl builds..."
fi

# Create build directory
mkdir -p build_musl

if [ -n "$MUSL_GCC" ]; then
    echo "Using musl compiler: $MUSL_GCC"
    
    # Build hello_musl
    if [ -f hello_musl.c ]; then
        echo "Building hello_musl..."
        $MUSL_GCC -static -nostdlib -o build_musl/hello_musl hello_musl.c
    fi
    
    # Build true
    if [ -f true.c ]; then
        echo "Building true..."
        $MUSL_GCC -static -nostdlib -o build_musl/true true.c
    fi
    
    # Build echo
    if [ -f echo.c ]; then
        echo "Building echo..."
        $MUSL_GCC -static -nostdlib -o build_musl/echo echo.c
    fi
    
    echo "Musl programs built successfully!"
    ls -lh build_musl/
    
    # Verify they're static
    echo ""
    echo "Verifying static linking:"
    for f in build_musl/*; do
        if [ -f "$f" ]; then
            echo "  $(basename $f): $(file $f | grep -o 'statically linked' || echo 'NOT STATIC!')"
        fi
    done
fi

echo ""
echo "Build complete!"
