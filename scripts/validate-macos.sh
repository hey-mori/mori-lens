#!/usr/bin/env bash
#
# Mori Lens — macOS Real-World Validation Procedure
#
# Prerequisites:
#   1. Accessibility permission granted for Terminal/lens binary
#   2. Screen Recording permission granted for Terminal/lens binary
#   3. (Optional) Ollama running with a model loaded
#
# Usage:
#   bash scripts/validate-macos.sh
#
# This script:
#   - Checks permission state
#   - Opens the controlled validation fixture in Safari
#   - Starts the product binary
#   - Records automated evidence
#   - Prompts for visual checkpoints
#   - Cleans up on exit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_ROOT/apps/lens/target/release/lens"
FIXTURE="$PROJECT_ROOT/docs/fixtures/validation-surface.html"
EVIDENCE_DIR="$PROJECT_ROOT/docs/benchmarks"
TIMESTAMP=$(date +%Y-%m-%d-%H%M%S)
EVIDENCE_FILE="$EVIDENCE_DIR/$TIMESTAMP-macos-validation.md"

mkdir -p "$EVIDENCE_DIR"

# --- Helper functions ---

log() { echo "[$(date +%H:%M:%S)] $*"; }
record() { echo "$*" >> "$EVIDENCE_FILE"; }

check_permission() {
    local name="$1"
    local result
    result=$(osascript -e "tell application \"System Events\" to tell application process \"$2\" to exists" 2>/dev/null) || true
    echo "$result"
}

# --- Pre-flight ---

log "Mori Lens macOS Validation"
log "=========================="
log ""

# Check binary exists
if [[ ! -x "$BINARY" ]]; then
    log "ERROR: Release binary not found at $BINARY"
    log "Run: cargo build --release --manifest-path apps/lens/Cargo.toml"
    exit 1
fi
log "✓ Binary exists: $BINARY"
log "  Version: $($BINARY --version)"

# Check fixture exists
if [[ ! -f "$FIXTURE" ]]; then
    log "ERROR: Validation fixture not found at $FIXTURE"
    exit 1
fi
log "✓ Fixture exists: $FIXTURE"

