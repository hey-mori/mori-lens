# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS is the active workstream.
This session audited handoff claims against production code, found and fixed real bugs.

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces
- Policy enforcement via HoverSession (per-app deny/allow)
- Live settings propagation via `take_pending()` + file watcher
- NSIS installer, signed release pipeline

**macOS (production-wired, correctness-hardened):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR production-wired (`OcrTextAcquirer`)
- Passive native overlay (NSPanel, transparent, click-through)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- **AppKit thread-safe**: all UI mutations dispatched to main queue via `dispatch_async`
- **Async translation**: Quick/Deep on separate dedicated threads, product loop non-blocking
- **Stale-safe**: generation-based result filtering, real anchor preserved through async pipeline
- **Live settings**: Settings UI changes propagate to runtime (language, endpoint, model, dwell, OCR mode, policy)
- **Correct capture**: ScreenCaptureKit uses pointer-containing display, sourceRect crop, display-local coordinates
- **Safe lifetime**: SCStream single-owner stop, no stack-capture in completion handlers
- **Hotkey recovery**: tap-disabled re-enable, ring buffer with overflow eviction
- Tauri event loop owns main thread; product loop on worker thread
- Save uses canonical `SharedLibrary` (same DB as Tauri Library)
- Policy enforcement: `process_name_at()` → `policy.allows()` before acquisition
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)
- Cross-platform CI configured (manual trigger)

## This Session

- **PR #281 merged** — fix/macos-runtime-correctness:
  1. Fixed critical live settings bug: `product_loop` params were immutable, `cur_*` tracking vars were updated but never used in actual code paths. Language/OCR mode/policy changes from Settings UI had no effect.
  2. Fixed SCStream start completion handler UAF: stack-local `__block` variables captured by completion that could fire after function return on timeout. Removed start completion; frame poll serves as combined deadline.
  3. Added missing `lens_hotkey_is_enabled` declaration to `hotkey.h`.
  4. Fixed pre-existing clippy warnings in hotkey.rs tests.

