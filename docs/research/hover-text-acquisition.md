# Hover text acquisition · research brief

Status · research baseline for M1, 2026-08-04

What has to be true for a pointer to become a translated sentence, what
the APIs actually offer, and what is still unknown. Claims sourced from
vendor documentation are marked as documentation; claims sourced from a
live run are marked as measured. Today almost everything here is
documentation, and Milestone 1 exists to convert it.

## 1. The chain, on Windows

```
GetCursorPos
  → IUIAutomation::ElementFromPoint          element under the coordinate
  → element supports UIA_TextPatternId ?     no  → Unsupported
  → IUIAutomationTextPattern::RangeFromPoint nearest text range
  → ExpandToEnclosingUnit(Word)              the word
  → ExpandToEnclosingUnit(Line | Paragraph)  the context to segment
  → GetText(cap)                             the characters
  → GetBoundingRectangles()                  where they are on screen
```

Documented behaviour worth designing around:

- `ElementFromPoint` returns the element at a *desktop* coordinate. The
  target can vanish mid-call, so `UIA_E_ELEMENTNOTAVAILABLE` is a normal
  outcome, not an exception path. It maps to `ElementGone`.
- `RangeFromPoint` returns a degenerate range at the text *nearest* the
  point. The pointer does not have to be exactly on a glyph, which is
  what makes hovering forgiving. It also means a point in the margin
  resolves to the nearest line, so "nearest" needs a distance sanity
  check against the returned rectangles or the card will explain a
  sentence the user was not looking at.
- `ExpandToEnclosingUnit` may normalise to a **larger** unit when the
  requested one is unsupported. The result must be measured, never
  assumed to match the request. This is the single most likely source of
  a wrong word.
- `GetText` takes a maximum length. Applying the cap at the provider
  means an accidental whole-document range never allocates the document.
- `GetBoundingRectangles` returns one rectangle per line, in physical
  screen pixels.

`IsPassword` is checked on the element **before** any text is read
([`../privacy-model.md`](../privacy-model.md)).

## 2. The chain, on macOS (M6, documentation only)

```
AXUIElementCopyElementAtPosition
  → kAXRangeForPositionParameterizedAttribute    range at the point
  → kAXStringForRangeParameterizedAttribute      the characters
  → kAXBoundsForRangeParameterizedAttribute      screen rectangles
```

`kAXRangeForPosition` returns a *composed character* range, so surrogate
pairs and combining sequences come back whole. Accessibility permission
is required and is checked with `AXIsProcessTrustedWithOptions`. Calls
into an unresponsive application can hang, so they need a timeout, a
serial queue, and a per-process backoff; the pointer thread must never
be the caller.

## 3. Hover intent

The wrong design calls the accessibility API on every pointer movement.
The right one decides that the user is *reading* first, and only then
spends anything.

```
IDLE
  ↓ pointer enters a plausible text region
CANDIDATE
  ↓ dwell threshold passed, movement stayed inside the radius
RESOLVING          ← the first expensive call happens here
  ↓ text acquired
READY
  ↓ pointer leaves → IDLE     ·     pinned → PINNED
```

Starting parameters, to be tuned against measurement rather than
defended:

```
pointer sampling         16 to 33ms
dwell threshold          320ms
movement radius          6 logical px
hide grace               250ms
same-target cooldown     1,000ms
max quick source         500 characters
max paragraph source     2,000 characters
```

Cancelling signals, any of which abandons a candidate: a button held, a
drag, a scroll, typing, the pointer over Lens's own overlay, a denied
application, a protected field, or the same target having just been
answered.

`GetCursorPos` polling at 30 to 60Hz is preferred to a `WH_MOUSE_LL`
hook for the first implementation. The hook receives events sooner but
must return almost immediately and needs a message loop, and neither
buys anything when the design deliberately waits 320ms before acting.

## 4. Word, sentence, paragraph

Three ranges are needed at once, because the word is what the user
pointed at and the sentence is what makes the word translatable. Sending
a bare word to a model produces confident nonsense on any polysemous
term: `gift` is a present in English and a poison in German, and
`produce` in "the test suite didn't produce anything" is not the verb a
dictionary leads with.

Boundaries are not whitespace. Chinese, Japanese, Thai, Lao, Khmer, and
Burmese need dictionary-based segmentation, and abbreviations break
naive sentence splitting in Latin scripts. ICU's `BreakIterator` is the
correct long-term answer and its own documentation is explicit that
sentence boundaries are still approximate around abbreviations.

