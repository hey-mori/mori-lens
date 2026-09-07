# ADR-0004 · The overlay is a native window, not a webview

**Status** · Accepted, 2026-08-04

Not yet built. Recorded now because it constrains the crate layout and
the choice of UI toolkit, both of which are decided before the overlay
is written.

## Context

Mori solved a similar problem with a second Tauri webview window marked
always-on-top, and it works well enough for a chord-triggered panel that
appears a few times an hour.

Hover changes the requirements. The card appears and disappears
continuously while the user reads, follows the pointer across monitors
with different scale factors, sits over full-screen applications, and
must never take focus from the application being read. A webview costs
initialisation on the first show, renders through a compositor path with
its own latency, and does not expose the flags that make focus and
hit-testing behave.

Two of these are hard blockers rather than performance concerns:

- **Focus.** If the card activates its owning process, the user's
  caret leaves the document they were reading. On Windows that needs
  `WS_EX_NOACTIVATE` and `SetWindowPos(..., SWP_NOACTIVATE)`; on macOS a
  `NSPanel` with `.nonactivatingPanel`.
- **Hit-testing.** A passive card must let clicks through to the
  application underneath, and an interactive card must not. One window
  cannot be both: a button inside a click-through window cannot be
  clicked.

## Decision

**The hover overlay is a native window on each platform, and there are
two of them.**

- **Passive tooltip** · never activates, ignores mouse events, shows the
  quick translation, appears and disappears fast.
- **Interactive card** · a real window the user pins or opens
  deliberately, with buttons, selectable text, and keyboard access.

Windows uses Win32 with `WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW`, and
`WS_EX_LAYERED`. macOS uses an AppKit `NSPanel` with
`.nonactivatingPanel` plus `.canJoinAllSpaces` and `.fullScreenAuxiliary`.

A webview is still allowed, and expected, for **settings, history, and
onboarding**, which are ordinary windows the user opens on purpose.
Tauri v2 is the likely choice there. It is not allowed for the hover
overlay.

Placement is a pure function of the source rectangle, the pointer, the
card size, and the monitor work area, living in `lens-core` so it is unit
tested for edge flipping, negative-origin monitors, and a card larger
than its screen. Mori's `language/overlay.rs` does the same and it is the
part of that feature that never needed debugging on a live desktop.

## Consequences

- Two rendering paths to build and style: native for the hover surface,
  webview for the settings surface. Keeping them visually consistent is
  real work and is accepted.
- The overlay cannot be styled with CSS, so its visual language must be
  simple enough to express natively. This is a constraint on the design,
  applied before the design exists rather than after.
- Placement logic is testable without a desktop, which removes the one
  part of the overlay that would otherwise only be verifiable by eye.
- If the native overlay proves unworkable on macOS, the fallback is a
  non-activating `NSPanel` hosting a webview, which keeps focus
  behaviour while giving up show latency. That would be a superseding
  ADR, not a quiet change.

---

*Implementation status (updated 2026-09-07):* Implemented on both
platforms. Windows uses Direct2D/DirectWrite in a native layered window.
macOS uses an `NSPanel` (transparent, click-through, ARC-clean). The
macOS fallback described above became the primary path, not a fallback.
