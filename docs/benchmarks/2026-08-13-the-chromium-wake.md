# What the Chromium accessibility wake costs, 2026-08-13

[Issue #4](https://github.com/swarea/mori-lens/issues/4) has said since the
first week that the wake is unmeasured. The instrument to measure it was built
and never run. This is the run.

Build `fa223a0`, `hover-probe wake`, against the desktop as it was: Chrome,
Electron, Explorer, a terminal and a few others, all opened by the reader for
their own reasons rather than arranged for this.

## The question that decides the issue

Not what one wake costs. **How often it recurs.**

> Paid once per window, 340ms is a first hover that is slightly slow. Paid per
> hover, it is more than the whole 320ms dwell budget, and the browser is where
> most people meet a second language.

## What was measured

```
quiet     re-checkable windows     went cold
 45s              3                    0
120s              1                    0
240s              2                    1
```

**The tree is not rebuilt per hover.** Three runs, and no window went cold
inside two minutes of complete assistive silence. On a hover path that is the
answer that matters: a reader hovering in a browser pays the wake once and then
keeps reading.

**And it does not last for the window's lifetime either.** One of two Chrome
windows was cold again after four minutes. So the shape is neither of the two
the issue proposed:

> A reader who comes back to a browser after a break pays the wake again.

Not on every hover. Not once ever. Once per stretch of reading, where a stretch
ends somewhere between two and four minutes of not reading.

## What was not measured, and why

**The cost of a wake.** Nothing Chromium-shaped was ever cold when asked, so
`message` and `settle` are empty in every Chrome row. The 140ms settle carried
over from Mori remains an assumption.

That is not a failure of the run, but it is a limit of it, and the reason is
worth writing down: **an instrument that reads a window is itself an assistive
client.** The `inventory` run that found the windows woke them. Lens itself ran
against this desktop earlier the same day. A cold Chromium window is one nobody
has looked at, and looking is what this measures with.

The two rows that did carry a number are `explorer`, at **0.1ms for the message
alone**, and in both the wake did not repair the failure within 600ms. Explorer
is not Chromium and its `Unsupported` is not the cold-tree failure the wake is
for, so that is a measurement of the message's own cost and nothing else.

**How long a wake takes end to end**, and therefore what a reader perceives on
that first hover, still needs a genuinely cold Chromium window. The honest way
to get one is a newly launched browser window on a machine where nothing has
read it: not a state this desktop can be argued into.

## What the numbers do not rule out

**A covered window looks like a cold one from the outside**, and the instrument
knows the difference only by whether a point is still that window's to ask
about. The single `WENT COLD` row was re-checkable, so it was asked and it
failed, but a window that moved behind another during four minutes of waiting
is a story this run cannot fully exclude.

**One window per interval is one sample.** The 45s and 120s runs saw three and
one re-checkable windows. Nothing here is a distribution, and the bisection
between 120s and 240s is a single crossing rather than a boundary.

## What follows for the product

The current design sends the wake once per window and reads again. **That is
right**, and this run supports it more than it supports the alternative.

What it also says is that the memo cannot be "once per window" without a time
limit on it. A reader who reads, works elsewhere for five minutes, and comes
back is a first hover again. Whether that shows up as one slow card or as a
failure depends on the 140ms settle being enough, which is exactly the number
this run could not reach.
