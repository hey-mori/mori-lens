#!/usr/bin/env bash
#
# Tests for scripts/validate-commit-message.sh.
#
# The validator is the single source of truth three callers share: the
# commit-msg hook, the pre-push hook, and scripts/check-commit-policy.sh. It had
# no tests, and the gap it is being given one for is the kind a reader cannot see
# by reading it: rule 9 skips blank lines, so a body whose bullets are separated
# by them passed every caller. 17 commits in this repository have that shape.
#
# Run from the repository root:
#   bash .githooks/tests/test-validate-commit-message.sh
set -euo pipefail

VALIDATOR="$(cd "$(dirname "$0")/../.." && pwd)/scripts/validate-commit-message.sh"
PASS=0
FAIL=0

# Assert the validator's exit code for a message.
# Usage: expect <accept|reject> <description> <message>
expect() {
  local mode="$1" desc="$2" msg="$3"
  local want rc=0
  case "$mode" in
    accept) want=0 ;;
    reject) want=1 ;;
    *) echo "  FAIL: bad mode $mode"; FAIL=$((FAIL + 1)); return ;;
  esac
  printf '%s' "$msg" | bash "$VALIDATOR" --stdin >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then
    echo "  PASS: $desc"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $desc (wanted exit $want, got $rc)"
    FAIL=$((FAIL + 1))
  fi
}

echo "Shape"
expect accept "a subject-only commit, which a trivial change may be" \
  'chore: ignore the release staging directory'
expect accept "consecutive bullets" \
  'fix: do a thing

- Change one thing, and say why
- Change another thing, and say why'
expect reject "no type prefix" \
  'do a thing'
expect reject "an unknown type" \
  'build: do a thing'
expect reject "a scope on the subject" \
  'fix(hover): do a thing'
expect reject "a trailing period" \
  'fix: do a thing.'
expect reject "a subject over 72 characters" \
  'fix: do a thing that is described at really quite considerable and excessive length'

echo
echo "Bullet spacing"
expect reject "a blank line between two bullets" \
  'fix: do a thing

- Change one thing

- Change another thing'
expect reject "a blank line between the second and third bullets" \
  'fix: do a thing

- Change one thing
- Change another thing

- Change a third thing'
expect accept "a trailing blank line after the last bullet" \
  'fix: do a thing

- Change one thing
- Change another thing
'
expect reject "a wrapped bullet continuation line" \
  'fix: do a thing

- Change one thing, in a sentence that carries on
  onto a second line'

echo
echo "Attribution"
expect reject "a Co-Authored-By trailer" \
  'fix: do a thing

- Change one thing
Co-Authored-By: Someone <someone@example.com>'
expect reject "a generated-with line" \
  'fix: do a thing

- Generated with some tool'
expect reject "an AI-assisted line" \
  'fix: do a thing

- AI-assisted change'

echo "Merge commits"
expect accept "a GitHub pull request merge with the PR title as its body" \
  'Merge pull request #661 from hey-mori/docs/security-report-channel

docs: point private reports at the organization form while private'
expect accept "main merged into a branch by git" \
  "Merge remote-tracking branch 'origin/main' into fix/workspace-linux-gate"
expect accept "a local branch merge" \
  "Merge branch 'main' into docs/a-very-long-branch-name-that-pushes-the-subject-past-72"
expect reject "a merge that carries a Co-Authored-By trailer" \
  'Merge pull request #1 from hey-mori/x

Co-Authored-By: Someone <someone@example.com>'
expect reject "a subject that only starts with the word Merge" \
  'Merge the two settings screens'

echo
echo "Result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
