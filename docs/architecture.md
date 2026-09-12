# Architecture

The constraints come from [`decisions/`](decisions/); this file describes the shape
they produce. It is descriptive, not aspirational: where something is
not built, it says so.

## 1. The pipeline

```
pointer moves
  │
  ▼
Hover intent          lens-core::hover        built
  dwell, radius, cooldown, generation id
  │  only when intent is confirmed
  ▼
Text acquisition      lens-core::acquire      trait, built
  ├─ accessibility    platform-windows        built (Windows)
  └─ OCR fallback     (M4)                    built (Windows)
  │
  ▼
Segmentation          lens-core::text         built
  word · sentence · paragraph around the pointer
  │
  ▼
Language routing      (M3)                    shipped
  script detection, safe fallback, engine choice
  script detection and target-language routing are built; Latin-script
  language identification (English vs French) returns Undecidable and
  translates, which is the safe default
  │
  ▼
Translation           (M3)                    shipped
  cache → quick model → degraded state
  │
  ▼
Overlay               (M2)                    shipped
  passive tooltip · interactive card
```

Everything above the acquisition boundary is platform-neutral and unit
tested. Everything at it is platform-specific and can only be verified by
running ([ADR-0003](decisions/ADR-0003-rust-core-platform-adapters.md)).

## 2. Crates

