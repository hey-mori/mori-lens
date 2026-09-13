#!/usr/bin/env bash
# Mori Lens — macOS .app bundle structural verification
#
# Validates the structure and metadata of a macOS .app bundle.
# This is NOT a code signing verification - it checks structural correctness.
#
# Usage: ./scripts/verify-app-bundle.sh <path-to-app-bundle>
#
# Exit codes:
#   0 - All checks passed
#   1 - One or more checks failed

set -e

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <path-to-app-bundle>"
  exit 1
fi

APP_BUNDLE="$1"

if [[ ! -d "$APP_BUNDLE" ]]; then
  echo "ERROR: $APP_BUNDLE is not a directory"
  exit 1
fi

echo "Verifying macOS .app bundle: $APP_BUNDLE"
echo ""

# Track pass/fail counts
PASS_COUNT=0
FAIL_COUNT=0

pass() {
  echo "✓ $1"
  ((PASS_COUNT++))
}

fail() {
  echo "✗ $1"
  ((FAIL_COUNT++))
}

# Check 1: Contents/MacOS directory exists
if [[ -d "$APP_BUNDLE/Contents/MacOS" ]]; then
  pass "Contents/MacOS directory exists"
else
  fail "Contents/MacOS directory missing"
fi

# Check 2: Main binary exists
BINARY_PATH="$APP_BUNDLE/Contents/MacOS/lens"
if [[ -f "$BINARY_PATH" ]]; then
  pass "Main binary exists (Contents/MacOS/lens)"
else
  fail "Main binary missing (Contents/MacOS/lens)"
fi

# Check 3: Binary is executable
if [[ -x "$BINARY_PATH" ]]; then
  pass "Main binary is executable"
else
  fail "Main binary is not executable"
fi

# Check 4: Binary is Mach-O format
if [[ -f "$BINARY_PATH" ]]; then
  # Mach-O magic numbers: 0xfeedface (32-bit) or 0xfeedfacf (64-bit)
  MAGIC=$(xxd -l 4 -p "$BINARY_PATH" 2>/dev/null || echo "")
  if [[ "$MAGIC" == "feedface" ]] || [[ "$MAGIC" == "feedfacf" ]] || [[ "$MAGIC" == "cafefedf" ]] || [[ "$MAGIC" == "cffaedfe" ]]; then
    pass "Binary is Mach-O format"
  else
    fail "Binary is not Mach-O format (magic: $MAGIC)"
  fi
fi

# Check 5: Contents/Resources directory exists
if [[ -d "$APP_BUNDLE/Contents/Resources" ]]; then
  pass "Contents/Resources directory exists"
else
  fail "Contents/Resources directory missing"
fi

# Check 6: Info.plist exists
PLIST_PATH="$APP_BUNDLE/Contents/Info.plist"
if [[ -f "$PLIST_PATH" ]]; then
  pass "Info.plist exists"
else
  fail "Info.plist missing"
fi

# Check 7: Info.plist is valid XML
if [[ -f "$PLIST_PATH" ]]; then
  if plutil -lint "$PLIST_PATH" >/dev/null 2>&1; then
    pass "Info.plist is valid XML"
  else
    fail "Info.plist is not valid XML"
  fi
fi

# Check 8: Extract and verify CFBundleIdentifier
if [[ -f "$PLIST_PATH" ]]; then
  BUNDLE_ID=$(plutil -extract CFBundleIdentifier raw "$PLIST_PATH" 2>/dev/null || echo "")
  if [[ -n "$BUNDLE_ID" ]]; then
    if [[ "$BUNDLE_ID" == "area.mori.lens" ]]; then
      pass "CFBundleIdentifier is correct ($BUNDLE_ID)"
    else
      fail "CFBundleIdentifier is wrong (expected area.mori.lens, got $BUNDLE_ID)"
    fi
  else
    fail "CFBundleIdentifier not found in Info.plist"
  fi
fi

# Check 9: Extract and verify CFBundleVersion
if [[ -f "$PLIST_PATH" ]]; then
  BUNDLE_VERSION=$(plutil -extract CFBundleVersion raw "$PLIST_PATH" 2>/dev/null || echo "")
  if [[ -n "$BUNDLE_VERSION" ]]; then
    pass "CFBundleVersion is set ($BUNDLE_VERSION)"
  else
    fail "CFBundleVersion not found in Info.plist"
  fi
fi

# Check 10: Extract and verify CFBundleShortVersionString
if [[ -f "$PLIST_PATH" ]]; then
  SHORT_VERSION=$(plutil -extract CFBundleShortVersionString raw "$PLIST_PATH" 2>/dev/null || echo "")
  if [[ -n "$SHORT_VERSION" ]]; then
    pass "CFBundleShortVersionString is set ($SHORT_VERSION)"
  else
    fail "CFBundleShortVersionString not found in Info.plist"
  fi
fi

# Check 11: LSMinimumSystemVersion is set
if [[ -f "$PLIST_PATH" ]]; then
  MIN_SYSTEM=$(plutil -extract LSMinimumSystemVersion raw "$PLIST_PATH" 2>/dev/null || echo "")
  if [[ -n "$MIN_SYSTEM" ]]; then
    pass "LSMinimumSystemVersion is set ($MIN_SYSTEM)"
  else
    fail "LSMinimumSystemVersion not found in Info.plist"
  fi
fi

# Check 12: Check for common resource files
if [[ -d "$APP_BUNDLE/Contents/Resources" ]]; then
  # Check for icon file
  if ls "$APP_BUNDLE/Contents/Resources"/*.icns >/dev/null 2>&1; then
    pass "Icon file (.icns) present"
  else
    fail "Icon file (.icns) missing"
  fi
fi

# Check 13: Code signature status
echo ""
echo "Code signature status:"
if command -v codesign >/dev/null 2>&1; then
  if codesign -v "$APP_BUNDLE" 2>/dev/null; then
    echo "  Status: SIGNED"
    codesign -dvv "$APP_BUNDLE" 2>&1 | grep -E "(Authority|TeamIdentifier)" | sed 's/^/  /'
  else
    echo "  Status: UNSIGNED (expected for development builds)"
  fi
else
  echo "  Status: UNKNOWN (codesign not available)"
fi

# Summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Verification Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Passed: $PASS_COUNT"
echo "Failed: $FAIL_COUNT"
echo ""

if [[ $FAIL_COUNT -eq 0 ]]; then
  echo "STATUS: All structural checks passed"
  exit 0
else
  echo "STATUS: $FAIL_COUNT check(s) failed"
  exit 1
fi
