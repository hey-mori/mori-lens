#!/usr/bin/env bash
# Mori Lens — Release preflight verification.
#
# Runs all verification checks before a release:
# - cargo fmt, clippy, test
# - Platform-specific package verification (NSIS/MSIX on Windows, app bundle on macOS)
# - Version consistency checks
#
# Usage:
#   ./scripts/release-preflight.sh [--version X.Y.Z]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
fi

echo "Mori Lens Release Preflight"
echo "Version: $VERSION"
echo ""

FAILURES=0

# Step 1: Full check
echo "=== Step 1: Full validation ==="
if bash scripts/check.sh --full; then
    echo "✓ Full validation passed"
else
    echo "✗ Full validation failed"
    FAILURES=$((FAILURES + 1))
fi
echo ""

# Step 2: Version consistency
echo "=== Step 2: Version consistency ==="
CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
if [ "$CARGO_VERSION" = "$VERSION" ]; then
    echo "✓ Cargo.toml version matches: $VERSION"
else
    echo "✗ Cargo.toml version mismatch: expected $VERSION, got $CARGO_VERSION"
    FAILURES=$((FAILURES + 1))
fi

# Check apps/lens/Cargo.toml
LENS_VERSION=$(grep '^version' apps/lens/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
if [ "$LENS_VERSION" = "$VERSION" ]; then
    echo "✓ apps/lens/Cargo.toml version matches: $VERSION"
else
    echo "✗ apps/lens/Cargo.toml version mismatch: expected $VERSION, got $LENS_VERSION"
    FAILURES=$((FAILURES + 1))
fi
echo ""

# Step 3: Platform-specific verification
echo "=== Step 3: Platform-specific package verification ==="

if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
    echo "Windows platform detected"

    # NSIS verification
    echo "--- NSIS verification ---"
    NSIS_INSTALLER="target/release/bundle/nsis/Mori Lens_${VERSION}_x64-setup.exe"
    if [ -f "$NSIS_INSTALLER" ]; then
        echo "Found NSIS installer: $NSIS_INSTALLER"
        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-nsis.ps1 \
            -ExePath "$NSIS_INSTALLER" \
            -ExpectedVersion "$VERSION" 2>&1; then
            echo "✓ NSIS verification passed"
        else
            echo "✗ NSIS verification failed"
            FAILURES=$((FAILURES + 1))
        fi
    else
        echo "⚠ NSIS installer not found (expected at $NSIS_INSTALLER)"
        echo "  Run 'cargo tauri build' first to generate the installer"
    fi
    echo ""

    # MSIX verification
    echo "--- MSIX verification ---"
    MSIX_PACKAGE="target/release/bundle/msix/Mori Lens_${VERSION}_x64.msix"
    if [ -f "$MSIX_PACKAGE" ]; then
        echo "Found MSIX package: $MSIX_PACKAGE"
        if powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-msix.ps1 \
            -MsixPath "$MSIX_PACKAGE" \
            -ExpectedVersion "$VERSION" 2>&1; then
            echo "✓ MSIX verification passed"
        else
            echo "✗ MSIX verification failed"
            FAILURES=$((FAILURES + 1))
        fi
    else
        echo "⚠ MSIX package not found (expected at $MSIX_PACKAGE)"
        echo "  Run 'cargo tauri build' first to generate the package"
    fi
    echo ""

elif [[ "$OSTYPE" == "darwin"* ]]; then
    echo "macOS platform detected"

    # App bundle verification
    echo "--- App bundle verification ---"
    APP_BUNDLE="target/release/bundle/macos/Mori Lens.app"
    if [ -d "$APP_BUNDLE" ]; then
        echo "Found app bundle: $APP_BUNDLE"
        if bash scripts/verify-app-bundle.sh "$APP_BUNDLE" "$VERSION" 2>&1; then
            echo "✓ App bundle verification passed"
        else
            echo "✗ App bundle verification failed"
            FAILURES=$((FAILURES + 1))
        fi
    else
        echo "⚠ App bundle not found (expected at $APP_BUNDLE)"
        echo "  Run 'cargo tauri build' first to generate the bundle"
    fi
    echo ""
else
    echo "⚠ Unsupported platform for package verification: $OSTYPE"
    echo "  Skipping platform-specific checks"
fi

# Summary
echo ""
echo "========================================="
echo "Release Preflight Summary"
echo "========================================="
if [ $FAILURES -eq 0 ]; then
    echo "✓ All checks passed"
    echo ""
    echo "Ready to release version $VERSION"
    exit 0
else
    echo "✗ $FAILURES check(s) failed"
    echo ""
    echo "Fix the issues above before releasing"
    exit 1
fi
