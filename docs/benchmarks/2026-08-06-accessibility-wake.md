# The Chromium accessibility wake, 2026-08-06

Issue #4. Chromium and Electron build no accessibility tree until an
assistive client asks. Lens asks with `SendMessageTimeoutW(WM_GETOBJECT,
OBJID_CLIENT)` and then sleeps 140ms before reading again, a value carried
over from Mori without being measured here.

The number the milestone turns on is not the cost of one wake. It is **how
often the wake recurs**. Paid once per window, 340ms is a first hover that
is slightly slow. Paid per hover, it is more than the entire 320ms dwell
budget, and the browser is where most people meet a second language.

## Environment

Windows 11 Home 10.0.26200, single display at 125%, build `1ce87fe`.
Chrome, an Electron application, Windows Terminal, Explorer, Settings and
SourceTree open, in ordinary use. Instrument:

```
hover-probe wake --quiet 180
```

## What was found

```
process                 message     settle       cold  state
chrome                        -          -         ok  was awake, still awake
WindowsTerminal               -          -         ok  was awake, still awake
claude                        -          -         ok  was awake, still awake
chrome                        -          -         ok  was awake, still awake
chrome                        -          -         ok  was awake, still awake
chrome                        -          -         ok  was awake, still awake
explorer                  0.1ms          - unsupported  the wake did not repair it

  0 window(s) needed a wake, 6 were already awake
  after 180s of no assistive traffic, 0 of 6 re-checkable windows went cold
```

**The wake message costs about 0.1ms.** Measured at 0.0, 0.1, 0.2 and 0.5ms
across four runs. `SendMessageTimeoutW` with `SMTO_ABORTIFHUNG` returns
almost immediately; the 340ms figure in the issue is the message plus the
140ms this crate sleeps afterwards, and the sleep is essentially all of it.

**Nothing needed waking.** Six windows, including four Chrome windows and an
Electron application, answered a cold read with no wake message sent at all.
On this desktop the wake is not being paid per hover, or per window, or at
all.

**Nothing went cold.** Three minutes of no assistive traffic, then every one
of the six was asked again without a wake, and every one answered.

The `explorer` row is not a counter-example. Its window holds no text at
that point, so `Unsupported` is the correct answer and a wake cannot repair
what is not there. It is in the table because the instrument reports what it
tried rather than what it liked.

## What this does not establish

**The settle is still unmeasured, and 140ms is still unvalidated.** The
instrument reads every 10ms after a wake and records when the first read
succeeds, which would answer whether 140 is right, too much, or unnecessary.
It never got to: there was no cold window on this desktop to wake. Measuring
it needs a Chromium window started fresh, with the probe running before
anything else has read it.

Until then the 140ms sleep is a cost paid on the first hover into any
window that returns `Unsupported` or `Empty`, whether or not that window is
Chromium and whether or not the wake would have helped. That is the shape of
the remaining risk, and it is smaller than the issue feared: it is once per
window rather than once per hover.

## Two readings that were the instrument, not the desktop

Both are recorded because a measurement tool is part of what is being
measured, and both said "WENT COLD" convincingly.

**Re-reading one point.** The first version asked the same coordinate again
after the quiet interval. Windows Terminal reported cold, and what had
changed was its contents: the text that had been at that pixel had scrolled
away. A tree is present if **anything** in the window reads, so the re-check
now re-scans the window.

**Counting an occluded window as cold.** The re-scan skips points that
another window is on top of, which is right. With every point skipped it
found nothing and called that cold. A Chrome window reported as having lost
its tree had simply been covered. The scan now counts how many points were
this window's to ask about, and a window with none is reported as not
re-checkable rather than as cold.

With both corrected, the figure is 0 of 6. With either uncorrected it was 1
or 2 of 6, which would have been published as evidence that the tree decays.

## What it means for the product

The per-window memo in `uia.rs` is confirmed on this evidence: the wake is
sent once per window and the tree persists for at least three minutes of
silence. Hovering in a browser does not pay 340ms repeatedly.

No change is proposed to `WAKE_SETTLE_MS`. Changing a constant on a run
that never exercised it would be worse than leaving it: the value would then
look measured.
