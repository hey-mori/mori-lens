# What an automatic OCR fall-through costs, 2026-08-06

[#101](https://github.com/hey-mori/mori-lens/pull/101) added a memory of
where the last reading found lines, so a hover in the same margin is not
another capture. It shipped with the saving **unmeasured**, and the
instrument added rather than the number claimed. This is that run.

Machine as in every other benchmark here.

```
cargo test -p lens-platform-windows -- --ignored --nocapture --test-threads=1 \
  measure_what_an_automatic_fall_through_costs
```

## The measurement

Eight readings at one point, twice: once with the memory cleared before
every call, which is what `AcquireContext::new` does by default, and once
with it left alone, which is what a reader who is not scrolling or typing
gets.

| Point | Without the memory | With it |
|---|---|---|
| a desktop corner | median **387ms**, samples 371 to 479 | 0ms, **8 of 8 refused** |
| the middle of a text-heavy window | median **1,366ms**, samples 91 to 1,581 | 0ms, **8 of 8 refused** |

Neither point resolved a word, which is the case the memory exists for
and the case an editor produces most of the time.

## The cost was recorded as smaller than it is

[#87](https://github.com/hey-mori/mori-lens/issues/87) says a fall-through
is "roughly half a second of work per hover". That is about right over a
margin at the edge of the screen, and it is **two and a half times too
low** over a window with text in it:

```
the foreground window: median 1366ms
  with the 60ms compositor wait: 1426ms, which at two settled
  hovers a second is 285% of one core
```

285% of one core is more than a core has. The band is the same size in
both cases; what differs is how much text is in it, because the
recogniser's work is proportional to what it finds rather than to the
pixels it is handed.

This does not contradict the CPU figure in
[`../privacy-model.md`](../privacy-model.md) §7, which is 179ms of CPU
per reading measured over twenty readings at a fixed rate. That is CPU
time and this is wall clock, and a reading that waits on the recogniser
spends more of the second than it burns.

## What the saving actually is, and what this does not establish

**8 of 8, at the same point.** That is the ceiling.

A reader's pointer does not return to one pixel. It moves along a line,
and the saving in real use depends on how often consecutive hovers land
inside the region the previous reading covered, which is 380x90 at the
size an editor renders. Nothing here measures that, and it needs a
reader's real pointer track rather than a loop.

So the honest claim is narrow: **when the answer is already known, it
costs nothing to say so**, and the previous behaviour was to spend
between 0.4 and 1.4 seconds rediscovering it.

## The other half of the file it exercised

The same run confirms the memory is cleared by default:
`AcquireContext::new` sets `view_may_have_changed`, so the first column
above is a fresh capture every time even though the acquirer is the same
one. A memory that leaked into that column would have shown as zeros in
both.
