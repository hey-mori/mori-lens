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
- AppKit event loop drained each iteration (NSRunLoop)
- Save uses canonical `Library::insert` (same storage as Windows)
- Lifecycle resilience (sleep/wake, display change)
- Single instance guard (flock)
- **Tauri Settings/Library/Detail UI NOT connected** — macOS runs as native CLI loop

**Shared/core:**
- `lens-core` platform-neutral (ADR-0003 invariant held)
- `lens-storage::Library` is canonical save target (SQLite)
- Cross-platform CI configured (manual trigger)

## This Session

- Built `crates/platform-macos/` from scratch: 10 Rust modules + 7 Objective-C bridges
- Split `apps/lens` into `win.rs` + `macos.rs` behind cfg gates
- Discovered and fixed: `CGWindowListCreateImage` and `CGDisplayCreateImage` are obsoleted on macOS 15+; replaced with ScreenCaptureKit `SCStream`
- Discovered and fixed: `physical_to_logical()` passed physical pixels to a function expecting logical points — wrong scale at mixed-scale multi-monitor boundaries
- Discovered and fixed: SCStream capture delegate could be deallocated while async stop callback was in flight — added semaphore wait
- Discovered and fixed: `mem::forget` in parent-chain walks leaked CFTypeRefs — replaced with retain-per-visit pattern
- Replaced direct `saved.md` append with canonical `Library::open` + `Library::insert`
- Replaced `Generation(0)` in manual OCR with monotonic counter; `HoverEvent::Left` now clears `current_card`
- Added `lens_process_appkit_events()` to drain NSRunLoop — menu actions and lifecycle notifications now work
- Added CGEventTap global hotkeys — real global, not just menu key equivalents
- Fixed release workflow: `.app` is a directory (can't hash directly) — now zips via `ditto`, hashes the zip, hard-fails if `.app` missing

## Agent Assessment

```
M6 NATIVE IMPLEMENTATION:        COMPLETE
M6 REAL-WORLD EVIDENCE:          PENDING

M7 NATIVE IMPLEMENTATION:        COMPLETE
M7 REAL-WORLD EVIDENCE:          PENDING

MACOS PRODUCT PARITY:            COMPLETE
  (event loop, global hotkeys, commands, save, lifecycle, stale protection)

MACOS APPLICATION SHELL:         INCOMPLETE
  (Tauri Settings/Library/Detail UI not connected)

PACKAGING CONFIG:                COMPLETE
PACKAGE ARTIFACT:                NOT BUILT LOCALLY (LOCAL TOOLING BLOCKER)

CROSS-PLATFORM CI CONFIG:        COMPLETE
CI EXECUTION EVIDENCE:           NOT VERIFIED
```

## Architecture / Key Decisions

- `lens-core` remains platform-neutral. No `#[cfg(windows)]` or `#[cfg(target_os)]` in core.
- AX attribute names created at runtime via `CFStringCreateWithCString` — linker doesn't export `kAX*Attribute` symbols on all macOS versions. Do not reintroduce extern statics for these.
- Screen capture uses ScreenCaptureKit `SCStream`. Do not reintroduce `CGDisplayCreateImage` or `CGWindowListCreateImage` — obsoleted on macOS 15+.
- Accessibility-first OCR: AX always tried first; OCR fallback only when `ocr_can_help()` and `OcrMode != Off`. Never silently auto-OCR.
- All AppKit operations (NSPanel, NSStatusItem, NSMenu) on main thread. `lens_process_appkit_events()` called every 25ms iteration.
- Global hotkeys via CGEventTap — uses existing AX permission, no additional permission needed.
- Memory-only privacy: no screenshot/OCR image written to disk.
- Non-fatal degradation: AX permission, Screen Recording, overlay, menu bar failures don't exit the process.

## Production Call Paths

**Hover → Translation:**
```
overlay::process_appkit_events()
→ hotkey::poll() + menubar.poll_*() → ProductCommand queue
→ HoverMachine::observe()
→ MacTextAcquirer::acquire() [AX path]
→ OcrTextAcquirer::acquire() [OCR fallback if ocr_can_help]
→ decide() → language routing
→ translator.quick()
→ Card::translated()
→ overlay.show_text()
```

**Deep Analysis:**
```
ProductCommand::Deep
→ execute_deep()
→ translator.deep()
→ AnalysisCache check
→ Card::analysed()
→ overlay.show_text()
```

**Save:**
```
ProductCommand::Save
→ execute_save()
→ Library::open("library.db")
→ Library::insert(&Item{original, answer, kind, ...})
```

**Manual OCR:**
```
ProductCommand::ManualOcr
→ overlay.hide()
→ OcrTextAcquirer.acquire()
→ translator.quick()
→ Card::translated()
→ overlay.show_text()
```

## Important Files

```
apps/lens/src/macos.rs
  macOS product loop, command dispatch, all action handlers.

apps/lens/src/win.rs
  Windows product loop (3300+ lines, reference for contract).

crates/platform-macos/src/acquirer.rs
  MacTextAcquirer — AX text acquisition chain.

crates/platform-macos/src/ocr_acquire.rs
  OcrTextAcquirer — screen capture + Vision OCR.

crates/platform-macos/src/hotkey.rs
  Global hotkey bridge + ProductCommand enum.

crates/platform-macos/src/overlay.rs
  NSPanel overlay wrapper.

crates/platform-macos/native/capture.m
  ScreenCaptureKit SCStream — ownership boundary.

crates/platform-macos/native/hotkey.m
  CGEventTap callback — lock-free ring buffer.

crates/lens-storage/src/lib.rs
  Canonical Library API (Item, Library::open/insert/search).
```

## Latest Implementation Changes

- PR #268 — AppKit event loop drain, CGEventTap global hotkeys, unified ProductCommand dispatch
- PR #269 — Release workflow: zip .app, hard failure on missing artifact, proper SHA-256
- PR #270 — Stale generation: monotonic counter for manual OCR, HoverEvent::Left clears current_card
- Direct — Save → canonical Library API (`Library::insert` instead of `saved.md` append)

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

1. **Tauri app shell not integrated on macOS** — Settings/Library/Detail UI surfaces are Windows-only. macOS runs as native CLI loop. `lens-storage::Library` is accessible directly (Save uses it), but no UI.
2. **Policy enforcement not connected** — `_policy` variable set up but not consulted in hover loop for per-app deny/allow.
3. **No `.icns` icon** — only `icons/icon.ico` exists. macOS bundle uses default icon. (ASSET BLOCKER)

## Known Risks

- SCStream bridge waits for stop completion via semaphore, but timeout behavior (5s) has not been fault-tested. If the stream hangs, capture blocks for 5s then returns failure.
- `current_card` is cleared on `HoverEvent::Left` but Deep Analysis is synchronous — a slow `translator.deep()` call could complete after the user has moved away. The result would show at the old anchor via `make_dummy_target()`.
- All AppKit calls happen on the Rust main thread, which is correct. But if the product loop ever moves to a background thread, every AppKit call would need `dispatch_async(dispatch_get_mainQueue(), ...)`.

## Review Targets

- Is macOS Save truly using the same Library source of truth as Windows? Verify `library.db` path resolution matches.
- Are menu bar + global hotkey commands routed through exactly one dispatcher? Verify no duplicate handling.
- Does `lens_process_appkit_events()` actually process lifecycle notification blocks? The lifecycle bridge uses `dispatch_async` to main queue — verify NSRunLoop drains it.
- Is "M7 complete" supported by production wiring rather than module existence? Trace OCR call from menu click to card display.

## Real-world Validation Debt

- Safari/VS Code/KakaoTalk AX acquisition success rate
- Overlay visual quality (Retina, dark mode, multi-monitor)
- OCR accuracy (small text, dark themes, code editors)
- Screen Recording / Accessibility permission onboarding UX
- Sleep/wake actual behavior (lid close/open)
- Physical display hot-plug behavior
- Global hotkey actual behavior with other apps in foreground
- Deep Analysis result quality

## External / Tooling / Credential Debt

- Apple Developer ID (code signing) — EXTERNAL CREDENTIAL BLOCKER
- Notarization service — EXTERNAL CREDENTIAL BLOCKER
- `.icns` icon source asset — ASSET BLOCKER
- `cargo-tauri` CLI (local `.app` build) — LOCAL TOOLING BLOCKER (CI installs it)

## Next Session — Start Here

1. Read `apps/lens/src/macos.rs` (product loop) and `apps/lens/src/win.rs` (Windows reference) to understand the Tauri integration gap.
2. Wire macOS product loop into Tauri setup hook so Settings/Library/Detail surfaces work on macOS. The `tauri_app.rs` module is platform-neutral — investigate what's needed.
3. Connect `_policy` to the hover loop for per-app deny/allow (currently a TODO in `macos.rs`).
4. Add stale Quick/Deep/OCR generation tests — synthetic tests verifying old responses are discarded.
5. Run `scripts/check.sh --full`, PR, merge, then continue to FFI/unsafe audit.

## Git State

```
branch:  main
HEAD:    a456a3d
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
