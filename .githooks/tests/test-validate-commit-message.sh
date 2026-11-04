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

echo
echo "DCO sign-off"
expect accept "bullets followed by a sign-off block" \
  'fix: do a thing

- Change one thing
- Change another thing

Signed-off-by: Some One <someone@example.com>'
expect accept "a subject-only commit with a sign-off" \
  'chore: ignore the release staging directory

Signed-off-by: Some One <someone@example.com>'
expect accept "two sign-offs in the final block" \
  'fix: do a thing

- Change one thing

Signed-off-by: Some One <someone@example.com>
Signed-off-by: Other Person <other@example.com>'
expect reject "a sign-off with no blank line before it" \
  'fix: do a thing

- Change one thing
Signed-off-by: Some One <someone@example.com>'
expect reject "a sign-off between bullets" \
  'fix: do a thing

- Change one thing

Signed-off-by: Some One <someone@example.com>

- Change another thing'
expect reject "a sign-off without an email address" \
  'fix: do a thing

- Change one thing

Signed-off-by: Some One'
expect reject "a Co-Authored-By trailer beside a sign-off" \
  'fix: do a thing

- Change one thing

Signed-off-by: Some One <someone@example.com>
Co-Authored-By: Someone <someone@example.com>'

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

echo "Emoji"
# The emoji rule once used grep -P, which needs a UTF-8 locale and which the
# BSD grep on macOS lacks. Both failures were silent and let every emoji
# through, so run the cases under each condition.
real_grep="$(command -v grep)"
no_p_grep="$(mktemp -d)"
printf '#!/usr/bin/env bash\nfor a in "$@"; do case "$a" in -*P*) echo "grep: invalid option -- P" >&2; exit 2 ;; esac; done\nexec "%s" "$@"\n' "$real_grep" >"$no_p_grep/grep"
chmod +x "$no_p_grep/grep"
saved_path="$PATH"
saved_lc_all="${LC_ALL-}"
for setting in "UTF-8 locale" "C locale" "a grep without -P"; do
  case "$setting" in
    "UTF-8 locale") export LC_ALL=C.UTF-8 PATH="$saved_path" ;;
    "C locale") export LC_ALL=C PATH="$saved_path" ;;
    "a grep without -P") export LC_ALL=C.UTF-8 PATH="$no_p_grep:$saved_path" ;;
  esac
  expect reject "a rocket, U+1F680, with $setting" $'docs: add a note \xf0\x9f\x9a\x80'
  expect reject "a check mark button, U+2705, with $setting" $'docs: add a note \xe2\x9c\x85'
  expect reject "a potted plant, U+1FAB4, with $setting" $'docs: add a note \xf0\x9f\xaa\xb4'
  expect accept "Korean, an arrow, a middle dot and an en dash, with $setting" \
    $'docs: note the 3\xe2\x80\x937 range \xc2\xb7 \xe2\x86\x92 \xed\x95\x9c\xea\xb8\x80'
done
export PATH="$saved_path"
if [ -n "$saved_lc_all" ]; then export LC_ALL="$saved_lc_all"; else unset LC_ALL; fi
rm -rf "$no_p_grep"

echo
echo "Result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
