# ADR-0010 · Acquisition is proven enough to build on

**Status** · Accepted, 2026-08-04
**Supersedes** the Milestone 2 gate in
[ADR-0008](ADR-0008-probe-before-product.md). The rest of ADR-0008,
that a claim needs a measurement, still stands.

## Context

ADR-0008 said no overlay until the compatibility matrix is filled in for
eight applications. The reasoning was sound: build the risky part first,
and do not decorate an acquisition path that does not work.

The acquisition path works, and that has been true since the first
sweep:

- **p50 10ms, p95 20ms** for the whole chain, against a budget of 35ms
  and 130ms.
- **86 to 100% on-target** on label controls, across Chromium, Electron,
  and Win32.
- The one dangerous failure, a resolution far from the pointer inside a
  document, is measured, understood, and gated
  ([ADR-0009](ADR-0009-pointer-distance-gate.md)).
- Protected fields are refused, executed against a real control.

Five windows across four applications produced that. The remaining four
applications would add rows; none of them would change a decision. The
gate has stopped being a check on risk and become a checklist.

The cost of holding it is not neutral. This repository has an
instrument, a benchmark suite, a decision record set, and no product.
Nobody has ever rested a pointer on a word and seen a translation. Every
property the product is supposed to have, that the card does not steal
focus, that a hover feels instant, that a suppressed hover is not
mistaken for a broken tool, is **unmeasurable until the card exists**.

The measurements that remain undone need an overlay more than the
overlay needs them.

## Decision

**Milestone 2 starts now. Filling in the remaining compatibility rows no
longer gates it.**

What still holds from ADR-0008, unchanged:

- A per-application claim still requires a probe or sweep run recorded
  in the matrix. Nothing may be reported as supported from expectation.
- A change to a platform adapter is still not verified by `cargo test`.
- Failure reasons stay distinguished rather than collapsed.

What changes:

- The matrix is filled in **as applications come up**, not as a
  prerequisite. It is a living record rather than a checklist to clear.
- The next milestone's target is the first end-to-end slice: pointer
  settles, text is acquired, a local model translates it, a native card
  appears without taking focus.

## Consequences

- The overlay will be built against four measured applications rather
  than eight. If a fifth behaves differently in a way that changes the
  overlay's design, that is rework, and it is cheaper than the delay.
- The metrics in
  [`../mori-lens-project-goal.md`](../mori-lens-project-goal.md) §6 that
  have never had a value, unwanted-card rate and missed rate, become
  measurable for the first time.
- **The risk this accepts explicitly:** ADR-0008 existed to stop exactly
  this move, building the visible part before the hard part is proven.
  The defence is that the hard part *was* proven, and the evidence is
  numbered above rather than asserted. If the overlay turns out to rest
  on acquisition behaviour that four applications did not show, this ADR
  was wrong and the matrix should have been finished first.
- VS Code's editor exposes nothing to read (#14). That is unresolved and
  is not a reason to delay: it is a question about reach, answered by
  OCR in M5, and no amount of finishing the matrix answers it either.
