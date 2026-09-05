# Mori Lens — Session Handoff

Last updated: 2026-09-05

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product. M6 (Accessibility acquisition), M7 (overlay + OCR), and production parity are the focus.

## Completed

### M6 — macOS Accessibility Acquisition
- `crates/platform-macos/` crate: FFI bindings (AX/CG), `MacTextAcquirer` implementing `TextAcquirer` trait
- Permission (`AxPermission`), coordinate conversion (per-display Retina/multi-monitor), UTF-8/UTF-16/CFRange helpers
- 31 unit tests including property-based and stress tests

### M7 — macOS Native Overlay + OCR
- NSPanel overlay (transparent, click-through, non-activating) via Objective-C bridge
- Vision framework OCR (`VNRecognizeTextRequest`) via native bridge
- ScreenCaptureKit `SCStream` capture (macOS 15+ only path; `CGWindowListCreateImage`/`CGDisplayCreateImage` obsoleted)
- `OcrTextAcquirer` implementing `TextAcquirer` for OCR fallback path

### Production Parity
- **Event loop**: `lens_process_appkit_events()` drains NSRunLoop each 25ms iteration — keeps AppKit responsive
- **Global hotkeys**: CGEventTap-based Cmd+Opt+L/D/C/S/O/Q (real global, not just menu key equivalents)
- **Unified command dispatch**: `ProductCommand` enum, menu bar + global hotkeys feed same dispatch
- **Product commands**: Deep Analysis, Copy (NSPasteboard), Save (canonical `Library::insert`), Manual OCR, Pause/Resume, Quit
- **Stale generation**: monotonic `current_generation` counter, `HoverEvent::Left` clears `current_card`
- **Lifecycle**: NSWorkspace wake/display-change notifications → topology refresh + AX re-check
- **Single instance**: `flock()` on `~/Library/Application Support/Mori Lens/.lock`
- **AppIdentity**: process name from AX tree via `CFNumberGetValue` + `proc_name`, window title from `AXTitle`
- **Coordinate correctness**: `display_scale_at_physical()` for mixed-scale multi-monitor display lookup
- **Save → Library API**: `Library::open("library.db")` + `Library::insert(&Item)` — matches Windows `library_service.rs`

### Packaging + CI
- `tauri.conf.json`: macOS `"app"` bundle target, `minimumSystemVersion: "12.3"`
- `ci.yml`: `verify-macos` job on `macos-latest` running `scripts/check.sh --full`
- `release.yml`: `build-windows` + `build-macos` + `release` 3-job structure
- macOS artifact: `.app` → `ditto` zip → SHA-256 → GitHub Release (hard failure if .app missing)

### App Shell Split
- `apps/lens/src/main.rs`: thin dispatcher (25 lines)
- `apps/lens/src/win.rs`: Windows product loop (3300+ lines, unchanged)
- `apps/lens/src/macos.rs`: macOS product loop with full command dispatch

## Key Design Decisions

1. **lens-core remains platform-neutral** — zero `#[cfg(windows)]` or `#[cfg(target_os)]` in core
2. **AX attribute names created at runtime** via `CFStringCreateWithCString` — linker doesn't export `kAX*Attribute` symbols on all macOS versions
3. **Accessibility-first OCR contract** — AX always tried first; OCR fallback only when `ocr_can_help()` and `OcrMode != Off`
4. **Memory-only privacy** — no screenshot/OCR image written to disk
5. **Non-fatal degradation** — AX permission, Screen Recording, overlay, menu bar failures don't exit the process
6. **CGEventTap for global hotkeys** — uses existing AX permission, no additional permission needed
7. **SCStream for capture** — only available path on macOS 15+; older CG APIs obsoleted

## Changed Files (This Session)

