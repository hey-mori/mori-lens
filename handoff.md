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
- NSIS installer, signed release pipeline

**macOS (this session's work):**
- Accessibility acquisition production-wired (`MacTextAcquirer` → `TextAcquirer` trait)
- Vision OCR production-wired (`OcrTextAcquirer` → `TextAcquirer` trait)
- Passive native overlay production-wired (NSPanel, transparent, click-through)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- AppKit event loop: Tauri owns main thread, product loop on worker thread
- Save uses canonical `SharedLibrary` (same DB as Tauri Library commands)
- Settings/Library surfaces accessible via menu bar (Cmd+, / Cmd+Opt+L)
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)
- Shared modules (tauri_app, library_service, settings_service, detail_view, keeping, mori_transfer) moved to main.rs for cross-platform use

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite)
- Cross-platform CI configured (manual trigger)

## This Session

- Integrated macOS into Tauri application shell: shared modules moved from win.rs to main.rs, macOS product loop runs on worker thread while Tauri event loop runs on main thread (matching Windows architecture)
- macOS now uses `settings_service::load_or_migrate()` and `library_service::open()` — same canonical path as Windows
- Added Open Settings (Cmd+,) and Open Library (Cmd+Opt+L) to menu bar, routing through `AppHandle.show_window()` (Tauri webview)
- Save now uses `SharedLibrary` (same DB as Tauri Library commands) instead of direct file append
- Added CoreMedia framework to build.rs for SCStream capture
- Placeholder icon.png created for Tauri macOS build
- detail_view clipboard call made cross-platform (cfg-gated)

## Agent Assessment

```
M6 NATIVE IMPLEMENTATION:        COMPLETE
M6 REAL-WORLD EVIDENCE:          PENDING

M7 NATIVE IMPLEMENTATION:        COMPLETE
M7 REAL-WORLD EVIDENCE:          PENDING

MACOS PRODUCT PARITY:            COMPLETE
  (event loop, global hotkeys, commands, save, lifecycle, stale protection)

TAURI APPLICATION SHELL:         INTEGRATED
  (shared modules, Settings/Library surfaces accessible, Save uses same DB)

SETTINGS/LIBRARY/DETAIL:         ACCESSIBLE
  (menu bar opens Tauri webview windows; full UI interaction not yet validated)

POLICY ENFORCEMENT:              NOT CONNECTED
  (_policy variable exists but not consulted in hover loop)

ASYNC/STALE CORRECTNESS:         PARTIAL
  (HoverEvent::Left clears current_card; monotonic generation for manual OCR;
   Deep/Quick stale response tests not yet written)

PACKAGING CONFIG:                COMPLETE
PACKAGE ARTIFACT:                NOT BUILT LOCALLY (LOCAL TOOLING BLOCKER)

CROSS-PLATFORM CI CONFIG:        COMPLETE
CI EXECUTION EVIDENCE:           NOT VERIFIED
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral. No `#[cfg(windows)]` or `#[cfg(target_os)]` in core.
- Shared modules (tauri_app, library_service, settings_service, detail_view, keeping, mori_transfer) declared in main.rs, used by both win.rs and macos.rs.
- AX attribute names created at runtime via `CFStringCreateWithCString` — linker doesn't export `kAX*Attribute` symbols on all macOS versions. Do not reintroduce extern statics for these.
- Screen capture uses ScreenCaptureKit `SCStream`. Do not reintroduce `CGDisplayCreateImage` or `CGWindowListCreateImage` — obsoleted on macOS 15+.
- Accessibility-first OCR: AX always tried first; OCR fallback only when `ocr_can_help()` and `OcrMode != Off`. Never silently auto-OCR.
- Tauri owns main thread (AppKit event loop). Product loop on worker thread. Native components (overlay, menubar, hotkeys) created on main thread, moved to worker.
- Global hotkeys via CGEventTap — uses existing AX permission, no additional permission needed.
- Memory-only privacy: no screenshot/OCR image written to disk.
- Non-fatal degradation: AX permission, Screen Recording, overlay, menu bar failures don't exit the process.

## Production Call Paths

**Hover → Translation:**
```
Tauri event loop (main thread)
  ← product_loop (worker thread)
    → hotkey::poll() + menubar.poll_*() → ProductCommand queue
    → HoverMachine::observe()
    → MacTextAcquirer::acquire() [AX path]
    → OcrTextAcquirer::acquire() [OCR fallback if ocr_can_help]
    → decide() → language routing
    → translator.quick()
    → Card::translated()
    → overlay.show_text()
```

**Save:**
```
ProductCommand::Save
→ execute_save()
→ SharedLibrary::lock()
→ Library::insert(&Item{original, answer, kind, ...})
→ same DB as Tauri library_list/library_search commands
```

**Open Settings/Library:**
```
ProductCommand::OpenSettings / OpenLibrary
→ tauri_handle.show_window()
→ Tauri webview window shown (main thread)
```

