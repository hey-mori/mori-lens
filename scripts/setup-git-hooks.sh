#!/usr/bin/env bash
#
# Install repository-local git hooks.
#
# Sets core.hooksPath to .githooks so the pre-push hook is active.
# Run this once after cloning.
#
# The pre-push hook rejects direct pushes to main and non-fast-forward
# pushes to any branch. See docs/git-workflow.md §6.
set -euo pipefail

cd "$(dirname "$0")/.."

git config --local core.hooksPath .githooks
echo "git hooks installed: .githooks/"
echo "  pre-push: rejects direct pushes to main"
echo "  pre-push: rejects non-fast-forward pushes"
