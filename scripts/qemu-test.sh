#!/usr/bin/env bash
# QEMU integration test runner for PandaOS
# Watches serial output for TEST PASS/FAIL markers

set -e

TIMEOUT=${QEMU_TIMEOUT:-30}
TIMEOUT_BIN="timeout"
if ! command -v timeout >/dev/null 2>&1; then
    if command -v gtimeout >/dev/null 2>&1; then
        TIMEOUT_BIN="gtimeout"
    else
        TIMEOUT_BIN=""
    fi
fi
SERIAL_LOG="/tmp/panda-qemu-serial.log"

echo "==================================="
echo "PandaOS QEMU Integration Tests"
echo "==================================="

# Build kernel first
echo "Building kernel..."
FEATURES=()
EXPECTED_MARKER=""
if [ "${SHELL_SMOKE:-0}" -eq 1 ] && [ "${VFS_CAT_SMOKE:-0}" -eq 1 ]; then
    echo "Error: SHELL_SMOKE and VFS_CAT_SMOKE are mutually exclusive"
    exit 1
fi
if [ "${SHELL_SMOKE:-0}" -eq 1 ]; then
    FEATURES+=(--features shell-smoke)
    EXPECTED_MARKER="TEST PASS shell_smoke"
fi
if [ "${VFS_CAT_SMOKE:-0}" -eq 1 ]; then
    FEATURES+=(--features vfs-cat-smoke)
    EXPECTED_MARKER="TEST PASS vfs_cat_smoke"
fi
cargo bootimage --manifest-path kernel/Cargo.toml --release --target x86_64-unknown-none "${FEATURES[@]}" 2>&1 | tail -3

# Find the kernel image
KERNEL_IMAGE=$(find target -name "bootimage-panda-kernel.bin" -type f | head -1)

if [ -z "$KERNEL_IMAGE" ]; then
    echo "Error: Could not find kernel image"
    exit 1
fi

echo "Kernel image: $KERNEL_IMAGE"
echo ""

# Run QEMU with serial output capture
echo "Starting QEMU (timeout: ${TIMEOUT}s)..."
if [ -n "$TIMEOUT_BIN" ]; then
    $TIMEOUT_BIN $TIMEOUT qemu-system-x86_64 \
        -drive format=raw,file="$KERNEL_IMAGE" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -serial stdio \
        -display none \
        2>&1 | tee "$SERIAL_LOG" &
else
    qemu-system-x86_64 \
        -drive format=raw,file="$KERNEL_IMAGE" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -serial stdio \
        -display none \
        2>&1 | tee "$SERIAL_LOG" &
fi

QEMU_PID=$!

# Wait for QEMU to finish
wait $QEMU_PID || EXIT_CODE=$?

echo ""
echo "==================================="

# Parse test results from serial output
if grep -q "TEST PASS" "$SERIAL_LOG"; then
    PASS_COUNT=$(grep -c "TEST PASS" "$SERIAL_LOG")
    echo "✓ Tests passed: $PASS_COUNT"
fi

if [ -n "$EXPECTED_MARKER" ] && ! grep -q "$EXPECTED_MARKER" "$SERIAL_LOG"; then
    echo "✗ Expected marker not found: $EXPECTED_MARKER"
    exit 1
fi

if grep -q "TEST FAIL" "$SERIAL_LOG"; then
    FAIL_COUNT=$(grep -c "TEST FAIL" "$SERIAL_LOG")
    echo "✗ Tests failed: $FAIL_COUNT"
    exit 1
fi

if grep -q "KERNEL PANIC" "$SERIAL_LOG"; then
    echo "✗ Kernel panic detected!"
    exit 1
fi

# Check exit code (QEMU exit device returns exit code + 1)
if [ "${EXIT_CODE:-0}" -eq 33 ]; then
    echo "✓ Kernel exited successfully"
    exit 0
fi

echo "✓ QEMU test completed"
