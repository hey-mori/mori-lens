#!/usr/bin/env bash
# Mori Lens -- Release preflight regression tests.
#
# Uses isolated temp git repositories with controlled clean/dirty states
# and stub commands so tests are fast and deterministic regardless of
# the host machine's repo state.
#
# Usage:
#   ./scripts/test_release_preflight.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFLIGHT="$SCRIPT_DIR/release-preflight.sh"
TESTS_PASSED=0
TESTS_FAILED=0
PREFLIGHT_EXIT=0
PREFLIGHT_OUTPUT=""

pass() {
    TESTS_PASSED=$((TESTS_PASSED + 1))
    echo "  PASS: $1"
}

fail() {
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "  FAIL: $1"
}

# Create an isolated temp repo with minimal structure for testing.
# The stub check.sh always succeeds so we can test preflight logic
# without building the whole project.
create_test_repo() {
    local tmpdir
    tmpdir="$(mktemp -d)"
    (
        cd "$tmpdir"
        git init -q
        git config user.email "test@test.local"
        git config user.name "Test"

        # Create minimal Cargo.toml with a known version
        printf '[workspace]\n\nversion = "0.1.0"\n' > Cargo.toml

        mkdir -p apps/lens
        printf '[package]\nname = "lens"\nversion = "0.1.0"\n' > apps/lens/Cargo.toml

        # Stub check.sh that always passes
        mkdir -p scripts
        printf '#!/usr/bin/env bash\necho "stub check.sh: all gates pass"\nexit 0\n' > scripts/check.sh
        chmod +x scripts/check.sh

        # Copy the preflight script into the test repo
        cp "$PREFLIGHT" scripts/release-preflight.sh
        chmod +x scripts/release-preflight.sh

        git add -A
        git commit -q -m "initial"
    )
    echo "$tmpdir"
}

# Run preflight inside a test repo, capturing exit code and output.
# Usage: run_preflight <repo_dir> [args...]
# Sets: PREFLIGHT_EXIT, PREFLIGHT_OUTPUT
run_preflight() {
    local repo="$1"
    shift
    local tmpfile
    tmpfile="$(mktemp)"
    set +e
    (cd "$repo" && bash scripts/release-preflight.sh "$@" 2>&1) > "$tmpfile"
    PREFLIGHT_EXIT=$?
    set -e
    PREFLIGHT_OUTPUT="$(cat "$tmpfile")"
    rm -f "$tmpfile"
}