```
crates/platform-macos/
├── Cargo.toml                    # cc build-dep added
├── build.rs                      # compiles 6 Objective-C bridges
├── native/
│   ├── overlay.h/m               # NSPanel, mouse position, display scale, event drain
│   ├── vision_ocr.m              # VNRecognizeTextRequest
│   ├── capture.m                 # SCStream single-frame capture
│   ├── menubar.h/m               # NSStatusItem with 7 menu items
│   ├── clipboard.h/m             # NSPasteboard
│   ├── lifecycle.h/m             # NSWorkspace wake/display notifications
│   └── hotkey.h/m                # CGEventTap global hotkeys
└── src/
    ├── lib.rs                    # module exports
    ├── ffi.rs                    # AX/CG FFI + runtime CFString attrs
    ├── acquirer.rs               # MacTextAcquirer (TextAcquirer impl)
    ├── overlay.rs                # Overlay RAII wrapper
    ├── vision_ocr.rs             # Vision OCR Rust wrapper
    ├── capture.rs                # Screen capture Rust wrapper
    ├── menubar.rs                # MenuBar RAII wrapper
    ├── clipboard.rs              # Clipboard Rust wrapper
    ├── lifecycle.rs              # Lifecycle observer
    ├── hotkey.rs                 # Global hotkey + ProductCommand
    ├── coords.rs                 # DisplayTopology, coordinate conversion
    ├── utf.rs                    # UTF-8/UTF-16/CFRange helpers
    ├── permission.rs             # AxPermission typed state
    ├── ocr_acquire.rs            # OcrTextAcquirer (TextAcquirer impl)
    └── single.rs                 # flock-based single instance

apps/lens/src/
├── main.rs                       # cross-platform dispatcher
├── win.rs                        # Windows product loop (unchanged)
└── macos.rs                      # macOS product loop (full implementation)

.github/workflows/
├── ci.yml                        # + verify-macos job
└── release.yml                   # + build-macos job, zip artifact

.gitignore                        # + .qwen/
```

## Test Results

- `scripts/check.sh --full`: **PASS** (fmt, clippy -D warnings, test, version consistency)
- Workspace tests: 479 (core) + 31 (macos) + 17 (windows) + 67 (settings) + 94 (translate) = **all green**
- macOS binary: builds and runs (`./target/debug/lens --version` → `Mori Lens 0.1.0 · macOS`)

## Known Issues / Risks

1. **Tauri app shell not integrated on macOS** — Settings/Library/Detail UI surfaces are Windows-only via Tauri. macOS runs as native CLI loop. Library DB is accessible directly (Save uses it), but no UI.
2. **Policy enforcement not connected** — `_policy` variable set up but not consulted in hover loop for per-app deny/allow.
3. **`cargo-tauri` not installed locally** — `.app` bundle can't be built locally. CI workflow installs it. TOOLING BLOCKER.
4. **No `.icns` icon** — only `icons/icon.ico` exists. macOS bundle uses default icon. ICON BLOCKER.
5. **No code signing** — unsigned development artifacts. Apple Developer ID needed for production.

## Incomplete Work

| Item | Priority | Notes |
|------|----------|-------|
| Tauri Settings/Library/Detail on macOS | High | Requires Tauri setup hook integration |
| Policy enforcement in hover loop | Medium | `_policy` TODO in macos.rs |
| macOS `.app` local build verification | Medium | Needs `cargo install tauri-cli` |
| Stress tests for command storm / long Ollama | Medium | Synthetic tests for native wrappers |
| FFI unsafe audit (full) | Low | All blocks have SAFETY comments; deeper review pending |

## Next Session — First Tasks

1. **Tauri app shell integration** — Wire macOS product loop into Tauri setup hook so Settings/Library/Detail surfaces work. This is the biggest remaining gap.
2. **Policy enforcement** — Connect `_policy` to the hover loop for per-app deny/allow.
3. **Local `.app` build** — Install `cargo-tauri` and verify the bundle builds correctly.

## Git State

```
Branch: main (clean)
Latest commit: c5bd0ff — fix: Save uses canonical Library API
Total PRs this session: #260-#270 (11 PRs merged) + 1 direct commit
Remote: origin → github.com/swarea/mori-lens
```

## Repository Conventions

- Commit format: `<type>: <imperative summary>` + bullet-list body
- All changes via branch → PR → merge (except 1 accidental direct push)
- `scripts/check.sh --full` must pass before commit
- No `#![cfg(windows)]` at crate level in shared code
- `lens-core` must remain platform-neutral (ADR-0003)
- macOS native code in Objective-C bridges compiled by `cc` crate
- All unsafe blocks require SAFETY comments (clippy -D warnings)