**M1 decision, deliberately small.** The probe ships a hand-written
segmenter over `unicode-segmentation` for words, plus terminator-based
sentence splitting with an abbreviation guard. It is honest about being
approximate, it has no build cost, and it is enough to measure whether
*acquisition* works. Replacing it with ICU is an M3 decision, taken when
translation quality is what is being measured rather than text
extraction. Recording it here prevents the shortcut from being mistaken
later for a considered choice about segmentation quality.

## 5. Language detection

One document is not one language:

```
이 API는 RangeFromPoint를 이용해 nearest text range를 가져온다.
```

Korean, English, and a code identifier in one sentence. So detection
produces *spans*, not a single verdict:

1. Unicode script detection first. It is nearly free and settles Hangul,
   Kana, Han, Cyrillic, Arabic, Hebrew, Devanagari immediately.
2. A statistical classifier only for what script cannot separate, which
   is mostly the Latin-script languages.
3. Never decide a language from a single short word without its
   sentence. Use the surrounding sentence and the application's recent
   language as priors.
4. Code identifiers, function names, paths, and URLs are protected from
   translation by substitution before the call and restoration after.

fastText's language identification model covers 176 languages and ships
in a roughly 917KB compressed form. It is a candidate, and its licence
and its short-string accuracy both need checking before adoption. Not an
M1 concern.

## 6. OCR, and why it is second

Only reached after a typed accessibility failure that OCR could repair,
or on an explicit request ([ADR-0002](../adr/0002-accessibility-first-acquisition.md)).

Capture is a crop around the pointer, starting near 500x260 logical
pixels and capped near 900x500, never a full screen and never a
continuous stream. Windows offers Windows Graphics Capture plus either
the Windows App SDK text recognition (which has an NPU-dependent path,
so it cannot be the only backend) or `Windows.Media.Ocr` (which has
packaging requirements to verify). macOS offers ScreenCaptureKit plus
Vision. Both sit behind one `OcrBackend` trait so the choice can be made
per machine at runtime.

The recursion hazard is specific and has to be handled by construction:
Lens draws on the screen it reads. The overlay is excluded from the
capture filter, the passive layer is hidden for the capture frame, and
the capture is delayed if the crop overlaps the overlay's bounds.

## 7. DPI and monitors

Per-monitor DPI aware v2 from process start. The cases that break
naive code, all of which must be exercised in M1:

- 100, 125, 150, and 200 percent scaling
- different scale factors on different monitors
- **negative coordinates**, which occur whenever a monitor sits left of
  or above the primary
- a window dragged between monitors mid-hover
- full-screen applications
- remote desktop

The rule that avoids most of it: keep one coordinate space internally,
physical screen pixels, and convert only at the boundary that needs
logical units.

## 8. Latency budget

Targets for the whole product, not measurements. Each has to be
re-measured per machine.

| Stage | P50 | P95 |
|-------|----:|----:|
| pointer sample handling | 1ms | 3ms |
| element resolution | 15ms | 50ms |
| text and bounds extraction | 20ms | 80ms |
| segmentation and language | 10ms | 30ms |
| memory cache hit shown | 30ms | 80ms |
| warm local translation | 500ms | 1,500ms |
| OCR extraction | 300ms | 1,200ms |
| deep analysis | 1.5s | 5s |

Idle: no capture, no OCR, no model request, and pointer sampling as the
only cost.

Mori's measured floor of a 946ms first token
([`inherited-from-mori.md`](inherited-from-mori.md) §8) means the card
must draw its frame and its source text before the translation arrives.
A card that waits for the model reads as a 1.4 second freeze.

## 9. Cache

Key:

```
sha256(
  normalized_source_text · source_lang · target_lang ·
  focused_span · profile · model_name · model_digest · prompt_version
)
```

The model digest and the prompt version are in the key so that changing
either invalidates naturally instead of serving an answer produced by
something that no longer exists.

Two tiers: an in-memory LRU for the session, and an opt-in SQLite tier
that survives restart. Never cached: screen captures, OCR images,
protected fields, denied applications.

## 10. Open questions M1 must answer

1. What fraction of hovers over real text return a correct word, per
   application?
2. Does `RangeFromPoint` resolve accurately enough near line and word
   edges, or is a distance check against the bounding rectangles
   required?
3. How often does `ExpandToEnclosingUnit(Word)` normalise to something
   larger, and in which providers?
4. What does the Chromium accessibility wake actually cost on a hover
   path, and does a per-window once-only wake hold for the lifetime of
   the window?
5. Do bounding rectangles line up with what the user sees at 125 and 150
   percent scaling, and on a negative-origin monitor?
6. What is the real element-resolution latency in a heavy Electron
   application under load?
7. How often is a stale result produced, meaning the pointer moved
   before the answer arrived?

Every one of these is a row the probe fills in. None of them can be
answered by reading more documentation.