```
crates/build-stamp/        the commit a binary was built from, for both apps

crates/lens-core/          no OS dependencies, no async runtime
  geometry.rs              points, rectangles, monitor-space maths
  hover.rs                 the intent state machine and generations
  session.rs               policy plus intent plus acquisition routing
  text.rs                  TextTarget and word/sentence/paragraph spans
  acquire.rs               the TextAcquirer trait and AcquireError
  barren.rs                not photographing the screen when there is nothing there
  language.rs              which script a text is in, and whether to translate
  policy.rs                per-application allow / modifier / deny
  translate.rs             the Translator trait, TranslateError, echo checks
  cache.rs                 the same sentence twice costs nothing
  card.rs                  what a card says, and where it sits
  panel.rs                 what any non-card surface shows, as blocks of text (ADR-0012)
  keep.rs                  what a reader keeps, and the file shape
  analyse.rs               deep analysis: schema, parser, answerable filter
  detail.rs                the read-only panel content from a card
  reach.rs                 when to tell a reader why nothing happened
  welcome.rs               the first-run lesson
  theme.rs                 what a card looks like, as data
  settings.rs              what the user chose, parsed from their file
  recent.rs                session-recent tier: 50 cards, 2 hours, memory only (ADR-0014)
  tally.rs                 session counts and what the numbers suggest
  diagnostics.rs           is this working for me, in a window rather than a console (#165)
  icon.rs                  the tray glyph, computed rather than shipped
  contract_tests.rs        property-based tests for the acquisition contract
  stress_tests.rs          load and stress tests for concurrent acquisition

crates/lens-translate/     the Ollama client, no HTTP or JSON dependency
  lib.rs                   OllamaTranslator: quick, streaming, deep
  http.rs                  just enough HTTP/1.1 for a loopback socket
  json.rs                  just enough JSON, with the escaping that matters
  analyse.rs               schema, prompt, and parse for deep analysis
  deep_eval.rs             deep evaluation and linkage quality benchmarks
  live_tests.rs            integration tests against a running Ollama instance

crates/lens-settings/      versioned TOML settings, round-trip preservation
  schema.rs                the V1 schema: general, quick, deep, applications
  write.rs                 atomic write with rollback on invalid output
  migrate.rs               settings.conf to settings.toml migration

crates/lens-storage/      SQLite library for saved items and persistent history (ADR-0013, ADR-0014, ADR-0015)
  lib.rs                   schema, migrations, CRUD, search, saved.md import, history with cap and provenance

crates/platform-windows/   the Windows implementation of TextAcquirer
  com.rs                   apartment handling for a call
  window.rs                foreground window, process name, DPI, monitors
  uia.rs                   ElementFromPoint → RangeFromPoint → ranges
  inventory.rs             what a window holds, asked by handle not coordinate
  capture.rs               a rectangle of screen, held only until words come out
  ocr.rs                   Windows.Media.Ocr, and which readings to doubt
  overlay.rs               the card window: layered, click-through, no-activate
  glass.rs                 what is painted on it, with per-pixel alpha
  tray.rs                  the notification area icon and its menu
  autostart.rs             the per-user run key, off unless asked
  appearance.rs            what Windows says the interface should look like
  protected_harness.rs     a real password control, so a refusal can be proved
  ocr_acquire.rs           OCR-based text acquisition fallback
  single.rs                single-instance enforcement via named mutex
  clipboard.rs             clipboard read/write for text transfer
  welcome.rs               first-run welcome message display
  fullscreen_harness.rs    fullscreen application detection and handling
  panel_window.rs          non-card panel surface window management
  live_tests.rs            integration tests against live applications

crates/platform-macos/     the macOS implementation of TextAcquirer
  acquirer.rs              the AXUIElement-based text acquisition
  capture.rs               screen region capture for OCR fallback
  clipboard.rs             clipboard read/write for text transfer
  coords.rs                coordinate conversion between screen and view space
  ffi.rs                   foreign function bindings to macOS frameworks
  hotkey.rs                global hotkey registration and handling
  lifecycle.rs             application lifecycle and window management
  menubar.rs               the menu bar icon and its menu
  ocr_acquire.rs           Vision framework OCR integration
  overlay.rs               the overlay window: transparent, click-through
  permission.rs            accessibility permission checking and prompting
  single.rs                single-instance enforcement
  utf.rs                   UTF-8/UTF-16 conversion utilities
  vision_ocr.rs            Vision framework wrapper for text recognition

apps/hover-probe/          the measurement instrument
  probe/sweep.rs           a grid across a window, pointer untouched
  probe/inventory.rs       what each window holds, by handle
  probe/ocr_probe.rs       what OCR reads where accessibility does not
  probe/wake.rs            what the accessibility wake costs, and how often
  probe/report.rs          percentiles and the shape of a run

apps/lens/                 the product
  hover_worker.rs          acquisition on its own thread, off the pointer loop
  lifecycle.rs             when a card appears, stays, and goes
  manual_ocr.rs            the band around the pointer, on a keypress
  pause.rs                 both hotkeys, because they share one message queue
  pointer.rs               sampling, modifiers, and the process under the point
  refresh.rs               noticing an edited settings file and dark mode
  silence.rs               why no card appeared
  console.rs               where the banner goes when there is no console
  keeping.rs               appending to saved.md and library.db
  library_service.rs       the shared library handle, saved.md import
  detail_view.rs           the pinned interactive detail window
  settings_service.rs      the Tauri-managed settings state
  tauri_app.rs             the Tauri webview surface and commands
  lesson.rs                the first-run interactive lesson
  macos.rs                 macOS-specific application setup and lifecycle
  main.rs                  the application entry point and platform dispatch
  mori_transfer.rs         Mori integration and text transfer protocol
  win.rs                   Windows-specific application setup and lifecycle
  web/index.html           settings, library and diagnostics in a Tauri webview
  web/detail.html          the pinned detail window with interactive actions
```

The dependency arrow only ever points from an adapter to the core. A
pull request that adds `windows` to `lens-core`'s manifest does not
merge.

## 3. The acquisition contract

```rust
pub trait TextAcquirer {
    fn acquire(&self, point: ScreenPoint, ctx: &AcquireContext)
        -> Result<TextTarget, AcquireError>;
}
```

`TextTarget` carries what was read, where it was on screen, and how it
was obtained. `AcquireError` is a closed enum, and the variants are the
product's failure taxonomy rather than an error-handling detail:

| Variant | Means | OCR can help |
|---------|-------|--------------|
| `Unsupported` | the provider exposes no text pattern | yes |
| `Empty` | the read succeeded and returned nothing | yes |
| `ElementGone` | the UI changed under the call | yes, on retry |
| `Protected` | a password or secure field | **never** |
| `Timeout` | the target application did not answer | no |
| `Stale` | the pointer moved; the result is discarded | no |
| `Unavailable` | the accessibility layer itself is unreachable | no |

Collapsing these into one error would destroy the only data Milestone 1
produces ([ADR-0008](decisions/ADR-0008-probe-before-product.md)).

### What bounds `Timeout`

