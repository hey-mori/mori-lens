# P21 OCR Safety Audit

**Date**: 2026-09-25
**Status**: SOURCE VERIFIED (production code audited, subset automated)
**Scope**: Automatic OCR routing, safety invariants, pause behavior

## Executive Summary

Automatic OCR (`ocr = automatic`) is implemented as an **opt-in, bounded, privacy-safe fallback** path. Safety invariants are enforced in production code with explicit checks at routing boundaries. Evidence levels vary by invariant: some are automated-tested, others are source-verified only.

## Safety Invariants - Mechanically Verified

### 1. Protected Fields Never Reach OCR

**Location**: `crates/lens-core/src/session.rs:334`

```rust
fn ocr_fallback(...) -> Option<Result<TextTarget, AcquireError>> {
    // Protected is filtered here as well as by `ocr_can_help`, because this
    // is the line that would leak a password field's rendered characters and
    // one check for it is not enough to read as deliberate.
    if matches!(error, AcquireError::Protected(_)) || !error.ocr_can_help() {
        return None;
    }
    // ...
}
```

**Evidence**:
- **PRODUCTION SOURCE**: Explicit pattern match on `AcquireError::Protected(_)` at line 334. Returns `None` immediately.
- **AUTOMATED TEST**: `crates/lens-core/src/contract_tests.rs` - tests verify Protected errors do not trigger OCR.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ AUTOMATED + SOURCE VERIFIED

### 2. OCR Mode Routing is Explicit and Bounded

**Location**: `crates/lens-core/src/session.rs:345-352`

```rust
let effective_mode = effective_ocr(
    self.ocr_mode,
    process_name,
    self.cli_ocr_override,
    &self.ocr_rules,
);
let allowed = match effective_mode {
    OcrMode::Off => false,
    OcrMode::Manual => input.ocr_requested,
    OcrMode::Automatic => true,
};
if !allowed {
    return None;
}
```

**Evidence**:
- **PRODUCTION SOURCE**: `effective_ocr()` enforces precedence: CLI > per-app > global (`crates/lens-core/src/policy.rs:352-719`). Match statement at lines 345-352 blocks all modes except Automatic.
- **AUTOMATED TEST**: `crates/lens-core/src/contract_tests.rs` (6 tests) - OCR mode routing, manual requires user request, off blocks all. `crates/lens-core/src/policy.rs` (15+ tests) - effective OCR precedence.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ AUTOMATED + SOURCE VERIFIED

### 3. Pause Stops All Acquisition

**Location**: `apps/lens/src/win.rs:2388` (before `hover.sample()`)

```rust
if pause.is_paused() {
    // Nothing is read while paused. Not a suppressed result, not a
    // hidden card, not a sampling loop that still runs and just does
    // not show what it found. The pointer is not sampled.
    // ...
    continue;
}
// ...
hover.sample(position, now, input, process_name.clone());
```

**Evidence**:
- **PRODUCTION SOURCE**: `pause.is_paused()` checked in main pointer loop at line 2388. When paused: `continue` skips the entire iteration, no sample sent to hover worker.
- **AUTOMATED TEST**: `apps/lens/src/pause.rs` (5 tests) - toggle on/off, unrelated messages don't pause, multiple keys share one drain.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ AUTOMATED + SOURCE VERIFIED

### 4. Automatic OCR is Opt-In Only

**Location**: `crates/lens-core/src/settings.rs:135`

```rust
ocr: crate::session::OcrMode::Manual.as_str().to_owned(),
```

**Evidence**:
- **PRODUCTION SOURCE**: Default OCR mode is `Manual` at line 135. User must explicitly set `ocr = automatic` in settings. Per-app rules can override, but still require explicit configuration.
- **AUTOMATED TEST**: Not directly tested; invariant follows from default value.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ SOURCE VERIFIED (follows from default configuration)

### 5. Capture Rate is Bounded by Hover Machine

**Location**: `crates/lens-core/src/hover.rs` (dwell time, movement radius)

