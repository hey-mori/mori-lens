# Aligning a translation with its source, 2026-08-11

The idea: colour the subject in the original and colour its translation the
same, so a reader sees **which part is which** instead of two blocks of text
they have to align in their head. Then the object in another colour, and so on.

It is the best product idea this project has had since the hover itself, and it
rests on one thing that can be measured before anything is designed: **can the
local model return spans that actually occur in both strings?**

A span that is paraphrased, invented, or empty colours nothing.

Model `qwen2.5:7b`, five sentences this repository wrote, `temperature 0`, a
JSON schema constraining `{role, source, target}`, and the prompt asking for
verbatim copies. Membership was then checked **in code**, which is the only
part of this that can be trusted.

## The numbers

```
pairs            20
source verbatim  20 (100%)
target verbatim  16 (80%)
both, so usable  16 (80%)
whole sentences  2 of 5

align latency    3.4s, 4.3s, 4.8s, 4.7s, 4.5s  median 4.5s
```

**100% on the source side.** Asked to copy a constituent out of an English
sentence, the model copies it exactly, every time. That is the mechanical
requirement and it is not in doubt.

**80% on the target side**, and the failures are the interesting part:

- `'came back' -> ''`, an empty span, twice.
- `'on the wrong sentence' -> '틀린 문장에 넣는 것'`, which is a paraphrase and
  appears nowhere in the translation.
- `'here' -> '이里'`, a Chinese character in the middle of Korean.

**2 sentences of 5 came back wholly usable**, and that is the number the
product lives by. One bad span is a card that colours a word wrongly, and a
card on the wrong word is the thing this repository tests its translations
against.

### The empty span nearly published a wrong figure

The first run of this reported **90%**. `"" in anything` is `True` in Python, so
every empty target counted as a match. Recorded because the correction is the
same shape as three others in this folder: the instrument agreed with the hope
until it was asked a sharper question.

## The failure the numbers do not show

Even among the usable pairs, **the roles are often wrong**:

```
2. Rest the pointer on this line and wait without clicking.
   subject   'Rest the pointer' -> '마우스'
   object    'on this line'     -> '이 줄 위에'

4. A card on the wrong sentence is worse than no card.
   verb      'on the wrong sentence' -> ...
   subject   'is'                    -> '는'
```

Sentence 2 is an imperative and has no subject; the model labelled the verb
phrase as one and then mapped it onto a noun. Sentence 4 called a prepositional
phrase a verb and the copula a subject.

Every one of those pairs passes the substring check. **A card built on them
would be verifiably well-formed and confidently wrong**, which is worse than
showing nothing, because a reader learning a language cannot tell.

## What this means for the feature

**Two of the three conclusions below were wrong**, and the measurement that
overturned them is
[`2026-08-11-one-word-and-its-counterpart.md`](2026-08-11-one-word-and-its-counterpart.md),
run the same day. Asking for the linkage without the roles made every figure
worse, not better, and verify-and-drop turns out to protect against a model that
invents text rather than against one that pairs real text wrongly, which is what
this one does. What survives is the third point: this belongs on the slow lane.

The rest of this section is left as it was written.

**It is not dead.** Three things follow from the measurement rather than from
an opinion.

**1. Verify and drop, do not verify and warn.** The repository already does
this on the deep path: `only_what_is_written_in` drops an explanation in the
wrong script, `without_restatements_of` drops one that repeats its source.
Colouring the pairs that survive and leaving the rest in plain ink gives a card
that is right about what it claims and silent about the rest. On these five
sentences that is 16 of 20 constituents rather than nothing.

**2. Link without naming.** The colours carry the useful half, which is *this
part corresponds to that part*. The role names carry the half the model is bad
at. They are separable, and dropping the labels costs the reader very little:
somebody looking at a coloured pair can see it is the subject without being
told, and cannot be told wrongly.

**3. It belongs on the deep path.** 4.5 seconds is three times a quick
translation and it is a second request on top of one. That is
[ADR-0005](../decisions/ADR-0005-two-speed-ai.md)'s slow lane, where
`Ctrl+Alt+D` already lives, and it is not a thing a hover can pay for.

## What has not been measured

- **Any other model.** `translategemma:latest` is installed here and was not
  tried; a translation-specific model may align better or may not do JSON at
  all.
- **A prompt that asks for linkage without roles**, which point 2 says is the
  version worth building. The 80% above is for a prompt that asks for both.
- **Korean as the source.** Every sentence here is English into Korean, and the
  reader this product is for meets both directions.
- **Anything about rendering.** The card draws with one colour per element
  today, and coloured runs inside a line is its own piece of work.

`scripts/` has no home for this yet; it was run from a scratch file, which is
why the exact script is not in the tree. What is in the tree is the number.