UI Automation's default is to wait **two minutes** for one cross-process
call. Nothing overrides it unless the client says so, which is why
`Timeout` sat in this table for weeks with no code path able to produce
one, and why a single wedged application could block an `acquire` for 45
seconds (issue #28).

Two guards, catching different failures:

- **Per call**, `IUIAutomation2::SetTransactionTimeout` at 800ms and
  `SetConnectionTimeout` at 300ms. This needs the `CUIAutomation8`
  co-class; the original `CUIAutomation` predates the interface, so a
  client built from it silently keeps the two-minute default.
- **Per chain**, a 400ms deadline across the eight calls one read makes.
  A provider answering every call just under the per-call limit is
  within its rights and still stalls the caller for seconds.

The per-call limit covers cross-process calls only. A provider inside
the client process is reached without one and can block indefinitely.
That is harmless here because Lens refuses to read its own process, and
it is recorded as a live test rather than left to be rediscovered.

## 4. Generations and staleness

Every hover gets a monotonically increasing `Generation`. It travels
with the request and is checked before anything is rendered or recorded.
A result whose generation is no longer current is dropped without being
used, because an asynchronous answer arriving after the pointer moved
would otherwise be attached to the wrong text.

This is not an optimisation. It is the rule that keeps a translation
from appearing over a sentence it does not belong to, and it is enforced
in `lens-core` so both platforms inherit it.

Concurrency is bounded per kind: one acquisition, one quick translation,
one deep analysis, one OCR in flight at a time. A hover never spawns an
unbounded number of tasks.

## 5. Threading and COM

UI Automation is COM. Each call site initialises an apartment for the
duration of the call and tolerates `RPC_E_CHANGED_MODE`, which means the
thread already had a usable apartment that is not ours to tear down.
Nothing caches a COM interface across calls, because an element from a
window that has since closed is a crash waiting for a slow day.

Acquisition runs off the pointer thread. The pointer sampler must stay
responsive to decide that a hover was abandoned, and a call into a wedged
foreign application must not be able to block that decision.

## 6. What is deliberately not here yet

**These are staging notes, not prohibitions**, and the heading has always
said so. Each entry is dated where it has been overtaken. An entry with a
condition in it is spent when the condition is met, and the one below
about a service split names its own: *until there is a UI to protect*.

- **No IPC, no service split.** *Condition tested 2026-08-13; the split
  is still not here, and now for a measured reason rather than an absent
  UI.* One process. Splitting the engine from the UI adds lifecycle,
  versioning, install, and log-correlation problems that buy nothing
  until there is a UI to protect. A UI is now decided
  ([ADR-0012](decisions/ADR-0012-three-product-surfaces.md)), and it does
  **not** force the split: a Tauri webview started beside the message
  loop, the tray, the overlay's own thread and the UI Automation
  apartment, and nothing refused
  ([`does a webview fit`](benchmarks/2026-08-13-does-a-webview-fit.md)).
  What would force a split is crash isolation, and nobody has measured
  whether a webview failure takes the hover path down with it. The seam
  that makes a later split possible is the trait boundary, and that
  already exists.
- **No async runtime.** Acquisition is synchronous on a worker thread.
  No async runtime entered with the HTTP client: it is a hand-written
  loopback client on blocking sockets, and translation runs on a worker
  thread rather than a runtime.
- **No structured storage layer.** *Written before `keep.rs`; corrected
  2026-08-13; updated 2026-09-02.* `library.db` (SQLite) is the
  structured source of truth
  ([ADR-0013](decisions/ADR-0013-the-local-library.md)). CRUD, search,
  tags, notes, manual review (Later/Known/Archive, Issue #141),
  Markdown and JSON export, `secure_wipe`, and `saved.md` import are
  built and wired into the running product. `saved.md` survives as the
  human-readable export. The translation cache is still memory and
  still dies with the process.
- **No settings UI.** *Written when the probe was the only binary;
  corrected 2026-08-13; updated 2026-08-28.* The product reads
  `%APPDATA%\Mori Lens\settings.toml` on launch. A versioned TOML
  settings file (`crates/lens-settings/`) and a Tauri v2 settings
  window (`apps/lens/web/index.html`) are built and wired into the
  running product ([#165](https://github.com/swarea/mori-lens/issues/165)).
