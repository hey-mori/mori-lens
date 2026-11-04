#!/usr/bin/env bash
#
# Verify that recent commit messages follow docs/commit-format.md.
#
# Checks the commits between HEAD and the merge-base with origin/main,
# or the last commit if already on main. Designed to run in CI and
# locally before push.
#
# Delegates to scripts/validate-commit-message.sh (single source of truth).
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
  # On main with no branch commits: pass. The pre-push hook in
  # .githooks/pre-push is the real gate that prevents direct pushes
  # to main. This check is for branch commits before merge.
  echo "commit-policy: on main, no branch commits to check"
  echo "commit-policy: pre-push hook prevents future direct pushes"
  exit 0
fi

failed=0

while IFS= read -r sha; do
  # A bot's commit (Dependabot's, for one) carries the message GitHub writes for it, which the format cannot ask it to change, so it is skipped.
  case "$(git log -1 --format='%ae' "$sha")" in
    *"[bot]@users.noreply.github.com") continue ;;
  esac
  msg="$(git log -1 --format='%B' "$sha")"
  subject="$(echo "$msg" | head -1)"
  errors="$(echo "$msg" | bash scripts/validate-commit-message.sh --stdin 2>&1)" || true

  if [ -n "$errors" ]; then
    echo "commit-policy: FAIL $(echo "$sha" | cut -c1-8) ${subject}"
    echo "$errors"
    failed=1
  fi
done <<< "$commits"

if [ "$failed" -eq 0 ]; then
  count="$(echo "$commits" | wc -l | tr -d ' ')"
  echo "commit-policy: ${count} commit(s) OK"
fi

exit "$failed"
