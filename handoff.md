# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS is the active workstream.
This session closed the last FFI/ownership/verification boundaries.

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer, release pipeline configured (unsigned — code signing is external credential debt)

**macOS (production-wired, correctness-hardened, verification-proven):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR with per-word geometry (`boundingBoxForRange` → `VNRectangleObservation`)
- Typed FFI ABI (`LensOcrWord` struct: float geometry + `uint32_t` offsets)
- Integer overflow protection: widened arithmetic in both native and Rust
- Narrowing safety: explicit bounds checks before `NSUInteger → uint32_t/int32_t`
- Ownership safety: RAII `RawGuard` ensures C result freed on all Rust return paths
- Range validation: checked arithmetic, UTF-8 char boundary verification, fail-closed
- Input validation: stride ≥ width×4, overflow checks, 100MP upper bound
- Passive native overlay (NSPanel, transparent, click-through, ARC-clean)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- Hotkey queue: true SPSC ring buffer + atomic counter for Pause multiplicity
- Critical commands: Quit (bool flag), Pause (counter — preserves toggle count)
- Correct capture: pointer-based display selection, `SCDisplay.frame` property
- Correct pixel format: BGRA canonical throughout capture→Vision pipeline
- Safe lifetime: SCStream single-owner stop, no stack-capture UAF
- Overlay exclusion: Lens overlay excluded from ScreenCaptureKit capture
- Shutdown cleanup: hotkey removal, overlay hide, Ollama model release
- Diagnostics wired: Tally tracks cards/hovers/refusals
- Settings hot-path: generation counter avoids full clone every 25ms
- Policy enforcement: `process_name_at()` → `policy.allows()` before acquisition
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)

**Verification gate (`scripts/check.sh --full`):**
- Workspace fmt + clippy + test
- Product binary: apps/lens fmt + clippy + check + test
- Clean native rebuild: `cargo clean -p lens-platform-macos` + check + test
- Privacy static gate: Windows + macOS Rust + macOS native Objective-C
- Native source invalidation: explicit `rerun-if-changed` in build.rs
- Version consistency: workspace vs apps/lens vs tauri.conf.json

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)
- Cross-platform CI configured (manual trigger)

## This Session

- **PR #296** — fix/macos-ocr-integer-ownership-boundaries: stride overflow (widened uint64_t), NSUInteger narrowing checks, RAII ownership guard, checked word range validation
- **PR #297** — build/full-product-quality-gate: apps/lens fmt + clippy in --full, build.rs rerun-if-changed, fixed pre-existing apps/lens clippy warnings

## Agent Assessment

