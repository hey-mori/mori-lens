# A card over a full-screen window, 2026-08-18

M2's gate asks for the overlay to be verified "in a full-screen application".
That row has read **open** since the milestone was written, because checking it
meant a person putting something into full screen and looking at it.

A window that covers the primary monitor is buildable, so `fullscreen_harness`
builds two: one ordinary and one `WS_EX_TOPMOST`, which is the difference
between a maximised application and a video player.

## The result

```
           monitor     full-screen window   overlay   card
ordinary   1920x1200   z 38                 z 13      282x114 at (778, 631)
topmost    1920x1200   z 18                 z 13      282x114 at (778, 631)
```

**The card is in front in both**, and the foreground window is unchanged across
the show, which is asserted rather than eyeballed.

Lower z is nearer the front. The harness is at 38 when ordinary and 18 when
topmost, which is the topmost flag doing exactly what it says, and the overlay
is at 13 either way.

## Z-order, because pixels cannot answer this

`WindowFromPoint` is the obvious instrument and it is the wrong one. The overlay
is `WS_EX_TRANSPARENT` and that call **skips transparent windows by design**, so
it names the window underneath even when the card is drawn perfectly. It would
report a failure that is not one, every time.

Walking the desktop z-order asks the question directly and has no such blind
spot.

## What this measured, and what it did not

**Borderless full screen.** A `WS_POPUP` window covering the monitor, which is
what a browser's F11, a video player and most modern games actually use.

**Not exclusive full screen.** A DirectX flip-model swap chain takes the display
out from under the desktop compositor and nothing another process draws is
guaranteed to appear over it. That case needs a real game and a person, and this
row of the gate should say so rather than be quietly counted as covered.

## The run that lied first

The first version of this reported `monitor 1536x960` and passed. That is a
1920x1200 screen at 125%, seen from a process that had not declared DPI
awareness: Windows virtualises the coordinates and then stretches the window to
cover the monitor anyway, so the window **was** full screen and every number
describing it was wrong.

`window::declare_dpi_awareness` already carries the long version of this,
including the forty-of-forty failure that paid for it, and `ProtectedHarness`
already called it. This harness did not. It does now, and the card's own size
changed with it, from 282x114 physical pixels reported as 224x90.

The assertions all passed in both runs. **A test can be green and measuring the
wrong desktop**, which is worth one paragraph here because the gate it closes is
about believing a number.

## What is not measured

- **One monitor, one scale, one machine.** The harness covers the primary
  monitor only, and the multiple-monitor condition is a separate row that was
  already measured.
- **Exclusive full screen**, as above.
- **Whether a reader can read it.** The card is in front and nobody looked at
  it. The ink check in the welcome window's live test exists because those are
  different facts, and no such check runs here.
- **A full-screen application that is not this repository's own window.** The
  harness is a plausible stand-in and it is a stand-in.
