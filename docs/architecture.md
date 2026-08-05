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
  └─ OCR fallback     (M5)                    planned
  │
  ▼
Segmentation          lens-core::text         built
  word · sentence · paragraph around the pointer
  │
  ▼
Language routing      (M3)                    planned
  script detection, language id, engine choice
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

crates/platform-windows/   the Windows implementation of TextAcquirer
  com.rs                   apartment handling for a call
  window.rs                foreground window, process name, DPI
  uia.rs                   ElementFromPoint → RangeFromPoint → ranges

apps/hover-probe/          the M1 measurement instrument
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
- **No storage layer.** Nothing is persisted. SQLite enters with the
  translation cache is in memory and mandatory; a durable one would be a
  record of what was read and stays a separate opt-in.
- **No settings UI.** Configuration is command-line flags on the probe.
