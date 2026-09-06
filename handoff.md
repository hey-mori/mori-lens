# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS is the active workstream.
This session completed remaining implementation-ready work across all tracks.

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer, signed release pipeline

**macOS (production-wired, correctness-hardened, fully wired):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR production-wired (`OcrTextAcquirer`)
- Passive native overlay (NSPanel, transparent, click-through)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- AppKit thread-safe: all UI mutations dispatched to main queue
- Async translation: Quick/Deep on separate dedicated threads
- Stale-safe: generation-based result filtering
- Live settings: Settings UI changes propagate to runtime
- Correct capture: ScreenCaptureKit with per-display sourceRect crop
- Safe lifetime: SCStream single-owner stop, no stack-capture UAF
- Hotkey recovery: tap-disabled re-enable, ring buffer with overflow eviction
- Overlay exclusion: Lens overlay excluded from ScreenCaptureKit capture
- Shutdown cleanup: hotkey removal, overlay hide, Ollama model release
- Diagnostics wired: Tally tracks cards/hovers/refusals, shared with webview
- Policy enforcement: `process_name_at()` → `policy.allows()` before acquisition
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)
- Cross-platform CI configured (manual trigger)

## This Session

- **PR #282** — fix/macos-shutdown-cleanup: explicit quit cleanup (hotkey remove, overlay hide, model release)
- **PR #283** — fix/macos-overlay-exclusion: exclude Lens overlay from ScreenCaptureKit via CGWindowID
- **PR #284** — test/http-fault-injection: 6 synthetic TCP server tests + fix oversized body detection gap
- **PR #285** — fix/macos-diagnostics: wire Tally + DiagnosticsSnapshot to product loop
- **PR #286** — test/settings-service-tests: 6 settings tests + fix AppPolicy PartialEq derive

## Agent Assessment

```
SCREEN CAPTURE DISPLAY SELECTION:  PRODUCTION-WIRED
SCREEN CAPTURE REGION CORRECTNESS: PRODUCTION-WIRED
SCSTREAM LIFETIME:                 PRODUCTION-WIRED
OCR MEMORY-ONLY PRIVACY:          STATICALLY AUDITED
OVERLAY EXCLUSION:                 PRODUCTION-WIRED
  (lens_overlay_window_id → SCContentFilter excludingWindows)

GLOBAL HOTKEY CONTRACT:           PRODUCTION-WIRED
CGEVENTTAP RECOVERY:              PRODUCTION-WIRED
HOTKEY QUEUE CORRECTNESS:         UNIT-TESTED

QUICK/DEEP ISOLATION:             PRODUCTION-WIRED
TRANSLATION BOUNDEDNESS:          FAULT-TESTED
  (HTTP connect/read/write timeouts, QUICK_TIMEOUT=20s, DEEP_TIMEOUT=60s,
   black-hole server test, headers-then-stall test, oversized body test,
   streaming stall test, streaming oversized test, empty response test)

LIVE SETTINGS PROPAGATION:        PRODUCTION-WIRED
  (mutable params, settings service tested: load/apply/pending/clamp/roundtrip)
CLI OVERRIDE PRECEDENCE:          PRODUCTION-WIRED

APPKIT THREAD CORRECTNESS:        RESOLVED
LIFECYCLE/SHUTDOWN:               PRODUCTION-WIRED
  (explicit cleanup: hotkey remove → overlay hide → model release → exit)

DIAGNOSTICS:                      PRODUCTION-WIRED
  (Tally tracks cards/hovers/refusals, DiagnosticsSnapshot updated each iteration)

M6 PRODUCTION INTEGRATION:        COMPLETE
M7 PRODUCTION INTEGRATION:        COMPLETE
M6 REAL-WORLD EVIDENCE:           PENDING
M7 REAL-WORLD EVIDENCE:           PENDING

MACOS PRODUCT PARITY:             COMPLETE
MACOS REAL-WORLD EVIDENCE:        PENDING

MANUAL REVIEW V1:                 COMPLETE
ISSUE #141:                       CLOSED
ISSUE #159:                       CLOSED

PACKAGING:                        NOT BUILT LOCALLY (LOCAL TOOLING BLOCKER)
CI:                               CONFIGURED, STATICALLY AUDITED

REMAINING IMPLEMENTATION-READY:   NONE
  (all remaining work is real-world validation, external credentials,
   asset blockers, quality gates, or owner decisions)
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral (ADR-0003).
- Screen capture uses ScreenCaptureKit `SCStream` with `sourceRect` for actual crop.
- Accessibility-first OCR: AX first; OCR fallback only when `ocr_can_help()`.
- Policy check BEFORE content acquisition.
- Tauri owns main thread. Product loop on worker thread.
- All AppKit UI mutations use `dispatch_async(dispatch_get_main_queue())`.
- Translation on two dedicated threads (Quick lane + Deep lane).
- Global hotkeys via CGEventTap with `kCGEventTapOptionDefault`.
- Memory-only privacy: no screenshot/OCR image written to disk.
- Live settings: product loop reads `shared_settings` each iteration, updates in place.
- SCStream: caller owns stop lifecycle. No stack-capture in completion handlers.
- Overlay exclusion: `lens_overlay_window_id()` → `SCContentFilter excludingWindows`.
- Shutdown: explicit cleanup order (hotkey → overlay → model release → exit).
- Diagnostics: `Tally` counters updated on card show, pushed to `DiagnosticsSnapshot`.

## Production Call Paths

**Hover → Translation (async, two-lane):**
```
Tauri event loop (main thread)
  ← product_loop (worker thread)
    → Live settings: read shared_settings, update language/endpoint/model/dwell/ocr/policy
    → Hotkey/menubar poll → ProductCommand dispatch
    → HoverMachine::observe() → process_name_at → policy.allows()
    → MacTextAcquirer [AX] or OcrTextAcquirer [OCR fallback]
    → quick_tx.send() or deep_tx.send()
    → result_rx.try_recv() → generation check → show_card_at
    → update_diagnostics (tally → shared snapshot)
