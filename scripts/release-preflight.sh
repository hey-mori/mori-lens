#!/usr/bin/env bash
# Mori Lens -- Release preflight verification.
#
# Determines whether THIS exact revision is structurally ready to hand
# to signing/distribution. Default behavior builds fresh artifacts from
# the current commit and verifies them.
#
# Usage:
#   ./scripts/release-preflight.sh [OPTIONS]
#
# Options:
#   --version X.Y.Z    Override version (default: Cargo.toml version)
#   --verify-only      Inspect already-built artifacts (do not build)
#   --allow-dirty      Allow uncommitted changes in working tree
#   --source-only      Skip platform package checks, report source gates only
#   --help             Show this help message
#
# Exit codes:
#   0  Structurally ready (all gates passed)
#   1  One or more gates failed
#   2  Usage error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ── Helpers ──────────────────────────────────────────────────────────────────

# Validate semver syntax: X.Y.Z where X, Y, Z are non-negative integers.
validate_version_syntax() {
    local ver="$1"
    if [[ ! "$ver" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "ERROR: Version '$ver' is not valid semver (expected X.Y.Z, e.g. 0.1.0)" >&2
        exit 2
    fi
}

# ── CLI parsing ──────────────────────────────────────────────────────────────
VERSION_OVERRIDE=""
VERIFY_ONLY=false
ALLOW_DIRTY=false
SOURCE_ONLY=false

usage() {
    cat <<'USAGE'
Usage: ./scripts/release-preflight.sh [OPTIONS]

Options:
  --version X.Y.Z    Override version (default: Cargo.toml version)
  --verify-only      Inspect already-built artifacts (do not build)
  --allow-dirty      Allow uncommitted changes in working tree
  --source-only      Skip platform package checks, report source gates only
  --help             Show this help message

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
            validate_version_syntax "$2"
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
        --source-only)
            SOURCE_ONLY=true
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
REVISION_SHORT="$(git rev-parse --short HEAD)"
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
    validate_version_syntax "$VERSION"
fi

echo "================================================================"
echo "  Mori Lens Release Preflight"
echo "================================================================"
echo ""
echo "REVISION:  $REVISION"
echo "VERSION:   $VERSION"
echo "TREE:      $TREE_STATE"
echo ""

GATE_FAILURES=0
gate_fail() { GATE_FAILURES=$((GATE_FAILURES + 1)); }

# ── Summary capture ──────────────────────────────────────────────────────────
# One variable per key (SUMMARY_FULL_GATE, ...) rather than an associative
# array: macOS ships bash 3.2, which has no `declare -A` and exits 2 on it.
summary_set() { printf -v "SUMMARY_$1" '%s' "$2"; }

summary_set "REVISION" "$REVISION"
summary_set "VERSION" "$VERSION"
summary_set "TREE" "$TREE_STATE"

# ── Step 1: Full validation ─────────────────────────────────────────────────
echo "=== Step 1: Full validation ==="
if bash scripts/check.sh --full 2>&1 | tail -20; then
    summary_set "FULL_GATE" "PASS"
    echo "PASS: Full validation passed"
else
    summary_set "FULL_GATE" "FAIL"
    echo "FAIL: Full validation failed"
    gate_fail
fi
echo ""

# ── Step 2: Version consistency ──────────────────────────────────────────────
echo "=== Step 2: Version consistency ==="
CARGO_VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"
LENS_VERSION="$(grep '^version' apps/lens/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"

VERSION_CONSISTENT=true
if [[ "$CARGO_VERSION" != "$VERSION" ]]; then
    echo "FAIL: Cargo.toml version mismatch: expected $VERSION, got $CARGO_VERSION"
    VERSION_CONSISTENT=false
fi
if [[ "$LENS_VERSION" != "$VERSION" ]]; then
    echo "FAIL: apps/lens/Cargo.toml version mismatch: expected $VERSION, got $LENS_VERSION"
    VERSION_CONSISTENT=false
fi
if [[ "$VERSION_CONSISTENT" == "true" ]]; then
    echo "PASS: All Cargo.toml versions match: $VERSION"
    summary_set "VERSION_CONSISTENCY" "PASS"
else
    summary_set "VERSION_CONSISTENCY" "FAIL"
    gate_fail
fi
echo ""

# ── Platform detection ───────────────────────────────────────────────────────
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
PAYLOAD_PATH=""

# ── Platform package gates ───────────────────────────────────────────────────
if [[ "$SOURCE_ONLY" == "true" ]]; then
    echo "=== Step 3: Platform packages (--source-only) ==="
    echo "  Source gates verified. Package readiness NOT verified."
    echo "  Platform: $(uname -s) (skipped per --source-only)"
    summary_set "PACKAGE_READINESS" "NOT VERIFIED"
    echo ""
elif [[ "$IS_WINDOWS" == "true" ]]; then
    echo "=== Step 3: Build and verify Windows packages ==="

    # ── Step 3a: Verify production assets ────────────────────────────────
    echo "--- Verifying production assets ---"
    if python scripts/verify-product-assets.py 2>&1 | tail -20; then
        summary_set "PRODUCTION_ASSETS" "PASS"
        echo "PASS: Production assets verified"
    else
        summary_set "PRODUCTION_ASSETS" "FAIL"
        echo "FAIL: Production asset verification failed"
        gate_fail
    fi
    echo ""

    if [[ "$VERIFY_ONLY" == "true" ]]; then
        echo "(--verify-only: skipping build, looking for existing artifacts)"
    else
        echo "--- Building NSIS installer ---"
        if (cd apps/lens && cargo tauri build --bundles nsis 2>&1 | tail -10); then
            echo "PASS: NSIS build completed"
        else
            echo "FAIL: NSIS build failed"
            gate_fail
        fi
        echo ""

        echo "--- Building MSIX package ---"
        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-msix.ps1 2>&1 | tail -10; then
            echo "PASS: MSIX build completed"
        else
            echo "FAIL: MSIX build failed"
            gate_fail
        fi
        echo ""
    fi

    # ── Locate NSIS artifact (deterministic, version-exact) ──────────────
    echo "--- Locating NSIS artifact ---"
    NSIS_EXPECTED="Mori Lens_${VERSION}_x64-setup.exe"
    NSIS_EXACT_PATH="${NSIS_DIR}/${NSIS_EXPECTED}"
    if [[ ! -d "$NSIS_DIR" ]]; then
        echo "FAIL: NSIS directory does not exist: $NSIS_DIR"
        summary_set "NSIS_BUILD" "MISSING"
        summary_set "NSIS_ARTIFACT" "NONE"
        summary_set "NSIS_VERIFY" "FAIL"
        gate_fail
    elif [[ ! -f "$NSIS_EXACT_PATH" ]]; then
        echo "FAIL: Expected NSIS artifact not found: $NSIS_EXACT_PATH"
        echo "  Looked for exactly: $NSIS_EXPECTED"
        echo "  0 matches for current version $VERSION. Cannot pass preflight."
        summary_set "NSIS_BUILD" "MISSING"
        summary_set "NSIS_ARTIFACT" "NONE"
        summary_set "NSIS_VERIFY" "FAIL"
        gate_fail
    else
        NSIS_ARTIFACT="$NSIS_EXACT_PATH"
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
            echo "FAIL: Payload binary not found at expected location"
            echo "  Cannot verify payload architecture without the binary."
            echo "  The release preflight cannot pass without proving the payload."
            summary_set "NSIS_PAYLOAD" "MISSING"
            gate_fail
        fi

        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-nsis.ps1 "${PS_ARGS[@]}" 2>&1 | tail -30; then
            summary_set "NSIS_VERIFY" "PASS"
            echo "PASS: NSIS verification passed"
        else
            summary_set "NSIS_VERIFY" "FAIL"
            echo "FAIL: NSIS verification failed"
            gate_fail
        fi
    fi
    echo ""

    # ── Locate MSIX artifact (deterministic, version-exact) ──────────────
    echo "--- Locating MSIX artifact ---"
    MSIX_EXPECTED="MoriLens_${VERSION}.0_x64.msix"
    MSIX_EXACT_PATH="${MSIX_DIR}/${MSIX_EXPECTED}"
    if [[ ! -d "$MSIX_DIR" ]]; then
        echo "FAIL: MSIX directory does not exist: $MSIX_DIR"
        summary_set "MSIX_BUILD" "MISSING"
        summary_set "MSIX_ARTIFACT" "NONE"
        summary_set "MSIX_VERIFY" "FAIL"
        gate_fail
    elif [[ ! -f "$MSIX_EXACT_PATH" ]]; then
        echo "FAIL: Expected MSIX artifact not found: $MSIX_EXACT_PATH"
        echo "  Looked for exactly: $MSIX_EXPECTED"
        echo "  0 matches for current version $VERSION. Cannot pass preflight."
        summary_set "MSIX_BUILD" "MISSING"
        summary_set "MSIX_ARTIFACT" "NONE"
        summary_set "MSIX_VERIFY" "FAIL"
        gate_fail
    else
        MSIX_ARTIFACT="$MSIX_EXACT_PATH"
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
            echo "PASS: MSIX verification passed"
        else
            summary_set "MSIX_VERIFY" "FAIL"
            echo "FAIL: MSIX verification failed"
            gate_fail
        fi
    fi
    echo ""

    # ── Step 4: Build provenance and embedded stamp ─────────────────────
    echo "=== Step 4: Build provenance and embedded stamp ==="

    # Determine BUILD_PROVENANCE based on build mode and tree state
    if [[ "$VERIFY_ONLY" == "true" ]]; then
        BUILD_PROVENANCE="VERIFY_ONLY"
        echo "Build provenance: VERIFY_ONLY (--verify-only mode)"
    elif [[ "$ALLOW_DIRTY" == "true" && -n "$TREE_STATUS" ]]; then
        BUILD_PROVENANCE="DIRTY_WORKTREE"
        echo "Build provenance: DIRTY_WORKTREE (--allow-dirty with dirty tree)"
    else
        BUILD_PROVENANCE="BUILT_FROM_CURRENT_CLEAN_HEAD"
        echo "Build provenance: BUILT_FROM_CURRENT_CLEAN_HEAD (default mode, clean tree)"
    fi
    summary_set "BUILD_PROVENANCE" "$BUILD_PROVENANCE"

    # Determine EMBEDDED_BUILD_STAMP from binary using runtime --version check
    EMBEDDED_STAMP="UNPROVEN"
    if [[ -f "$PAYLOAD_PATH" ]]; then
        # Use the runtime --version interface to extract the build stamp.
        # This is the canonical contract: lens.exe --version reports the embedded SHA.
        VERSION_OUTPUT="$("$REPO_ROOT/$PAYLOAD_PATH" --version 2>/dev/null || true)"

        if [[ -n "$VERSION_OUTPUT" ]]; then
            # Expected format: "Mori Lens 0.1.0 · build <sha>" or "Mori Lens 0.1.0 · build <sha>-dirty"
            # Extract the SHA from the build stamp
            BUILD_SHA_FROM_RUNTIME="$(echo "$VERSION_OUTPUT" | grep -oE 'build [0-9a-f]{7,40}(-dirty)?' | awk '{print $2}' || true)"

            if [[ -n "$BUILD_SHA_FROM_RUNTIME" ]]; then
                CLEAN_SHA="${BUILD_SHA_FROM_RUNTIME%-dirty}"
                if [[ "$CLEAN_SHA" == "$REVISION_SHORT" ]]; then
                    EMBEDDED_STAMP="VERIFIED"
                    echo "Embedded build stamp: VERIFIED (matches $REVISION_SHORT)"
                else
                    EMBEDDED_STAMP="MISMATCH"
                    echo "Embedded build stamp: MISMATCH (binary has $BUILD_SHA_FROM_RUNTIME, expected $REVISION_SHORT)"
                    gate_fail
                fi
            else
                EMBEDDED_STAMP="UNPROVEN"
                echo "Embedded build stamp: UNPROVEN (--version output did not contain build stamp)"
                echo "  Output: $VERSION_OUTPUT"
            fi
        else
            EMBEDDED_STAMP="UNPROVEN"
            echo "Embedded build stamp: UNPROVEN (--version execution failed)"
        fi
    else
        EMBEDDED_STAMP="UNPROVEN"
        echo "Embedded build stamp: UNPROVEN (no payload binary available)"
    fi
    summary_set "EMBEDDED_BUILD_STAMP" "$EMBEDDED_STAMP"
    echo ""

elif [[ "$IS_MACOS" == "true" ]]; then
    echo "=== Step 3: Build and verify macOS packages ==="

    APP_BUNDLE="apps/lens/target/release/bundle/macos/Mori Lens.app"

    if [[ "$VERIFY_ONLY" == "true" ]]; then
        echo "(--verify-only: skipping build)"
    else
        echo "--- Building macOS app bundle ---"
        if (cd apps/lens && cargo tauri build --bundles app 2>&1 | tail -10); then
            echo "PASS: macOS build completed"
        else
            echo "FAIL: macOS build failed"
            gate_fail
        fi
        echo ""
    fi

    if [[ -d "$APP_BUNDLE" ]]; then
        echo "--- Verifying app bundle ---"
        if bash scripts/verify-app-bundle.sh "$APP_BUNDLE" "$VERSION" 2>&1 | tail -30; then
            summary_set "APP_BUNDLE_VERIFY" "PASS"
            echo "PASS: App bundle verification passed"
        else
            summary_set "APP_BUNDLE_VERIFY" "FAIL"
            echo "FAIL: App bundle verification failed"
            gate_fail
        fi
    else
        echo "FAIL: App bundle not found at $APP_BUNDLE"
        summary_set "APP_BUNDLE_VERIFY" "FAIL"
        gate_fail
    fi
    echo ""

    # ── Step 4: Build provenance and embedded stamp (macOS) ─────────────
    echo "=== Step 4: Build provenance and embedded stamp ==="

    # Determine BUILD_PROVENANCE based on build mode and tree state
    if [[ "$VERIFY_ONLY" == "true" ]]; then
        BUILD_PROVENANCE="VERIFY_ONLY"
        echo "Build provenance: VERIFY_ONLY (--verify-only mode)"
    elif [[ "$ALLOW_DIRTY" == "true" && -n "$TREE_STATUS" ]]; then
        BUILD_PROVENANCE="DIRTY_WORKTREE"
        echo "Build provenance: DIRTY_WORKTREE (--allow-dirty with dirty tree)"
    else
        BUILD_PROVENANCE="BUILT_FROM_CURRENT_CLEAN_HEAD"
        echo "Build provenance: BUILT_FROM_CURRENT_CLEAN_HEAD (default mode, clean tree)"
    fi
    summary_set "BUILD_PROVENANCE" "$BUILD_PROVENANCE"

    # Determine EMBEDDED_BUILD_STAMP from binary
    MACOS_PAYLOAD="apps/lens/target/release/lens"
    EMBEDDED_STAMP="UNPROVEN"
    if [[ -f "$MACOS_PAYLOAD" ]]; then
        BUILD_SHA_FROM_BINARY="$(strings "$MACOS_PAYLOAD" 2>/dev/null | grep -oE '[0-9a-f]{7,40}' | head -1 || true)"
        if [[ -n "$BUILD_SHA_FROM_BINARY" ]]; then
            CLEAN_SHA="${BUILD_SHA_FROM_BINARY%-dirty}"
            if [[ "$CLEAN_SHA" == "$REVISION_SHORT" ]]; then
                EMBEDDED_STAMP="VERIFIED"
                echo "Embedded build stamp: VERIFIED (matches $REVISION_SHORT)"
            else
                EMBEDDED_STAMP="MISMATCH"
                echo "Embedded build stamp: MISMATCH (binary has $BUILD_SHA_FROM_BINARY, expected $REVISION_SHORT)"
                gate_fail
            fi
        else
            EMBEDDED_STAMP="UNPROVEN"
            echo "Embedded build stamp: UNPROVEN (could not extract from binary)"
        fi
    else
        EMBEDDED_STAMP="UNPROVEN"
        echo "Embedded build stamp: UNPROVEN (no payload binary available)"
    fi
    summary_set "EMBEDDED_BUILD_STAMP" "$EMBEDDED_STAMP"
    echo ""

else
    # Unsupported platform without --source-only: FAIL
    echo "=== Step 3: Platform packages ==="
    echo "FAIL: Unsupported platform for package verification: $(uname -s)" >&2
    echo "  Package checks cannot run on this platform." >&2
    echo "  Use --source-only to skip package checks and verify source gates only." >&2
    echo ""
    summary_set "PACKAGE_READINESS" "FAIL"
    gate_fail
fi

# ── Final summary ────────────────────────────────────────────────────────────
echo "================================================================"
echo "  Release Preflight Summary"
echo "================================================================"
echo ""
echo "REVISION:             ${SUMMARY_REVISION:-unknown}"
echo "VERSION:              ${SUMMARY_VERSION:-unknown}"
echo "TREE:                 ${SUMMARY_TREE:-unknown}"
echo ""
echo "FULL_GATE:            ${SUMMARY_FULL_GATE:-SKIP}"
echo "VERSION_CONSISTENCY:  ${SUMMARY_VERSION_CONSISTENCY:-SKIP}"
echo "PRODUCTION_ASSETS:    ${SUMMARY_PRODUCTION_ASSETS:-SKIP}"
echo ""

if [[ "$SOURCE_ONLY" == "true" ]]; then
    echo "PACKAGE_READINESS:    ${SUMMARY_PACKAGE_READINESS:-NOT VERIFIED}"
elif [[ "$IS_WINDOWS" == "true" ]]; then
    echo "NSIS_BUILD:           ${SUMMARY_NSIS_BUILD:-SKIP}"
    echo "NSIS_ARTIFACT:        ${SUMMARY_NSIS_ARTIFACT:-NONE}"
    echo "NSIS_SHA256:          ${SUMMARY_NSIS_SHA256:-NONE}"
    echo "NSIS_VERIFY:          ${SUMMARY_NSIS_VERIFY:-SKIP}"
    if [[ -n "${SUMMARY_NSIS_PAYLOAD:-}" ]]; then
        echo "NSIS_PAYLOAD:         ${SUMMARY_NSIS_PAYLOAD}"
    fi
    echo ""
    echo "MSIX_BUILD:           ${SUMMARY_MSIX_BUILD:-SKIP}"
    echo "MSIX_ARTIFACT:        ${SUMMARY_MSIX_ARTIFACT:-NONE}"
    echo "MSIX_SHA256:          ${SUMMARY_MSIX_SHA256:-NONE}"
    echo "MSIX_VERIFY:          ${SUMMARY_MSIX_VERIFY:-SKIP}"
elif [[ "$IS_MACOS" == "true" ]]; then
    echo "APP_BUNDLE_VERIFY:    ${SUMMARY_APP_BUNDLE_VERIFY:-SKIP}"
fi

echo ""
echo "BUILD_PROVENANCE:     ${SUMMARY_BUILD_PROVENANCE:-SKIP}"
echo "EMBEDDED_BUILD_STAMP: ${SUMMARY_EMBEDDED_BUILD_STAMP:-SKIP}"
echo ""
echo "SIGNING:              BLOCKED / UNSIGNED"
echo "STORE PUBLISHER:      PLACEHOLDER / CONFIGURED"
echo "HUMAN INSTALL SMOKE:  PENDING"
echo ""

if [[ $GATE_FAILURES -eq 0 ]]; then
    if [[ "$SOURCE_ONLY" == "true" ]]; then
        echo "OVERALL:              SOURCE GATES PASS, PACKAGE READINESS NOT VERIFIED"
        echo ""
        echo "Source gates passed. Package checks were skipped (--source-only)."
        echo "Run without --source-only on a supported platform to verify packages."
        exit 0
    fi

    # Check build provenance and embedded stamp for final verdict
    if [[ "$VERIFY_ONLY" == "true" ]]; then
        if [[ "${SUMMARY_EMBEDDED_BUILD_STAMP}" == "UNPROVEN" ]]; then
            echo "OVERALL:              ARTIFACT STRUCTURE VALID, EMBEDDED STAMP UNPROVEN"
            echo ""
            echo "Artifacts are structurally valid, but embedded build stamp could not be verified."
            echo "The artifacts may not be from this exact revision."
            echo "For exact-revision readiness, rebuild from clean tree or use artifacts with verified stamp."
            exit 0
        elif [[ "${SUMMARY_EMBEDDED_BUILD_STAMP}" == "MISMATCH" ]]; then
            echo "OVERALL:              FAILED (embedded stamp mismatch)"
            echo ""
            echo "Artifacts are structurally valid but the embedded build stamp does not match this revision."
            exit 1
        elif [[ "${SUMMARY_EMBEDDED_BUILD_STAMP}" == "VERIFIED" ]]; then
            echo "OVERALL:              STRUCTURALLY READY"
            echo ""
            echo "All gates passed. Packages are structurally valid and verified for this revision."
            echo "Signing and distribution remain external blockers."
            exit 0
        else
            echo "OVERALL:              FAILED (unexpected embedded stamp: ${SUMMARY_EMBEDDED_BUILD_STAMP})"
            exit 1
        fi
    elif [[ "${SUMMARY_BUILD_PROVENANCE}" == "DIRTY_WORKTREE" ]]; then
        echo "OVERALL:              STRUCTURE VALID, DIRTY-WORKTREE PROVENANCE"
        echo ""
        echo "All gates passed. Packages are structurally valid but were built from a dirty working tree."
        echo "The artifacts include uncommitted changes and cannot be associated cleanly with the reported Git revision."
        echo "For exact-revision readiness, rebuild from clean tree."
        echo "Signing and distribution remain external blockers."
        exit 0
    else
        # Canonical clean final RC build: require EMBEDDED_BUILD_STAMP = VERIFIED
        if [[ "${SUMMARY_EMBEDDED_BUILD_STAMP}" == "VERIFIED" ]]; then
            echo "OVERALL:              STRUCTURALLY READY"
            echo ""
            echo "All gates passed. Packages are structurally valid and verified for this revision."
            echo "Signing and distribution remain external blockers."
            exit 0
        elif [[ "${SUMMARY_EMBEDDED_BUILD_STAMP}" == "UNPROVEN" ]]; then
            echo "OVERALL:              FAILED (embedded stamp unproven for canonical clean build)"
            echo ""
            echo "Canonical clean build requires EMBEDDED_BUILD_STAMP = VERIFIED."
            echo "The embedded build stamp could not be verified, which means the exact-source provenance is not proven."
            echo "Investigate the build-stamp subsystem and runtime --version contract."
            exit 1
        elif [[ "${SUMMARY_EMBEDDED_BUILD_STAMP}" == "MISMATCH" ]]; then
            echo "OVERALL:              FAILED (embedded stamp mismatch)"
            echo ""
            echo "Artifacts are structurally valid but the embedded build stamp does not match this revision."
            exit 1
        else
            echo "OVERALL:              FAILED (unexpected embedded stamp: ${SUMMARY_EMBEDDED_BUILD_STAMP})"
            exit 1
        fi
    fi
else
    echo "OVERALL:              FAILED ($GATE_FAILURES gate(s))"
    echo ""
    echo "Fix the failures above before release."
    exit 1
fi
