# Taking a container apart, 2026-08-07

Where the day's other three measurements left it: a window's text is
exposed, and it is exposed in elements too large to aim inside, so a
point tells you the document and not the sentence. That is
[#14](https://github.com/swarea/mori-lens/issues/14) and
[#34](https://github.com/swarea/mori-lens/issues/34) in one sentence,
and `RangeFromPoint` on such an element answers about text somewhere
else 42 times out of 43
([`2026-08-07-walking-up-from-the-point.md`](2026-08-07-walking-up-from-the-point.md)).

There is one thing left that the provider already knows.

## `GetBoundingRectangles` returns one rectangle per rendered line

Not per element. Per **line**. So a container's visible range can be
asked where its lines are without asking it what they say, and
`lens_core::geometry::line_under` already turns a list of line
rectangles and a point into an answer. That rule was extracted for the
OCR path and it does not care where the rectangles came from.

Measured on the window in front, an Electron application, taking its
five largest text elements:

```
  element        rects  median height  tallest  points on a line
  #0 h=890         132             17      854    784/2331  (34%)
  #1 h=318           0              0        0      0/403   (0%)
  #2 h=98            0              0        0      0/124   (0%)
  #3 h=98            0              0        0      0/124   (0%)
  #4 h=78            0              0        0      0/93    (0%)
```

The 890px container, which is the whole conversation area, hands back
**132 rectangles with a median height of 17px**. Those are lines.

Laying the sweep's own 24px grid over that element and asking
`line_under` which line each point is on: **784 of 2331**.

The four smaller elements return nothing, which is what a scrolled-out
or empty range does and is not a failure of the technique.

## What this is not

**Not comparable to the 17% figure** from the ancestor measurement. That
was points over the whole window with a text pattern at the point; this
is points inside one container that land on one of its lines. They
answer different questions and the second does not supersede the first.

**Not a working acquisition.** Knowing which rectangle a point is on is
not the same as having that line's text. Getting it means walking a
range by `TextUnit_Line` and matching rectangles, which is a
cross-process call per line and is exactly the cost this has not
measured. A hover has 800ms and the walk is bounded by what is visible,
so it is plausible and it is not established.

**Not VS Code**, which was covered all day. The 2.2% that started this
was VS Code's and the 34% is not.

## Why it is worth building anyway

Because the alternative that failed, failed for a reason this one does
not share. `RangeFromPoint` on a container asks the **provider** what is
at a point, and a provider that owns a whole document answers about the
document. This asks the provider only for **geometry**, and decides the
targeting here, with a rule this repository wrote and tested.

It is also the same shape the OCR path already has: rectangles in,
`line_under`, a line out. Two acquisition paths converging on one rule
is a better position than two paths with two rules.

## What would settle it

An implementation that, when the element at the point has no text
pattern, walks up to the nearest container, asks for its visible
ranges' rectangles, picks the line with `line_under`, and retrieves that
line's text. Then the sweep, against the applications in the matrix,
with the acquisition and on-target rates side by side against the rows
already there.

That is a change to `uia.rs` and it belongs in its own piece of work. It
reverses no ADR: acquisition stays accessibility-first
([ADR-0002](../decisions/ADR-0002-accessibility-first-acquisition.md)),
and the distance gate
([ADR-0009](../decisions/ADR-0009-pointer-distance-gate.md)) still has
the last word on whether an answer is near enough to be the one meant.
