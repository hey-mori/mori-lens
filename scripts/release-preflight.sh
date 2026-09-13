#!/usr/bin/env bash
# Mori Lens — Release preflight verification.
#
# Determines whether THIS exact revision is structurally ready to hand
# to signing/distribution. Default behavior builds fresh artifacts from
# the current commit and verifies them.
#
# Usage:
#   ./scripts/release-preflight.sh [OPTIONS]
#
# Options:
#   --version X.Y.Z   Override version (default: Cargo.toml version)
#   --verify-only     Inspect already-built artifacts (do not build)
#   --allow-dirty     Allow uncommitted changes in working tree
#   --help            Show this help message
#
# Exit codes:
#   0  Structurally ready (all gates passed)
#   1  One or more gates failed
#   2  Usage error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ── CLI parsing ──────────────────────────────────────────────────────────────
VERSION_OVERRIDE=""
VERIFY_ONLY=false
ALLOW_DIRTY=false

usage() {
    cat <<'USAGE'
Usage: ./scripts/release-preflight.sh [OPTIONS]

Options:
  --version X.Y.Z   Override version (default: Cargo.toml version)
  --verify-only     Inspect already-built artifacts (do not build)
  --allow-dirty     Allow uncommitted changes in working tree
  --help            Show this help message

Exit codes:
  0  Structurally ready
  1  Gate failed
  2  Usage error
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            if [[ $# -lt 2 ]]; then
                echo "ERROR: --version requires an argument" >&2
                exit 2
            fi
            VERSION_OVERRIDE="$2"
            shift 2
            ;;
        --verify-only)
            VERIFY_ONLY=true
            shift
            ;;
        --allow-dirty)
            ALLOW_DIRTY=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# ── Source identity ──────────────────────────────────────────────────────────
REVISION="$(git rev-parse HEAD)"
TREE_STATUS="$(git status --porcelain)"

if [[ -n "$TREE_STATUS" && "$ALLOW_DIRTY" != "true" ]]; then
    echo "ERROR: Working tree has uncommitted changes." >&2
    echo "Use --allow-dirty to proceed anyway, but the artifact cannot be" >&2
    echo "associated cleanly with the reported Git revision." >&2
    echo "" >&2
    echo "Uncommitted changes:" >&2
    echo "$TREE_STATUS" >&2
    exit 1
fi

if [[ -n "$TREE_STATUS" && "$ALLOW_DIRTY" == "true" ]]; then
    TREE_STATE="dirty (allowed)"
else
    TREE_STATE="clean"
fi

# ── Version ──────────────────────────────────────────────────────────────────
if [[ -n "$VERSION_OVERRIDE" ]]; then
    VERSION="$VERSION_OVERRIDE"
else
    VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"
fi

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Mori Lens Release Preflight                                ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "REVISION:  $REVISION"
echo "VERSION:   $VERSION"
echo "TREE:      $TREE_STATE"
echo ""

GATE_FAILURES=0
gate_fail() { GATE_FAILURES=$((GATE_FAILURES + 1)); }

# ── Summary capture ──────────────────────────────────────────────────────────
declare -A SUMMARY
summary_set() { SUMMARY["$1"]="$2"; }

summary_set "REVISION" "$REVISION"
summary_set "VERSION" "$VERSION"
summary_set "TREE" "$TREE_STATE"

# ── Step 1: Full validation ─────────────────────────────────────────────────
echo "=== Step 1: Full validation ==="
if bash scripts/check.sh --full 2>&1 | tail -20; then
    summary_set "FULL_GATE" "PASS"
    echo "✓ Full validation passed"
else
    summary_set "FULL_GATE" "FAIL"
    echo "✗ Full validation failed"
    gate_fail
fi
echo ""

# ── Step 2: Version consistency ──────────────────────────────────────────────
echo "=== Step 2: Version consistency ==="
CARGO_VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"
LENS_VERSION="$(grep '^version' apps/lens/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"

VERSION_CONSISTENT=true
if [[ "$CARGO_VERSION" != "$VERSION" ]]; then
    echo "✗ Cargo.toml version mismatch: expected $VERSION, got $CARGO_VERSION"
    VERSION_CONSISTENT=false
fi
if [[ "$LENS_VERSION" != "$VERSION" ]]; then
    echo "✗ apps/lens/Cargo.toml version mismatch: expected $VERSION, got $LENS_VERSION"
    VERSION_CONSISTENT=false
fi
if [[ "$VERSION_CONSISTENT" == "true" ]]; then
    echo "✓ All Cargo.toml versions match: $VERSION"
fi
echo ""

# ── Platform-specific paths ─────────────────────────────────────────────────
IS_WINDOWS=false
IS_MACOS=false
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*|Windows*) IS_WINDOWS=true ;;
    Darwin*) IS_MACOS=true ;;
