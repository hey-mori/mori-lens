#!/usr/bin/env bash
#
# Test check-pr-ready.sh with deterministic fixtures via fake `gh` CLI.
#
# Tests the actual production script by injecting a fake gh executable
# that returns controlled responses for each test scenario.
#
set -euo pipefail

cd "$(dirname "$0")/.."

pass_count=0
fail_count=0
test_count=0

# Create the fake gh script template
FAKE_GH_TEMPLATE='#!/usr/bin/env bash
case "$*" in
  *"pr view"*"--json state"*)
    cat "$FIXTURE_DIR/state"
    ;;
  *"pr view"*"--json baseRefName"*)
    cat "$FIXTURE_DIR/base"
    ;;
  *"pr view"*"--json title"*)
    cat "$FIXTURE_DIR/title"
    ;;
  *"pr view"*"--json body"*)
    cat "$FIXTURE_DIR/body"
    ;;
  *"pr view"*"--json headRefName"*)
    cat "$FIXTURE_DIR/head"
    ;;
  *"pr view"*"--json commits"*"--jq .commits[].oid"*)
    cat "$FIXTURE_DIR/oids"
    ;;
  *"pr view"*"--json mergeable"*)
    cat "$FIXTURE_DIR/mergeable"
    ;;
  *"api repos/"*"commits/"*)
    # Extract oid from the commits/<oid> path segment
    for arg in "$@"; do
      case "$arg" in
        */commits/*) oid="${arg##*/commits/}"; break ;;
      esac
    done
    if [[ -f "$FIXTURE_DIR/commit_$oid" ]]; then
      cat "$FIXTURE_DIR/commit_$oid"
    else
      echo "fake gh: no fixture for commit $oid" >&2
      exit 1
    fi
    ;;
  *)
    echo "fake gh: unhandled: $*" >&2
    exit 1
    ;;
esac
'

make_fixture() {
  local dir="$1"
  mkdir -p "$dir/bin"
  printf '%s' "$FAKE_GH_TEMPLATE" > "$dir/bin/gh"
  chmod +x "$dir/bin/gh"
  # Defaults
  printf 'OPEN' > "$dir/state"
  printf 'main' > "$dir/base"
  printf 'test-branch' > "$dir/head"
  printf 'MERGEABLE' > "$dir/mergeable"
  printf 'clean body' > "$dir/body"
}

run_case() {
  local test_name="$1"
  local expected_exit="$2"
  local fixture_dir="$3"

  test_count=$((test_count + 1))
  printf '%2d. %-60s ' "$test_count" "$test_name"

  local actual_exit=0
  FIXTURE_DIR="$fixture_dir" PATH="$fixture_dir/bin:$PATH" \
    scripts/check-pr-ready.sh 999 >/dev/null 2>&1 || actual_exit=$?

  if [[ $actual_exit -eq $expected_exit ]]; then
    printf 'PASS\n'
    pass_count=$((pass_count + 1))
  else
    printf 'FAIL (expected %d, got %d)\n' "$expected_exit" "$actual_exit"
    fail_count=$((fail_count + 1))
  fi
}

printf 'Testing check-pr-ready.sh production path with fake gh...\n\n'

# ── Test 1: Clean single-commit PR ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: add feature' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\n- Added new functionality\n- Updated tests' > "$fdir/commit_abc123def456"
run_case "Clean single-commit PR" 0 "$fdir"
rm -rf "$fdir"

# ── Test 2: Clean multi-commit PR ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: multi-commit feature' > "$fdir/title"
printf 'abc123def456\ndef456ghi789' > "$fdir/oids"
printf 'feat: add initial implementation\n\n- Started work' > "$fdir/commit_abc123def456"
printf 'fix: address review comments\n\n- Fixed issues' > "$fdir/commit_def456ghi789"
run_case "Clean multi-commit PR" 0 "$fdir"
rm -rf "$fdir"

# ── Test 3: Commit with Co-Authored-By (forbidden) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: add feature' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\n- Added functionality\n\nCo-Authored-By: Claude <noreply@anthropic.com>' > "$fdir/commit_abc123def456"
run_case "Commit with Co-Authored-By → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 4: Commit with Generated with (forbidden) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: add feature' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\n- Added functionality\n\n🤖 Generated with Claude Code' > "$fdir/commit_abc123def456"
run_case "Commit with 'Generated with' → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 5: Valid body with multiple bullet lines ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'refactor: improve code quality' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'refactor: improve code quality\n\n- Refactored module A\n- Refactored module B\n- Refactored module C\n- Updated documentation' > "$fdir/commit_abc123def456"
run_case "Valid body with multiple bullets" 0 "$fdir"
rm -rf "$fdir"

# ── Test 6: Two commits with multiline bodies (parser correctness) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: two-part feature' > "$fdir/title"
printf 'abc123def456\ndef456ghi789' > "$fdir/oids"
printf 'feat: implement part one\n\n- First feature\n- Second feature\n- Third feature' > "$fdir/commit_abc123def456"
printf 'feat: implement part two\n\n- Fourth feature\n- Fifth feature' > "$fdir/commit_def456ghi789"
run_case "Two commits with multiline bodies" 0 "$fdir"
rm -rf "$fdir"

# ── Test 7: Invalid PR title (missing type) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'Add new feature' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\n- Added functionality' > "$fdir/commit_abc123def456"
run_case "Invalid PR title (no type) → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 8: Invalid PR title (uppercase type) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'FEAT: add feature' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\n- Added functionality' > "$fdir/commit_abc123def456"
run_case "Invalid PR title (uppercase) → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 9: Invalid PR title (trailing period) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: add feature.' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\n- Added functionality' > "$fdir/commit_abc123def456"
run_case "Invalid PR title (trailing period) → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 10: Second commit has forbidden pattern ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: multi-commit' > "$fdir/title"
printf 'abc123def456\ndef456ghi789' > "$fdir/oids"
printf 'feat: first commit\n\n- Added feature' > "$fdir/commit_abc123def456"
printf 'feat: second commit\n\n- More work\n\nGenerated with Claude Code' > "$fdir/commit_def456ghi789"
run_case "Second commit forbidden pattern → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 11: perf type PR title ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'perf: optimize query performance' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'perf: optimize query performance\n\n- Optimized database queries' > "$fdir/commit_abc123def456"
run_case "perf type PR title" 0 "$fdir"
rm -rf "$fdir"

# ── Test 12: Commit with prose body (forbidden) ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'feat: add feature' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'feat: add feature\n\nThis commit adds a new feature.' > "$fdir/commit_abc123def456"
run_case "Commit with prose body → FAIL" 1 "$fdir"
rm -rf "$fdir"

# ── Test 13: Valid body with blank line between bullets ──
fdir="$(mktemp -d)"
make_fixture "$fdir"
printf 'docs: update documentation' > "$fdir/title"
printf 'abc123def456' > "$fdir/oids"
printf 'docs: update documentation\n\n- Updated section A\n\n- Updated section B\n- Updated section C' > "$fdir/commit_abc123def456"
run_case "Valid body with blank line between bullets" 0 "$fdir"
rm -rf "$fdir"

# Summary
printf '\n'
printf '═══════════════════════════════════════════════════════════\n'
if [[ $fail_count -eq 0 ]]; then
  printf '✓ All %d tests passed\n' "$test_count"
  exit 0
else
  printf '✗ %d of %d tests failed\n' "$fail_count" "$test_count"
  exit 1
fi
