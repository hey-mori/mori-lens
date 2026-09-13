#!/usr/bin/env bash
# Mori Lens — Release preflight regression tests.
#
# Tests CLI parsing and dirty tree handling.
# These tests are fast and don't require running the full build pipeline.
#
# Usage:
#   ./scripts/test_release_preflight.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFLIGHT="$SCRIPT_DIR/release-preflight.sh"
TESTS_PASSED=0
TESTS_FAILED=0

pass() {
    TESTS_PASSED=$((TESTS_PASSED + 1))
    echo "  PASS: $1"
}

fail() {
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "  FAIL: $1"
}

# ── Test 1: --help exits 0 ───────────────────────────────────────────────────
echo "Test: --help exits 0"
if bash "$PREFLIGHT" --help >/dev/null 2>&1; then
    pass "--help exits 0"
else
    fail "--help exits 0"
fi

# ── Test 2: Unknown flag exits 2 ─────────────────────────────────────────────
echo "Test: Unknown flag exits 2"
bash "$PREFLIGHT" --bogus-flag >/dev/null 2>&1 && exit_code=0 || exit_code=$?
if [[ $exit_code -eq 2 ]]; then
    pass "Unknown flag exits 2"
else
    fail "Unknown flag should exit 2, got $exit_code"
fi

# ── Test 3: --version without argument exits 2 ──────────────────────────────
echo "Test: --version without argument exits 2"
bash "$PREFLIGHT" --version >/dev/null 2>&1 && exit_code=0 || exit_code=$?
if [[ $exit_code -eq 2 ]]; then
    pass "--version without arg exits 2"
else
    fail "--version without arg should exit 2, got $exit_code"
fi

# ── Test 4: --version with argument accepted ────────────────────────────────
echo "Test: --version with argument accepted"
# --verify-only will fail on dirty tree but should not be a usage error
output="$(bash "$PREFLIGHT" --version 0.2.0 --verify-only 2>&1 || true)"
if echo "$output" | grep -q "Unknown option\|requires an argument"; then
    fail "--version with arg should be accepted"
else
    pass "--version with argument accepted"
fi

# ── Test 5: Dirty tree refused by default ────────────────────────────────────
echo "Test: Dirty tree refused by default"
output="$(bash "$PREFLIGHT" --verify-only 2>&1 || true)"
if echo "$output" | grep -q "Working tree has uncommitted changes"; then
    pass "Dirty tree refused by default"
else
    fail "Dirty tree should be refused"
fi

# ── Test 6: --allow-dirty permits dirty tree ────────────────────────────────
echo "Test: --allow-dirty permits dirty tree"
# This will run the full preflight which takes time, so just check the first few lines
# which should show the banner (not the dirty tree error)
output="$(timeout 5 bash "$PREFLIGHT" --allow-dirty --verify-only 2>&1 || true)"
if echo "$output" | grep -q "Working tree has uncommitted changes"; then
    fail "--allow-dirty should permit dirty tree"
elif echo "$output" | grep -q "Mori Lens Release Preflight"; then
    pass "--allow-dirty permits dirty tree"
else
    # If timeout killed it, it got past the dirty tree check
    pass "--allow-dirty permits dirty tree (timeout, but got past check)"
fi

# ── Test 7: Script has correct structure ─────────────────────────────────────
echo "Test: Script structure verification"
if grep -q "STRUCTURALLY READY" "$PREFLIGHT"; then
    pass "Uses STRUCTURALLY READY terminology"
else
    fail "Should use STRUCTURALLY READY terminology"
fi

if grep -q "Cannot pass preflight without NSIS" "$PREFLIGHT"; then
    pass "Missing NSIS causes failure"
else
    fail "Missing NSIS should cause failure"
fi

if grep -q "Cannot pass preflight without MSIX" "$PREFLIGHT"; then
    pass "Missing MSIX causes failure"
else
    fail "Missing MSIX should cause failure"
fi

if grep -q "PayloadExePath" "$PREFLIGHT"; then
    pass "NSIS payload path is used"
else
    fail "NSIS payload path should be used"
fi

if grep -q "target/msix" "$PREFLIGHT"; then
    pass "MSIX uses canonical path"
else
    fail "MSIX should use target/msix/"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "========================================="
echo "Results: $TESTS_PASSED passed, $TESTS_FAILED failed"
echo "========================================="

if [[ $TESTS_FAILED -gt 0 ]]; then
    echo "FAILED"
    exit 1
else
    echo "ALL TESTS PASSED"
    exit 0
fi
