#!/usr/bin/env bash
# QEMU integration test runner for PandaOS
# Watches serial output for TEST PASS/FAIL markers
#
# Usage:
#   SHELL_SMOKE=1 ./scripts/qemu-test.sh
#   VFS_CAT_SMOKE=1 ./scripts/qemu-test.sh
#   FORK_EXEC_SMOKE=1 ./scripts/qemu-test.sh

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

echo "==================================="
echo "PandaOS QEMU Integration Tests"
echo "==================================="

# Determine which test to run
TEST_NAME=""
FEATURES=()
EXPECTED_MARKER=""
FEATURE_COUNT=0

if [ "${SHELL_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="shell_smoke"
    FEATURES+=(--features shell-smoke)
    EXPECTED_MARKER="TEST PASS shell_smoke"
fi
if [ "${VFS_CAT_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="vfs_cat_smoke"
    FEATURES+=(--features vfs-cat-smoke)
    EXPECTED_MARKER="TEST PASS vfs_cat_smoke"
fi
if [ "${FORK_EXEC_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="fork_exec_smoke"
    FEATURES+=(--features fork-exec-smoke)
    EXPECTED_MARKER="TEST PASS fork_exec_smoke"
fi
if [ "${PIPE_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="pipe_smoke"
    FEATURES+=(--features pipe-smoke)
    EXPECTED_MARKER="TEST PASS pipe_smoke"
fi
if [ "${CTRLC_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="ctrlc_smoke"
    FEATURES+=(--features ctrlc-smoke)
    EXPECTED_MARKER="TEST PASS ctrlc_smoke"
fi
if [ "${LS_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="ls_smoke"
    FEATURES+=(--features ls-smoke)
    EXPECTED_MARKER="TEST PASS ls_smoke"
fi
if [ "${CD_SMOKE:-0}" -eq 1 ]; then
    FEATURE_COUNT=$((FEATURE_COUNT + 1))
    TEST_NAME="cd_smoke"
    FEATURES+=(--features cd-smoke)
    EXPECTED_MARKER="TEST PASS cd_smoke"
fi

if [ $FEATURE_COUNT -gt 1 ]; then
    echo "Error: SHELL_SMOKE, VFS_CAT_SMOKE, FORK_EXEC_SMOKE, PIPE_SMOKE, CTRLC_SMOKE, LS_SMOKE, and CD_SMOKE are mutually exclusive"
    exit 1
fi

if [ $FEATURE_COUNT -eq 0 ]; then
    echo "Error: Must set one of SHELL_SMOKE, VFS_CAT_SMOKE, FORK_EXEC_SMOKE, PIPE_SMOKE, CTRLC_SMOKE, LS_SMOKE, or CD_SMOKE"
    exit 1
fi

# Create logs directory
mkdir -p target/qemu
SERIAL_LOG="target/qemu/${TEST_NAME}.log"
rm -f "$SERIAL_LOG"

echo "Test: $TEST_NAME"
echo "Serial log: $SERIAL_LOG"
echo ""

# Build kernel
echo "Building kernel..."
BUILD_OUTPUT=$(mktemp)
if cargo bootimage --manifest-path kernel/Cargo.toml --release --target x86_64-unknown-none "${FEATURES[@]}" 2>&1 | tee "$BUILD_OUTPUT" | tail -3; then
    BUILD_SUCCESS=1
else
    BUILD_SUCCESS=0
    echo ""
    echo "==================================="
    echo "Build failed! Full output:"
    echo "==================================="
    cat "$BUILD_OUTPUT"
    rm -f "$BUILD_OUTPUT"
    exit 1
fi
rm -f "$BUILD_OUTPUT"

# Find the kernel image - use ls -t to get newest file deterministically
KERNEL_IMAGE=$(find target -name "bootimage-panda-kernel.bin" -type f -print0 2>/dev/null | xargs -0 ls -t 2>/dev/null | head -1)

if [ -z "$KERNEL_IMAGE" ]; then
    echo "Error: Could not find kernel image"
    exit 1
fi

echo "Kernel image: $KERNEL_IMAGE"
echo ""

# Run QEMU with serial output to file
# Using -serial file: ensures output is written directly without buffering
# Additional flags for robustness:
#   -no-reboot: exit instead of rebooting on triple fault
#   -no-shutdown: keep QEMU running after guest shutdown for log capture
#   -smp 1: single CPU for deterministic behavior
#   -m 256M: 256MB RAM
echo "Starting QEMU (timeout: ${TIMEOUT}s)..."
EXIT_CODE=0
if [ -n "$TIMEOUT_BIN" ]; then
    $TIMEOUT_BIN $TIMEOUT qemu-system-x86_64 \
        -drive format=raw,file="$KERNEL_IMAGE" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -serial file:"$SERIAL_LOG" \
        -display none \
        -no-reboot \
        -no-shutdown \
        -smp 1 \
        -m 256M \
        2>&1 || EXIT_CODE=$?
else
    qemu-system-x86_64 \
        -drive format=raw,file="$KERNEL_IMAGE" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -serial file:"$SERIAL_LOG" \
        -display none \
        -no-reboot \
        -no-shutdown \
        -smp 1 \
        -m 256M \
        2>&1 || EXIT_CODE=$?
fi

echo ""
echo "==================================="
echo "Test Results: $TEST_NAME"
echo "==================================="

# Verify serial log exists and has content
if [ ! -f "$SERIAL_LOG" ]; then
    echo "✗ Serial log file not created: $SERIAL_LOG"
    echo "QEMU may have failed to start or exited immediately"
    exit 1
fi

LOG_SIZE=$(wc -c < "$SERIAL_LOG" 2>/dev/null || echo "0")
if [ "$LOG_SIZE" -eq 0 ]; then
    echo "✗ Serial log is empty!"
    echo "Kernel may not be outputting to serial port"
    exit 1
fi

echo "Serial log captured: $LOG_SIZE bytes"
echo ""

# Show first few lines of output for debugging
echo "Serial output preview:"
echo "---"
head -15 "$SERIAL_LOG" 2>/dev/null || cat "$SERIAL_LOG"
echo "---"
echo ""

# Parse test results from serial output
PASS_COUNT=$(grep -c "TEST PASS" "$SERIAL_LOG" 2>/dev/null || echo "0")
if [ "$PASS_COUNT" -gt 0 ]; then
    echo "✓ Found $PASS_COUNT TEST PASS marker(s)"
fi

# Check for expected marker - this is the PRIMARY success signal
if [ -n "$EXPECTED_MARKER" ]; then
    if grep -q "$EXPECTED_MARKER" "$SERIAL_LOG" 2>/dev/null; then
        echo "✓ Expected marker found: $EXPECTED_MARKER"
        MARKER_FOUND=1
    else
        echo "✗ Expected marker not found: $EXPECTED_MARKER"
        echo ""
        echo "All TEST markers in log:"
        grep "TEST" "$SERIAL_LOG" 2>/dev/null || echo "(none found)"
        MARKER_FOUND=0
    fi
fi

# Check for failures
FAIL_COUNT=$(grep -c "TEST FAIL" "$SERIAL_LOG" 2>/dev/null || echo "0")
if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "✗ Tests failed: $FAIL_COUNT"
    exit 1
fi

if grep -q "KERNEL PANIC" "$SERIAL_LOG" 2>/dev/null; then
    echo "✗ Kernel panic detected!"
    exit 1
fi

# Primary success check: TEST PASS marker found
if [ "${MARKER_FOUND:-0}" -eq 1 ]; then
    echo ""
    echo "==================================="
    echo "✓ Test $TEST_NAME PASSED"
    echo "==================================="
    if [ "$EXIT_CODE" -ne 0 ] && [ "$EXIT_CODE" -ne 33 ]; then
        echo "(Note: QEMU exit code was $EXIT_CODE, but test marker was found)"
    fi
    exit 0
fi

# Secondary check: permissive exit code handling
# QEMU isa-debug-exit returns exit_code * 2 + 1
# Success (0) -> 33, but be permissive about other codes
if [ "$EXIT_CODE" -eq 33 ]; then
    echo "✓ Kernel exited successfully (QEMU exit code: $EXIT_CODE)"
    echo ""
    echo "==================================="
    echo "✓ Test $TEST_NAME PASSED"
    echo "==================================="
    exit 0
elif [ "$EXIT_CODE" -eq 124 ] || [ "$EXIT_CODE" -eq 143 ]; then
    echo "✗ Test timed out (exit code: $EXIT_CODE)"
    exit 1
elif [ "$EXIT_CODE" -ne 0 ]; then
    echo "⚠ QEMU exited with unexpected code: $EXIT_CODE"
    echo "✗ Test failed: no TEST PASS marker and unexpected exit code"
    exit 1
fi

echo ""
echo "==================================="
echo "✓ Test $TEST_NAME completed"
echo "==================================="
