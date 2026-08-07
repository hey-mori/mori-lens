# Walking up from the point, 2026-08-07

The acquirer gives up the moment `ElementFromPoint`'s element has no
text pattern, and answers `Unsupported`. In a Chromium or Electron
window a point very often lands on an overlay or a container, with the
text-bearing element one or two steps above it.

So: would walking up find the text? It is the obvious next move after
2026-08-07 showed VS Code's document is in the tree while only 2.2% of
its points sit inside a line-shaped element.

**Measured first. It does not work, and the way it fails is worse than
not working.**

```
cargo test -p lens-platform-windows -- --ignored --nocapture --test-threads=1 \
  measure_whether_an_ancestor
```

## The numbers

The window in front, an Electron application, 794 grid points over it:

| | points | share |
|---|---:|---:|
| a text pattern **at** the point | 135 | 17.0% |
| a text pattern within **3 steps up** | 43 | 5.4% |
| ...whose range is **on target** | **1** | **0.1%** |

Median steps to find one: 2.

## What that means

The first two lines say the idea is real: a third again as many points
have text reachable above them as have it at them. An acquirer that
walked would answer where it now says `Unsupported`.

The third line says what it would answer **with**. Of 43 points where an
ancestor had a text pattern, `RangeFromPoint` on that pattern returned a
range containing the point **once**.

Forty-two times out of forty-three it answered with text somewhere else
in the window. That is precisely the failure
[ADR-0009](../decisions/ADR-0009-pointer-distance-gate.md) was written
for, measured then at up to 1,465px from the pointer:

> Inside a document that call never fails.

An ancestor's range covers more than the pixel. Asking it *what is at
this point* gets an answer, and the answer is about the document rather
than about the point.

## So the change is not worth making

It would raise the acquisition rate and lower the correctness, by
turning an honest `Unsupported` into a confident answer about the wrong
sentence. The distance gate would catch most of them and the product
would show nothing either way, having spent three cross-process calls
per point to get there.

A card on the wrong sentence is worse than no card, which is the
sentence this repository uses to test its own translations.

## What this does not measure

**VS Code**, which was covered for all of this, and where
`ElementFromPoint` therefore answers about whatever is on top. The
window measured here is a different Electron application, and the
2.2%-on-a-line figure that prompted the idea was VS Code's.

It would be worth re-running with VS Code in front, and the result would
have to be very different to change the conclusion: the failure here is
not about which application it is, it is about what a container's range
means.

## The idea that is left

Both this and the 2.2% point the same way. The text is exposed, and it
is exposed in pieces too big to aim inside. Getting from a point to the
sentence under it within one large element is the open problem
[#34](https://github.com/swarea/mori-lens/issues/34) already has for
KakaoTalk, and nothing measured here shortens it.