esac

# Canonical artifact paths (must match package scripts)
NSIS_DIR="apps/lens/target/release/bundle/nsis"
MSIX_DIR="target/msix"
NSIS_ARTIFACT=""
MSIX_ARTIFACT=""

# ── Step 3: Build or locate artifacts ────────────────────────────────────────
if [[ "$IS_WINDOWS" == "true" ]]; then
    echo "=== Step 3: Build and verify Windows packages ==="

    if [[ "$VERIFY_ONLY" == "true" ]]; then
        echo "(--verify-only: skipping build, looking for existing artifacts)"
    else
        echo "--- Building NSIS installer ---"
        if (cd apps/lens && cargo tauri build --bundles nsis 2>&1 | tail -10); then
            echo "✓ NSIS build completed"
        else
            echo "✗ NSIS build failed"
            gate_fail
        fi
        echo ""

        echo "--- Building MSIX package ---"
        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-msix.ps1 2>&1 | tail -10; then
            echo "✓ MSIX build completed"
        else
            echo "✗ MSIX build failed"
            gate_fail
        fi
        echo ""
    fi

    # ── Locate NSIS artifact ─────────────────────────────────────────────
    echo "--- Locating NSIS artifact ---"
    if [[ -d "$NSIS_DIR" ]]; then
        NSIS_ARTIFACT="$(find "$NSIS_DIR" -maxdepth 1 -name "*.exe" -type f | head -1)"
    fi
    if [[ -z "$NSIS_ARTIFACT" ]]; then
        echo "✗ NSIS installer not found in $NSIS_DIR"
        echo "  Cannot pass preflight without NSIS artifact."
        summary_set "NSIS_BUILD" "MISSING"
        summary_set "NSIS_ARTIFACT" "NONE"
        summary_set "NSIS_VERIFY" "FAIL"
        gate_fail
    else
        echo "Found: $NSIS_ARTIFACT"
        NSIS_FILENAME="$(basename "$NSIS_ARTIFACT")"
        summary_set "NSIS_ARTIFACT" "$NSIS_FILENAME"
        summary_set "NSIS_BUILD" "OK"

        # SHA256
        NSIS_SHA256="$(sha256sum "$NSIS_ARTIFACT" | awk '{print $1}')"
        summary_set "NSIS_SHA256" "$NSIS_SHA256"
        echo "SHA256: $NSIS_SHA256"

        # ── Locate payload for architecture check ────────────────────────
        PAYLOAD_PATH="apps/lens/target/release/lens.exe"
        if [[ ! -f "$PAYLOAD_PATH" ]]; then
            PAYLOAD_PATH="target/release/lens.exe"
        fi

        echo ""
        echo "--- Verifying NSIS installer ---"
        PS_ARGS=("-ExePath" "$NSIS_ARTIFACT" "-ExpectedVersion" "$VERSION")
        if [[ -f "$PAYLOAD_PATH" ]]; then
            PS_ARGS+=("-PayloadExePath" "$PAYLOAD_PATH")
            echo "Payload: $PAYLOAD_PATH"
        else
            echo "WARNING: Payload binary not found at $PAYLOAD_PATH"
            echo "  Architecture check will be limited to bootstrap only."
        fi

        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-nsis.ps1 "${PS_ARGS[@]}" 2>&1 | tail -30; then
            summary_set "NSIS_VERIFY" "PASS"
            echo "✓ NSIS verification passed"
        else
            summary_set "NSIS_VERIFY" "FAIL"
            echo "✗ NSIS verification failed"
            gate_fail
        fi
    fi
    echo ""

    # ── Locate MSIX artifact ─────────────────────────────────────────────
    echo "--- Locating MSIX artifact ---"
    if [[ -d "$MSIX_DIR" ]]; then
        MSIX_ARTIFACT="$(find "$MSIX_DIR" -maxdepth 1 -name "MoriLens_*.msix" -type f | head -1)"
    fi
    if [[ -z "$MSIX_ARTIFACT" ]]; then
        echo "✗ MSIX package not found in $MSIX_DIR"
        echo "  Cannot pass preflight without MSIX artifact."
        summary_set "MSIX_BUILD" "MISSING"
        summary_set "MSIX_ARTIFACT" "NONE"
        summary_set "MSIX_VERIFY" "FAIL"
        gate_fail
    else
        echo "Found: $MSIX_ARTIFACT"
        MSIX_FILENAME="$(basename "$MSIX_ARTIFACT")"
        summary_set "MSIX_ARTIFACT" "$MSIX_FILENAME"
        summary_set "MSIX_BUILD" "OK"

        # SHA256
        MSIX_SHA256="$(sha256sum "$MSIX_ARTIFACT" | awk '{print $1}')"
        summary_set "MSIX_SHA256" "$MSIX_SHA256"
        echo "SHA256: $MSIX_SHA256"

        echo ""
        echo "--- Verifying MSIX package ---"
        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-msix.ps1 \
            -MsixPath "$MSIX_ARTIFACT" 2>&1 | tail -30; then
            summary_set "MSIX_VERIFY" "PASS"
            echo "✓ MSIX verification passed"
        else
            summary_set "MSIX_VERIFY" "FAIL"
            echo "✗ MSIX verification failed"
            gate_fail
        fi
    fi
    echo ""

