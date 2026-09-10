#!/usr/bin/env bash
#
# Verify that recent commit messages follow docs/commit-format.md.
#
# Checks the commits between HEAD and the merge-base with origin/main,
# or the last commit if already on main. Designed to run in CI and
# locally before push.
#
# Exit 0 if all messages pass, exit 1 with diagnostics otherwise.
set -euo pipefail

cd "$(dirname "$0")/.."

# Which commits to check: the ones on this branch that are not on main.
if git rev-parse origin/main >/dev/null 2>&1; then
  base="$(git merge-base origin/main HEAD 2>/dev/null || echo "")"
else
  base=""
fi

if [ -z "$base" ]; then
  # No remote or no merge-base: check just the last commit.
  commits="$(git rev-list -n 1 HEAD)"
else
  commits="$(git rev-list "${base}..HEAD")"
fi

if [ -z "$commits" ]; then
  echo "commit-policy: nothing to check (no commits ahead of main)"
  exit 0
fi

valid_types="feat|fix|perf|refactor|test|docs|chore"
failed=0

while IFS= read -r sha; do
  msg="$(git log -1 --format='%B' "$sha")"
  subject="$(echo "$msg" | head -1)"
  errors=""

  # 1. Subject must start with a valid type prefix.
  if ! echo "$subject" | grep -qE "^(${valid_types}): .+"; then
    errors="${errors}  - subject must start with '<type>: ' where type is one of: ${valid_types}\n"
  fi

  # 2. Subject must be under ~70 characters.
  if [ "${#subject}" -gt 72 ]; then
    errors="${errors}  - subject is ${#subject} chars (target: under 70)\n"
  fi

  # 3. No trailing period on subject.
  if echo "$subject" | grep -q '\.$'; then
    errors="${errors}  - subject must not end with a period\n"
  fi

  # 4. No Co-Authored-By lines.
  if echo "$msg" | grep -qi 'co-authored-by:'; then
    errors="${errors}  - Co-Authored-By is forbidden (docs/commit-format.md)\n"
  fi

  # 5. No "Generated with" lines.
  if echo "$msg" | grep -qi 'generated with'; then
    errors="${errors}  - 'Generated with' lines are forbidden\n"
  fi

  # 6. No emoji in subject (basic check for common emoji codepoints).
  # Using grep -P for Unicode properties if available, otherwise skip.
  if echo "$msg" | grep -qP '[\x{1F300}-\x{1F9FF}]' 2>/dev/null; then
    errors="${errors}  - emoji in commit messages is forbidden\n"
  fi

  # 7. Body must be bullet-only (every non-blank line after subject starts with "- ").
  # Skip the subject and the blank line after it.
  body="$(echo "$msg" | tail -n +3)"
  if [ -n "$body" ]; then
    while IFS= read -r line; do
      [ -z "$line" ] && continue
      if ! echo "$line" | grep -q '^- '; then
        errors="${errors}  - body line does not start with '- ': $(echo "$line" | head -c 60)\n"
        break
      fi
    done <<< "$body"
  fi

  if [ -n "$errors" ]; then
    echo "commit-policy: FAIL $(echo "$sha" | cut -c1-8) ${subject}"
    echo -e "$errors"
    failed=1
  fi
done <<< "$commits"

if [ "$failed" -eq 0 ]; then
  count="$(echo "$commits" | wc -l | tr -d ' ')"
  echo "commit-policy: ${count} commit(s) OK"
fi

exit "$failed"
