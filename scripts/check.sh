#!/usr/bin/env bash
#
# The verification gate, in two tiers:
#
#   scripts/check.sh          fast repository gate (workspace fmt/clippy/test)
#   scripts/check.sh --full   product-quality gate:
#                               workspace fmt/clippy/test
#                               apps/lens fmt/clippy/check/test
#                               clean native rebuild (lens-platform-macos)
#                               release build of hover-probe
#                               privacy static gates
#                               version consistency
#
# Local runs and CI both call this script, so there is exactly one definition of
# "verified" and the two cannot drift. CI must never re-list these commands in
# YAML; it runs `scripts/check.sh --full`.
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

# A string literal continued with a trailing backslash is banned, because
# rustfmt rejoins one onto a single line and leaves the next line's indentation
# INSIDE the string. It has done that ten times in this tree: two were sentences
# a reader saw, one of them a refusal with twenty-two spaces in the middle of it,
# and two were fixtures a test measured the length of. Nothing else catches it.
# The string is still valid Rust, clippy is happy, and the test goes on passing
# against the wrong text. Use one long line, or concat!, or build it in pieces.
step "no literal continued with a backslash"
if git ls-files '*.rs' | xargs grep -nP '"[^"]*\\\s*$'; then
  printf '\nthose lines continue a string literal with a trailing backslash.\n'
  printf 'rustfmt rejoins them and puts the indentation inside the string.\n'
  exit 1
fi

# The strongest promise in this project, kept by refusing the construct.
#
# `capture.rs` reads pixels the user did not choose to give it, and CLAUDE.md
# says twice that nothing it reads may reach disk, ever, including for
# debugging. A live check watches for the result
# (`an_ocr_acquisition_writes_nothing_to_disk`), and a check that looks for a
# file appearing can only fail after one has. This fails on the way to write
# one, which is the half that can be enforced rather than observed.
step "no file API in the capture path"
if git ls-files \
    'crates/platform-windows/src/capture.rs' \
    'crates/platform-windows/src/ocr_acquire.rs' \
    'crates/platform-macos/src/capture.rs' \
    'crates/platform-macos/src/ocr_acquire.rs' \
    'crates/platform-macos/src/vision_ocr.rs' \
  | xargs grep -nE 'fs::|File::|CreateFile|WriteFile|BitmapEncoder|SaveToStream'; then
  printf '\nthe capture path must not touch a file. See docs/privacy-model.md\n'
  printf 'and the module comment at the top of capture.rs.\n'
  exit 1
fi

# Also check native Objective-C capture/OCR sources for persistence APIs.
step "no file API in native capture/OCR path"
if git ls-files \
    'crates/platform-macos/native/capture.m' \
    'crates/platform-macos/native/vision_ocr.m' \
  | xargs grep -nE 'fopen|fwrite|writeToFile|writeToURL|NSFileHandle|CGImageDestination|NSData\s+write'; then
  printf '\nthe native capture/OCR path must not write to disk. See docs/privacy-model.md\n'
  exit 1
fi

step "cargo fmt --check"
cargo fmt --all -- --check

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test"
cargo test --workspace

# The workspace version is the single source of truth. `apps/lens` is excluded
# from the workspace (Tauri dependency) and `apps/lens/tauri.conf.json` is not
# Cargo at all, so both duplicate the version by necessity. If they disagree,
# a release would ship with one number in `--version` and another in the
# installer metadata, and a reader comparing them would conclude the build is
# broken even when it is not.
step "version consistency"
WORKSPACE_VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
for f in apps/lens/Cargo.toml; do
  if [[ -f "$f" ]]; then
    V="$(grep -m1 '^version' "$f" | cut -d'"' -f2)"
    if [[ "$V" != "$WORKSPACE_VERSION" ]]; then
      printf '%s has version %s, workspace has %s\n' "$f" "$V" "$WORKSPACE_VERSION"
      exit 1
    fi
  fi
done
if [[ -f apps/lens/tauri.conf.json ]]; then
  V="$(grep -m1 '"version"' apps/lens/tauri.conf.json | cut -d'"' -f4)"
  if [[ "$V" != "$WORKSPACE_VERSION" ]]; then
    printf 'tauri.conf.json has version %s, workspace has %s\n' "$V" "$WORKSPACE_VERSION"
    exit 1
  fi
fi

if [[ "$FULL" == "1" ]]; then
  step "cargo build --release -p hover-probe"
  cargo build --release -p hover-probe

  # The actual product binary (apps/lens) is not a workspace member (Tauri
  # dependency). Verify it compiles, passes lint, and its tests pass.
  # This catches errors that the workspace gate misses.
  step "product binary: cargo fmt apps/lens"
  cargo fmt --manifest-path apps/lens/Cargo.toml -- --check

  step "product binary: cargo clippy apps/lens"
  cargo clippy --manifest-path apps/lens/Cargo.toml --all-targets -- -D warnings

  step "product binary: cargo check apps/lens"
  cargo check --manifest-path apps/lens/Cargo.toml

  step "product binary: cargo test apps/lens"
  cargo test --manifest-path apps/lens/Cargo.toml

  # Force a clean native rebuild of the platform-macos crate to verify that
  # native .m sources actually compile (cached artifacts can hide errors).
  step "clean native rebuild: lens-platform-macos"
  cargo clean -p lens-platform-macos
  cargo check -p lens-platform-macos
  cargo test -p lens-platform-macos
fi

printf '\nverified\n'
