# Deep analysis, first live run, 2026-08-06

The second path of
[ADR-0005](../decisions/ADR-0005-two-speed-ai.md), asked of a real model
for the first time. Machine and Ollama as in every other benchmark here.
Model resolved by the product's own router: `qwen2.5-coder:7b`.

Run it again with:

```
cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 analyse_
```

## What it answers, and what it declines to answer

Four sources, three of them sentences from this repository and one a
single word. Printing them is allowed for the same reason the
translation benchmark's are: [`../privacy-model.md`](../privacy-model.md)
forbids printing what was **read**, and a benchmark that cannot show its
output is not one.

| Source | word | structure |
|---|---|---|
| The pointer must rest before anything is read. | `rest` | 제한 조건을 설정하는 구문 |
| I would rather it failed loudly than passed for a reason nobody checked. | `passed` | 비교구로, 두 가지 선택 중에서 더 나은 것을 선택한다. |
| A card placed on the wrong sentence is worse than no card. | none pointed at | 비교문 |
| vacuously | `vacuously` | **not asked for** |

The last row is the rule ADR-0005 states and the one worth checking: a
single word has no clause structure, so the prompt tells the model to
leave that field empty and the live test **asserts it came back empty**.
The same assertion holds the other way: the third row pointed at no word,
and no dictionary form was invented for one.

Both assertions are in the live test rather than in prose, so the next
run fails if a model starts filling them in.

## The defect the first run found

The first version of this prompt said *every value must be written in
Korean*. Against this model that was ignored twice in four sentences:

```
    품사           adverb
    구조           The sentence compares placing a card on an incorrect
                   sentence to having no card at all.
```

An explanation in English, under a Korean heading, for a reader whose
reason for asking is that they could not read the English. That is worse
than no explanation.

Two changes, both kept:

1. **The prompt names the fields.** `part_of_speech`, `sense`,
   `structure` and `note` must be in the reader's language; `lemma` is
   the word's own dictionary form and stays in the language it is
   written in. A rule the model has to apply became a list it has to
   follow, which is the same fix that made identifiers survive on
   2026-08-06.
2. **A field in the wrong script is dropped**, in
   `lens_core::analyse::DeepAnalysis::only_what_is_written_in`. Not
   retried: a retry doubles the wait for an answer the model has just
   shown it does not notice is wrong.

`lemma` is exempt from the drop, and that exemption is the subtle part.
It is correct precisely when it looks wrong: `rest` and `passed` are
English, on a Korean card, and right.

After both changes, every explanation in the table above is in Korean.

**It cost content.** `구조` for the third sentence went from an English
sentence to 비교문, which is two syllables and true. The first sentence's
structure went from naming subject and object to 제한 조건을 설정하는
구문, which is vaguer. Constraining the language made the model terser.
Recorded rather than tuned away: a short true line beats a long one
nobody asked for in a language they cannot read.

## Latency

| | |
|---|---:|
| cold, first call | 18,444ms |
| warm | 2,628 · 2,846 · 3,980 · 4,911ms |

Against a quick translation's warm median of 1,613ms. Two to three times
a hover, for something a reader pressed a key to get and is watching the
screen for. `DEEP_TIMEOUT` is 60 seconds, not the hover path's 20, for
the cold case.

## The quick path is not slowed by it

The M5 gate's first clause. Measured rather than argued, because the two
paths share a process, an endpoint and a model lease, so "separate
paths" is a claim about behaviour and not only about modules.

```
quick before  1607ms
deep          3670ms
quick after   1668ms
```

61ms apart, either side of an analysis. Nothing to see, which is the
result.

## Through the product, on a real window

`lens --selftest` runs the whole chain against text already on screen and
now carries on into the deep path. Against a terminal window:

```
1. acquired   49 chars from WindowsTerminal
2. translated 44 chars in 3024ms via qwen2.5-coder:7b
3. shown      card is on screen for 4 seconds
4. explaining  word=true structure=true
   analysed   3 lines in 4400ms via qwen2.5-coder:7b
   원형           6 chars
   이 문장에서       6 chars
   구조           27 chars
   shown      on screen for 4 seconds
```

Lengths, not values: the headings are this repository's own strings and
the values are a model's account of text off a screen, which
[`../privacy-model.md`](../privacy-model.md) §3 forbids printing.

Three lines of five. 품사 came back empty or in English and was dropped,
which is the same result as the table above and the reason to expect it.

The card was **showable**, which is the part that needed a screen: an
analysis is the one card body with newlines in it, and until this ran
nothing had laid one out.

## What this run does not establish

**Whether the explanations are right.** 비교구로, 두 가지 선택 중에서 더
나은 것을 선택한다 is a plausible description of *I would rather X than
Y* and nobody has checked it against a grammar. 원형 for `passed` came
back as `passed` rather than `pass`, which is a wrong dictionary form
presented as one.

That is the reason `DeepAnalysis::LABEL` says AI 설명 · 사실 확인 필요 and
the reason ADR-0005 requires it. A learner cannot tell a wrong grammar
explanation from a right one, which is the whole point of the reader
this is built for.
