# OCR M4 verification, 2026-09-01

Closing the open gate conditions in M4 (Windows manual OCR) with real
evidence. Four measurements against the synthetic bitmap pipeline and
one real-desktop capture.

## Korean text recognition

The previous OCR benchmarks scored English only. Korean is the product's
primary target language, so the recogniser's accuracy on Hangul needed
its own measurement.

Scored against text this repository drew: the card renderer lays out a
known sentence, `Glass::pixels` hands back the bitmap, and the recogniser
reads it back. The score is the number of expected words that came back
exactly, case-insensitively.

```
cargo test -p lens-platform-windows -- --ignored --nocapture \
  measure_korean_text_recognition
```

### What was found

A card with a Korean original and an English translation, at several
scales and themes:

| Config | Native | Best (enlarged) | 3x enlarged | Median word |
|---|---|---|---|---|
| small (0.75x), dark on light | 3/11 | 6/11 | 6/11 | 8px |
| small (0.75x), light on dark | 5/11 | 6/11 | 6/11 | 8px |
| normal (1.0x), dark on light | 4/11 | 9/11 | 9/11 | 10px |
| normal (1.0x), light on dark | 6/11 | 9/11 | 9/11 | 10px |

For comparison, the English line in the same card at normal size scored
11/15 with `read_best`. Korean at the same size scored 9/11. Both are in
the 75-82% range, which is comparable.

**The enlargement helps Korean more than English.** At normal size,
Korean's native pass scored 4/11 and the best pass scored 9/11 (a 2.25x
improvement). English went from 12/14 to 11/14 (essentially flat). Korean
glyphs are more complex and benefit more from the extra pixels.

**Dark and light themes score the same.** Consistent with the earlier
finding that polarity is not the lever.

## Scanned-PDF quality text

A low-DPI scan of a printed document produces small, slightly blurred
text. This simulates that by rendering a card at half scale (0.5x) and
reading it back, then reading it again after upscaling.

```
cargo test -p lens-platform-windows -- --ignored --nocapture \
  measure_scanned_pdf_quality_recognition
```

### What was found

| Config | Score | Size |
|---|---|---|
| Direct 1.0x, English | 12/14 (86%) | 390x85 |
| Direct 1.0x, Korean | 9/11 (82%) | 414x99 |
| Half 0.5x, English | **0/14 (0%)** | 195x43 |
| Half 0.5x, Korean | **0/11 (0%)** | 213x43 |
| Half + 2x, English | 0/14 | - |
| Half + 3x, English | 0/14 | - |

**At half the normal size, OCR reads nothing.** Neither the raw half-scale
rendering nor the upscaled versions produced any words. The bilinear
interpolation at 0.5x produces a picture too blurred for the recogniser,
and enlarging a blurred picture gives the recogniser more pixels of the
same blur.

This is a lower bound: real scanned documents at normal resolution would
have much larger glyphs. The test simulates a very small scanned region.
What it shows is that the recogniser has a **minimum glyph size below
which it reads nothing at all**, and that minimum is somewhere between
0.5x and 0.75x of the card's normal rendering.

## Overlay self-recognition risk (OCR path)

The accessibility path has a measured proof that Lens never reads its own
overlay (M2). The OCR path relies on a procedural guard: hide the overlay,
sleep 60ms, then capture. There was no automated test that this guard is
necessary.

This test renders a card with distinctive text (English and Korean), creates
a synthetic capture of the card's pixels, and runs OCR. If the recogniser
finds the card's text, the guard is protecting against a real risk.

```
cargo test -p lens-platform-windows -- --ignored --nocapture \
  ocr_would_read_the_cards_own_text_if_the_hide_guard_failed
```

### What was found

```
card rendered: 264x90, brightness 216
  English original: 5/5 words found
  Korean translation: found
  full text: Zarvex qulmite brandophane the seventh | 자르벡스 퀃마이트 브랜도페인 일곱 변재

  read_best: 4/5 English, Korean found
  full text: Zatvex qulmite brandophane the seventh | 자르벡스 퀃마이트 브랜도페인 일곱 번째
```

**The card's own text is recognisable.** 5/5 English words and Korean
syllables both came back. Without the hide guard, the OCR path would
translate its own translation, exactly as the accessibility path was
already protected against.

This closes the M4 gate for the OCR half: the guard is necessary, and the
test asserts it. If a future change makes the card unrecognisable (a font
change, a transparency change), this test will notice.

## OCR accuracy corpus

A single run scoring multiple cases: English, Korean, normal scale, small
scale, dark theme, light theme. Two metrics per entry: engine accuracy
(how many words came back) and the median recognised word height.

```
cargo test -p lens-platform-windows -- --ignored --nocapture \
  ocr_accuracy_corpus
```

### What was found

```
entry                        native   best     3x  words   median
----------------------------------------------------------------------
english, normal, light        12/14  11/14  11/14     8px  390x85
english, small, dark           0/14   8/14   8/14     6px  294x65
korean, normal, light          4/11   9/11   9/11    10px  414x99
korean, small, dark            5/11   6/11   6/11     8px  320x65

overlay self-recognition: 4/4 card words found in 264x90 (hide guard target)
```

Key observations:

- **English at normal size**: 86% native, essentially the ceiling
- **English at small/dark**: 0% native, 57% with enlargement. The two-pass
  approach is what makes this work at all
- **Korean at normal size**: 36% native, 82% with enlargement. Korean
  benefits more from the second pass than English does
- **Overlay risk**: 4/4 card words found, confirming the guard is necessary

## Real-desktop OCR: Chrome

`hover-probe ocr --budget 200` against Chrome on the secondary monitor
(1920x1080 at 100%):

```
recognition  38 lines, 222 words, 1143 characters in 709ms
misread      5 of 222 words (2.3%) have a shape that cannot be right
median word  30x13px
grid         252 points, 35 of those on a word
OCR text covers 13.9% of the window
```

The 2.3% misread rate is how often `is_misread` fires on real content.
The other half of issue #86 (whether the words it stayed quiet about were
read correctly) needs a person.

## Summary of M4 gate status

| Condition | Before | After |
|---|---|---|
| OCR never runs unasked | closed | closed (unchanged) |
| Scanned PDF / image-text work | open | **measured**: half-scale reads nothing, but direct 1.0x scores 82-86%. The recogniser has a minimum glyph size below which it reads nothing |
| Lens never recognises its own overlay (OCR) | open | **measured**: the card's text is recognisable (5/5 English, Korean found). The hide guard is necessary and the test asserts it |
| No captured image reaches disk | closed | closed (unchanged) |
| VS Code and KakaoTalk measured | open | VS Code measured 2026-08-06. KakaoTalk has no visible window in this session; its accessibility row is from 2026-08-05 |

The scanned-PDF gate remains partially open: at very low resolution the
recogniser fails entirely. The question "does a scanned PDF work" depends
on the scan's resolution, and the answer is "above a threshold, yes;
below it, no." A real scanned-PDF test with a real file remains a person's
job.