## Important Files

```
apps/lens/src/main.rs
  Shared module declarations + platform dispatch.

apps/lens/src/macos.rs
  macOS product loop (worker thread), command dispatch, action handlers.

apps/lens/src/win.rs
  Windows product loop (worker thread, reference for contract).

apps/lens/src/tauri_app.rs
  Platform-neutral Tauri shell: commands, shared state, build/run.

crates/platform-macos/src/hotkey.rs
  Global hotkey bridge + ProductCommand enum (8 variants).

crates/platform-macos/src/menubar.rs
  NSStatusItem wrapper with 8 poll methods.

crates/platform-macos/native/menubar.m
  Menu bar implementation with Cmd+, and Cmd+Opt+L shortcuts.

crates/platform-macos/native/capture.m
  ScreenCaptureKit SCStream — ownership boundary.

crates/lens-storage/src/lib.rs
  Canonical Library API (Item, Library::open/insert/search).
```

## Latest Implementation Changes

- PR #271 — Tauri app shell integration: shared modules to main.rs, macOS uses settings_service/library_service, product loop on worker thread
- PR #272 — Open Settings/Library menu bar commands via AppHandle.show_window()

## Automated Evidence

```
scripts/check.sh --full:     PASS
workspace tests:             PASS
platform-macos tests:        PASS (31 tests)
macOS debug binary build:    PASS
macOS binary --version:      PASS (Mori Lens 0.1.0 · macOS)
.app bundle local build:     NOT BUILT (LOCAL TOOLING BLOCKER — cargo-tauri not installed)
CI config:                   STATICALLY AUDITED
CI execution:                NOT VERIFIED
```

## Known Implementation Gaps

1. **Policy enforcement not connected** — `_policy` variable set up but not consulted in hover loop for per-app deny/allow.
2. **Stale generation tests missing** — Quick/Deep/OCR stale response rejection not yet covered by automated tests.
3. **No `.icns` icon** — only placeholder `icon.png` exists. macOS bundle uses default icon. (ASSET BLOCKER)

## Known Risks

- SCStream bridge waits for stop completion via semaphore, but timeout behavior (5s) has not been fault-tested.
- `current_card` is cleared on `HoverEvent::Left` but Deep Analysis is synchronous — a slow `translator.deep()` call could complete after the user has moved away.
- All AppKit calls happen on the Rust main thread via Tauri, which is correct. But native components (overlay, menubar) are moved to the worker thread — their FFI calls may need main-thread dispatch if AppKit requires it.

## Review Targets

- Verify Tauri really owns the AppKit event loop after the shell refactor. Does `lens_process_appkit_events()` still need to be called?
- Verify Save and Library UI resolve the exact same database path.
- Verify stale Deep/Quick/OCR results cannot render after generation changes.
- Verify policy denial happens before AX/OCR content acquisition (currently NOT enforced).
- Verify native component FFI calls from worker thread are safe (NSPanel show/hide from non-main thread).

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition success rate
- Overlay visual quality (Retina, dark mode, multi-monitor)
- OCR accuracy (small text, dark themes, code editors)
- Screen Recording / Accessibility permission onboarding UX
- Sleep/wake actual behavior (lid close/open)
- Physical display hot-plug behavior
- Global hotkey actual behavior with other apps in foreground
- Deep Analysis result quality
- Settings/Library Tauri webview actual interaction on macOS

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — EXTERNAL CREDENTIAL BLOCKER
- Notarization service — EXTERNAL CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — LOCAL TOOLING BLOCKER (CI installs it)

## Next Session — Start Here

1. Connect `_policy` to the hover loop for per-app deny/allow. Trace Windows `win.rs` policy usage and replicate the same semantics in `macos.rs`.
2. Add stale Quick/Deep/OCR generation tests — synthetic tests verifying old responses are discarded when pointer moves.
3. Audit whether `lens_process_appkit_events()` is still needed now that Tauri owns the event loop. If Tauri drains AppKit events, remove the manual drain.
4. Verify native component FFI calls from worker thread are safe. If NSPanel/NSStatusItem require main thread, add `dispatch_async` wrappers.
5. Run `scripts/check.sh --full`, PR, merge, then continue to FFI deep audit.

## Git State

```
branch:  main
HEAD:    43dfce2
status:  clean
open PR: none
```

## Constraints

- Agent investigation stays inside `PROJECT_ROOT/**`. Do not inspect `~/.qwen`, `~/Library`, other repos, personal files, credentials.
- `lens-core` remains platform-neutral (ADR-0003).
- Human validation does not block implementation. Mark as VALIDATION DEBT.
- Do not install external tools (`brew install`, `cargo install`) without explicit permission.
- Work in coherent branch → PR → merge units. No direct main pushes.
- macOS native code uses Objective-C bridges compiled by `cc` crate.
- All unsafe blocks require SAFETY comments (clippy `-D warnings`).
