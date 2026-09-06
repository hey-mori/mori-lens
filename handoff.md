# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS is the active workstream.
This session verified and fixed the automated verification gate itself.

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
- Typed FFI ABI (`LensOcrWord` struct with `uint32_t` offsets, not floats)
- Input validation + memory safety (overflow checks, allocation failure handling)
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
- Privacy static gate: Windows + macOS Rust + macOS native Objective-C
- Product binary: `apps/lens` cargo check + test (not workspace member)
- Clean native rebuild: `cargo clean -p lens-platform-macos` + check + test
- Version consistency: workspace vs apps/lens vs tauri.conf.json

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)
- Cross-platform CI configured (manual trigger)

## This Session

- **PR #292** — fix/macos-native-compile-errors: 4 native compile errors found by clean rebuild (ARC violations, VNRectangleObservation return type, SCDisplay.frame, forward declaration)
- **PR #293** — fix/ocr-typed-ffi-and-safety: typed LensOcrWord struct, uint32_t offsets, input validation, memory safety
- **PR #294** — fix/macos-hotkey-toggle-semantics: Pause uses atomic_int counter (preserves multiplicity), Quit stays bool
- **PR #295** — build/product-verification-gate: apps/lens in --full, clean native rebuild, macOS privacy gate, cross-platform keeping tests

## Agent Assessment

```
VISION API CONTRACT:            CLEAN-COMPILED
  (boundingBoxForRange: returns VNRectangleObservation *,
   .boundingBox extracted to CGRect, finite coordinate validation)

VISION CLEAN NATIVE BUILD:      VERIFIED
  (cargo clean -p lens-platform-macos + cargo check passes)

OCR FFI ABI:                    PRODUCTION-WIRED
  (LensOcrWord struct: float geometry + uint32_t offsets,
   repr(C) mirror in Rust, no float-encoded integers)

OCR MEMORY SAFETY:              STATICALLY AUDITED
  (input validation: stride >= width*4, overflow checks,
   allocation failure handling, safe cleanup on all error paths)

OCR WORD/RANGE TARGETING:       PRODUCTION-WIRED
  (boundingBoxForRange per-word, UTF-8 offset/length,
   repeated words resolved by index not String::find)

HOTKEY TOGGLE SEMANTICS:        PRODUCTION-WIRED
  (Pause: atomic_int counter preserves multiplicity,
   Quit: atomic_bool idempotent, 11 tests)

HOTKEY CONCURRENCY:             PRODUCTION-WIRED
  (true SPSC: producer only writes head, consumer only writes tail,
   drop-newest overflow, critical commands bypass ring buffer)

WORKSPACE VERIFICATION:         PASS
  (fmt, clippy -D warnings, 665+ tests)

PRODUCT BINARY VERIFICATION:    PASS
  (apps/lens cargo check + test in scripts/check.sh --full)

MACOS PRIVACY GATE:             STATICALLY AUDITED
  (Rust capture/OCR paths + native .m sources checked for
   file/persistence APIs in scripts/check.sh)

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
- Word text extracted via UTF-8 offsets (not `String::find`).
- Hotkey queue: true SPSC ring buffer + atomic flags/counter for critical commands.
- Pause uses atomic_int counter to preserve toggle multiplicity.
- All AppKit UI mutations use `dispatch_async(dispatch_get_mainQueue())` (ARC-clean).
- Verification gate includes product binary + clean native rebuild.
- Privacy gate covers Windows + macOS Rust + macOS native Objective-C.

## Important Files

```
scripts/check.sh                    Verification gate (workspace + product + native)
apps/lens/src/macos.rs              macOS product loop, live settings, diagnostics, shutdown
crates/platform-macos/native/vision_ocr.m  Vision per-word geometry, BGRA, typed ABI
crates/platform-macos/native/capture.m     ScreenCaptureKit, pointer display selection
crates/platform-macos/native/hotkey.m      CGEventTap, true SPSC, pause counter
crates/platform-macos/native/overlay.m     NSPanel, ARC-clean, overlay exclusion
crates/platform-macos/src/vision_ocr.rs    LensOcrWord repr(C), input validation
crates/platform-macos/src/capture.rs       Capture struct with origin_x/y/scale
crates/platform-macos/src/hotkey.rs        Ring buffer + pause counter tests (11)
```

## Automated Evidence

```
scripts/check.sh --full:          PASS (all steps)
workspace tests:                  PASS (665+ tests)
product binary (apps/lens):       PASS (check + test)
clean native rebuild:             PASS (cargo clean + check + test)
platform-macos tests:             PASS (49 tests including 11 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
cargo fmt --check:                CLEAN
cargo clippy -D warnings:         CLEAN
privacy static gate:              CLEAN (Windows + macOS Rust + native)
```

## Known Implementation Gaps

1. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- SCStream: if stop completion never arrives, 5s timeout returns. ARC prevents UAF.
- Translation: per-read timeout allows long generation if each read makes progress.
- Settings: CLI overrides superseded by Settings UI at runtime. Matches Windows contract.

## Review Targets

- Verify per-word boundingBoxForRange returns correct boxes for Korean/Japanese/emoji.
- Verify BGRA interpretation is correct end-to-end with real OCR.
- Verify pointer-based display selection near display edges with real hardware.
- Verify Pause counter survives 100k+ rapid presses without overflow.

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
latest implementation merge: PR #295 (product verification gate)
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
- `apps/lens` is not a workspace member (Tauri dependency); verified by `scripts/check.sh --full`.
