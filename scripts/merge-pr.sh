#!/usr/bin/env bash
#
# Safe merge wrapper for pull requests.
#
# This script enforces the merge gate by running all checks before merging:
# 1. scripts/check.sh --full (code quality)
# 2. scripts/check-pr-ready.sh (PR readiness)
# 3. Final PR body re-check (catch late edits)
# 4. gh pr merge (actual merge)
# 5. Postcondition checks (verify merged state)
#
# Usage:
#   scripts/merge-pr.sh <pr-number>
#
# Exit codes:
#   0 - PR merged successfully
#   1 - Checks failed or merge failed
#   2 - Script error
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ne 1 ]]; then
  printf 'usage: %s <pr-number>\n' "$(basename "$0")"
  exit 2
fi

pr="$1"

printf '=== Safe Merge Wrapper for PR #%s ===\n\n' "$pr"

# Step 1: Run full check suite
printf 'Step 1: Running full check suite...\n'
if ! scripts/check.sh --full; then
  printf '\n✗ check.sh failed. Merge aborted.\n'
  exit 1
fi
printf '✓ check.sh passed\n\n'

# Step 2: Run PR readiness check
printf 'Step 2: Running PR readiness check...\n'
if ! scripts/check-pr-ready.sh "$pr"; then
  printf '\n✗ check-pr-ready.sh failed. Merge aborted.\n'
  exit 1
fi
printf '✓ check-pr-ready.sh passed\n\n'

# Step 3: Final PR body re-check (fetch fresh from GitHub)
printf 'Step 3: Final PR body verification...\n'
if ! scripts/check-pr-body.sh "$pr"; then
  printf '\n✗ PR body check failed. Merge aborted.\n'
  exit 1
fi
printf '✓ PR body is clean\n\n'

# Step 4: Get PR info before merge
printf 'Step 4: Gathering PR info...\n'
pr_title="$(gh pr view "$pr" --json title --jq '.title')"
pr_branch="$(gh pr view "$pr" --json headRefName --jq '.headRefName')"
printf '  Title: %s\n' "$pr_title"
printf '  Branch: %s\n\n' "$pr_branch"

# Step 5: Merge the PR
printf 'Step 5: Merging PR #%s...\n' "$pr"
if ! gh pr merge "$pr" --merge --delete-branch; then
  printf '\n✗ Merge failed.\n'
  exit 1
fi
printf '✓ PR merged\n\n'

# Step 6: Postcondition checks
printf 'Step 6: Postcondition verification...\n'

# Verify PR state is MERGED
pr_state="$(gh pr view "$pr" --json state --jq '.state')"
if [[ "$pr_state" != "MERGED" ]]; then
  printf '✗ PR state is %s, expected MERGED\n' "$pr_state"
  exit 1
fi
printf '  ✓ PR state: MERGED\n'

# Verify branch was deleted (if --delete-branch was used)
if gh api "repos/{owner}/{repo}/branches/$pr_branch" >/dev/null 2>&1; then
  printf '  ⚠ Branch %s still exists (may take time to delete)\n' "$pr_branch"
else
  printf '  ✓ Branch deleted\n'
fi

# Final PR body check (post-merge)
if scripts/check-pr-body.sh "$pr" >/dev/null 2>&1; then
  printf '  ✓ Merged PR body is clean\n'
else
  printf '  ⚠ Merged PR body has issues (cannot undo merge)\n'
  printf '    This should not happen if pre-merge checks passed.\n'
fi

printf '\n✓ PR #%s merged successfully\n' "$pr"
printf '\nNext steps:\n'
printf '  - git pull origin main\n'
printf '  - Update related issues\n'