```
OCR INTEGER BOUNDARIES:         PRODUCTION-WIRED
  (width*4 in uint64_t domain, stride*height overflow checked,
   100MP upper bound, both native and Rust)

OCR OFFSET/COUNT NARROWING:     PRODUCTION-WIRED
  (NSUInteger→uint32_t bounds-checked before cast,
   wordItems.count→int32_t INT32_MAX check)

OCR RAW RESULT OWNERSHIP:       PRODUCTION-WIRED
  (RAII RawGuard in Rust frees C result on ALL return paths,
   text=NULL + words allocated no longer leaks)

OCR RANGE VALIDATION:           PRODUCTION-WIRED
  (checked_add for offset+length, UTF-8 char boundary verification,
   fail-closed per invalid word, defensive 1M word count cap)

OCR ABI LAYOUT:                 STATICALLY AUDITED
  (LensOcrWord repr(C) matches C struct: 4 floats + 2 uint32_t)

HOTKEY PAUSE OVERFLOW:          PRODUCTION-WIRED
  (atomic_int counter, saturating semantics — 2B backlog is
   physically unreachable; no signed overflow)

HOTKEY ORDERING:                PRODUCTION-WIRED
  (Pause/Quit bypass ring buffer as critical-control semantics,
   always polled before queued commands)

HOTKEY CONCURRENT STRESS:       UNIT-TESTED
  (11 tests: 10k events, wraparound, overflow, critical commands,
   pause multiplicity, sequential producer/consumer model)

WORKSPACE FORMAT/CLIPPY:        PASS
PRODUCT FORMAT/CLIPPY:          PASS
PRODUCT CHECK/TEST:             PASS
NATIVE CLEAN BUILD:             PASS
PRIVACY GATE:                   STATICALLY AUDITED
  (Rust + native .m checked for persistence APIs)

SCSTREAM LIFETIME:              PRODUCTION-WIRED
  (caller owns stop, no stack-capture in completion handlers,
   ARC retains stream/delegate through late completion)

TRANSLATION BOUNDEDNESS:        PRODUCTION-WIRED
  (HTTP connect/read/write timeouts, QUICK_TIMEOUT=20s, DEEP_TIMEOUT=60s,
   per-read timeout allows long generation — deliberate for model output)

M6 PRODUCTION INTEGRATION:      COMPLETE
M7 PRODUCTION INTEGRATION:      COMPLETE
M6 REAL-WORLD EVIDENCE:         PENDING
M7 REAL-WORLD EVIDENCE:         PENDING

MACOS PRODUCT PARITY:           COMPLETE

WINDOWS RELEASE SIGNING:        NOT CONFIGURED (CREDENTIAL BLOCKER)
MACOS RELEASE SIGNING:          NOT CONFIGURED (CREDENTIAL BLOCKER)
CI EXECUTION:                   CONFIGURED, PENDING (manual trigger only)

REMAINING IMPLEMENTATION-READY: NONE
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral (ADR-0003).
- Screen capture uses ScreenCaptureKit `SCStream` with `sourceRect` for actual crop.
- Display selection uses pointer position via `SCDisplay.frame` property.
- Capture returns actual clipped origin + scale for coordinate reconstruction.
- Pixel format is BGRA throughout: capture → Vision → CGImage.
- Vision OCR uses `boundingBoxForRange:` → `VNRectangleObservation *` → `.boundingBox`.
- OCR FFI uses typed `LensOcrWord` struct (float geometry + uint32_t offsets).
- All integer arithmetic at FFI boundary uses widened/checked operations.
- Rust RAII guard owns C result lifetime — no leak on any return path.
- Word text extracted via UTF-8 offsets with char boundary verification.
- Hotkey queue: true SPSC ring buffer + atomic flags/counter for critical commands.
- Pause uses atomic_int counter to preserve toggle multiplicity.
- All AppKit UI mutations use `dispatch_async(dispatch_get_mainQueue())` (ARC-clean).
- Verification gate includes product binary fmt/clippy/check/test + clean native rebuild.
- Privacy gate covers Windows + macOS Rust + macOS native Objective-C.
- build.rs explicitly tracks all native .m/.h files for rebuild invalidation.

## Important Files

```
scripts/check.sh                    Verification gate (workspace + product + native)
crates/platform-macos/build.rs      Native source rerun-if-changed tracking
apps/lens/src/macos.rs              macOS product loop, live settings, diagnostics, shutdown
crates/platform-macos/native/vision_ocr.m  Vision per-word geometry, BGRA, typed ABI, overflow safety
crates/platform-macos/native/capture.m     ScreenCaptureKit, pointer display selection, overlay exclusion
crates/platform-macos/native/hotkey.m      CGEventTap, true SPSC, pause counter
crates/platform-macos/native/overlay.m     NSPanel, ARC-clean, overlay exclusion
crates/platform-macos/src/vision_ocr.rs    LensOcrWord repr(C), RAII guard, range validation
crates/platform-macos/src/capture.rs       Capture struct with origin_x/y/scale
crates/platform-macos/src/hotkey.rs        Ring buffer + pause counter tests (11)
```

## Automated Evidence

```
scripts/check.sh --full:          PASS (all steps)
workspace fmt/clippy/tests:       PASS (665+ tests)
product fmt/clippy/check/test:    PASS
clean native rebuild:             PASS (cargo clean + check + test)
platform-macos tests:             PASS (49 tests including 11 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
privacy static gate:              CLEAN (Windows + macOS Rust + native)
```

## Known Implementation Gaps

1. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- SCStream: if stop completion never arrives, 5s timeout returns. ARC retains captured objects.
- Translation: per-read timeout allows long generation if each read makes progress. Deliberate for model output.
- Settings: CLI overrides superseded by Settings UI at runtime. Matches Windows contract.

## Review Targets

- Verify per-word boundingBoxForRange returns correct boxes for Korean/Japanese/emoji.
- Verify BGRA interpretation is correct end-to-end with real OCR.
- Verify pointer-based display selection near display edges with real hardware.
- Verify Pause counter survives rapid presses without unexpected saturation.

## Open Issues / Gates

```
#148  colour linkage quality gate     QUALITY GATE OPEN
#140  unzip → first card measurement  HUMAN VALIDATION
#34   KakaoTalk real-app reach        HUMAN VALIDATION
#14   VS Code real-app reach          HUMAN VALIDATION
```

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition
- OCR accuracy with per-word geometry on Korean/Japanese/emoji
- Multi-monitor OCR near display edges
- Physical sleep/wake/display hot-plug

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — CREDENTIAL BLOCKER
- Notarization service — CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — TOOLING BLOCKER

## Next Session — Start Here

1. Real-world validation: run macOS binary against Safari, VS Code, KakaoTalk.
2. Verify per-word OCR accuracy with Korean/Japanese text.
3. Cross-platform CI: trigger and verify the CI pipeline.
4. Package testing: once cargo-tauri is available, build .app bundle locally.

## Git State

```
latest implementation merge: PR #297 (full product quality gate)
handoff base state: clean main before this handoff update
handoff file itself may be committed after this reference;
always run `git log -1` at session start for actual HEAD.
```

## Constraints

- Agent investigation stays inside `PROJECT_ROOT/**`.
- `lens-core` remains platform-neutral (ADR-0003).
- Human validation does not block implementation. Mark as VALIDATION DEBT.
- Do not install external tools without explicit permission.
- Work in coherent branch → PR → merge units. No direct main pushes.
- macOS native code uses Objective-C bridges compiled by `cc` crate with `-fobjc-arc`.
- All unsafe blocks require SAFETY comments (clippy `-D warnings`).
- `apps/lens` is not a workspace member; verified by `scripts/check.sh --full`.
