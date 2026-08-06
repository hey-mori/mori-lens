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

## Contrast is not the lever either, and nor is more enlargement

The dark-versus-light run above compared the card's two themes, which
differ in polarity and in their actual colours. Polarity was ruled out.
Colour was not, and a reader's problem is a code editor's dark theme rather
than this card's, so a comparison of two of our themes could not speak to it.

This fixes the paper at `#1e1e1e`, which is VS Code's default dark
background, fixes the size at 8pt, and varies one thing: how far the ink is
from the paper. Every cell is the number of seven English words that came
back exactly.

```
paper #1e1e1e, text at 8pt

  ink        contrast    3x    4x    5x    6x
  #3a3a3a      1.5:1     0     0     0     0
  #555555      2.2:1     0     0     0     0
  #707070      3.4:1     3     2     1     2
  #8b8b8b      4.9:1     3     5     3     4
  #a6a6a6      6.8:1     3     4     3     4
  #d4d4d4     11.2:1     5     4     4     5
  #ffffff     16.7:1     3     2     4     4
```

Three things fall out of it.

**Below about 3:1 there is a cliff.** Nothing is read at all, at any
enlargement. That is the one place contrast decides the outcome.

**Above it, contrast does nothing.** Pure white on near-black, at 16.7:1, is
no better than `#8b8b8b` at 4.9:1, and worse than `#d4d4d4` at 11.2:1. The
numbers are not noise: the recogniser returns identical output for identical
input, which the inversion run proved. They are the rasterisation of
particular glyphs at a particular size, and they do not lie on a curve.

**Nothing past 3x helps.** Four, five and six times give the same three to
five words. The cap in `read_best` is where the improvement stopped, and now
there is a table saying so rather than a sentence.

VS Code's own comment colour is `#d4d4d4` on `#1e1e1e`, the 11.2:1 row, at
four or five words of seven. That is exactly what the reader photographed:
mostly right, one or two words mangled, translated with a straight face.

**So preprocessing is spent.** There is no picture Lens can hand this
recogniser that reads a dark 8pt editor reliably.

## Which leaves detection

`ocr::is_misread`. Not confidence, which `Windows.Media.Ocr` does not
provide, but shape: a Hangul syllable with a letter or a digit on both sides
of it, which is what `do은5`, `k국05t` and `de님op디5` all are and what real
text is not.

A suffix does not count, and that distinction is the rule. Korean technical
writing attaches particles to English identifiers constantly, `export_csv를`,
`orbbase에`, `k8s를`, and flagging those would fire on most of what this
product's first reader writes.

It is a floor rather than a ceiling, and the gaps are asserted in their own
test so nobody reads the passing ones as completeness:

- `님hOle` for `whole`, where the Hangul is first with nothing before it
- `5h0l` for `show`, digits standing in for letters with no Hangul at all,
  which needs a rule about digits inside Latin words, and `k8s`, `qwen2.5`
  and `sha256` are all that shape

What the product does with it: the card's footer says 읽기 불확실 beside the
model and the time. Not a refusal. A doubtful reading is still the best
reading available and refusing to translate it leaves somebody who pressed a
key with nothing; what they must not have is a confident translation of
nonsense with nothing to say so. And it goes on the card they are still
looking at a second later, rather than on one shown first and covered by the
answer.

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
