# ADR-0002 · Accessibility first, OCR as fallback

**Status** · Accepted, 2026-08-04

This is the load-bearing decision of the product. Every other one
either follows from it or is constrained by it.

## Context

There are two ways to learn what text sits under the pointer.

**Capture pixels and run OCR.** This is what most screen translators
do. It works on anything that renders, including games, video, and
scanned documents. It costs a screen capture and a recognition pass per
look, needs the screen-recording permission from first launch, and
returns *recognised* text rather than the real characters, so word
boundaries, casing, and punctuation are all approximations.

**Ask the operating system.** Windows UI Automation exposes a text
control's content as a stream, and `RangeFromPoint` returns the range
nearest a screen coordinate. macOS Accessibility answers
`kAXRangeForPositionParameterizedAttribute` for a point and
`kAXStringForRangeParameterizedAttribute` for the range. Both return the
actual characters and the actual bounding rectangles.

The failure modes are the interesting part. Asking the OS fails on a
predictable, enumerable set of surfaces: games, video, images, scanned
PDFs, canvas web applications, remote desktop, and a handful of
frameworks that expose no text pattern. OCR fails unpredictably, on
small glyphs and low contrast, and it fails *quietly*, returning
plausible wrong words.

There is also a recursion problem unique to OCR: Lens draws its own
overlay on the screen it is reading, so a naive capture reads its own
translation back and translates the translation.

Mori has already measured the Windows half of this on a live desktop
(see [`../research/inherited-from-mori.md`](../research/inherited-from-mori.md)):
WPF controls answer with full text, Chromium and Electron answer with
nothing until their accessibility tree is woken, and Win32 `EDIT`
controls expose no text pattern at all. So the accessibility path is
neither universally reliable nor a coin flip. It is a known set of cases
with a known repair for the largest one.

## Decision

**The accessibility API is the primary text source. OCR runs only when
the accessibility path has failed for this target, or when the user
explicitly asks for it.**

Concretely:

- The acquisition router tries the platform adapter first, on every
  hover, always.
- OCR is attempted only after a typed accessibility failure that OCR
  could plausibly repair (`NoTextPattern`, `Empty`, `ElementGone`), and
  never after `Protected` or `Timeout`.
- There is no automatic full-screen OCR, no capture on a timer, and no
  frame stream. A capture is a small crop around the pointer, taken once.
- In the first shipping version OCR is **manual only**, behind a
  modifier or a shortcut, so the screen-recording permission is never
  requested until the user reaches for the feature.
- The Lens overlay is excluded from any capture, by filter and by hiding
  the passive layer for the capture frame.

## Consequences

- Lens does nothing at all in a game, over a video, or on a scanned
  page until the user asks for OCR. That is a real capability gap and it
  is stated in the README rather than hidden behind a spinner.
- Per-application behaviour becomes a first-class artifact. Coverage is
  not a percentage we assert, it is a matrix we fill in by running
  ([`../compatibility/windows-app-matrix.md`](../compatibility/windows-app-matrix.md)).
  This is why `type:compatibility` is an issue label distinct from
  `type:bug`.
- Idle cost is close to zero, because nothing is captured or recognised
  until a pointer settles on text.
- The permission ask is staged: accessibility at first run, screen
  recording only on first OCR use.
- The adapter is where the difficulty lives, and it cannot be unit
  tested. That is the price, and it is why
  [ADR-0008](0008-probe-before-product.md) puts a measurement instrument
  before the product.
