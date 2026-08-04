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

A second caveat, from the second run: the same windows measured
**p50 4.4 to 5.8ms and p95 7.0 to 12.1ms**, roughly half the first run's
figures, on the same build and the same machine. The difference is
machine load, not code. It is a concrete reason the rule in
[`README.md`](README.md) holds: a regression is a change on the same
machine between two builds measured close together, never a comparison
across runs.

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

## Second run: the shape of the misses

The first run left one question open, and it mattered because the two
answers call for opposite products. If a far-away resolution is the
nearest text to an **empty region**, suppressing the card is right,
because there is nothing there to explain. If resolution inside a
document is simply **inaccurate**, suppressing throws away text the user
was pointing at.

A second run recorded, for every miss, which side of the resolved
rectangle the point fell on and which rectangle it landed on. The two
hypotheses have different signatures: empty space is directional and
concentrates on a few attractors, inaccuracy scatters evenly.

| Window | Controls | Misses | Distinct rects | Per rect | below / above / left / right |
|---|---|---:|---:|---:|---|
| chrome 1777x1072 | `50030`, `50004` | 59 | 16 | 3.7 | 9 / **44** / 0 / 6 |
| chrome 956x925 | `50004`, `50020` | 18 | 10 | 1.8 | 8 / 0 / 0 / **10** |
| claude 1884x1112 | `50020` | 6 | 6 | 1.0 | 0 / 2 / 0 / 4 |

**The misses are strongly directional, and that settles it.** Inaccurate
resolution cannot produce `left 0, above 44` or `above 0, left 0,
right 10`. Noise is symmetric. A layout is not.

Note that the direction **differs per window**: one resolves mostly
downward from the point, one mostly leftward. That is exactly what
"nearest text to an empty region" predicts, because the direction is set
by where the empty region sits relative to the text, which is a property
of the page.

So: the far cluster is empty space. Suppressing is correct.

### The correction that came with it

The same run overturned a simpler reading. **The misses on Text controls
are not the same phenomenon at all.** On `claude`, six misses across six
rectangles, at p95 7px and max 14px. That is glyph-box and line-height
slack, not a wrong target.

Which means a threshold of exactly zero, "the rectangle must contain the
point", is **wrong**. It would discard 19 to 23% of perfectly good Text
control hits over a few pixels.

The two clusters are separated by an enormous gap: roughly 14px against
322px. Almost any threshold in between separates them, so the value is
not delicate. Something on the order of one line height is defensible
and scales with DPI and font size.

## Third run: the gate, checked against live data

[ADR-0009](../decisions/ADR-0009-pointer-distance-gate.md) sets the
tolerance at one line height. A third run reports what that gate would
do to each miss, so the threshold is re-tested on every later run rather
than justified only by the run that chose it.

| Misses | Distance p50 / max | Gate accepts | Reading |
|---:|---:|---:|---|
| 1 | 1px / 1px | **1 of 1** | slack, correctly kept |
| 3 | 12px / 287px | **2 of 3** | two slack kept, one empty-space miss dropped |
| 10 | 110px / 572px | **1 of 10** | one slack kept, nine dropped |
| 20 | 520px / 935px | **0 of 20** | all empty space, all dropped |

The gate turns strict-containment rates of 80%, 50%, 38%, and 0% into
effective rates of 100%, 83%, 44%, and 0%. It keeps what a strict rule
would have thrown away and drops what a permissive one would have shown
on the wrong sentence.

This is a check, not a proof. It confirms the threshold separates the
two populations on this data; it says nothing about a font size or a
display scaling that has not been measured. The sweep now prints the
line on every run, which is the point.

## Fourth run: what a word-unit expansion actually returns

`ExpandToEnclosingUnit` is allowed to answer with a **larger** unit when
a provider does not implement the one requested. That is documented
behaviour, and it matters twice over: the expanded range supplies the
anchor rectangle, and it had been supplying the pointer's character
offset.

Classifying what came back:

| Result | Count |
|---|---:|
| one word | **88** |
| empty | 1 |
| several words | 0 |
| several lines | 0 |

Chromium, Electron, and a Win32 window all returned a single word. No
normalisation was observed anywhere.

So the anchor rectangles are genuine word rectangles, and the earlier
on-target figures were measured against words rather than against lines.

**This also downgrades a bug from active to latent.** The pointer's
character offset had been measured to the *expanded* range's start,
which is correct exactly as long as expansion returns a word. Had any
provider normalised to a line, the offset would have landed at the
line's start and segmentation would have returned that line's **first
word** rather than the word under the pointer. Plausible output, wrong
answer, no error.

It is fixed regardless: the offset is now measured to the degenerate
range from `RangeFromPoint`, which is the pointer's own character
position. Four windows not exhibiting a documented behaviour is not
evidence that it does not happen.

Also in this run: **Notepad appeared for the first time**, with 5
acquisitions on `50020` Text controls at 0 to 1px. Five points is a weak
sample and they are more likely to be window chrome than the document
body, so this is a foothold rather than a row.

## What this does not establish

- **That "empty space" is proven rather than strongly indicated.** Three
  windows, and the argument is from directional asymmetry rather than
  from knowing which pixels carry glyphs. It is enough to choose a
  product behaviour; it is not a proof.
- **A per-application coverage number.** `acquired / sampled` measures
  how much of a window is text, not how good the provider is. A uniform
  grid lands mostly on chrome, margins, and images, where `unsupported`
  is the correct answer.
- **Anything about a window that was not in front.** See the occlusion
  limit in
  [`../compatibility/windows-app-matrix.md`](../compatibility/windows-app-matrix.md).
- **Anything durable about an application.** A row describes a window's
  **content at that moment**. The 956x925 Chrome window read as `50020`
  only at 100% on-target in the first run and as `50004` plus `50020` at
  31% in the second, because the page in it changed. An application's
  row is really a sample of the pages it was showing, and one row is a
  weak sample.

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
