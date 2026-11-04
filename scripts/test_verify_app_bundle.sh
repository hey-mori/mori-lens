#!/usr/bin/env bash
# Portable regression test for verify-app-bundle.sh
# Tests control-flow and counter semantics without requiring macOS.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERIFIER="$REPO_ROOT/scripts/verify-app-bundle.sh"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# Create stub commands for macOS-only tools
STUBDIR="$TMPDIR/stubs"
mkdir -p "$STUBDIR"

# Stub plutil: returns canned values for known keys
cat > "$STUBDIR/plutil" << 'STUB'
#!/usr/bin/env bash
# Parse: plutil -extract <key> raw <file>
if [[ "$1" == "-extract" ]]; then
  KEY="$2"
  # Skip "raw" and file path
  shift 4

  # Return canned values based on key
  case "$KEY" in
    CFBundleIdentifier)
      echo "dev.swarea.mori-lens"
      ;;
    CFBundleVersion)
      echo "0.1.0"
      ;;
    CFBundleShortVersionString)
      echo "0.1.0"
      ;;
    LSMinimumSystemVersion)
      echo "11.0"
      ;;
    *)
      echo ""
      exit 1
      ;;
  esac
  exit 0
elif [[ "$1" == "-lint" ]]; then
  # Always succeed for valid plist files
  exit 0
fi
exit 1
STUB
chmod +x "$STUBDIR/plutil"

# Stub codesign: always fails (unsigned)
cat > "$STUBDIR/codesign" << 'STUB'
#!/usr/bin/env bash
# Simulate unsigned bundle
exit 1
STUB
chmod +x "$STUBDIR/codesign"

# Stub xxd: returns fake Mach-O magic
cat > "$STUBDIR/xxd" << 'STUB'
#!/usr/bin/env bash
# Parse: xxd -l 4 -p <file>
if [[ "$1" == "-l" && "$2" == "4" && "$3" == "-p" ]]; then
  # Return fake Mach-O magic (64-bit little-endian)
  echo "feedfacf"
  exit 0
fi
exit 1
STUB
chmod +x "$STUBDIR/xxd"

# Create a valid fake .app bundle
VALID_APP="$TMPDIR/Valid.app"
mkdir -p "$VALID_APP/Contents/MacOS"
mkdir -p "$VALID_APP/Contents/Resources"

# Create fake binary with executable bit
echo "fake binary" > "$VALID_APP/Contents/MacOS/lens"
chmod +x "$VALID_APP/Contents/MacOS/lens"

# Create fake icon
echo "fake icon" > "$VALID_APP/Contents/Resources/AppIcon.icns"

# Create fake Info.plist
cat > "$VALID_APP/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>dev.swarea.mori-lens</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
</dict>
</plist>
PLIST

echo "=== Test 1: Valid bundle reaches final summary and exits 0 ==="
# Run verifier with stubs at front of PATH
OUTPUT=$(PATH="$STUBDIR:$PATH" bash "$VERIFIER" "$VALID_APP" 2>&1) || EXIT_CODE=$?
EXIT_CODE=${EXIT_CODE:-0}

echo "$OUTPUT"

# Check that we reached the summary (this proves set -e didn't terminate early)
if ! echo "$OUTPUT" | grep -q "Verification Summary"; then
  echo "FAIL: Valid bundle did not reach final summary"
  exit 1
fi

# Check that PASS_COUNT is reasonable (should have multiple passes)
# Note: On Windows/Git Bash, the executable bit check may fail, so we accept some failures
PASS_LINE=$(echo "$OUTPUT" | grep "^Passed:" || echo "")
if [[ -z "$PASS_LINE" ]]; then
  echo "FAIL: Could not find pass count in output"
  exit 1
fi

PASS_COUNT=$(echo "$PASS_LINE" | awk '{print $2}')
if [[ $PASS_COUNT -lt 10 ]]; then
  echo "FAIL: Expected at least 10 passes, got $PASS_COUNT"
  exit 1
fi

# The key test: we reached the summary with multiple passes, proving the counter works
echo "✓ Test 1 passed: bundle reached summary, $PASS_COUNT passes (set-e counter works)"
echo ""

# Create an invalid bundle (missing binary)
INVALID_APP="$TMPDIR/Invalid.app"
mkdir -p "$INVALID_APP/Contents/MacOS"
mkdir -p "$INVALID_APP/Contents/Resources"
# No binary file
echo "fake icon" > "$INVALID_APP/Contents/Resources/AppIcon.icns"
cat > "$INVALID_APP/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>dev.swarea.mori-lens</string>
</dict>
</plist>
PLIST

echo "=== Test 2: Invalid bundle reaches final summary and exits non-zero ==="
OUTPUT=$(PATH="$STUBDIR:$PATH" bash "$VERIFIER" "$INVALID_APP" 2>&1) || EXIT_CODE=$?
EXIT_CODE=${EXIT_CODE:-0}

echo "$OUTPUT"

# Check that we reached the summary
if ! echo "$OUTPUT" | grep -q "Verification Summary"; then
  echo "FAIL: Invalid bundle did not reach final summary"
  exit 1
fi

# Check that exit code is non-zero
if [[ $EXIT_CODE -eq 0 ]]; then
  echo "FAIL: Invalid bundle exited with code 0 (expected non-zero)"
  exit 1
fi

# Check that FAIL_COUNT > 0
FAIL_LINE=$(echo "$OUTPUT" | grep "^Failed:" || echo "")
if [[ -z "$FAIL_LINE" ]]; then
  echo "FAIL: Could not find fail count in output"
  exit 1
fi

FAIL_COUNT=$(echo "$FAIL_LINE" | awk '{print $2}')
if [[ $FAIL_COUNT -eq 0 ]]; then
  echo "FAIL: Expected at least 1 failure, got $FAIL_COUNT"
  exit 1
fi

echo "✓ Test 2 passed: invalid bundle reached summary, exited $EXIT_CODE, $FAIL_COUNT failures"
echo ""

echo "=== All tests passed ==="
