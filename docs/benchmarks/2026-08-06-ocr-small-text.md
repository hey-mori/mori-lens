# OCR on small text, 2026-08-06

A reader photographed cards reading `do은5` where the screen said `does`,
`5h0l word_banner` where it said `show_word`, and
`50 de˚님op디5 know what•s missing` where it said
`so developers know what's missing`. Lens translated all three and showed
the result with nothing to suggest anything had gone wrong.

`Windows.Media.Ocr` is trained on text at document sizes. A code editor is
well below them: the OCR probe measuring VS Code returned a **median word
of 28x10px**.

## Method

Scored against text this repository drew, which is the one way to score a
recogniser without printing somebody's screen. The card renderer lays out a
known sentence at a known point size, `Glass::pixels` hands back the bitmap,
and the recogniser reads it back. What it should say is known before it is
asked, so the score is the number of the seven English words that came back
exactly.

```
cargo test -p lens-platform-windows -- --ignored --nocapture \
  enlarging_a_capture
```

## What was found

A card drawn at 8pt, 231x61 physical pixels, median recognised word 9px:

| Enlargement | Words exact | Time | What it read |
|---|---|---|---|
| 1x | **0 of 7** | 121ms | the English line was lost entirely |
| 2x | 1 of 7 | 154ms | `rhe test c:orr•ctfy r•fuses t0` |
| 3x | **6 of 7** | 155ms | `The test correctly refuses to pass` |

The same card at 306x80, median word 8px:

| Enlargement | Words exact | Time |
|---|---|---|
| 1x | 5 of 7 | 144ms |
| 2x | 5 of 7 | 186ms |
| 3x | **6 of 7** | 161ms |

**Nothing changed between those runs except pixels per glyph.** Same
sentence, same typeface, same recogniser, same machine.

The 2x row on the first card is worth keeping rather than smoothing away:
one word exact, and the *shape* of the failure changed from losing the line
to misreading it. Enlarging is not a switch.

## What ships

`ocr::read_best`. One pass to find out how big the glyphs are, which is the
question the caller cannot answer, and a second enlarged pass only when the
median recognised word is under 16px. The factor is chosen to bring a glyph
to about 30px and capped at 3x, past which the picture is mostly
interpolation and a recogniser given that reads the interpolation.

Bilinear rather than nearest. Nearest gives the recogniser more pixels of
the same staircase; bilinear gives it back the smooth edges the glyphs were
antialiased into before the screen quantised them, which is what the
classifier is looking for.

Measured end to end, both passes together: **374ms** and **494ms** for the
two cards above, against 121ms and 144ms for a single native pass. That is
the price, and it is paid only on small text, on a path a user reaches by
pressing a key and already waits over a second for.

## Dark backgrounds, and a hypothesis that was wrong

Every card a reader photographed with garbage in it was **light text on a
dark background**, in a code editor. `Windows.Media.Ocr` is trained on
documents, which are the other way round, so inverting a dark capture
before recognising it looked like the obvious next improvement.

The effect is real. The same sentence, the same 8pt, the same 3x
enlargement:

| Card | Words exact at 3x |
|---|---|
| dark text on light | **6 of 7** |
| light text on dark | **3 of 7** |

The fix does nothing. Inverting the dark capture returns the **identical
string**, word for word:

```
3x           3/7   The test cnrrectly refi」ses tD pass
3x inverted  3/7   The test cnrrectly refi」ses tD pass
  inverted brightness 215 from 38
```

That last line is there because the identical output looked like a broken
measurement first, and this repository has published broken measurements
before. The inversion happened: mean luminance went from 38 to 215. The
recogniser normalises polarity away before its classifier sees anything.

So `Capture::inverted` is kept, used by this benchmark, and not used by the
product. A negative result that cannot be re-run is a claim rather than a
measurement.

**What does cost the dark card three words is still unknown.** Contrast and
antialiasing are the obvious candidates and neither has been measured. One
run per cell, so the gap itself deserves repeating before it is built on.

## What this does not fix

**Lens still cannot tell a good reading from a bad one.**
`Windows.Media.Ocr` exposes no per-word confidence, so `5h0l word_banner`
and `show_word` arrive as equally plausible strings. Enlarging makes the bad
reading rarer; it does not make it detectable.

That matters because the failure is silent and confident. A card that says
`판단이 아무것도 인쇄할 필요가 없다` for a comment about printing is worse
than a card that says nothing, and nothing in this path currently notices.

A heuristic exists and is not built: a recognised "word" mixing scripts, like
`do은5` or `k국05t`, is a misreading in almost every case, because real text
does not switch alphabets mid-word. Left out on purpose until it is measured
against text that legitimately mixes them, which Korean technical prose does
constantly.
