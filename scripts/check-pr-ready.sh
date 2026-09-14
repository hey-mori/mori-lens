#!/usr/bin/env bash
#
# Merge-readiness gate for pull requests.
#
# This script validates that a PR is ready to merge by checking:
# - PR exists and is open
# - Base branch is main
# - PR body attribution is clean (no forbidden AI markers)
# - PR title follows commit format
# - All commits have clean attribution
# - No unpushed commits on the branch
# - PR is mergeable (when queryable)
#
# Usage:
#   scripts/check-pr-ready.sh <pr-number>
#
# Exit codes:
#   0 - PR is ready to merge
#   1 - PR has issues that must be fixed
#   2 - Script error (missing args, network failure, etc.)
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ne 1 ]]; then
  printf 'usage: %s <pr-number>\n' "$(basename "$0")"
  exit 2
fi

pr="$1"
errors=0

printf 'Checking PR #%s merge readiness...\n\n' "$pr"

# 1. PR exists and is open
printf '1. PR state: '
pr_state="$(gh pr view "$pr" --json state --jq '.state' 2>&1)" || {
  printf 'FAILED (could not fetch PR)\n'
  printf '%s\n' "$pr_state"
  exit 2
}

if [[ "$pr_state" != "OPEN" ]]; then
  printf 'FAILED (PR is %s, not OPEN)\n' "$pr_state"
  errors=$((errors + 1))
else
  printf 'OK (OPEN)\n'
fi

# 2. Base branch is main
printf '2. Base branch: '
base="$(gh pr view "$pr" --json baseRefName --jq '.baseRefName' 2>&1)" || {
  printf 'FAILED (could not fetch base)\n'
  exit 2
}

if [[ "$base" != "main" ]]; then
  printf 'FAILED (base is %s, expected main)\n' "$base"
  errors=$((errors + 1))
else
  printf 'OK (main)\n'
fi

# 3. PR body attribution clean
printf '3. PR body attribution: '
body_check="$(scripts/check-pr-body.sh "$pr" 2>&1)" && body_exit=0 || body_exit=$?

if [[ $body_exit -eq 0 ]]; then
  printf 'OK (clean)\n'
elif [[ $body_exit -eq 1 ]]; then
  printf 'FAILED\n'
  printf '%s\n' "$body_check"
  errors=$((errors + 1))
else
  printf 'FAILED (script error)\n'
  printf '%s\n' "$body_check"
  errors=$((errors + 1))
fi

# 4. PR title follows commit format
printf '4. PR title format: '
title="$(gh pr view "$pr" --json title --jq '.title' 2>&1)" || {
  printf 'FAILED (could not fetch title)\n'
  exit 2
}

# Validate PR title using the same logic as commit subject validation
# PR title is treated as a single-line commit subject (no body)
if printf '%s\n' "$title" | scripts/validate-commit-message.sh --stdin >/dev/null 2>&1; then
  printf 'OK (%s)\n' "$title"
else
  printf 'FAILED (does not follow commit format)\n'
  printf '  title: %s\n' "$title"
  errors=$((errors + 1))
fi

# 5. Commits attribution clean
printf '5. Commit attribution: '
# Get all commits in the PR with full message (subject + body)
commit_data="$(gh pr view "$pr" --json commits --jq '.commits[] | "\(.messageHeadline)\n\(.messageBody)"' 2>&1)" || {
  printf 'FAILED (could not fetch commits)\n'
  exit 2
}

commit_errors=0
current_commit=""
while IFS= read -r line; do
  if [[ -z "$current_commit" ]]; then
    # First line is the subject
    current_commit="$line"
  else
    # Accumulate body lines
    current_commit="$current_commit\n$line"
  fi

  # When we hit an empty line after body, or end of input, validate
  if [[ -z "$line" ]] || [[ $(printf '%s' "$commit_data" | tail -1) == "$line" ]]; then
    if [[ -n "$current_commit" ]]; then
      # Validate full commit message using the production validator
      if ! printf '%b\n' "$current_commit" | scripts/validate-commit-message.sh --stdin >/dev/null 2>&1; then
        if [[ $commit_errors -eq 0 ]]; then
          printf 'FAILED\n'
        fi
        # Extract first line for error reporting
        subject="$(printf '%b' "$current_commit" | head -1)"
        printf '  - commit failed validation: %s\n' "$subject"
        commit_errors=$((commit_errors + 1))
      fi
      current_commit=""
    fi
  fi
done <<< "$commit_data"

if [[ $commit_errors -eq 0 ]]; then
  printf 'OK (all commits clean)\n'
else
  errors=$((errors + 1))
fi

# 6. No unpushed commits
printf '6. Push state: '
head_ref="$(gh pr view "$pr" --json headRefName --jq '.headRefName' 2>&1)" || {
  printf 'FAILED (could not fetch head ref)\n'
  exit 2
}

# Check if current branch matches PR head
current_branch="$(git rev-parse --abbrev-ref HEAD 2>&1)" || {
  printf 'FAILED (not on a branch)\n'
  exit 2
}

if [[ "$current_branch" != "$head_ref" ]]; then
  printf 'SKIPPED (not on PR branch %s)\n' "$head_ref"
else
  # Check for unpushed commits on current branch only
  unpushed="$(git log @{u}..HEAD 2>&1)" || {
    printf 'FAILED (could not check push state)\n'
    exit 2
  }

  if [[ -n "$unpushed" ]]; then
    printf 'FAILED (unpushed commits exist on current branch)\n'
    errors=$((errors + 1))
  else
    printf 'OK (all commits pushed)\n'
  fi
fi

# 7. Mergeable state
printf '7. Mergeable state: '
mergeable="$(gh pr view "$pr" --json mergeable --jq '.mergeable' 2>&1)" || {
  printf 'SKIPPED (could not query mergeable state)\n'
  mergeable=""
}

if [[ -n "$mergeable" ]]; then
  case "$mergeable" in
    MERGEABLE)
      printf 'OK (MERGEABLE)\n'
      ;;
    CONFLICTING)
      printf 'FAILED (CONFLICTING)\n'
      errors=$((errors + 1))
      ;;
    UNKNOWN)
      printf 'SKIPPED (UNKNOWN, GitHub may still be checking)\n'
      ;;
    *)
      printf 'SKIPPED (unknown state: %s)\n' "$mergeable"
      ;;
  esac
fi

# Summary
printf '\n'
if [[ $errors -eq 0 ]]; then
  printf '✓ PR #%s is ready to merge\n' "$pr"
  exit 0
else
  printf '✗ PR #%s has %d issue(s) that must be fixed before merge\n' "$pr" "$errors"
  exit 1
fi
