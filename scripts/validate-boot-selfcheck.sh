#!/usr/bin/env bash
# Simple validation that boot-selfcheck feature compiles and is properly integrated

set -e

echo "==================================="
echo "Boot Selfcheck Feature Validation"
echo "==================================="
echo ""

echo "1. Testing normal build (without boot-selfcheck)..."
if cargo build --manifest-path kernel/Cargo.toml --target x86_64-unknown-none 2>&1 | grep -q "Finished"; then
    echo "   ✓ Normal build succeeds"
else
    echo "   ✗ Normal build failed"
    exit 1
fi
echo ""

echo "2. Testing build with boot-selfcheck feature..."
if cargo build --manifest-path kernel/Cargo.toml --target x86_64-unknown-none --features boot-selfcheck 2>&1 | grep -q "Finished"; then
    echo "   ✓ Boot-selfcheck build succeeds"
else
    echo "   ✗ Boot-selfcheck build failed"
    exit 1
fi
echo ""

echo "3. Checking for BOOT_STEP macro usage..."
if grep -q "BOOT_STEP!" kernel/src/main.rs; then
    echo "   ✓ BOOT_STEP macros found in main.rs"
else
    echo "   ✗ No BOOT_STEP macros in main.rs"
    exit 1
fi
echo ""

echo "4. Checking for selfcheck module..."
if [ -f kernel/src/selfcheck.rs ]; then
    echo "   ✓ selfcheck.rs exists"
else
    echo "   ✗ selfcheck.rs missing"
    exit 1
fi
echo ""

echo "5. Checking for boot_diagnostics module..."
if [ -f kernel/src/boot_diagnostics.rs ]; then
    echo "   ✓ boot_diagnostics.rs exists"
else
    echo "   ✗ boot_diagnostics.rs missing"
    exit 1
fi
echo ""

echo "6. Checking documentation..."
if [ -f BOOT_DIAGNOSTICS.md ]; then
    echo "   ✓ BOOT_DIAGNOSTICS.md exists"
else
    echo "   ✗ BOOT_DIAGNOSTICS.md missing"
    exit 1
fi
echo ""

echo "7. Checking test harness integration..."
if grep -q "BOOT_SELFCHK" scripts/qemu-test.sh; then
    echo "   ✓ BOOT_SELFCHK support in qemu-test.sh"
else
    echo "   ✗ BOOT_SELFCHK not in qemu-test.sh"
    exit 1
fi
echo ""

echo "8. Checking for feature in Cargo.toml..."
if grep -q "boot-selfcheck" kernel/Cargo.toml; then
    echo "   ✓ boot-selfcheck feature declared"
else
    echo "   ✗ boot-selfcheck feature missing"
    exit 1
fi
echo ""

echo "9. Verifying panic handler includes boot diagnostics..."
if grep -q "boot_diagnostics::dump_boot_diagnostics" kernel/src/main.rs; then
    echo "   ✓ Panic handler enhanced with diagnostics"
else
    echo "   ✗ Panic handler not enhanced"
    exit 1
fi
echo ""

echo "10. Checking for TEST PASS marker in selfcheck..."
if grep -q "TEST PASS boot_selfcheck" kernel/src/main.rs; then
    echo "   ✓ TEST PASS marker present"
else
    echo "   ✗ TEST PASS marker missing"
    exit 1
fi
echo ""

echo "==================================="
echo "✓ All validation checks passed!"
echo "==================================="
echo ""
echo "The boot-selfcheck feature is properly integrated."
echo ""
echo "To use it:"
echo "  BOOT_SELFCHK=1 ./scripts/qemu-test.sh"
echo ""
echo "Or build manually:"
echo "  cargo bootimage --features boot-selfcheck"
echo "  qemu-system-x86_64 -drive format=raw,file=<kernel.bin> -serial stdio"
echo ""
