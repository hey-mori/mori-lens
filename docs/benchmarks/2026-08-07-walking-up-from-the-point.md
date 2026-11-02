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

## Doubted, corrected, and re-run

The first version of this measured whether the range `RangeFromPoint`
returns contains the point. It tested that range **as returned**, which
is degenerate: a caret-shaped sliver whose rectangle may contain no
sampled point at all. That would report everything off target for a
mechanical reason, and the product expands to a word and a paragraph
before deciding anything.

It also measured only the **foreground** window, so it could not run when
nothing was in front, which is how a doubtful figure stood for an
afternoon with nothing able to check it.

Both are fixed. The range is expanded to a line first, and the grid
samples every window at the points where that window is the one a
pointer would hit, so there is nothing to arrange and nothing to be in
front.

## The numbers

```
process              points  at the point   within 3 up   and on target
WindowsTerminal         325    300 (92.3%)     0 ( 0.0%)     0 ( 0.0%)
chrome                  610    421 (69.0%)     0 ( 0.0%)     0 ( 0.0%)
claude                  640     32 ( 5.0%)    17 ( 2.7%)     0 ( 0.0%)
explorer                 88      0 ( 0.0%)     0 ( 0.0%)     0 ( 0.0%)
all                    1663    753 (45.3%)    17 ( 1.0%)     0 ( 0.0%)
```

**The conclusion held, and got stronger.** Of the 17 points where an
ancestor had a text pattern, **none** answered with a line containing the
point. The correction did not rescue it: 0 of 17 is worse than the 1 of
43 it replaces.

**The lift is also much smaller than it looked.** 1.0% of points across
the desktop, not the 5.4% a single-window run suggested, because that
run happened to sample the one application where it is least rare.

## The number that was worth the trouble

`at the point` is what the acquirer uses today, and it is **45.3%**
across the desktop: 92.3% in a terminal, 69.0% in Chrome, and **5.0% in
an Electron application**.

The failure is not general. It is that one shape of application, and
this is the first figure in this repository that separates them by
process on one desktop in one pass.

## So the change is not worth making

It would raise the acquisition rate by one point and lower the
correctness, by turning an honest `Unsupported` into a confident answer
about the wrong sentence. The distance gate would reject them and the
product would show nothing either way, having spent three cross-process
calls per point to get there.

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
[#34](https://github.com/hey-mori/mori-lens/issues/34) already has for
KakaoTalk, and nothing measured here shortens it.