- **Hypothesis verification** — audited 6 handoff claims against production code:
  - A. capture ignores x/y → DISPROVEN (sourceRect set correctly)
  - B. capture uses only displays[0] → DISPROVEN (find_display_for_point iterates all)
  - C. CGEventTap is ListenOnly + NULL → DISPROVEN (uses kCGEventTapOptionDefault)
  - D. Quick/Deep serial → DISPROVEN (separate threads, PR #280)
  - E. Settings startup snapshot → CONFIRMED BUG, fixed in PR #281
  - F. "Ollama hangs forever" → INACCURATE (HTTP has connect/read/write timeouts, QUICK_TIMEOUT=20s, DEEP_TIMEOUT=60s)

## Agent Assessment

```
SCREEN CAPTURE DISPLAY SELECTION:  PRODUCTION-WIRED
  (find_display_for_point iterates all SCDisplay, clips to bounds,
   display-local sourceRect, native-scale output dimensions)

SCREEN CAPTURE REGION CORRECTNESS: PRODUCTION-WIRED
  (x/y used for sourceRect, display-local conversion, boundary clipping,
   NaN/Inf/large-dimension rejection, overflow guard on bytesPerRow*height)

SCSTREAM LIFETIME:                 PRODUCTION-WIRED
  (single-owner stop lifecycle, no stack-capture in completion handlers,
   frame poll as combined start+frame deadline, 5s stop semaphore)

OCR MEMORY-ONLY PRIVACY:          STATICALLY AUDITED
  (no PNG/JPEG/File::create/tmp paths for pixel data,
   capture → Vision OCR → text, pixels freed after recognition)

GLOBAL HOTKEY CONTRACT:           PRODUCTION-WIRED
  (kCGEventTapOptionDefault — consumes matched hotkeys, returns NULL,
   foreground app does not receive Cmd+Opt+key)

CGEVENTTAP RECOVERY:              PRODUCTION-WIRED
  (kCGEventTapDisabledByTimeout/UserInput → CGEventTapEnable(true),
   is_enabled() health seam exposed to Rust)

HOTKEY QUEUE CORRECTNESS:         UNIT-TESTED
  (SPSC ring buffer, producer-only head, consumer-only tail,
   overflow evicts oldest so Quit survives command storm,
   10,000-event stress test, wraparound test)

QUICK/DEEP ISOLATION:             PRODUCTION-WIRED
  (separate dedicated threads, separate channels, separate translators,
   Quick lane drains pending jobs keeping latest (stale coalescing),
   Deep lane single-flight)

TRANSLATION BOUNDEDNESS:          PRODUCTION-WIRED
  (HTTP connect/read/write timeouts, QUICK_TIMEOUT=20s, DEEP_TIMEOUT=60s,
   TranslateError::Timeout classified separately from Unavailable,
   prewarm 180s for cold model load, release on exit)

LIVE SETTINGS PROPAGATION:        PRODUCTION-WIRED
  (mutable params updated each iteration: language, endpoint, model,
   dwell, ocr_mode, policy. Endpoint/model change rebuilds translators
   via config channels. Dwell change rebuilds HoverMachine.
   Policy change takes effect before next acquisition.)

CLI OVERRIDE PRECEDENCE:          PRODUCTION-WIRED
  (CLI flags applied at startup in main(), initial values passed to
   product_loop. Settings UI changes override CLI values at runtime.
   This matches Windows contract where take_pending() supersedes.)

APPKIT THREAD CORRECTNESS:        RESOLVED
  (all UI mutations via dispatch_async to main queue)

LIFECYCLE/SHUTDOWN:               STATICALLY AUDITED
  (quit → tauri_handle.exit() → Tauri event loop exits → process exits.
   Translation threads die when channels drop. Ctrl+C handler bounded.)

M6 PRODUCTION INTEGRATION:        COMPLETE
M7 PRODUCTION INTEGRATION:        COMPLETE
M6 REAL-WORLD EVIDENCE:           PENDING
M7 REAL-WORLD EVIDENCE:           PENDING

MACOS PRODUCT PARITY:             COMPLETE
  (correct per-display crop, SCStream lifetime safe, hotkey semantics
   truthful, tap recovery, Quick insulated from Deep, live settings
   propagation, timeout/fault bounded, shutdown safe)

MACOS REAL-WORLD EVIDENCE:        PENDING
  (Safari/VS Code/KakaoTalk AX acquisition, overlay visual quality,
   OCR accuracy, permission onboarding, sleep/wake, display hot-plug)

TAURI APPLICATION SHELL:          INTEGRATED
POLICY ENFORCEMENT:               CONNECTED
PACKAGING ARTIFACT:               NOT BUILT LOCALLY (LOCAL TOOLING BLOCKER)
CROSS-PLATFORM CI:                CONFIGURED, NOT VERIFIED
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral (ADR-0003).
- Shared modules (tauri_app, library_service, settings_service, detail_view, keeping, mori_transfer) in main.rs.
- Screen capture uses ScreenCaptureKit `SCStream` with `sourceRect` for actual crop (CGDisplayCreateImage obsoleted on macOS 15+).
- Accessibility-first OCR: AX first; OCR fallback only when `ocr_can_help()` and `OcrMode != Off`.
- Policy check BEFORE content acquisition: `process_name_at()` → `policy.allows()`.
- Tauri owns main thread. Product loop on worker thread.
- All AppKit UI mutations use `dispatch_async(dispatch_get_main_queue())`.
- Translation on two dedicated threads (Quick lane + Deep lane). Results delivered via mpsc channel with generation tokens.
- Global hotkeys via CGEventTap with `kCGEventTapOptionDefault` (consumes matched events).
- Memory-only privacy: no screenshot/OCR image written to disk.
- Live settings: product loop reads `shared_settings` each iteration, compares against current values, updates in place.
- SCStream: caller owns stop lifecycle. Delegate only records frame. No stack-capture in completion handlers.
- **Manual Review (Issue #141)**: Later / Known / Archive lifecycle. No automatic scheduling.
- **Library storage**: SQLite (`library.db`) is canonical source of truth (ADR-0013).
- **History**: Three tiers (ADR-0014) — SessionRecent, SavedLibrary, PersistentHistory.

## Production Call Paths

**Hover → Translation (async, two-lane):**
```
Tauri event loop (main thread)
  ← product_loop (worker thread)
    → dispatch_async UI mutations to main queue
    → hotkey::poll() + menubar.poll_*() → ProductCommand queue
    → Live settings: read shared_settings, update language/endpoint/model/dwell/ocr/policy
    → HoverMachine::observe()
    → process_name_at(point) → policy.allows()
    → MacTextAcquirer::acquire() [AX]
    → OcrTextAcquirer::acquire() [OCR fallback]
    → job_tx.send(QuickJob::Quick{anchor, generation})
    → result_rx.try_recv() → generation check → show_card_at(anchor)
```

**Quick lane thread:**
```
quick_rx.recv() → drain pending (keep latest) → translator.quick() → result_tx.send()
```

**Deep lane thread:**
```
deep_rx.recv() → translator.deep() → result_tx.send()
```

**Screen capture:**
```
capture_region(x, y, w, h) [logical points]
  → find_display_for_point(content, x, y) → SCDisplay
  → display-local crop, clipped to bounds
  → config.sourceRect = crop rect
  → config.width/height = native pixel dimensions
  → start capture → poll for frame → stop → return pixels
```

**Save:** `ProductCommand::Save → SharedLibrary::lock() → Library::insert(&Item)` (review_status: Later)

## Important Files

```
apps/lens/src/macos.rs          macOS product loop, live settings, two-lane translation
apps/lens/src/main.rs           Shared module declarations + platform dispatch
apps/lens/src/tauri_app.rs      Platform-neutral Tauri shell
apps/lens/src/settings_service.rs  Settings state, pending slot, live apply
apps/lens/web/index.html        Library/Review UI
crates/platform-macos/src/acquirer.rs   MacTextAcquirer + process_name_at
crates/platform-macos/src/capture.rs    Rust wrapper for ScreenCaptureKit bridge
crates/platform-macos/src/hotkey.rs     ProductCommand enum, ring buffer tests
crates/platform-macos/src/coords.rs     Display topology, crop rect computation, tests
crates/platform-macos/src/ocr_acquire.rs  OCR fallback acquirer
crates/platform-macos/native/overlay.m  NSPanel + dispatch_async UI mutations
crates/platform-macos/native/menubar.m  NSStatusItem + dispatch_async
crates/platform-macos/native/hotkey.m   CGEventTap callback, ring buffer, tap recovery
crates/platform-macos/native/capture.m  ScreenCaptureKit SCStream, display selection, crop
crates/lens-translate/src/lib.rs        OllamaTranslator, system prompt, fence
crates/lens-translate/src/http.rs       Hand-rolled HTTP/1.1, timeouts, classify
crates/lens-translate/src/analyse.rs    Deep analysis path, DEEP_TIMEOUT
crates/lens-storage/src/lib.rs          Canonical Library API (manual review, migration 7)
```

## Latest Implementation Changes

- PR #281 — Live settings propagation fix, SCStream lifetime fix, hotkey header (this session)
- PR #280 — Two-speed AI runtime: Quick/Deep lane separation (ADR-0005)
- PR #279 — CGEventTap correctness: Default option, tap recovery, queue overflow
- PR #278 — ScreenCaptureKit capture: correct display selection, sourceRect crop
- PR #277 — Manual Review v1 contract (Later/Known/Archive, migration 7)

## Automated Evidence

```
scripts/check.sh --full:     PASS
workspace tests:             PASS (659 tests)
lens-storage tests:          PASS (73 tests including manual review)
platform-macos tests:        PASS (43 tests including coordinate/crop/ring buffer)
macOS debug binary build:    PASS
Tauri app cargo check:       PASS
cargo fmt --check:           CLEAN
cargo clippy -D warnings:    CLEAN
.app bundle local build:     NOT BUILT (LOCAL TOOLING BLOCKER)
CI config:                   STATICALLY AUDITED
CI execution:                NOT VERIFIED
```

## Known Implementation Gaps

1. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)
2. **Overlay self-capture timing** — overlay hide is dispatched async; if capture starts before hide completes on main queue, OCR could read Lens's own overlay. Mitigated in practice by dispatch ordering but not formally guaranteed. ScreenCaptureKit window exclusion API could eliminate this.

## Known Risks

- SCStream: if stop completion never arrives, 5s timeout returns but delegate/stream may still receive callbacks. ARC prevents UAF (stream/delegate retained by completion block). Not fault-tested with real SCStream hang.
- Translation: if Ollama model generates indefinitely within per-read timeout (each read makes progress but total time is unbounded), the translation thread blocks for up to QUICK_TIMEOUT/DEEP_TIMEOUT. This is intentional for large model generation.
- Settings: CLI overrides are superseded by Settings UI changes at runtime. This matches Windows contract.

## Review Targets

- Verify ScreenCaptureKit sourceRect uses the exact pointer-containing display with correct display-local coordinates.
- Verify no timeout path permits a late SCStream callback to mutate freed state (start completion removed; stop uses heap semaphore).
- Verify global hotkey semantics match kCGEventTapOptionDefault (consumes matched events, passes through non-matching).
- Verify Quick latency cannot queue behind a running Deep request (separate threads, separate channels).
- Verify changing an app policy in Settings affects acquisition without restart (mutable policy param, checked before each acquisition).
- Verify language change in Settings affects new translation requests (mutable language param, used in QuickRequest::new).
- Verify overlay hide completes before OCR capture begins (dispatch ordering, not formally guaranteed).

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition
- Overlay visual quality, OCR accuracy
- Permission onboarding, sleep/wake, display hot-plug
- Global hotkey with other apps in foreground
- Settings/Library Tauri webview interaction
- Translation behavior during Ollama model load/generation
- Overlay self-capture timing in practice

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — EXTERNAL CREDENTIAL BLOCKER
- Notarization service — EXTERNAL CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — LOCAL TOOLING BLOCKER

## Next Session — Start Here

1. Real-world validation: run macOS binary against Safari, VS Code, KakaoTalk. Record AX acquisition results.
2. Overlay self-capture: verify in practice that overlay hide completes before OCR capture. If not, add ScreenCaptureKit window exclusion.
3. SCStream fault testing: create test harness that simulates timeout/late-callback scenarios without real ScreenCaptureKit.
4. Cross-platform CI: trigger and verify the CI pipeline.
5. Package testing: once cargo-tauri is available, build and test .app bundle locally.

## Git State

```
branch:  main
HEAD:    56a979c (Merge PR #281 — runtime correctness fixes)
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
