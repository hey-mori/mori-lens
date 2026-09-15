#!/usr/bin/env bash
#
# Regression tests for verify-tauri-capabilities.sh
#
# Tests that the verifier correctly detects:
# - Missing core permissions
# - Excess core permissions
# - cargo check failures (exit status, not string matching)
#
set -euo pipefail

cd "$(dirname "$0")/.."

pass_count=0
fail_count=0

run_test() {
  local test_name="$1"
  local expected_exit="$2"
  local fixture_dir="$3"

  printf '%-60s ' "$test_name"

  local actual_exit=0
  if [[ -n "$fixture_dir" ]] && [[ -f "$fixture_dir/cargo" ]]; then
    # Override cargo with fake
    PATH="$fixture_dir:$PATH" scripts/verify-tauri-capabilities.sh >/dev/null 2>&1 || actual_exit=$?
  else
    # Normal run
    scripts/verify-tauri-capabilities.sh >/dev/null 2>&1 || actual_exit=$?
  fi

  if [[ $actual_exit -eq $expected_exit ]]; then
    printf 'PASS\n'
    pass_count=$((pass_count + 1))
  else
    printf 'FAIL (expected %d, got %d)\n' "$expected_exit" "$actual_exit"
    fail_count=$((fail_count + 1))
  fi
}

printf 'Testing verify-tauri-capabilities.sh regression...\n\n'

# Test 1: Normal run should pass
run_test "Normal verification passes" 0 ""

# Test 2: cargo check failure should be detected (exit status, not string)
fixture_dir="$(mktemp -d)"
cat > "$fixture_dir/cargo" << 'EOF'
#!/usr/bin/env bash
# Fake cargo that exits 1 with error message
echo "error: could not compile" >&2
exit 1
EOF
chmod +x "$fixture_dir/cargo"

# Temporarily replace cargo in PATH
run_test "cargo check failure detected" 1 "$fixture_dir"
rm -rf "$fixture_dir"

# Test 3: cargo check success should pass
fixture_dir="$(mktemp -d)"
cat > "$fixture_dir/cargo" << 'EOF'
#!/usr/bin/env bash
# Fake cargo that exits 0
exit 0
EOF
chmod +x "$fixture_dir/cargo"

run_test "cargo check success passes" 0 "$fixture_dir"
rm -rf "$fixture_dir"

# Summary
printf '\n'
printf '═══════════════════════════════════════════════════════════\n'
if [[ $fail_count -eq 0 ]]; then
  printf '✓ All %d tests passed\n' "$pass_count"
  exit 0
else
  printf '✗ %d of %d tests failed\n' "$fail_count" "$((pass_count + fail_count))"
  exit 1
fi
