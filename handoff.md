# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS is the active workstream.
This session fixed external-review findings across OCR, hotkey, and docs.

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer, release pipeline configured (unsigned — code signing is external credential debt)

**macOS (production-wired, correctness-hardened, fully wired):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR production-wired with per-word geometry (`boundingBoxForRange`)
- Passive native overlay (NSPanel, transparent, click-through)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- AppKit thread-safe: all UI mutations dispatched to main queue
- Async translation: Quick/Deep on separate dedicated threads
- Stale-safe: generation-based result filtering
- Live settings: Settings UI changes propagate to runtime
- Correct capture: pointer-based display selection, actual clipped origin returned
- Correct pixel format: BGRA canonical throughout capture→Vision pipeline
- Safe lifetime: SCStream single-owner stop, no stack-capture UAF
- Hotkey recovery: tap-disabled re-enable, true SPSC ring buffer
- Critical commands: Quit/Pause use atomic flags bypassing ring buffer
- Overlay exclusion: Lens overlay excluded from ScreenCaptureKit capture
- Shutdown cleanup: hotkey removal, overlay hide, Ollama model release
- Diagnostics wired: Tally tracks cards/hovers/refusals, shared with webview
- Settings hot-path: generation counter avoids full clone every 25ms
- Policy enforcement: `process_name_at()` → `policy.allows()` before acquisition
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)
- Cross-platform CI configured (manual trigger)

## This Session

- **PR #288** — fix/macos-ocr-capture-geometry: pointer-based display selection, actual clipped origin returned, BGRA pixel format corrected
- **PR #289** — fix/macos-hotkey-queue-concurrency: true SPSC (drop-newest overflow), critical command flags for Quit/Pause
- **PR #290** — docs/product-truth-reconciliation: unsigned release (not signed), M7 COMPLETE, git-state non-self-referential
- **PR #291** — fix/macos-vision-word-ranges: per-word geometry via `boundingBoxForRange`, UTF-8 offset tracking

## Agent Assessment

```
OCR DISPLAY OWNERSHIP:            PRODUCTION-WIRED
  (pointer-based display selection, not band-origin)

OCR CLIPPED GEOMETRY:             PRODUCTION-WIRED
  (actual clipped origin returned via out_origin_x/y/scale,
   Rust uses captured origin for global coordinate reconstruction)

VISION WORD TARGETING:            PRODUCTION-WIRED
  (boundingBoxForRange per-word geometry, UTF-8 offset/length,
   repeated words resolved by index not String::find)

PIXEL FORMAT:                     PRODUCTION-WIRED
  (BGRA canonical: capture outputs BGRA, Vision interprets BGRA,
   kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little)

HOTKEY QUEUE CONCURRENCY:         PRODUCTION-WIRED
  (true SPSC: producer only writes head, consumer only writes tail,
   drop-newest overflow, critical commands via atomic flags)

CGEVENTTAP RECOVERY:              PRODUCTION-WIRED
SCSTREAM LIFETIME:                PRODUCTION-WIRED

M6 PRODUCTION INTEGRATION:        COMPLETE
M7 PRODUCTION INTEGRATION:        COMPLETE
  (per-word geometry, correct display selection, BGRA format)
M6 REAL-WORLD EVIDENCE:           PENDING
M7 REAL-WORLD EVIDENCE:           PENDING

MACOS PRODUCT PARITY:             COMPLETE

WINDOWS RELEASE SIGNING:          NOT CONFIGURED (CREDENTIAL BLOCKER)
MACOS RELEASE SIGNING:            NOT CONFIGURED (CREDENTIAL BLOCKER)
CI:                               CONFIGURED, STATICALLY AUDITED (manual trigger only)

REMAINING IMPLEMENTATION-READY:   NONE
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral (ADR-0003).
- Screen capture uses ScreenCaptureKit `SCStream` with `sourceRect` for actual crop.
- Display selection uses pointer position (not band origin) for correct multi-monitor.
- Capture returns actual clipped origin + scale for coordinate reconstruction.
- Pixel format is BGRA throughout: capture → Vision → CGImage.
- Vision OCR uses `boundingBoxForRange:` for per-word geometry.
- Word text extracted via UTF-8 offsets (not `String::find`).
- Accessibility-first OCR: AX first; OCR fallback only when `ocr_can_help()`.
- Policy check BEFORE content acquisition.
- Tauri owns main thread. Product loop on worker thread.
- All AppKit UI mutations use `dispatch_async(dispatch_get_mainQueue())`.
- Translation on two dedicated threads (Quick lane + Deep lane).
- Global hotkeys via CGEventTap with `kCGEventTapOptionDefault`.
- Hotkey queue: true SPSC ring buffer + atomic flags for critical commands.
- Memory-only privacy: no screenshot/OCR image written to disk.
- Live settings: generation counter avoids full Settings clone every 25ms.
- SCStream: caller owns stop lifecycle. No stack-capture in completion handlers.
- Overlay exclusion: `lens_overlay_window_id()` → `SCContentFilter excludingWindows`.
- Shutdown: explicit cleanup order (hotkey → overlay → model release → exit).

## Production Call Paths

**Hover → Translation (async, two-lane):**
```
Tauri event loop (main thread)
  ← product_loop (worker thread)
    → Live settings: generation check → clone only if changed
    → Hotkey/menubar poll → ProductCommand dispatch
    → HoverMachine::observe() → process_name_at → policy.allows()
    → MacTextAcquirer [AX] or OcrTextAcquirer [OCR fallback]
    → quick_tx.send() or deep_tx.send()
    → result_rx.try_recv() → generation check → show_card_at
    → update_diagnostics (tally → shared snapshot)
