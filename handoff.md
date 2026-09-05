# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Manual Review v1 contract (Issue #141) is implemented and merged.
Windows implementation is complete and stable. macOS is the active workstream.

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer, signed release pipeline

**macOS (active workstream):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR production-wired (`OcrTextAcquirer`)
- Passive native overlay (NSPanel, transparent, click-through)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- **AppKit thread-safe**: all UI mutations dispatched to main queue via `dispatch_async`
- **Async translation**: Quick/Deep/OCR run on dedicated thread, product loop non-blocking
- **Stale-safe**: generation-based result filtering, real anchor preserved through async pipeline
- Tauri event loop owns main thread; product loop on worker thread
- Save uses canonical `SharedLibrary` (same DB as Tauri Library)
- Settings/Library surfaces accessible via menu bar (Cmd+, / Cmd+Opt+L)
- Policy enforcement: `process_name_at()` → `policy.allows()` before acquisition
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite, ADR-0013)
- Cross-platform CI configured (manual trigger)

## This Session

- **Manual Review v1 contract implemented (PR #277, merged)**: Replaced automatic spaced-repetition scheduling (new/learning/mastered, 1d/3d/7d/14d/30d intervals, next_review_at, record_review) with owner-approved manual lifecycle: Later / Known / Archive.
  - Migration 7: legacy SRS data mapped deterministically (mastered→Known, new/learning→Later), unknown states preserved in legacy_review_state, SRS columns dropped
  - Storage API: set_review_status, list_by_status, count_by_status, Filter gains review_status
  - Tauri: library_record_review(success:bool) replaced by library_set_review_status(status), fail-closed on invalid input
  - Web UI: Review surface rewritten with Known/Still Later/Archive buttons + inline note, Library status filter, no SRS wording
  - 73 lens-storage tests, 643 total workspace tests pass

## Agent Assessment

```
MANUAL REVIEW CONTRACT:      COMPLETE
  (Later/Known/Archive lifecycle, no scheduling, no notifications,
   reversible transitions, note/status independence, migration 7)

AUTOMATIC SRS IN PRODUCTION: REMOVED
  (REVIEW_INTERVALS, record_review, list_due, count_due, next_review_at,
   review_count all removed from production paths)

LEGACY DATA MIGRATION:       COMPLETE
  (mastered→Known, new/learning→Later, unknown→preserved, atomic, idempotent)

JSONL COMPATIBILITY:         COMPLETE
  (v1 import with serde default Later, roundtrip preserves status/note/tags)

REVIEW UI:                   COMPLETE
  (manual queue, one item at a time, Known/Later/Archive actions, inline note)

SECURE WIPE REGRESSION:      PASS
  (all data including review statuses removed by secure_wipe)

ISSUE #141:                  CLOSED (evidence comment added)
ISSUE #159:                  ALL 12 CHILD ISSUES CLOSED

M6 NATIVE IMPLEMENTATION:        COMPLETE
M6 PRODUCTION INTEGRATION:       COMPLETE
M6 REAL-WORLD EVIDENCE:          PENDING

M7 NATIVE IMPLEMENTATION:        COMPLETE
M7 PRODUCTION INTEGRATION:       COMPLETE
M7 REAL-WORLD EVIDENCE:          PENDING

MACOS PRODUCT PARITY:            COMPLETE
  (async runtime, thread-safe UI, stale-safe results, policy enforcement,
   global hotkeys, Save/Library integration, lifecycle resilience)

TAURI APPLICATION SHELL:         INTEGRATED
  (shared modules, Settings/Library surfaces accessible, Save uses same DB)

POLICY ENFORCEMENT:              CONNECTED
APPKIT THREAD CORRECTNESS:       RESOLVED (dispatch_async to main queue)
ASYNC RUNTIME:                   RESOLVED (dedicated translation thread)
STALE RESULT CORRECTNESS:        RESOLVED (generation check + anchor preservation)

SCSTREAM LIFETIME:               PARTIAL
  (5s timeout with semaphore; late callback after timeout not fault-tested)

CGEVENTTAP LIFETIME/RECOVERY:    PARTIAL
  (no re-enable on timeout-disable; ring buffer overflow drops oldest)

PACKAGING ARTIFACT:              NOT BUILT LOCALLY (LOCAL TOOLING BLOCKER)
CROSS-PLATFORM CI:               CONFIGURED, NOT VERIFIED
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral.
- Shared modules (tauri_app, library_service, settings_service, detail_view, keeping, mori_transfer) in main.rs.
- AX attribute names created at runtime via `CFStringCreateWithCString`.
- Screen capture uses ScreenCaptureKit `SCStream` (CGDisplayCreateImage obsoleted on macOS 15+).
- Accessibility-first OCR: AX first; OCR fallback only when `ocr_can_help()` and `OcrMode != Off`.
- Policy check BEFORE content acquisition: `process_name_at()` → `policy.allows()`.
- Tauri owns main thread. Product loop on worker thread.
- All AppKit UI mutations use `dispatch_async(dispatch_get_main_queue())`.
- Translation runs on dedicated thread. Results delivered via mpsc channel with generation tokens.
- Global hotkeys via CGEventTap (uses existing AX permission).
- Memory-only privacy: no screenshot/OCR image written to disk.
- **Manual Review (Issue #141)**: Later / Known / Archive lifecycle. No automatic scheduling. No notifications. Reader opens Review manually. Note is independent annotation.
- **Library storage**: SQLite (`library.db`) is canonical source of truth (ADR-0013). `saved.md` is human-readable export/compatibility survivor.
- **History**: Three tiers (ADR-0014) — SessionRecent, SavedLibrary, PersistentHistory.

## Production Call Paths

**Hover → Translation (async, thread-safe):**
```
Tauri event loop (main thread)
  ← product_loop (worker thread)
    → dispatch_async UI mutations to main queue
    → hotkey::poll() + menubar.poll_*() → ProductCommand queue
    → HoverMachine::observe()
    → process_name_at(point) → policy.allows()
    → MacTextAcquirer::acquire() [AX]
    → OcrTextAcquirer::acquire() [OCR fallback]
    → job_tx.send(TransJob::Quick{anchor, generation})
    → result_rx.try_recv() → generation check → show_card_at(anchor)
```

**Translation thread:**
```
job_rx.recv() → translator.quick()/deep() → result_tx.send(TransResult{anchor, generation})
```

**Save:** `ProductCommand::Save → SharedLibrary::lock() → Library::insert(&Item)` (review_status: Later)

**Review:** `library_review_list → list_by_status(Later)` → UI shows one item → user picks Known/Later/Archive → `library_set_review_status`

## Important Files

```
apps/lens/src/macos.rs          macOS product loop, async translation, command dispatch
apps/lens/src/main.rs           Shared module declarations + platform dispatch
apps/lens/src/tauri_app.rs      Platform-neutral Tauri shell (manual review commands)
apps/lens/web/index.html        Library/Review UI (manual review surface)
crates/platform-macos/src/acquirer.rs   MacTextAcquirer + process_name_at
crates/platform-macos/src/hotkey.rs     ProductCommand enum (8 variants)
crates/platform-macos/native/overlay.m  NSPanel + dispatch_async UI mutations
crates/platform-macos/native/menubar.m  NSStatusItem + dispatch_async
crates/platform-macos/native/hotkey.m   CGEventTap callback
crates/platform-macos/native/capture.m  ScreenCaptureKit SCStream
crates/lens-storage/src/lib.rs          Canonical Library API (manual review, migration 7)
```

## Latest Implementation Changes

- PR #277 — Manual Review v1 contract (Later/Known/Archive, migration 7, SRS removed)
- PR #274 — AppKit main-thread dispatch (dispatch_async for all UI mutations)
- PR #275 — Async translation runtime (dedicated thread, channel-based, non-blocking)
- PR #276 — Anchor preservation (real anchor through async pipeline, make_dummy_target removed)

## Automated Evidence

```
scripts/check.sh --full:     PASS
workspace tests:             PASS (643 tests)
lens-storage tests:          PASS (73 tests including manual review)
platform-macos tests:        PASS (31 tests)
macOS debug binary build:    PASS
Tauri app cargo check:       PASS
cargo fmt --check:           CLEAN
cargo clippy -D warnings:    CLEAN
.app bundle local build:     NOT BUILT (LOCAL TOOLING BLOCKER)
CI config:                   STATICALLY AUDITED
CI execution:                NOT VERIFIED
```

## Known Implementation Gaps

1. **SCStream timeout fault testing** — 5s semaphore timeout not fault-tested. Late callback after timeout could access freed state.
2. **CGEventTap recovery** — No re-enable logic if macOS disables tap due to timeout.
3. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- SCStream: if stop completion never arrives, 5s timeout returns but delegate/stream may still receive callbacks. ARC should prevent UAF but this is not verified.
- CGEventTap: if macOS disables the tap (timeout), hotkeys silently stop working until app restart.
- Translation thread: if Ollama hangs indefinitely, the translation thread blocks forever. No timeout on translator.quick()/deep() calls.

## Review Targets

- Verify SCStream late completion after timeout cannot access freed delegate/state.
- Verify CGEventTap disable/re-enable lifecycle is handled or documented.
- Verify translation thread handles Ollama timeout gracefully.
- Verify all AppKit calls from worker thread go through dispatch_async.
- Verify Settings changes propagate to product runtime (shared state divergence check).

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition
- Overlay visual quality, OCR accuracy
- Permission onboarding, sleep/wake, display hot-plug
- Global hotkey with other apps in foreground
- Settings/Library Tauri webview interaction
- AppKit thread safety in practice (dispatch_async correctness)
- Translation thread behavior during Ollama hang/timeout

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — EXTERNAL CREDENTIAL BLOCKER
- Notarization service — EXTERNAL CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — LOCAL TOOLING BLOCKER

## Next Session — Start Here

1. SCStream lifetime hardening: add fault test for timeout + late callback. Verify ARC prevents UAF. Consider self-owned callback context.
2. CGEventTap recovery: detect `kCGEventTapDisabledByTimeout` and re-enable. Add event storm stress test.
3. Translation timeout: add bounded timeout to translator.quick()/deep() calls so translation thread can recover from hung Ollama.
4. Settings propagation audit: verify macOS runtime picks up settings changes from Tauri Settings window.
5. Run `scripts/check.sh --full`, PR, merge, then update handoff.

## Git State

```
branch:  main
HEAD:    7975a88 (Merge PR #277 — manual review contract)
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
