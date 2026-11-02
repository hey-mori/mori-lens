#!/usr/bin/env bash
#
# Tests for .githooks/pre-push.
#
# Creates a temporary git repository to exercise the hook's two rules:
#   1. Reject pushes to refs/heads/main.
#   2. Reject non-fast-forward pushes to any other branch.
#
# Run from the repository root:
#   bash .githooks/tests/test-pre-push.sh
set -euo pipefail

HOOK="$(cd "$(dirname "$0")/../.." && pwd)/.githooks/pre-push"
PASS=0
FAIL=0

cleanup() {
  rm -rf "$TMPDIR"
}

die() {
  echo "  FAIL: $*"
  FAIL=$((FAIL + 1))
}

ok() {
  echo "  PASS: $*"
  PASS=$((PASS + 1))
}

# Feed stdin lines to the hook and check exit code.
# Usage: run_hook <expected_exit> <stdin_lines...>
run_hook() {
  local expected="$1"; shift
  local input=""
  for line in "$@"; do
    input+="$line"$'\n'
  done
  local rc=0
  printf '%s' "$input" | GIT_DIR="$TESTREPO" bash "$HOOK" origin http://example 2>/dev/null || rc=$?
  if [ "$rc" -eq "$expected" ]; then
    return 0
  else
    return 1
  fi
}

TMPDIR="$(mktemp -d)"
trap cleanup EXIT

# Set up a test repository with known commits.
TESTREPO="$TMPDIR/repo/.git"
mkdir -p "$TMPDIR/repo"
git init -q -b main "$TMPDIR/repo"
git -C "$TMPDIR/repo" config user.email "test@test"
git -C "$TMPDIR/repo" config user.name "Test"

# Create the initial commit on main.
echo "a" > "$TMPDIR/repo/a.txt"
git -C "$TMPDIR/repo" add a.txt
git -C "$TMPDIR/repo" commit -q -m "test: add the initial file"
BASE_SHA=$(git -C "$TMPDIR/repo" rev-parse HEAD)

# Create a second commit (fast-forward from BASE).
echo "b" > "$TMPDIR/repo/b.txt"
git -C "$TMPDIR/repo" add b.txt
git -C "$TMPDIR/repo" commit -q -m "test: add the second file"
FF_SHA=$(git -C "$TMPDIR/repo" rev-parse HEAD)

# Create a divergent commit (non-fast-forward from FF_SHA).
# We reset to BASE, then make a different commit.
git -C "$TMPDIR/repo" checkout -q -b divergent "$BASE_SHA"
echo "c" > "$TMPDIR/repo/c.txt"
git -C "$TMPDIR/repo" add c.txt
git -C "$TMPDIR/repo" commit -q -m "test: add the divergent file"
DIVERGE_SHA=$(git -C "$TMPDIR/repo" rev-parse HEAD)

# Go back to main so merge-base works across refs.
git -C "$TMPDIR/repo" checkout -q main

ZERO="0000000000000000000000000000000000000000"

echo "Testing pre-push hook..."

# Test 1: push to main is rejected.
if run_hook 1 "refs/heads/main $FF_SHA refs/heads/main $BASE_SHA"; then
  ok "push to main is rejected"
else
  die "push to main should be rejected (expected exit 1)"
fi

# Test 2: fast-forward push to a feature branch is allowed.
if run_hook 0 "refs/heads/feature $FF_SHA refs/heads/feature $BASE_SHA"; then
  ok "fast-forward push to feature branch is allowed"
else
  die "fast-forward push should be allowed (expected exit 0)"
fi

# Test 3: non-fast-forward push is rejected.
if run_hook 1 "refs/heads/feature $DIVERGE_SHA refs/heads/feature $FF_SHA"; then
  ok "non-fast-forward push is rejected"
else
  die "non-fast-forward push should be rejected (expected exit 1)"
fi

# Test 4: new branch push (remote sha is zero) is allowed.
if run_hook 0 "refs/heads/new-branch $FF_SHA refs/heads/new-branch $ZERO"; then
  ok "new branch push (zero remote sha) is allowed"
else
  die "new branch push should be allowed (expected exit 0)"
fi

# Test 5: branch deletion (local sha is zero) is allowed.
if run_hook 0 "refs/heads/old-branch $ZERO refs/heads/old-branch $FF_SHA"; then
  ok "branch deletion (zero local sha) is allowed"
else
  die "branch deletion should be allowed (expected exit 0)"
fi

# Test 6: multiple refs in one push, main among them, is rejected.
if run_hook 1 "refs/heads/feature $FF_SHA refs/heads/feature $BASE_SHA" "refs/heads/main $FF_SHA refs/heads/main $BASE_SHA"; then
  ok "push with main among multiple refs is rejected"
else
  die "push with main among multiple refs should be rejected (expected exit 1)"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
