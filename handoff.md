# Mori Lens — Session Handoff

Last updated: 2026-09-05

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
M6 (Accessibility acquisition), M7 (overlay + OCR), and production parity are the focus.
Windows implementation is complete and stable. macOS is the active workstream.

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer, signed release pipeline

**macOS (this session's work):**
- Accessibility acquisition production-wired (`MacTextAcquirer` → `TextAcquirer` trait)
- Vision OCR production-wired (`OcrTextAcquirer` → `TextAcquirer` trait)
- Passive native overlay production-wired (NSPanel, transparent, click-through)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- Tauri event loop owns main thread; product loop on worker thread
- Save uses canonical `SharedLibrary` (same DB as Tauri Library commands)
- Settings/Library surfaces accessible via menu bar (Cmd+, / Cmd+Opt+L)
- **Policy enforcement connected**: `process_name_at()` → `policy.allows()` before acquisition
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)
- Shared modules (tauri_app, library_service, settings_service, detail_view, keeping, mori_transfer) in main.rs

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite)
- Cross-platform CI configured (manual trigger)

## This Session

- Connected `AppPolicy` to macOS hover loop: `process_name_at()` does lightweight AX metadata lookup (no text read), policy check blocks both AX and OCR for denied apps
- `policy.deny_self("lens")` prevents reading Lens's own windows
- Matches Windows `HoverSession` policy semantics

## Agent Assessment

```
M6 NATIVE IMPLEMENTATION:        COMPLETE
M6 PRODUCTION INTEGRATION:       COMPLETE
M6 REAL-WORLD EVIDENCE:          PENDING

M7 NATIVE IMPLEMENTATION:        COMPLETE
M7 PRODUCTION INTEGRATION:       COMPLETE
M7 REAL-WORLD EVIDENCE:          PENDING

MACOS PRODUCT PARITY:            COMPLETE
  (event loop, global hotkeys, commands, save, lifecycle, policy enforcement)

TAURI APPLICATION SHELL:         INTEGRATED
  (shared modules, Settings/Library surfaces accessible, Save uses same DB)

POLICY ENFORCEMENT:              CONNECTED
  (process_name_at → policy.allows → blocks AX + OCR for denied apps)

SETTINGS/LIBRARY/DETAIL:         ACCESSIBLE
  (menu bar opens Tauri webview windows)

ASYNC/STALE CORRECTNESS:         PARTIAL
  (HoverEvent::Left clears current_card; monotonic generation for manual OCR;
   Deep/Quick stale response tests not yet written;
   translator.quick/deep are synchronous — block worker during Ollama call)

APPKIT THREAD CORRECTNESS:       UNRESOLVED RISK
  (native components created on main thread, moved to worker thread;
   AppKit API calls from worker thread have not been validated;
   lens_process_appkit_events() may be redundant with Tauri event loop)

PACKAGING CONFIG:                COMPLETE
PACKAGE ARTIFACT:                NOT BUILT LOCALLY (LOCAL TOOLING BLOCKER)

CROSS-PLATFORM CI CONFIG:        COMPLETE
CI EXECUTION EVIDENCE:           NOT VERIFIED
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral. No `#[cfg(windows)]` or `#[cfg(target_os)]` in core.
- Shared modules declared in main.rs, used by both win.rs and macos.rs.
- AX attribute names created at runtime via `CFStringCreateWithCString`. Do not reintroduce extern statics.
- Screen capture uses ScreenCaptureKit `SCStream`. `CGDisplayCreateImage`/`CGWindowListCreateImage` are obsoleted on macOS 15+.
- Accessibility-first OCR: AX always tried first; OCR fallback only when `ocr_can_help()` and `OcrMode != Off`.
- Policy check BEFORE content acquisition: `process_name_at()` → `policy.allows()` → skip if denied.
- Tauri owns main thread (AppKit event loop). Product loop on worker thread.
- Global hotkeys via CGEventTap — uses existing AX permission.
- Memory-only privacy: no screenshot/OCR image written to disk.
- Non-fatal degradation: permission/overlay/menubar failures don't exit the process.

## Production Call Paths

**Hover → Translation (with policy):**
```
Tauri event loop (main thread)
  ← product_loop (worker thread)
    → process_appkit_events() [may be redundant with Tauri]
    → hotkey::poll() + menubar.poll_*() → ProductCommand queue
    → HoverMachine::observe()
    → process_name_at(point) [metadata only, no text read]
    → policy.allows(process_name) [deny → skip AX + OCR]
    → MacTextAcquirer::acquire() [AX path]
    → OcrTextAcquirer::acquire() [OCR fallback]
    → decide() → translator.quick()
    → Card::translated() → overlay.show_text()
```

**Save:**
```
ProductCommand::Save → SharedLibrary::lock() → Library::insert(&Item)
```

