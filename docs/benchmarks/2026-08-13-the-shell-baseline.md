# The floor a main window has to beat, 2026-08-13

[#164](https://github.com/hey-mori/mori-lens/issues/164) is a staged spike, and
its first stage is the baseline: **what a plain native window costs inside the
process Lens already has.** Every other option is compared against this and not
against zero.

Build `67d2bd8`, release, on the machine this repository is written on.

## What the product costs today

```
binary                             1,212,416 bytes   (1.16 MB)
idle RSS, tray only                53.1 MB, 54.6 MB  (two runs)
idle CPU over ~22s, tray only      0.8s, 0.9s
```

That binary is the whole product. `README.md` used to call it an absolute and
[ADR-0012](../decisions/ADR-0012-three-product-surfaces.md)'s successor replaced
that with a narrower promise, but the number is still what a reader downloads
today and what any packaging change is measured against.

## What a native window costs

```
idle RSS, first run with the welcome window open      53.0 MB
idle CPU over the same window                         0.7s
```

**Nothing measurable.** The window here is
`crates/platform-windows/src/welcome.rs`, which is a real Win32 window on its
own thread with a message pump, created inside the process that already owns a
message loop, a tray icon, global hotkeys, a layered overlay and a UI Automation
apartment. It is exactly the shape stage one is about.

**The spread between two identical tray-only runs is 1.5 MB. The difference
between tray-only and window-open is 0.1 MB.** Run-to-run variance is more than
ten times the effect, so the honest statement is not "a window costs 0.1 MB", it
is:

> A plain native window in this process is free within the noise this machine
> produces.

## The first version of this measurement was wrong

It reported 56.6 MB against 57.4 MB and looked like a real 0.8 MB cost.

**Both runs were first runs.** The "no window" arm pointed at a settings file
that did not exist, so the product wrote the template, treated it as a first
launch, and staged the welcome. Two windows were compared and one of them was
labelled as none.

The fix was to run again once the file existed. Recorded because the shape is
familiar from this folder: the instrument agreed with the expectation until it
was asked a sharper question, and the expectation was mine.

## What this does not measure

**Tauri.** Nothing here says what a webview costs, whether it coexists with the
message loop, or what it does to the binary. That is stage two, and this number
exists so that stage two has something to be compared against.

**A window that is opened and closed repeatedly**, which is where destroying the
window rather than hiding it matters. The welcome opens once and stays.

**Anything under load.** These are idle samples on a desk with a browser and an
editor open. The hover path was not exercised during them.

## What it means for the spike

The floor is **53 to 55 MB and 1.16 MB on disk**, and the native option adds
nothing to it.

That makes the comparison sharper rather than softer: a webview does not have to
be free, it has to be worth what it costs, and now there is a number to put on
the other side of that sentence.
