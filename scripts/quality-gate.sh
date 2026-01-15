#!/usr/bin/env bash
# Quality gate script for PandaOS
# This script enforces code quality standards before any commit

set -e

echo "==================================="
echo "PandaOS Quality Gate"
echo "==================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓ $2${NC}"
    else
        echo -e "${RED}✗ $2${NC}"
        exit 1
    fi
}

# 1. Check code formatting
echo ""
echo "1. Checking code formatting..."
cargo fmt --all -- --check
print_status $? "Code formatting"

# 2. Run clippy with strict linting
echo ""
echo "2. Running clippy lints..."
cargo clippy --workspace --target x86_64-unknown-linux-gnu --lib -- -D warnings
print_status $? "Clippy lints"

# 3. Run host unit tests
echo ""
echo "3. Running host unit tests..."
cargo test --lib --workspace --target x86_64-unknown-linux-gnu
print_status $? "Host unit tests"

# 4. Check for unsafe outside allowed modules
echo ""
echo "4. Checking unsafe code placement..."
UNSAFE_COUNT=$(grep -r "unsafe" hal/src/*.rs kernel/src/*.rs 2>/dev/null | grep -v "^hal/src/serial.rs" | grep -v "^hal/src/vga.rs" | grep -v "test" | grep -v "SAFETY" | wc -l || true)
if [ "$UNSAFE_COUNT" -gt 0 ]; then
    echo -e "${YELLOW}Warning: Found $UNSAFE_COUNT potential unsafe blocks outside drivers${NC}"
    echo "Review these carefully:"
    grep -rn "unsafe" hal/src/*.rs kernel/src/*.rs 2>/dev/null | grep -v "^hal/src/serial.rs" | grep -v "^hal/src/vga.rs" | grep -v "test" | grep -v "SAFETY" || true
fi
print_status 0 "Unsafe code check (manual review required)"

echo ""
echo -e "${GREEN}==================================="
echo "All quality gates passed!"
echo "===================================${NC}"
