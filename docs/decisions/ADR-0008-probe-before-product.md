# ADR-0008 · Measure the acquisition rate before building the product

**Status** · Accepted, 2026-08-04

## Context

Lens's value proposition rests on one unproven assumption: that the
operating system will hand over the right text under the pointer, in
enough applications, fast enough, to make hovering feel reliable.

Everything else is ordinary engineering. Overlays, translation prompts,
caches, and vocabulary lists are all things we know how to build. The
acquisition layer is the only part that could turn out to be impossible,
and it is the only part that cannot be unit tested, because UI
Automation needs a real desktop with a real focused window.

The failure mode this ADR exists to prevent is well known: build the
attractive parts first, discover late that the foundation only works in
Notepad, and be left with an overlay and a model router attached to
nothing.

Mori's measurements make the risk concrete rather than theoretical. Its
selection reader found that Chromium exposes no accessibility tree at
all until it is woken, and that Win32 `EDIT` controls expose no text
pattern ever. Those were found by running a probe, not by reading
documentation, and documentation would have predicted neither.

Hover is also strictly harder than selection. Selection asks
`GetFocusedElement`, one call with an unambiguous answer. Hover asks
`ElementFromPoint` and then `RangeFromPoint`, which must resolve the
element under an arbitrary coordinate, expand a range to a word, find the
enclosing sentence, and return screen rectangles that match what the user
sees, at every DPI scale and on every monitor. None of that carries over
from Mori for free.

## Decision

**Milestone 1 ships a measurement instrument, not a feature. No
translation, no overlay, no model call is written until the probe has
recorded a per-application result for a named set of applications.**

The probe:

- follows the pointer and resolves the target under it
- reports how far the acquisition chain got, as a typed reason:
  `unsupported`, `empty`, `timeout`, `stale`, `protected`,
  `element-gone`
- reports character counts, screen rectangles, and per-stage latency
- **prints no content by default**, only its shape. A diagnostic that
  echoed whatever the user had on screen would be a worse privacy
  problem than the feature it is checking. Mori's `CaptureProbe` holds
  the same line and it is the right one.

  One opt-in flag, `--show-word`, prints the resolved word and nothing
  wider, because the matrix needs a correct-word judgement that a human
  cannot make blind. It announces itself while it is on, and
  `--expect <word>` gets the same judgement without printing anything
  (`../privacy-model.md` §3).

The gate to Milestone 2 is a filled-in
[`../compatibility/windows-app-matrix.md`](../compatibility/windows-app-matrix.md),
not an opinion that the probe seems to work.

## Consequences

- The repository shows nothing a user could use for the length of this
  milestone. That is the intended trade and the README says so plainly.
- The probe is not throwaway. It becomes the regression harness for the
  adapter: every later acquisition change is re-measured with it, which
  is the only way an adapter change can be verified at all.
- The per-application matrix becomes the product's honest coverage
  claim, and the source of the `type:compatibility` issue label.
- If the matrix comes back poor, the correct response is to change the
  product (modifier-triggered rather than automatic, or OCR-forward for
  named applications) while the cost of changing it is still one crate.
  Finding that out after the overlay and the model router exist would
  cost the whole design.