cleanup_test_repo() {
    rm -rf "$1"
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
ec=0
bash "$PREFLIGHT" --bogus-flag >/dev/null 2>&1 || ec=$?
if [[ $ec -eq 2 ]]; then
    pass "Unknown flag exits 2"
else
    fail "Unknown flag should exit 2, got $ec"
fi

# ── Test 3: --version without argument exits 2 ──────────────────────────────
echo "Test: --version without argument exits 2"
ec=0
bash "$PREFLIGHT" --version >/dev/null 2>&1 || ec=$?
if [[ $ec -eq 2 ]]; then
    pass "--version without arg exits 2"
else
    fail "--version without arg should exit 2, got $ec"
fi

# ── Test 4: Invalid version syntax exits 2 ───────────────────────────────────
echo "Test: Invalid version syntax exits 2"
all_bad_pass=true
for bad_ver in "banana" "1.2" "1.2.3.4" "0.1" "v1.0.0" "1.0.0-beta"; do
    ec=0
    bash "$PREFLIGHT" --version "$bad_ver" >/dev/null 2>&1 || ec=$?
    if [[ $ec -ne 2 ]]; then
        fail "Version '$bad_ver' should exit 2, got $ec"
        all_bad_pass=false
    fi
done
# Empty string case
ec=0
bash "$PREFLIGHT" --version "" >/dev/null 2>&1 || ec=$?
if [[ $ec -ne 2 ]]; then
    fail "Empty version should exit 2, got $ec"
    all_bad_pass=false
fi
if [[ "$all_bad_pass" == "true" ]]; then
    pass "All invalid version strings exit 2"
fi

# ── Test 5: Valid version syntax accepted ─────────────────────────────────────
echo "Test: Valid version syntax accepted"
all_good_pass=true
for good_ver in "0.1.0" "1.2.3" "10.20.30"; do
    # Use --help to avoid running the full preflight
    output="$(bash "$PREFLIGHT" --version "$good_ver" --help 2>&1 || true)"
    if echo "$output" | grep -q "not valid semver"; then
        fail "Version '$good_ver' should be accepted"
        all_good_pass=false
    fi
done
if [[ "$all_good_pass" == "true" ]]; then
    pass "All valid version strings accepted"
fi

# ── Test 6: Dirty tree refused by default (isolated repo) ────────────────────
echo "Test: Dirty tree refused by default"
REPO="$(create_test_repo)"
(cd "$REPO" && echo "dirty" > untracked_file.txt)
run_preflight "$REPO" --verify-only
if echo "$PREFLIGHT_OUTPUT" | grep -q "Working tree has uncommitted changes"; then
    pass "Dirty tree refused by default"
else
    fail "Dirty tree should be refused"
fi
cleanup_test_repo "$REPO"

# ── Test 7: --allow-dirty permits dirty tree (isolated repo) ────────────────
echo "Test: --allow-dirty permits dirty tree"
REPO="$(create_test_repo)"
(cd "$REPO" && echo "dirty" > untracked_file.txt)
run_preflight "$REPO" --allow-dirty --verify-only --source-only
if echo "$PREFLIGHT_OUTPUT" | grep -q "Working tree has uncommitted changes"; then
    fail "--allow-dirty should permit dirty tree"
elif echo "$PREFLIGHT_OUTPUT" | grep -q "Mori Lens Release Preflight"; then
    pass "--allow-dirty permits dirty tree"
else
    fail "--allow-dirty should show the preflight banner"
fi
cleanup_test_repo "$REPO"

# ── Test 8: Clean tree passes dirty check (isolated repo) ────────────────────
echo "Test: Clean tree passes dirty check"
REPO="$(create_test_repo)"
run_preflight "$REPO" --verify-only --source-only
if echo "$PREFLIGHT_OUTPUT" | grep -q "Working tree has uncommitted changes"; then
    fail "Clean tree should not trigger dirty error"
else
    pass "Clean tree passes dirty check"
fi
cleanup_test_repo "$REPO"

# ── Test 9: --source-only skips platform checks ─────────────────────────────
echo "Test: --source-only skips platform checks"
REPO="$(create_test_repo)"
run_preflight "$REPO" --source-only
if echo "$PREFLIGHT_OUTPUT" | grep -q "PACKAGE READINESS NOT VERIFIED"; then
    pass "--source-only reports package readiness not verified"
else
    fail "--source-only should report package readiness not verified"
fi
cleanup_test_repo "$REPO"

# ── Test 10: Version mismatch fails (isolated repo) ─────────────────────────
echo "Test: Version mismatch calls gate_fail"
REPO="$(create_test_repo)"
# Modify apps/lens/Cargo.toml to have a different version (creates dirty tree)
(cd "$REPO" && printf '[package]\nname = "lens"\nversion = "0.2.0"\n' > apps/lens/Cargo.toml)
# --allow-dirty needed because we modified Cargo.toml without committing
run_preflight "$REPO" --allow-dirty --source-only
if echo "$PREFLIGHT_OUTPUT" | grep -q "VERSION_CONSISTENCY:"; then
    if echo "$PREFLIGHT_OUTPUT" | grep "VERSION_CONSISTENCY:" | grep -q "FAIL"; then
        pass "Version mismatch reports VERSION_CONSISTENCY: FAIL"
    else
        fail "Version mismatch should report FAIL, got: $(echo "$PREFLIGHT_OUTPUT" | grep "VERSION_CONSISTENCY:")"
    fi
else
    fail "Summary should contain VERSION_CONSISTENCY line"
fi
# The exit code should be 1 (gate failure)
if [[ $PREFLIGHT_EXIT -eq 1 ]]; then
    pass "Version mismatch exits 1"
else
    fail "Version mismatch should exit 1, got $PREFLIGHT_EXIT"
fi
cleanup_test_repo "$REPO"

# ── Test 11: Missing NSIS artifact fails (Windows only) ──────────────────────
if [[ "$(uname -s)" == "Windows"* || "$(uname -s)" == "MINGW"* ]]; then
    echo "Test: Missing NSIS artifact fails"
    REPO="$(create_test_repo)"
    # Create MSIX directory and artifact but no NSIS
    mkdir -p "$REPO/target/msix"
    touch "$REPO/target/msix/MoriLens_0.1.0.0_x64.msix"
    run_preflight "$REPO" --allow-dirty --verify-only
    if echo "$PREFLIGHT_OUTPUT" | grep -q "NSIS_BUILD.*MISSING\|NSIS_VERIFY.*FAIL"; then
        pass "Missing NSIS artifact reports MISSING/FAIL"
    else
        fail "Missing NSIS should report MISSING or FAIL"
    fi
    if [[ $PREFLIGHT_EXIT -ne 0 ]]; then
        pass "Missing NSIS exits non-zero"
    else
        fail "Missing NSIS should exit non-zero"
    fi
    cleanup_test_repo "$REPO"
else
    echo "Test: Missing NSIS artifact fails (skipped: not Windows)"
fi

# ── Test 12: Missing MSIX artifact fails (Windows only) ──────────────────────
if [[ "$(uname -s)" == "Windows"* || "$(uname -s)" == "MINGW"* ]]; then
    echo "Test: Missing MSIX artifact fails"
    REPO="$(create_test_repo)"
    # Create NSIS directory and artifact but no MSIX
    mkdir -p "$REPO/target/nsis"
    touch "$REPO/target/nsis/Mori Lens_0.1.0_x64-setup.exe"
    run_preflight "$REPO" --allow-dirty --verify-only
    if echo "$PREFLIGHT_OUTPUT" | grep -q "MSIX_BUILD.*MISSING\|MSIX_VERIFY.*FAIL"; then
        pass "Missing MSIX artifact reports MISSING/FAIL"
    else
        fail "Missing MSIX should report MISSING or FAIL"
    fi
    if [[ $PREFLIGHT_EXIT -ne 0 ]]; then
        pass "Missing MSIX exits non-zero"
    else
        fail "Missing MSIX should exit non-zero"
    fi
    cleanup_test_repo "$REPO"
else
    echo "Test: Missing MSIX artifact fails (skipped: not Windows)"
fi

# ── Test 13: Script structure verification ───────────────────────────────────
echo "Test: Script structure verification"
if grep -q "STRUCTURALLY READY" "$PREFLIGHT"; then
    pass "Uses STRUCTURALLY READY terminology"
else
    fail "Should use STRUCTURALLY READY terminology"
fi

if grep -q "VERSION_CONSISTENCY" "$PREFLIGHT"; then
    pass "Summary includes VERSION_CONSISTENCY"
else
    fail "Summary should include VERSION_CONSISTENCY"
fi

if grep -q "source-only" "$PREFLIGHT"; then
    pass "Supports --source-only flag"
else
    fail "Should support --source-only flag"
fi

if grep -q "REVISION_PROVENANCE" "$PREFLIGHT"; then
    pass "Summary includes REVISION_PROVENANCE"
else
    fail "Summary should include REVISION_PROVENANCE"
fi

# Check deterministic artifact naming (no find | head -1)
if grep -q 'NSIS_EXPECTED=' "$PREFLIGHT"; then
    pass "NSIS uses exact version-matched artifact name"
else
    fail "NSIS should use exact version-matched artifact name"
fi

if grep -q 'MSIX_EXPECTED=' "$PREFLIGHT"; then
    pass "MSIX uses exact version-matched artifact name"
else
    fail "MSIX should use exact version-matched artifact name"
fi

# Verify no 'find ... | head -1' pattern remains
if grep -q 'find.*head -1' "$PREFLIGHT"; then
    fail "Should not use 'find | head -1' for artifact selection"
else
    pass "No nondeterministic find|head-1 for artifacts"
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