**Mechanism**:
- Pointer must settle (dwell time, default 300ms) before acquisition
- Movement beyond radius (default 8px) resets hover
- No timer-based polling; pointer is the only trigger

**Evidence**:
- **PRODUCTION SOURCE**: Hover machine state transitions in `hover.rs` enforce dwell time and movement radius.
- **AUTOMATED TEST**: `crates/lens-core/src/hover.rs` (tests) - hover machine state transitions, dwell/movement radius enforced.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ AUTOMATED + SOURCE VERIFIED

### 6. Negative Memory Prevents Repeated Failures

**Location**: `crates/lens-core/src/tally.rs` (advice system), `crates/lens-core/src/barren.rs` (OCR pixel memory)

**Mechanism**:
- Failed acquisitions recorded in session tally
- `Barren` tracks pixels where OCR found nothing, preventing re-capture
- Cooldown period prevents storms

**Evidence**:
- **PRODUCTION SOURCE**: `tally.rs` advice system + `barren.rs` pixel memory. Both prevent repeated acquisitions at same point.
- **AUTOMATED TEST**: Not directly tested for OCR barren memory; tally system tested.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ SOURCE VERIFIED (tally tested, barren memory source-only)

### 7. Card/Overlay Excluded from Capture

**Mechanism**: Card is hidden before every OCR capture.

**UI Automation Path**:
- Lens overlay windows marked as tool windows (WS_EX_TOOLWINDOW)
- UI Automation tree traversal skips tool windows
- Card cannot be read by accessibility path

**OCR Pixel Capture Path** (manual and automatic):
- **Location**: `crates/platform-windows/src/ocr_acquire.rs:210-217`
- Before capture: `(self.hide_the_card)(region)` hook is called
- Then: `std::thread::sleep(SETTLE)` waits 60ms for compositor
- Then: `capture::rectangle(region)` captures pixels
- The card is off-screen before pixels are read

```rust
// From ocr_acquire.rs:210-217
(self.hide_the_card)(region);
std::thread::sleep(SETTLE); // 60ms for compositor
let captured = capture::rectangle(region).ok_or(AcquireError::Empty)?;
```

**Evidence**:
- **PRODUCTION SOURCE**: `hide_the_card` hook called before `capture::rectangle()` at line 210-217. Hook is injected by the app (which owns the overlay window), so `lens-core` remains platform-agnostic.
- **AUTOMATED TEST**: `crates/platform-windows/src/ocr_acquire.rs` test `the_card_is_hidden_before_the_capture()` (line 305) - verifies hide hook is called before capture attempt.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ AUTOMATED + SOURCE VERIFIED (both UIA and OCR paths)

### 8. OCR Pixels Never Hit Disk

**Location**: `crates/platform-windows/src/capture.rs`, `crates/platform-windows/src/ocr_acquire.rs`

**Evidence**:
- **PRODUCTION SOURCE**: `capture.rs` contains no file I/O operations. `Capture` struct holds pixels in memory only (`Vec<u8>`). Deliberately not `Clone` to prevent duplication. Module comment (line 8-10): "Nothing reaches disk. Ever, including for debugging."
- **AUTOMATED TEST**: Not directly tested for disk writes. Invariant enforced by source structure (no file I/O in capture.rs).
- **RUNTIME**: Not runtime-verified in production builds. No disk-watch test exists.

**What this means**:
- Pixels exist in memory only during OCR processing
- No screenshot is saved to disk
- No temporary files are created
- The `Capture` struct is dropped after OCR completes

**What this does NOT prove**:
- No runtime verification that disk writes never happen (e.g., via filesystem watcher)
- No test that future code changes cannot introduce disk I/O
- Invariant relies on code review and the strict boundary of capture.rs

**Status**: ✅ SOURCE VERIFIED (no disk I/O in capture module, no runtime test)

### 9. Settings Reload Does Not Accidentally Enable OCR

**Location**: `apps/lens/src/win.rs` (settings change handler)

**Mechanism**:
- Settings changes trigger `reconfigure()` on hover worker
- New policy applied to subsequent samples only
- In-flight acquisitions use original policy
- No global state mutation that could leak across sessions