**Open Settings/Library:**
```
ProductCommand::OpenSettings/OpenLibrary → tauri_handle.show_window()
```

## Important Files

```
apps/lens/src/macos.rs          macOS product loop, command dispatch, policy check
apps/lens/src/win.rs            Windows product loop (reference for contract)
apps/lens/src/main.rs           Shared module declarations + platform dispatch
apps/lens/src/tauri_app.rs      Platform-neutral Tauri shell
crates/platform-macos/src/acquirer.rs   MacTextAcquirer + process_name_at
crates/platform-macos/src/hotkey.rs     ProductCommand enum (8 variants)
crates/platform-macos/src/overlay.rs    NSPanel overlay wrapper
crates/platform-macos/src/menubar.rs    NSStatusItem wrapper
crates/platform-macos/native/capture.m  ScreenCaptureKit SCStream
crates/platform-macos/native/hotkey.m   CGEventTap callback
crates/lens-core/src/session.rs         HoverSession (Windows policy reference)
crates/lens-core/src/policy.rs          AppPolicy, AppTier
crates/lens-storage/src/lib.rs          Canonical Library API
```

## Latest Implementation Changes

- PR #273 — Policy enforcement: `process_name_at()` + `policy.allows()` before acquisition

## Automated Evidence

```
scripts/check.sh --full:     PASS
workspace tests:             PASS
platform-macos tests:        PASS (31 tests)
macOS debug binary build:    PASS
.app bundle local build:     NOT BUILT (LOCAL TOOLING BLOCKER)
CI config:                   STATICALLY AUDITED
CI execution:                NOT VERIFIED
```

## Known Implementation Gaps

1. **Async runtime** — `translator.quick()`/`translator.deep()` are synchronous, blocking the worker thread during Ollama calls. Quit/Pause/Settings unresponsive during long Deep calls.
2. **Stale generation tests** — Quick/Deep/OCR stale response rejection not covered by automated tests.
3. **`make_dummy_target()` in Deep path** — Deep Analysis renders at a dummy anchor instead of the original target anchor.
4. **AppKit thread safety** — Native components (overlay, menubar) moved to worker thread. AppKit API calls from non-main thread not validated.
5. **`lens_process_appkit_events()` redundancy** — May be unnecessary now that Tauri owns the event loop.
6. **No `.icns` icon** — placeholder `icon.png` only. (ASSET BLOCKER)

## Known Risks

- **Worker thread AppKit calls**: NSPanel `orderFront:`/`orderOut:`, NSStatusItem `button.title` called from worker thread. AppKit documentation requires main thread. May work in practice but is undefined behavior.
- **SCStream timeout**: 5s semaphore wait for stop completion. Late callback after timeout could access freed delegate. Not fault-tested.
- **CGEventTap timeout disable**: macOS may disable the event tap if the callback takes too long. No re-enable logic implemented.
- **Deep blocks worker**: During a 4-5s Ollama deep call, the entire product loop is frozen. No commands processed, no pointer tracking.

## Review Targets

- Verify AppKit API calls from worker thread are safe or add main-thread dispatch.
- Verify `lens_process_appkit_events()` is still needed with Tauri event loop.
- Verify stale Deep/Quick/OCR results cannot render after generation changes.
- Verify `process_name_at()` + `policy.allows()` correctly blocks all content acquisition for denied apps.
- Verify SCStream late callback after timeout cannot cause UAF.

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition success rate
- Overlay visual quality (Retina, dark mode, multi-monitor)
- OCR accuracy (small text, dark themes, code editors)
- Permission onboarding UX
- Sleep/wake, display hot-plug
- Global hotkey with other apps in foreground
- Deep Analysis result quality
- Settings/Library Tauri webview interaction on macOS
- AppKit thread safety in practice

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — EXTERNAL CREDENTIAL BLOCKER
- Notarization service — EXTERNAL CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — LOCAL TOOLING BLOCKER

## Next Session — Start Here

1. **AppKit thread correctness**: Audit all native FFI calls from worker thread. If AppKit requires main thread, add `dispatch_async(dispatch_get_main_queue(), ...)` wrappers in the Objective-C bridge or create a UiCommand channel from worker → main.
2. **Async runtime**: Move `translator.quick()`/`translator.deep()` to background tasks so the product loop remains responsive. Use bounded executor (not unbounded thread spawn).
3. **Stale generation tests**: Write synthetic tests for Quick/Deep/OCR stale response rejection.
4. **Remove `make_dummy_target()`**: Deep Analysis should render at the original target anchor.
5. **Evaluate `lens_process_appkit_events()`**: If Tauri drains AppKit events, remove the redundant manual drain.

## Git State

```
branch:  main
HEAD:    d5384c5
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
