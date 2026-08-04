# Acquisition sweep · 2026-08-04

First real measurement of the Windows acquisition chain. Produced by
`hover-probe sweep`, which samples a grid of screen coordinates without
moving the pointer.

```
machine       Windows 11 Home, build 26200
build         d1e236b-dirty  (the sweep harness, before it was merged)
monitors      2, side by side
  primary     1920x1200 physical at 125% scaling, origin (0, 0)
  secondary   1920x1080 physical at 100% scaling, origin (1920, 0)
dpi           per-monitor aware v2, declared at process start
ollama        not involved; nothing here calls a model
```

Three runs. The first two are recorded in the pull request as the ones
that found the harness bugs; only the third is measured here, because
the first two sampled occluded windows and the second ran a stale
binary.

## Latency, the whole chain

`ElementFromPoint` through `GetBoundingRectangles`, per sample point,
across every measured window.

| | p50 | p95 |
|---|---:|---:|
| observed range across windows | 7.9 to 11.2ms | 12.4 to 23.7ms |

Against the targets in
[`../research/hover-text-acquisition.md`](../research/hover-text-acquisition.md)
§8, which budget element resolution at p50 15ms / p95 50ms and text plus
bounds extraction at p50 20ms / p95 80ms, so 35ms and 130ms combined:

**The whole chain lands inside the budget for one of its two stages.**
Roughly 10ms at p50 and 20ms at p95, against a combined budget of 35ms
and 130ms. Latency is not the problem.

One caveat on the numbers: a cold first call into a process is far
slower than the rest. One window showed p95 53.8ms against a p50 of
11.0ms, and that spread is first-call COM and provider warm-up, not a
tail in steady-state reads.

## On-target rate, and the finding that matters

"On-target" is the share of acquisitions whose resolved word rectangle
**contains the sample point**. It is a correctness proxy that needs no
human and prints no text.

The result splits cleanly by UI Automation control type, and the split
held across all three runs and every application:

| Control type | On-target | Distance p50 | Distance max |
|---|---:|---:|---:|
| `50020` Text | **86 to 100%** | 0px | 0 to 21px |
| `50030` Document, `50004` Edit | **3 to 18%** | 82 to 322px | 641 to 1465px |

Measured windows in the clean run:

| Window | Controls seen | Acquired | On-target | Distance p50 / max |
|---|---|---:|---:|---:|
| chrome, 956x925, secondary @100% | `50020` only | 7/132 | 86 to 100% | 0px / 3px |
| chrome, 1777x1072, secondary | `50030`, `50004` | 62/128 | 5% | 322px / 935px |
| claude (Electron), 1884x1112, primary @125% | `50020` only | 16/240 | 69 to 75% | 0px / 14px |

## What that means

`RangeFromPoint` on a **label** returns the label. The element is the
text, its rectangle is small, and the point is inside it.

`RangeFromPoint` on a **document or edit control** returns the nearest
text *within that whole scrollable region*. A point in the empty area
below the last line of a document resolves to that last line, hundreds
of pixels away, and reports success. It never fails, so there is no
error to catch.

That is the hazard the research brief predicted, now with a
distribution attached. **Inside a document, the distance check is the
only defence against explaining a sentence the user is not looking at.**
It is not a refinement; without it the product is wrong in exactly the
surfaces it exists for: editors, documents, and browser page bodies.

Recorded against issue #5, which asked whether a threshold is needed.
It is.

## What this does not establish

- **That the low on-target rate inside documents is empty space.** It is
  the likely explanation, and the distances are consistent with "nearest
  line above an empty region". It has not been separated from the
  alternative, that resolution inside a document control is simply
  inaccurate. Distinguishing them needs sampling only points known to
  carry glyphs, which this harness cannot tell.
- **A per-application coverage number.** `acquired / sampled` measures
  how much of a window is text, not how good the provider is. A uniform
  grid lands mostly on chrome, margins, and images, where `unsupported`
  is the correct answer.
- **Anything about a window that was not in front.** See the occlusion
  limit in
  [`../compatibility/windows-app-matrix.md`](../compatibility/windows-app-matrix.md).

## Rectangles across two DPI scales

Both monitors produced correct-looking rectangles: 100% on-target on
Text controls on the 100% secondary, and 69 to 75% with a maximum
distance of 14px on the 125% primary.

The 14px is small enough to be line-height or rounding rather than the
gross misalignment a DPI bug produces, which would put rectangles off by
a factor of 1.25 and show up as distances in the hundreds. So
per-monitor v2 appears to be working. "Appears" is deliberate: this was
not a controlled DPI test, it is an observation from a sweep that
happened to cross two scales.