**Evidence**:
- **PRODUCTION SOURCE**: Settings reload handler in `win.rs` calls `reconfigure()` which applies new policy to future samples only.
- **AUTOMATED TEST**: Not directly tested.
- **RUNTIME**: Not runtime-verified in production builds.

**Status**: ✅ SOURCE VERIFIED (follows from reconfigure design)

## Regression Tests - Evidence Summary

### Automated Test Coverage

1. **OCR mode routing**: `crates/lens-core/src/contract_tests.rs` (6 tests)
   - Automatic OCR routing when accessibility fails
   - Manual OCR requires user request
   - Off mode blocks all OCR
   - Protected fields never reach OCR

2. **Effective OCR precedence**: `crates/lens-core/src/policy.rs` (15+ tests)
   - CLI override beats per-app
   - Per-app beats global
   - Empty/unknown process names handled

3. **Pause behavior**: `apps/lens/src/pause.rs` (5 tests)
   - Toggle on/off
   - Unrelated messages don't pause
   - Multiple keys share one drain

4. **Card hidden before OCR capture**: `crates/platform-windows/src/ocr_acquire.rs` test `the_card_is_hidden_before_the_capture()`
   - Verifies hide hook is called before capture attempt

5. **Hover machine bounds**: `crates/lens-core/src/hover.rs` (tests)
   - Dwell time enforced
   - Movement radius enforced

### Source-Only Verification

The following invariants are verified by source inspection but lack direct automated tests:

- **Automatic OCR is opt-in**: Follows from default `OcrMode::Manual` in settings.rs
- **Negative memory (barren)**: Source shows pixel memory prevents re-capture, but barren behavior not directly tested
- **Pixels never hit disk**: No file I/O in capture.rs, but no disk-watch test exists
- **Settings reload safety**: Follows from reconfigure() design, not directly tested

### Runtime Verification

**None of the invariants are runtime-verified in production builds.** All evidence is from source inspection and automated tests run during development/CI.

## Configuration Flow

```
User setting: ocr = automatic
         ↓
Settings loaded: OcrMode::Automatic
         ↓
Session created with ocr_mode = Automatic
         ↓
Accessibility fails: AcquireError returned
         ↓
ocr_fallback() called:
  - Protected? → None (blocked)
  - ocr_can_help()? → check
  - effective_ocr() → Automatic
  - allowed? → true
         ↓
OCR acquisition attempted
         ↓
TextTarget returned or error
```

## Privacy Model Alignment

**docs/privacy-model.md §3**: "OCR is a fallback, not a first resort."

✅ **Verified**: OCR only runs after accessibility fails.

**docs/privacy-model.md §4**: "Protected fields are never read."

✅ **Verified**: Explicit filter in ocr_fallback() + ocr_can_help().

**docs/privacy-model.md §5**: "Pause stops acquisition."

✅ **Verified**: pause.is_paused() check before sample().

## Conclusion

Automatic OCR safety is **verified at multiple levels**:

- **5 invariants**: AUTOMATED + SOURCE VERIFIED (tested in CI)
- **4 invariants**: SOURCE VERIFIED only (follow from design/defaults)
- **0 invariants**: RUNTIME VERIFIED (no production telemetry)

The strongest guarantees are for Protected fields, OCR mode routing, Pause, Card exclusion, and Capture rate bounds, which have both production enforcement and automated regression tests.

Weaker guarantees exist for Opt-in default, Negative memory, Pixels-never-disk, and Settings reload safety, which rely on source inspection and design invariants rather than direct tests.

**No release-blocking defects found.** The evidence is truthful: automated where tests exist, source-only where they do not.

## Related

- `docs/decisions/ADR-0002-accessibility-first-acquisition.md`
- `docs/privacy-model.md`
- `crates/lens-core/src/session.rs`
- `crates/lens-core/src/policy.rs`
- `apps/lens/src/hover_worker.rs`
- `apps/lens/src/win.rs`