elif [[ "$IS_MACOS" == "true" ]]; then
    echo "=== Step 3: Build and verify macOS packages ==="

    APP_BUNDLE="apps/lens/target/release/bundle/macos/Mori Lens.app"

    if [[ "$VERIFY_ONLY" == "true" ]]; then
        echo "(--verify-only: skipping build)"
    else
        echo "--- Building macOS app bundle ---"
        if (cd apps/lens && cargo tauri build --bundles app 2>&1 | tail -10); then
            echo "✓ macOS build completed"
        else
            echo "✗ macOS build failed"
            gate_fail
        fi
        echo ""
    fi

    if [[ -d "$APP_BUNDLE" ]]; then
        echo "--- Verifying app bundle ---"
        if bash scripts/verify-app-bundle.sh "$APP_BUNDLE" "$VERSION" 2>&1 | tail -30; then
            summary_set "APP_BUNDLE_VERIFY" "PASS"
            echo "✓ App bundle verification passed"
        else
            summary_set "APP_BUNDLE_VERIFY" "FAIL"
            echo "✗ App bundle verification failed"
            gate_fail
        fi
    else
        echo "✗ App bundle not found at $APP_BUNDLE"
        summary_set "APP_BUNDLE_VERIFY" "FAIL"
        gate_fail
    fi
    echo ""
else
    echo "=== Step 3: Platform packages ==="
    echo "⚠ Unsupported platform for package verification: $(uname -s)"
    echo "  Skipping platform-specific checks"
fi

# ── Final summary ────────────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Release Preflight Summary                                  ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "REVISION:             ${SUMMARY[REVISION]:-unknown}"
echo "VERSION:              ${SUMMARY[VERSION]:-unknown}"
echo "TREE:                 ${SUMMARY[TREE]:-unknown}"
echo ""
echo "FULL_GATE:            ${SUMMARY[FULL_GATE]:-SKIP}"
echo ""

if [[ "$IS_WINDOWS" == "true" ]]; then
    echo "NSIS_BUILD:           ${SUMMARY[NSIS_BUILD]:-SKIP}"
    echo "NSIS_ARTIFACT:        ${SUMMARY[NSIS_ARTIFACT]:-NONE}"
    echo "NSIS_SHA256:          ${SUMMARY[NSIS_SHA256]:-NONE}"
    echo "NSIS_VERIFY:          ${SUMMARY[NSIS_VERIFY]:-SKIP}"
    echo ""
    echo "MSIX_BUILD:           ${SUMMARY[MSIX_BUILD]:-SKIP}"
    echo "MSIX_ARTIFACT:        ${SUMMARY[MSIX_ARTIFACT]:-NONE}"
    echo "MSIX_SHA256:          ${SUMMARY[MSIX_SHA256]:-NONE}"
    echo "MSIX_VERIFY:          ${SUMMARY[MSIX_VERIFY]:-SKIP}"
elif [[ "$IS_MACOS" == "true" ]]; then
    echo "APP_BUNDLE_VERIFY:    ${SUMMARY[APP_BUNDLE_VERIFY]:-SKIP}"
fi

echo ""
echo "SIGNING:              BLOCKED / UNSIGNED"
echo "STORE PUBLISHER:      PLACEHOLDER / CONFIGURED"
echo "HUMAN INSTALL SMOKE:  PENDING"
echo ""

if [[ $GATE_FAILURES -eq 0 ]]; then
    echo "OVERALL:              STRUCTURALLY READY"
    echo ""
    echo "All gates passed. Packages are structurally valid for this revision."
    echo "Signing and distribution remain external blockers."
    exit 0
else
    echo "OVERALL:              FAILED ($GATE_FAILURES gate(s))"
    echo ""
    echo "Fix the failures above before release."
    exit 1
fi
