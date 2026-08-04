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
or on an explicit request ([ADR-0002](../decisions/ADR-0002-accessibility-first-acquisition.md)).

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

Updated 2026-08-04 against the first sweep
([`../benchmarks/2026-08-04-acquisition-sweep.md`](../benchmarks/2026-08-04-acquisition-sweep.md)).

1. **What fraction of hovers over real text return a correct word, per
   application?** · *partly answered.* A machine-checkable proxy exists:
   whether the resolved word's rectangle contains the sample point. On
   `50020` Text controls it is 86 to 100%. On `50030` Document and
   `50004` Edit controls it is 3 to 18%. Three windows measured; the
   remaining five target applications are still open, and no human has
   yet confirmed that a resolved word is the intended one.

2. **Is a distance check required?** · **answered: yes.** Inside a
   document or edit control `RangeFromPoint` never fails. It returns the
   nearest text in the whole scrollable region, up to 1,465px from the
   sample point, and reports success. There is no error to catch, so the
   distance is the only signal. Where the threshold belongs is issue #5.

3. **How often does `ExpandToEnclosingUnit(Word)` normalise to something
   larger, and in which providers?** · *open.* The sweep does not
   compare the requested unit against what came back. It needs the
   returned range length measured against the word it was expanded from.

4. **What does the Chromium wake cost, and does once-per-window hold?**
   · *partly answered.* It helps where the tree is genuinely cold: +2
   acquisitions on one Chrome window, and larger gains on Electron in an
   earlier run. Most windows reported "no change", which is consistent
   with Chromium enabling accessibility **process-wide** rather than per
   window, so waking one window warms its siblings. That would make the
   per-window memo over-eager but harmless. Not yet proven, and the
   140ms settle is still unmeasured on a hover budget. Issue #4.

5. **Do rectangles line up at 125% and on a second monitor?** ·
   *provisionally yes.* Measured across a 125% primary and a 100%
   secondary: on-target rates of 69 to 75% and 86 to 100% on Text
   controls, with a maximum distance of 14px on the 125% display. A DPI
   bug would put rectangles off by a factor of 1.25 and show as
   hundreds of pixels. Not a controlled DPI test, and no negative-origin
   monitor was available.

6. **Element-resolution latency in a heavy Electron application?** ·
   **answered: not a problem.** The whole chain runs at p50 7.9 to
   11.2ms and p95 12.4 to 23.7ms, against a combined budget of 35ms and
   130ms. First-call COM warm-up is the only outlier, at p95 53.8ms.

7. **How often is a stale result produced?** · *open, and the sweep
   cannot answer it.* Staleness is a property of the pointer moving
   during a hover, so it needs the follow instrument during real
   reading.

### A question the sweep added, and answered

8. **Is the low on-target rate inside documents empty space, or
   inaccurate resolution?** · **answered: empty space, and there are two
   populations rather than one.**

   Recording which side of the resolved rectangle each miss fell on
   settles it. Misses are strongly directional (`left 0, above 44` in
   one window; `above 0, left 0, right 10` in another) and the direction
   differs per window. Inaccurate resolution is symmetric noise and
   cannot produce that; a layout can, because the direction is set by
   where the empty region sits relative to the text.

   The same measurement corrected a simpler reading: **misses on Text
   controls are a different phenomenon.** They run to 7px at p95 and
   14px at maximum, one rectangle each, which is glyph-box and
   line-height slack rather than a wrong target.

   So there are two clusters, separated by roughly 14px against 322px:
   slack, which must be tolerated, and empty space, which must be
   suppressed. A threshold of exactly zero would discard about a fifth
   of good hits.

### Still open

9. **Is the once-per-window wake memo correct, or should it be
   once-per-process?** Most windows report "no change" after a wake,
   consistent with Chromium enabling accessibility process-wide. Needs a
   freshly launched browser, swept before anything else touches it.
10. **What does the 140ms wake settle cost on a hover budget** whose
    entire dwell is 320ms? The sweep drives the wake explicitly and so
    cannot answer it.
