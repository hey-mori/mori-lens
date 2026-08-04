# ADR-0009 · A resolution too far from the pointer is suppressed

**Status** · Accepted, 2026-08-04

## Context

`RangeFromPoint` returns the text *nearest* a coordinate, not the text
under it. Over a label that distinction is invisible, because the label
is the element and the point is inside it. Inside a document or an edit
control it is the whole problem: the control covers most of the window,
so a point anywhere in it resolves to something, and **the call never
fails**. There is no error to catch.

Measured on 2026-08-04
([`../benchmarks/2026-08-04-acquisition-sweep.md`](../benchmarks/2026-08-04-acquisition-sweep.md)):

| Control type | On-target | Distance p50 | Distance max |
|---|---:|---:|---:|
| `50020` Text | 86 to 100% | 0px | 3 to 21px |
| `50030` Document, `50004` Edit | 3 to 18% | 82 to 322px | 641 to 1,465px |

A second run recorded which side of the resolved rectangle each miss
fell on. The misses are strongly directional: `left 0, above 44` in one
window, `above 0, left 0, right 10` in another, with the direction
differing per window. Symmetric noise cannot produce that. A layout can,
because the direction is set by where the blank area sits relative to
the text.

So a distant resolution means the pointer is over **empty space**, and
the nearest text is somewhere else on the page.

The same measurement found a second, separate population that a naive
rule would destroy. Misses on Text controls run to 7px at p95 and 14px
at maximum, one rectangle each. That is glyph-box and line-height slack,
not a wrong target.

## Decision

**A resolution whose rectangle is further from the pointer than roughly
one line height is discarded, and no card is shown. Anything closer is
accepted as the word under the pointer.**

Concretely:

- The gate is **relative to the resolved line's height**, not a fixed
  pixel count. The near cluster is line-height and glyph-box slack by
  nature, so it scales with font size and with display scaling. A
  constant tuned on a 125% display at one font size is wrong on the next
  machine.
- Discarding is silent. It is not an error state, not a message, and not
  a retry: there is genuinely nothing under the pointer to explain.
- The gate lives in `lens-core` beside `pointer_distance`, so both
  platform adapters inherit it and it is testable without a desktop.

The two clusters sit at roughly 14px against 322px, so the exact
multiple is not delicate. One line height is the starting value, to be
tuned once there is an overlay to judge it against.

## Consequences

- **Lens shows nothing when the pointer is in a margin or below the last
  line.** That is the correct behaviour and it is also indistinguishable
  from a broken product if it happens often. How often it fires during
  real reading is an M2 question, and it is the reason the value stays
  open to tuning.
- A strict containment rule is ruled out. It would have discarded 19 to
  23% of good Text control hits over a few pixels, and it is the rule
  most people would write first.
- The gate is the only defence in a document, so a change that weakens
  it is a product regression rather than a tuning choice, whatever the
  tests say.
- The measurement rests on three windows and on directional asymmetry
  rather than on knowing which pixels carry glyphs. That is enough to
  choose a behaviour and it is not a proof. If a later run shows a
  window whose misses are symmetric and scattered, this ADR is the thing
  to revisit, because that would mean resolution really is inaccurate
  somewhere and suppression is throwing away text.
