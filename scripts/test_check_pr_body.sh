#!/usr/bin/env bash
#
# Regression tests for scripts/check-pr-body.sh.
#
# Uses fixture files and a mock `gh` command so the tests need no
# network access. Each test writes a fixture, prepends a directory
# with a fake `gh` to PATH, and runs the checker against it.
#
# Usage:
#   scripts/test_check_pr_body.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECKER="$SCRIPT_DIR/check-pr-body.sh"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

tests_passed=0
tests_failed=0

pass() {
  tests_passed=$((tests_passed + 1))
  printf '  PASS: %s\n' "$1"
}

fail() {
  tests_failed=$((tests_failed + 1))
  printf '  FAIL: %s\n' "$1"
}

# Create a fixture file and a matching mock `gh` that prints its
# body as `gh pr view <N> --json body --jq .body` would.
make_fixture() {
  local n="$1"
  local content="$2"
  printf '%s' "$content" > "$tmpdir/body_$n.txt"
  mkdir -p "$tmpdir/mock_$n"
  cat > "$tmpdir/mock_$n/gh" <<MOCK
#!/usr/bin/env bash
cat '$tmpdir/body_$n.txt'
MOCK
  chmod +x "$tmpdir/mock_$n/gh"
}

# ── Test 1: Clean body exits 0 ───────────────────────────────────────────────
echo "Test: Clean body exits 0"
make_fixture 1 "This PR fixes the widget alignment issue."
output="$(PATH="$tmpdir/mock_1:$PATH" bash "$CHECKER" 1 2>&1)" && rc=0 || rc=$?
if [[ $rc -eq 0 ]]; then
  pass "clean body exits 0"
else
  fail "clean body should exit 0, got $rc"
  printf '    output: %s\n' "$output"
fi

# ── Test 2: Robot + Generated with exits 1 ───────────────────────────────────
echo "Test: Body with robot + Generated with exits 1"
make_fixture 2 "🤖 Generated with Claude Code (https://claude.ai/code)"
output="$(PATH="$tmpdir/mock_2:$PATH" bash "$CHECKER" 2 2>&1)" && rc=0 || rc=$?
if [[ $rc -eq 1 ]]; then
  pass "robot + generated with exits 1"
else
  fail "should exit 1, got $rc"
fi
if echo "$output" | grep -qi "Generated with"; then
  pass "prints 'Generated with' match"
else
  fail "should print 'Generated with' match"
fi

# ── Test 3: Co-Authored-By exits 1 ───────────────────────────────────────────
echo "Test: Body with Co-Authored-By exits 1"
make_fixture 3 "Summary of changes.

Co-Authored-By: Claude <noreply@anthropic.com>"
output="$(PATH="$tmpdir/mock_3:$PATH" bash "$CHECKER" 3 2>&1)" && rc=0 || rc=$?
if [[ $rc -eq 1 ]]; then
  pass "co-authored-by exits 1"
else
  fail "should exit 1, got $rc"
fi
if echo "$output" | grep -qi "Co-Authored-By"; then
  pass "prints 'Co-Authored-By' match"
else
  fail "should print 'Co-Authored-By' match"
fi

# ── Test 4: Multiple violations exits 1 and prints all ───────────────────────
echo "Test: Multiple violations exits 1 and prints all matches"
make_fixture 4 "🤖 Generated with Claude Code

Co-Authored-By: Claude <noreply@anthropic.com>

AI-assisted changes to the parser.
Also uses Qwen for fallback OCR."
output="$(PATH="$tmpdir/mock_4:$PATH" bash "$CHECKER" 4 2>&1)" && rc=0 || rc=$?
if [[ $rc -eq 1 ]]; then
  pass "multiple violations exits 1"
else
  fail "should exit 1, got $rc"
fi
found=0
for token in "Co-Authored-By" "Generated with" "AI-assisted" "Claude Code" "Qwen"; do
  if echo "$output" | grep -qiF "$token"; then
    found=$((found + 1))
  fi
done
if [[ $found -eq 5 ]]; then
  pass "all 5 distinct tokens reported"
else
  fail "expected 5 distinct tokens reported, got $found"
  printf '    output: %s\n' "$output"
fi

# ── Test 5: Empty body exits 0 ───────────────────────────────────────────────
echo "Test: Empty body exits 0"
make_fixture 5 ""
output="$(PATH="$tmpdir/mock_5:$PATH" bash "$CHECKER" 5 2>&1)" && rc=0 || rc=$?
if [[ $rc -eq 0 ]]; then
  pass "empty body exits 0"
else
  fail "empty body should exit 0, got $rc"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "========================================="
printf 'Results: %d passed, %d failed\n' "$tests_passed" "$tests_failed"
echo "========================================="

if [[ $tests_failed -gt 0 ]]; then
  echo "FAILED"
  exit 1
else
  echo "ALL TESTS PASSED"
  exit 0
fi
