#!/usr/bin/env bash
#
# The single verification gate.
#
# Local runs and CI both call this script, so there is exactly one definition of
# "verified" and the two cannot drift. CI must never re-list these commands in
# YAML; it runs `scripts/check.sh --full`.
#
#   scripts/check.sh          fmt, clippy, tests
#   scripts/check.sh --full   the above plus a release build of the probe
#
# What this script CANNOT check, stated so a green run is not over-read: the
# platform adapters call UI Automation, which needs a live desktop with a
# focused window. `cargo test` compiles them and exercises nothing. An
# acquisition change is verified by running apps/hover-probe against named
# applications and recording the result (docs/git-workflow.md §5).
set -euo pipefail

cd "$(dirname "$0")/.."

FULL=0
[[ "${1:-}" == "--full" ]] && FULL=1

step() { printf '\n· %s\n' "$1"; }

step "cargo fmt --check"
cargo fmt --all -- --check

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test"
cargo test --workspace

if [[ "$FULL" == "1" ]]; then
  step "cargo build --release -p hover-probe"
  cargo build --release -p hover-probe
fi

printf '\nverified\n'