```

**Screen capture:**
```
capture_region(x, y, w, h) → find_display_for_point → sourceRect crop
  → excludingWindows: [overlay window ID]
  → start → poll frame → stop → return pixels
```

## Important Files

```
apps/lens/src/macos.rs              macOS product loop, live settings, diagnostics, shutdown
apps/lens/src/settings_service.rs   Settings state, pending slot, tests (6)
crates/platform-macos/native/capture.m  ScreenCaptureKit, display selection, overlay exclusion
crates/platform-macos/native/hotkey.m   CGEventTap, ring buffer, tap recovery
crates/platform-macos/native/overlay.m  NSPanel, window ID export
crates/platform-macos/src/hotkey.rs     Ring buffer tests (7)
crates/platform-macos/src/coords.rs     Display topology, crop rect tests (11)
crates/lens-translate/src/http.rs       HTTP/1.1, timeouts, fault tests (8)
crates/lens-core/src/policy.rs          AppPolicy with PartialEq
```

## Latest Implementation Changes

- PR #286 — Settings service tests + AppPolicy PartialEq
- PR #285 — Diagnostics wiring (Tally + DiagnosticsSnapshot)
- PR #284 — HTTP fault injection tests + oversized body fix
- PR #283 — Overlay exclusion from ScreenCaptureKit
- PR #282 — Shutdown cleanup (hotkey/overlay/model release)

## Automated Evidence

```
scripts/check.sh --full:     PASS
workspace tests:             PASS (665 tests)
lens binary tests:           PASS (6 settings tests, platform-specific)
platform-macos tests:        PASS (43 tests)
lens-translate tests:        PASS (8 HTTP fault tests)
cargo fmt --check:           CLEAN
cargo clippy -D warnings:    CLEAN
git diff --check:            CLEAN
```

## Known Implementation Gaps

1. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- SCStream: if stop completion never arrives, 5s timeout returns. ARC prevents UAF.
- Translation: per-read timeout allows long generation if each read makes progress. Intentional.
- Settings: CLI overrides superseded by Settings UI at runtime. Matches Windows contract.

## Review Targets

- Verify ScreenCaptureKit excludingWindows correctly hides the overlay in all capture paths.
- Verify shutdown cleanup order prevents late callbacks from touching freed state.
- Verify oversized body detection works under TCP buffer pressure (tested with small overflow).
- Verify diagnostics snapshot is fresh when webview reads it (updated each 25ms iteration).

## Open Issues / Gates

```
#148  colour linkage quality gate     QUALITY GATE OPEN (threshold not yet met)
#140  unzip → first card measurement  HUMAN VALIDATION
#34   KakaoTalk real-app reach        HUMAN VALIDATION
#14   VS Code real-app reach          HUMAN VALIDATION
```

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition
- Overlay visual quality, OCR accuracy
- Permission onboarding, sleep/wake, display hot-plug
- Global hotkey with other apps in foreground

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — CREDENTIAL BLOCKER
- Notarization service — CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — TOOLING BLOCKER

## Next Session — Start Here

1. Real-world validation: run macOS binary against Safari, VS Code, KakaoTalk.
2. Cross-platform CI: trigger and verify the CI pipeline.
3. Package testing: once cargo-tauri is available, build .app bundle locally.
4. Issue #148: if new translation quality evidence is available, evaluate against gate.

## Git State

```
branch:  main
HEAD:    933117a (style: fmt)
status:  clean
open PR: none
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
