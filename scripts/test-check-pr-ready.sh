#!/usr/bin/env bash
#
# Regression tests for check-pr-ready.sh commit validation
#
# Tests that the script correctly validates full commit messages
# (subject + body) using the production validator.
#
# Usage:
#   scripts/test-check-pr-ready.sh
#
# Exit codes:
#   0 - All tests pass
#   1 - One or more tests failed
set -euo pipefail

cd "$(dirname "$0")/.."

passed=0
failed=0

test_commit() {
  local desc="$1"
  local message="$2"
  local should_pass="$3"

  printf 'Testing: %s... ' "$desc"

  if printf '%b\n' "$message" | scripts/validate-commit-message.sh --stdin >/dev/null 2>&1; then
    if [[ $should_pass -eq 1 ]]; then
      printf 'PASS\n'
      passed=$((passed + 1))
    else
      printf 'FAIL (should have failed but passed)\n'
      failed=$((failed + 1))
    fi
  else
    if [[ $should_pass -eq 0 ]]; then
      printf 'PASS\n'
      passed=$((passed + 1))
    else
      printf 'FAIL (should have passed but failed)\n'
      failed=$((failed + 1))
    fi
  fi
}

printf 'Running check-pr-ready.sh regression tests...\n\n'

# Test 1: Clean full commit (subject + body)
test_commit "clean full commit" \
  "fix: correct validation logic\n\n- Fixed bug in parser\n- Added tests" \
  1

# Test 2: Clean subject + forbidden Co-Authored-By in body
test_commit "forbidden Co-Authored-By in body" \
  "fix: clean subject\n\nCo-Authored-By: Claude <noreply@anthropic.com>" \
  0

# Test 3: Clean subject + Generated with in body
test_commit "forbidden Generated with in body" \
  "fix: clean subject\n\n🤖 Generated with Claude Code" \
  0

# Test 4: perf type is valid
test_commit "perf type is valid" \
  "perf: optimize query performance" \
  1

# Test 5: Invalid type
test_commit "invalid type" \
  "invalid: this should fail" \
  0

# Test 6: Subject too long (72+ chars)
test_commit "subject too long" \
  "fix: this subject line is way too long and should be rejected by the validator because it exceeds the maximum length" \
  0

# Test 7: Trailing period in subject
test_commit "trailing period in subject" \
  "fix: add validation." \
  0

# Test 8: AI product attribution in body
test_commit "AI product attribution in body" \
  "fix: clean subject\n\nThis was AI-assisted development" \
  0

# Test 9: Robot emoji in body
test_commit "robot emoji in body" \
  "fix: clean subject\n\n🤖" \
  0

# Test 10: Clean subject-only (no body)
test_commit "clean subject-only" \
  "docs: update README" \
  1

# Summary
printf '\n'
if [[ $failed -eq 0 ]]; then
  printf '✓ All %d tests passed\n' "$passed"
  exit 0
else
  printf '✗ %d of %d tests failed\n' "$failed" "$((passed + failed))"
  exit 1
fi
