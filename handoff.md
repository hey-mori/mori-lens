# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS is the active workstream.
**Implementation phase is complete. Evidence collection phase is underway.**

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer, release pipeline configured (unsigned — code signing is external credential debt)

**macOS (implementation complete, evidence pending):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR with per-word geometry (`boundingBoxForRange` → `VNRectangleObservation`)
- Typed FFI ABI (`LensOcrWord` struct: float geometry + `uint32_t` offsets)
- Integer overflow protection: widened arithmetic in both native and Rust
- Narrowing safety: explicit bounds checks before `NSUInteger → uint32_t/int32_t`
- Ownership safety: RAII `RawGuard` ensures C result freed on all Rust return paths
- Range validation: checked arithmetic, UTF-8 char boundary verification, fail-closed
- Passive native overlay (NSPanel, transparent, click-through, ARC-clean)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- Hotkey queue: true SPSC ring buffer + saturating unsigned Pause counter (max 1024)
- Critical commands: Quit (bool, highest priority), Pause (saturating counter, second priority)
- Ordering contract: Quit > Pause > queued FIFO (tested with 16 tests)
- Correct capture: pointer-based display selection, `SCDisplay.frame` property
- Correct pixel format: BGRA canonical throughout capture→Vision pipeline
- Safe lifetime: SCStream single-owner stop, no stack-capture UAF
- Overlay exclusion: Lens overlay excluded from ScreenCaptureKit via CGWindowID
- Shutdown cleanup: hotkey removal, overlay hide, Ollama model release
- Diagnostics wired: Tally tracks cards/hovers/refusals
- Settings hot-path: generation counter avoids full clone every 25ms
- Policy enforcement: `process_name_at()` → `policy.allows()` before acquisition

**Verification gate (`scripts/check.sh --full`):**
- Fast tier (`scripts/check.sh`): workspace fmt + clippy + test
- Product tier (`--full`): workspace + apps/lens fmt/clippy/check/test + clean native rebuild + privacy gates + version consistency + probe release build

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)

## This Session

- **PR #298** — fix/macos-final-hotkey-boundary: Pause saturating unsigned counter (CAS, max 1024), ordering contract tests (Quit > Pause > FIFO), check.sh doc update

## Agent Assessment

```
IMPLEMENTATION PHASE:             COMPLETE
  (all concrete production correctness gaps closed)

HOTKEY PAUSE SATURATION:          PRODUCTION-WIRED
  (atomic_uint, CAS saturating increment, max 1024, no signed overflow)

HOTKEY ORDERING:                  PRODUCTION-WIRED
  (Quit > Pause > queued FIFO, deliberate critical-control semantics,
   5 ordering tests)

HOTKEY CONCURRENCY EVIDENCE:      CONCURRENCY-MODEL-TESTED
  (16 sequential SimQueue tests mirroring native algorithm;
   native concurrent runtime evidence: PENDING)

OCR INTEGER BOUNDARIES:           PRODUCTION-WIRED
OCR OFFSET/COUNT NARROWING:       PRODUCTION-WIRED
OCR RAW RESULT OWNERSHIP:         PRODUCTION-WIRED (RAII guard)
OCR RANGE VALIDATION:             PRODUCTION-WIRED (fail-closed)
OCR ABI LAYOUT:                   STATICALLY AUDITED

FULL PRODUCT GATE:                PRODUCT-GATED
  (workspace + apps/lens fmt/clippy/check/test + clean native rebuild)
NATIVE CLEAN BUILD:               CLEAN-COMPILED
PRIVACY GATE:                     STATICALLY AUDITED

SCSTREAM LIFETIME:                PRODUCTION-WIRED
  (caller owns stop, no stack-capture in completion,
   ARC retains stream/delegate through late completion,
   no caller-owned raw pointers captured)

TRANSLATION BOUNDEDNESS:          PRODUCTION-WIRED
  (HTTP connect/read/write timeouts, QUICK_TIMEOUT=20s, DEEP_TIMEOUT=60s,
   per-read timeout allows long generation — deliberate for model output)

M6 IMPLEMENTATION:                COMPLETE
M6 AUTOMATED EVIDENCE:            PRODUCT-GATED (check.sh --full PASS)
M6 REAL-WORLD EVIDENCE:           PENDING

M7 IMPLEMENTATION:                COMPLETE
M7 AUTOMATED EVIDENCE:            PRODUCT-GATED (check.sh --full PASS)
M7 REAL-WORLD EVIDENCE:           PENDING

MACOS PRODUCT PARITY:             COMPLETE (implementation)

CI EXECUTION:                     CONFIGURED, NOT EXECUTED
  (manual trigger only — deliberate policy, no workflow_dispatch runs)

REMAINING IMPLEMENTATION-READY:   NONE
REMAINING QUALITY GATES:          #148 (colour linkage threshold)
REMAINING HUMAN VALIDATION:       #14 #34 #140 + real OCR quality
REMAINING EXTERNAL DEBT:          signing, notarization, .icns, cargo-tauri
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
- Hotkey queue: true SPSC ring buffer + saturating unsigned Pause counter.
- Critical commands (Quit/Pause) bypass ring buffer — deliberate priority reordering.
- All AppKit UI mutations use `dispatch_async(dispatch_get_mainQueue())` (ARC-clean).
- build.rs explicitly tracks all native .m/.h files for rebuild invalidation.

## Important Files

```
scripts/check.sh                    Two-tier verification gate
crates/platform-macos/build.rs      Native source rerun-if-changed tracking
apps/lens/src/macos.rs              macOS product loop, live settings, shutdown
crates/platform-macos/native/vision_ocr.m  Vision per-word geometry, typed ABI, overflow safety
crates/platform-macos/native/capture.m     ScreenCaptureKit, pointer display selection
crates/platform-macos/native/hotkey.m      CGEventTap, true SPSC, saturating Pause counter
crates/platform-macos/native/overlay.m     NSPanel, ARC-clean, overlay exclusion
crates/platform-macos/src/vision_ocr.rs    LensOcrWord repr(C), RAII guard, range validation
crates/platform-macos/src/capture.rs       Capture struct with origin_x/y/scale
crates/platform-macos/src/hotkey.rs        16 tests (ordering, saturation, stress)
```

## Automated Evidence

```
scripts/check.sh --full:          PASS (all steps)
workspace fmt/clippy/tests:       PASS (665+ tests)
product fmt/clippy/check/test:    PASS
clean native rebuild:             PASS
platform-macos tests:             PASS (55 tests including 16 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
privacy static gate:              CLEAN
```

## Known Implementation Gaps

1. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- Translation: per-read timeout allows long generation if each read makes progress. This is deliberate for model output (not a bug).
- Settings: CLI overrides superseded by Settings UI at runtime. Matches Windows contract.

## Review Targets

- Korean/Japanese Vision word geometry on real rendered text
- Real multi-monitor edge crop behavior
- VS Code editor accessibility text acquisition
- KakaoTalk conversation targeting
- Sleep/wake/display hot-plug lifecycle

## Open Issues / Gates

```
#148  colour linkage quality gate     QUALITY GATE OPEN (threshold not met)
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
- CI execution (manual trigger only, deliberate policy) — POLICY DEBT

## Next Session — Start Here

1. Real-world validation: run macOS binary against Safari, VS Code, KakaoTalk.
2. Verify per-word OCR accuracy with Korean/Japanese text.
3. Cross-platform CI: trigger `scripts/check.sh --full` via workflow_dispatch.
4. Package testing: once cargo-tauri is available, build .app bundle locally.

## Git State

```
latest implementation merge: PR #298 (hotkey saturation + ordering)
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
