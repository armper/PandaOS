#!/usr/bin/env bash
# PandaOS QEMU Runner Script
# One-command runner with flexible display modes
#
# Usage:
#   GUI_VGA=1 ./scripts/run-qemu.sh           # QEMU window with VGA text
#   SERIAL_STDIO=1 ./scripts/run-qemu.sh      # Serial output in terminal
#   BOTH=1 ./scripts/run-qemu.sh              # VGA window + serial in terminal
#   HEADLESS=1 ./scripts/run-qemu.sh          # No display, serial only
#   QEMU_ARGS="-m 512M" ./scripts/run-qemu.sh # Custom QEMU args
#
# Default: SERIAL_STDIO=1 (terminal shows serial output)

set -e

# Find the newest bootimage automatically
find_bootimage() {
    local search_paths=(
        "target/x86_64-unknown-none/debug/bootimage-*.bin"
        "target/x86_64-unknown-none/release/bootimage-*.bin"
    )
    
    local newest=""
    for pattern in "${search_paths[@]}"; do
        for file in $pattern; do
            if [ -f "$file" ]; then
                if [ -z "$newest" ] || [ "$file" -nt "$newest" ]; then
                    newest="$file"
                fi
            fi
        done
    done
    
    echo "$newest"
}

# Check if QEMU is available
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "Error: qemu-system-x86_64 not found"
    echo ""
    echo "Please install QEMU:"
    echo "  macOS:  brew install qemu"
    echo "  Linux:  sudo apt-get install qemu-system-x86"
    echo "          or sudo dnf install qemu-system-x86"
    exit 1
fi

# Find bootimage
BOOTIMAGE=$(find_bootimage)
if [ -z "$BOOTIMAGE" ] || [ ! -f "$BOOTIMAGE" ]; then
    echo "Error: No bootimage found!"
    echo ""
    echo "Please build the bootimage first:"
    echo "  make bootimage"
    echo ""
    echo "Or ensure 'bootimage' is installed:"
    echo "  cargo install bootimage --version '^0.10'"
    exit 1
fi

echo "Using bootimage: $BOOTIMAGE"

# Check for filesystem image
FS_IMG="fs.img"
if [ ! -f "$FS_IMG" ]; then
    echo "Warning: fs.img not found, creating it..."
    if [ -f "scripts/mkdiskimg.py" ]; then
        python3 scripts/mkdiskimg.py
    else
        echo "Warning: Cannot create fs.img (scripts/mkdiskimg.py not found)"
        echo "Continuing without filesystem..."
        FS_IMG=""
    fi
fi

# Base QEMU arguments
QEMU_BASE_ARGS=(
    -drive "format=raw,file=$BOOTIMAGE"
    -netdev user,id=n0
    -device virtio-net-pci,netdev=n0
)

# Add filesystem if available
if [ -n "$FS_IMG" ] && [ -f "$FS_IMG" ]; then
    QEMU_BASE_ARGS+=(-drive "file=$FS_IMG,format=raw,if=ide")
fi

# Determine display mode
MODE=""
MODE_COUNT=0

if [ "${GUI_VGA:-0}" -eq 1 ]; then
    MODE="gui_vga"
    MODE_COUNT=$((MODE_COUNT + 1))
fi
if [ "${SERIAL_STDIO:-0}" -eq 1 ]; then
    MODE="serial_stdio"
    MODE_COUNT=$((MODE_COUNT + 1))
fi
if [ "${BOTH:-0}" -eq 1 ]; then
    MODE="both"
    MODE_COUNT=$((MODE_COUNT + 1))
fi
if [ "${HEADLESS:-0}" -eq 1 ]; then
    MODE="headless"
    MODE_COUNT=$((MODE_COUNT + 1))
fi

# Default to SERIAL_STDIO if no mode specified
if [ $MODE_COUNT -eq 0 ]; then
    MODE="serial_stdio"
    MODE_COUNT=1
fi

# Check for conflicting modes
if [ $MODE_COUNT -gt 1 ]; then
    echo "Error: Multiple modes specified (only one allowed)"
    echo "  GUI_VGA=1      - QEMU window shows VGA text"
    echo "  SERIAL_STDIO=1 - Terminal shows serial output (default)"
    echo "  BOTH=1         - VGA window + serial in terminal"
    echo "  HEADLESS=1     - No display, serial only"
    exit 1
fi

# Configure QEMU based on mode
case "$MODE" in
    gui_vga)
        echo "Starting QEMU in GUI VGA mode (window shows VGA text)..."
        QEMU_ARGS=(
            "${QEMU_BASE_ARGS[@]}"
            -serial "file:target/qemu/run.log"
        )
        # No -display or -serial stdio, let QEMU show VGA window
        ;;
    serial_stdio)
        echo "Starting QEMU in serial stdio mode (terminal shows serial)..."
        QEMU_ARGS=(
            "${QEMU_BASE_ARGS[@]}"
            -serial "mon:stdio"
            -display "none"
        )
        ;;
    both)
        echo "Starting QEMU with VGA window + serial in terminal..."
        QEMU_ARGS=(
            "${QEMU_BASE_ARGS[@]}"
            -serial "mon:stdio"
        )
        # VGA window shows by default, serial goes to stdio
        ;;
    headless)
        echo "Starting QEMU in headless mode (serial only)..."
        QEMU_ARGS=(
            "${QEMU_BASE_ARGS[@]}"
            -serial "mon:stdio"
            -display "none"
        )
        ;;
esac

# Create log directory
mkdir -p target/qemu

# Add custom QEMU args if provided
if [ -n "$QEMU_ARGS" ]; then
    echo "Adding custom QEMU arguments: $QEMU_ARGS"
    # shellcheck disable=SC2206
    QEMU_ARGS+=(${QEMU_ARGS})
fi

# Run QEMU
echo "Launching QEMU..."
echo ""
exec qemu-system-x86_64 "${QEMU_ARGS[@]}"
