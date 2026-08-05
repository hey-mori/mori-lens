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
  └─ OCR fallback     (M4)                    planned
  │
  ▼
Segmentation          lens-core::text         built
  word · sentence · paragraph around the pointer
  │
  ▼
Language routing      (M3)                    not built
  script detection, language id, engine choice
  the target language is a flag; nothing detects the source
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
crates/lens-core/          no OS dependencies, no async runtime
  geometry.rs              points, rectangles, monitor-space maths
  hover.rs                 the intent state machine and generations
  text.rs                  TextTarget and word/sentence/paragraph spans
  acquire.rs               the TextAcquirer trait and AcquireError
  policy.rs                per-application allow / modifier / deny
  translate.rs             the Translator trait and TranslateError
  cache.rs                 what has already been translated
  card.rs                  what a card says, and where it sits
  theme.rs                 what a card looks like, as data
  settings.rs              what the user chose, parsed from their file
  icon.rs                  the tray glyph, computed rather than shipped
  cache.rs                 the same sentence twice costs nothing

crates/lens-translate/     the Ollama client, no HTTP or JSON dependency
  http.rs                  just enough HTTP/1.1 for a loopback socket
  json.rs                  just enough JSON, with the escaping that matters

crates/platform-windows/   the Windows implementation of TextAcquirer
  com.rs                   apartment handling for a call
  window.rs                foreground window, process name, DPI, monitors
  uia.rs                   ElementFromPoint → RangeFromPoint → ranges
  inventory.rs             what a window holds, asked by handle not coordinate
  overlay.rs               the card window: layered, click-through, no-activate
  glass.rs                 what is painted on it, with per-pixel alpha
  tray.rs                  the notification area icon and its menu
  autostart.rs             the per-user run key, off unless asked
  appearance.rs            what Windows says the interface should look like

apps/hover-probe/          the measurement instrument
apps/lens/                 the product
  lifecycle.rs             when a card appears, stays, and goes
  refresh.rs               noticing an edited settings file and dark mode
  silence.rs               why no card appeared
  console.rs               where the banner goes when there is no console
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

- **No IPC, no service split.** One process. Splitting the engine from
  the UI adds lifecycle, versioning, install, and log-correlation
  problems that buy nothing until there is a UI to protect. The seam that
  makes a later split possible is the trait boundary, and that already
  exists.
- **No async runtime.** Acquisition is synchronous on a worker thread.
  No async runtime entered with the HTTP client: it is a hand-written
  loopback client on blocking sockets, and translation runs on a worker
  thread rather than a runtime.
- **No storage layer.** Nothing is persisted. The translation cache is
  in memory and dies with the process. A durable one would be a record of
  what the user read, so it stays a separate opt-in with its own
  decision rather than arriving as a performance detail.
- **No settings UI.** Configuration is command-line flags on the probe.