```

**Screen capture:**
```
capture_region(req_x/y/w/h, ptr_x/y)
  → find_display_for_point(content, ptr_x, ptr_y)
  → clip to display bounds
  → sourceRect + BGRA pixel format
  → excludingWindows: [overlay window ID]
  → start → poll frame → stop
  → return pixels + actual_origin_x/y + scale
```

**Vision OCR:**
```
recognize(pixels, w, h, stride) [BGRA]
  → CGImage with BGRA bitmapInfo
  → VNRecognizeTextRequest
  → per observation: enumerateSubstringsByWords
  → boundingBoxForRange per word
  → return text + per-word [x,y,w,h,utf8_offset,utf8_length]
```

## Important Files

```
apps/lens/src/macos.rs              macOS product loop, live settings, diagnostics, shutdown
apps/lens/src/settings_service.rs   Settings state, pending slot, generation counter, tests (6)
crates/platform-macos/native/capture.m  ScreenCaptureKit, pointer display selection, overlay exclusion
crates/platform-macos/native/vision_ocr.m  Vision per-word geometry, BGRA format
crates/platform-macos/native/hotkey.m   CGEventTap, true SPSC ring buffer, critical flags
crates/platform-macos/src/capture.rs    Capture struct with origin_x/y/scale
crates/platform-macos/src/vision_ocr.rs RecognizedWord with utf8_offset/length
crates/platform-macos/src/ocr_acquire.rs  Uses captured origin + word offsets
crates/platform-macos/src/hotkey.rs     Ring buffer tests (9)
crates/platform-macos/src/coords.rs     Display topology, crop rect tests (11)
crates/lens-translate/src/http.rs       HTTP/1.1, timeouts, fault tests (8)
crates/lens-core/src/policy.rs          AppPolicy with PartialEq
```

## Latest Implementation Changes

- PR #291 — Vision per-word geometry via boundingBoxForRange
- PR #290 — Docs truth reconciliation (unsigned release, M7 complete, git-state)
- PR #289 — Hotkey queue true SPSC + critical command flags
- PR #288 — OCR capture geometry + pixel format fix

## Automated Evidence

```
scripts/check.sh --full:     PASS
workspace tests:             PASS (665+ tests)
lens binary tests:           PASS (6 settings tests, platform-specific)
platform-macos tests:        PASS (49 tests)
lens-translate tests:        PASS (8 HTTP fault tests)
cargo fmt --check:           CLEAN
cargo clippy -D warnings:    CLEAN
```

## Known Implementation Gaps

1. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- SCStream: if stop completion never arrives, 5s timeout returns. ARC prevents UAF.
- Translation: per-read timeout allows long generation if each read makes progress.
- Settings: CLI overrides superseded by Settings UI at runtime. Matches Windows contract.

## Review Targets

- Verify per-word boundingBoxForRange returns correct boxes for Korean/Japanese/emoji.
- Verify BGRA interpretation is correct end-to-end (capture → Vision → OCR text).
- Verify pointer-based display selection works near display edges.
- Verify critical command flags survive 100k+ command storms.
- Verify actual clipped origin matches Vision local-to-global coordinate mapping.

## Open Issues / Gates

```
#148  colour linkage quality gate     QUALITY GATE OPEN
#140  unzip → first card measurement  HUMAN VALIDATION
#34   KakaoTalk real-app reach        HUMAN VALIDATION
#14   VS Code real-app reach          HUMAN VALIDATION
```

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition
- Overlay visual quality, OCR accuracy with per-word geometry
- Permission onboarding, sleep/wake, display hot-plug
- Multi-monitor OCR near display edges
- Korean/Japanese/emoji word targeting accuracy

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
latest implementation merge: PR #291 (Vision per-word geometry)
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
- macOS native code uses Objective-C bridges compiled by `cc` crate.
- All unsafe blocks require SAFETY comments (clippy `-D warnings`).
- `apps/lens` is not a workspace member (Tauri dependency); test separately.