# Check Ollama
OLLAMA_STATUS="BLOCKED"
if curl -s --connect-timeout 2 http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
    OLLAMA_STATUS="AVAILABLE"
    log "✓ Ollama is running"
    MODELS=$(curl -s http://127.0.0.1:11434/api/tags | python3 -c "import sys,json; print(', '.join(m['name'] for m in json.load(sys.stdin).get('models',[])))" 2>/dev/null || echo "unknown")
    log "  Models: $MODELS"
else
    log "⚠ Ollama not running — Quick/Deep will be BLOCKED"
fi

# --- Start evidence recording ---

record "# Mori Lens macOS Real-World Validation"
record ""
record "Date: $(date)"
record "Binary: $($BINARY --version)"
record "Commit: $(cd "$PROJECT_ROOT" && git log -1 --oneline)"
record "Ollama: $OLLAMA_STATUS"
record ""

# --- Open fixture ---

log ""
log "Opening validation fixture in Safari..."
open -a Safari "$FIXTURE"
sleep 2
log "✓ Safari opened with fixture"

record "## Fixture"
record "- File: $FIXTURE"
record "- Opened in: Safari"
record ""

# --- Permission check ---

log ""
log "Checking Accessibility permission..."
# The binary itself needs AX permission when run from terminal
AX_CHECK=$(swift -e 'import ApplicationServices; print(AXIsProcessTrusted())' 2>/dev/null || echo "error")
if [[ "$AX_CHECK" == *"true"* ]]; then
    log "✓ Accessibility permission granted"
    record "## Permissions"
    record "- Accessibility: GRANTED"
else
    log "⚠ Accessibility permission NOT granted"
    log ""
    log "  To grant:"
    log "  1. System Settings → Privacy & Security → Accessibility"
    log "  2. Add/enable your Terminal app or the lens binary"
    log "  3. Re-run this script"
    log ""
    read -p "  Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
    record "## Permissions"
    record "- Accessibility: NOT GRANTED (user chose to continue)"
fi

# --- Start product ---

log ""
log "Starting Mori Lens..."
"$BINARY" &
LENS_PID=$!
sleep 3

if ps -p "$LENS_PID" >/dev/null 2>&1; then
    log "✓ Mori Lens running (PID: $LENS_PID)"
    record "## Product Startup"
    record "- Status: RUNNING"
    record "- PID: $LENS_PID"
else
    log "✗ Mori Lens failed to start or crashed"
    record "## Product Startup"
    record "- Status: CRASHED"
fi

# --- Visual checkpoints ---

log ""
log "========================================="
log "VISUAL CHECKPOINT — Please verify:"
log "========================================="
log ""
log "1. Is the Mori Lens overlay/card visible near text in Safari?"
log "2. Can you click through the card to interact with Safari?"
log "3. Does the card appear near your pointer?"
log ""
read -p "Overlay visible? (y/N): " -n 1 -r OVERLAY_VISIBLE
echo
read -p "Click-through works? (y/N): " -n 1 -r CLICK_THROUGH
echo
read -p "Card follows pointer? (y/N): " -n 1 -r FOLLOWS_POINTER
echo

record "## Overlay Validation"
record "- Overlay visible: ${OVERLAY_VISIBLE:-N}"
record "- Click-through: ${CLICK_THROUGH:-N}"
record "- Follows pointer: ${FOLLOWS_POINTER:-N}"
record ""

# --- Hotkey test ---

log ""
log "========================================="
log "HOTKEY CHECKPOINT"
log "========================================="
log ""
log "With Safari in foreground, try:"
log "  Cmd+Opt+L → Pause/Resume"
log "  Cmd+Opt+O → Manual OCR"
log "  Cmd+Opt+Q → Quit (saves state, don't press yet!)"
log ""
read -p "Hotkeys working? (y/N): " -n 1 -r HOTKEYS_WORK
echo

record "## Hotkey Validation"
record "- Hotkeys working: ${HOTKEYS_WORK:-N}"
record ""

# --- OCR test ---

log ""
log "========================================="
log "OCR CHECKPOINT"
log ""
log "Move pointer over Korean text in fixture:"
log "  '모리 렌즈는 화면의 글자를 읽습니다.'"
log ""
log "Then over Japanese text:"
log "  'モリレンズは画面の文字を読みます。'"
log ""
log "Then over repeated words:"
log "  'cat cat cat' — hover over the SECOND 'cat'"
log "========================================="
log ""
read -p "Korean OCR works? (y/N): " -n 1 -r KOREAN_OCR
echo
read -p "Japanese OCR works? (y/N): " -n 1 -r JAPANESE_OCR
echo
read -p "Repeated word targets correctly? (y/N): " -n 1 -r REPEATED_WORD
echo

record "## OCR Validation"
record "- Korean: ${KOREAN_OCR:-N}"
record "- Japanese: ${JAPANESE_OCR:-N}"
record "- Repeated word: ${REPEATED_WORD:-N}"
record ""

# --- Cleanup ---

log ""
log "Cleaning up..."
if ps -p "$LENS_PID" >/dev/null 2>&1; then
    kill "$LENS_PID" 2>/dev/null || true
    sleep 1
    if ps -p "$LENS_PID" >/dev/null 2>&1; then
        kill -9 "$LENS_PID" 2>/dev/null || true
    fi
    log "✓ Mori Lens stopped"
fi

# Close Safari fixture window (not the whole app)
osascript -e 'tell application "Safari" to close front window' 2>/dev/null || true
log "✓ Safari fixture closed"

record "## Cleanup"
record "- Product stopped: YES"
record "- Safari fixture closed: YES"
record ""
record "---"
record "Validation complete."

log ""
log "========================================="
log "Evidence recorded to:"
log "  $EVIDENCE_FILE"
log "========================================="
log ""
log "Done."
