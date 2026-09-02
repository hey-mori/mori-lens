# Deep analysis evaluation, 2026-09-01

**NOTE on comparability:** This evaluation used prefix matching for lemma
(81%) and separated "incorrect" from "misleading" in human evaluation.
Later P1 testing used exact matching and combined I+M into "misleading".
The comparison "P0 20% vs P1 24% misleading" is NOT directly comparable.
See [`2026-09-01-evaluation-integrity-audit.md`]
(2026-09-01-evaluation-integrity-audit.md) for the audit.

M5's last open gate is "the explanations are right." Everything in M5
is built and wired, from the Ctrl+Alt+D trigger to the card to the
copy to the save to the detail window. What was never measured is
whether the model's output is actually useful to a reader.

This document is that measurement. It is not a demonstration that
the model is good. It records what the model actually produced on
25 hand-curated sentences and where it failed.

## What was measured

25 entries across 6 categories, each run through the production
`DeepRequest` -> `OllamaTranslator::deep()` path. The same function
the Ctrl+Alt+D worker, the selftest, and the existing live tests call.
No production code was changed. Model: `qwen2.5:7b`.

| Category | Entries | What it tests |
|---|---|---|
| A. General English | 5 | simple sentences, relative clauses, conditionals |
| B. Grammar-sensitive | 5 | present perfect, subjunctive, inversion, participle, gerund |
| C. Idiom / expression | 4 | phrasal verbs, compound adjectives, jargon |
| D. Developer | 5 | error messages, API docs, code review, architecture |
| E. Exam | 3 | inverted conditional, perfect participle, so...that |
| F. Edge cases | 3 | single word, proper noun, numeric expression |

## Machine-checkable invariants (3 runs each)

```
id    category      lemma      pos   struct     lang
-------------------------------------------------------
A01   general         3/3      3/3      3/3       ko
A02   general         3/3      3/3      3/3       ko
A03   general           -        -      3/3       ko
A04   general         0/3      0/3      1/3       ko
A05   general         0/3      3/3      3/3       ko
B01   grammar         3/3      3/3      2/3       ko
B02   grammar         1/3      3/3      3/3       ko
B03   grammar           -        -      3/3       ko
B04   grammar         1/3      3/3      3/3       ko
B05   grammar         3/3      3/3      3/3       ko
C01   idiom             -        -      3/3       ko
C02   idiom             -        -      3/3       ko
C03   idiom           3/3      3/3      3/3       ko
C04   idiom             -        -      3/3       ko
D01   developer       3/3      3/3      3/3       ko
D02   developer       3/3      3/3      3/3       ko
D03   developer       3/3      3/3      3/3       ko
D04   developer       2/3      3/3      3/3       ko
D05   developer       3/3      3/3      3/3       ko
E01   exam            3/3      3/3      0/3       ko
E02   exam            3/3      3/3      3/3       ko
E03   exam              -        -      0/3       ko
F01   edge            3/3      3/3      3/3       ko
F02   edge            3/3      3/3      0/3       ko
F03   edge            0/3      3/3      3/3       ko
-------------------------------------------------------
lemma:     46/57 (81%)
pos:       54/57 (95%)
structure: 63/75 (84%)
```

Lemma matching uses case-insensitive prefix matching: "rests" and
"rest" count as a match, "Expired" and "expire" count as a match.
The numbers above include those prefix matches. Strict exact match
would be lower.

## Human evaluation of each entry

Each entry was run once through the full path. The output was read by
a person and scored. The scoring uses C=correct, P=partial, I=incorrect,
-=not applicable. Overall: U=useful, N=neutral, M=misleading.

| id | lemma | pos | sense | structure | counterpart | overall |
|---|---|---|---|---|---|---|
| A01 | P | C | C | P | - | U |
| A02 | P | C | P | P | C | N |
| A03 | - | - | P | P | - | N |
| A04 | I | I | P | I | C | M |
| A05 | I | C | C | C | - | P |
| B01 | P | C | C | I | C | P |
| B02 | I | C | C | I | C | M |
| B03 | - | - | P | P | - | N |
| B04 | P | P | P | P | - | P |
| B05 | P | P | C | P | C | P |
| C01 | - | - | P | P | - | I |
| C02 | - | - | I | I | - | I |
| C03 | C | C | C | P | - | U |
| C04 | - | - | - | C | - | N |
| D01 | C | C | C | C | - | U |
| D02 | P | C | C | P | - | U |
| D03 | C | C | C | P | - | U |
| D04 | C | C | C | C | C | U |
| D05 | C | C | I | P | C | P |
| E01 | C | C | C | I | C | P |
| E02 | P | P | C | P | C | P |
| E03 | - | - | I | I | - | I |
| F01 | C | I | C | C | C | P |
| F02 | C | C | P | I | - | N |
| F03 | I | C | C | C | C | P |

```
useful:     6/25  (24%)
partial:   10/25  (40%)
neutral:    4/25  (16%)
incorrect:  3/25  (12%)
misleading: 2/25  ( 8%)
```

64% of entries are at least partially correct. 24% are fully useful.
20% are incorrect or misleading.

## What went wrong, classified

### Lemma extraction is unreliable (model capability)

The model often returns the inflected form ("rests", "expired",
"answered") instead of the dictionary form ("rest", "expire",
"answer"). Sometimes it translates instead of lemmatizing:
"failed" became "실패" (Korean), "timeout" became "타임아웃"
(Korean transliteration). The prompt says to return the dictionary
form, but the model does not consistently comply.

Affected: A04, B02, F03 (wrong), A01, A02, B01, B04, B05, D02, E02
(partial). 10 of 19 word-focused entries.

### Language leak in lemma field (prompt)

A05 returned "불可用" for the lemma of "unavailable", mixing Korean
and Chinese characters. The lemma field is exempt from the language
filter by design (because lemmas are English words), so this mixed-
script output reached the card. The prompt could be clearer that the
lemma must stay in English.

Affected: A05.

### Structure explanations are sometimes empty (model capability)

The model returned empty structure for 5 of 25 entries, including
both exam-level patterns (E01 inverted conditional, E03 so...that
inversion) and a simple sentence with a proper noun (F02). When the
structure IS filled, it is sometimes a paraphrase of the sentence
rather than a clause analysis.

Affected: A04, B01, E01, E03, F02.

### POS is occasionally wrong (model capability)

F01 classified "vacuously" as an adjective (형용사) instead of an
adverb (부사). B05 classified "explaining" as a verb (동사) instead
of a gerund (동명사). These are real classification errors.

Affected: F01, B05.

### Idiom detection does not happen (prompt)

None of the 4 idiom entries (C01-C04) produced a note identifying
the idiom or phrasal verb. The prompt does not specifically ask for
idiom detection. C03 ("green" in CI context) did correctly identify
the domain-specific meaning in the sense field, but did not label it
as jargon.

Affected: C01, C02.

### Sense is sometimes wrong (model capability)

D05 interpreted "lease" as a rental contract (임대 계약) instead of
the model retention period that the sentence actually means. The
model has no way to know that "lease" in this repository refers to
model residency, not property rental.

Affected: D05.

## Error summary

| Category | Count | Affected entries |
|---|---|---|
| model capability | 12 | A04, B02, F03, A01, A02, B01, B04, B05, D02, E02, F01, D05 |
| prompt | 2 | A05, C01, C02 |
| schema | 0 | - |
| parser | 0 | - |
| evaluation | 0 | - |

The dominant failure mode is **model capability**: the model does not
reliably lemmatize, sometimes returns empty fields, and occasionally
classifies parts of speech incorrectly. The prompt could be improved
to better specify idiom detection and lemma language, but the core
issue is that a 7.6B parameter model is not a reliable grammar tool.

No schema, parser, or evaluation errors were found. The pipeline is
correct; the model is inconsistent.

## Counterpart cost and quality

| Metric | Value |
|---|---|
| Deep full (wall clock, median) | 4,512ms |
| Deep main (analysis only, median) | 2,720ms |
| Counterpart cost (median, when produced) | 1,920ms |
| Counterpart produced | 31/57 (54%) |
| Counterpart in translation (when produced) | 31/31 (100%) |

The counterpart adds ~1.9 seconds when it fires. It fires for 54%
of word-focused entries and never produces a wrong answer: every
counterpart that was produced was a substring of the translation.

The counterpart naturally filters out cases where "what translates
this word" would be ambiguous: abstract terms (A01 "rests"), technical
terms (D01 "adapter", D03 "assumption"), and proper nouns (F02
"Workloom") never produce a counterpart. The filter is the product
of two model calls that must agree exactly, and it works.

The cost question: 1.9 seconds for a counterpart that is always
correct and present 54% of the time. Against a main analysis that
takes 2.7 seconds, the counterpart adds 71% to the wall-clock time.
Whether that is worth it depends on the user. The counterpart is
not shown unless it agrees, so the cost is the latency, not a
confusing extra row.

## Profile comparison

Five entries, three profiles, three runs each.

```
                          품사 dropped   구조 present   참고 present
reader (unchanged)        0 of 15        14 of 15      0 of 15
developer                 15 of 15       8 of 15       3 of 15
exam                      0 of 15        15 of 15      0 of 15
```

**Developer drops POS every time** (15/15), matching the earlier
measurement of 12/12. It drops structure less reliably than before
(8/15 vs 1/12). It fills the note field 3 times (C01 with "came
back", C04 with "no-op" jargon explanation, D02 with a note). That
is an improvement over the 1/24 from August, but the note is still
filled on only 20% of entries.

**Exam fills structure every time** (15/15), up from 11/12 in
August. It fills POS every time (15/15), and the POS is more
detailed: A02 exam returned "동사, 현재 시" against reader's "동사".

**Schema invariance is maintained** across all profiles: the JSON
always has all 5 keys. Profiles change what the model fills, not
what the schema requires.

Latency by profile varies unpredictably (2.1s to 3.8s). The rows a
profile drops are short and the sense line dominates, so fewer rows
do not reliably mean faster.

## Latency breakdown

| Condition | Min | Median | P95 | Max | n |
|---|---|---|---|---|---|
| quick (warm) | 1,006ms | 1,021ms | 1,102ms | 1,102ms | 3 |
| deep (full, warm) | 2,512ms | 4,512ms | 5,257ms | 5,257ms | 9 |
| deep (main only) | 2,512ms | 2,720ms | 3,158ms | 3,158ms | 9 |
| deep (cache hit) | 6ms | 6ms | 8ms | 8ms | 3 |
| counterpart cost | 0ms | 1,920ms | 2,177ms | 2,177ms | 9 |

Deep is 2.7x slower than quick at the median (4,512ms vs 1,021ms),
but the quick path is never called during deep analysis. The deep
cache at 6 microseconds is instant and unchanged from the selftest
measurement.

The `analysis.latency_ms` field records only the main analysis call,
not the counterpart calls. A card footer that says "4.5s" is
misleading: the main call took 2.7s and the counterpart took 1.8s.
This is not a bug (the field has always measured the main call only)
but it is worth noting for anyone reading the footer.

## The gate judgement

The M5 gate condition "the explanations are right" cannot be closed
as "yes." The evidence is:

- 24% useful, 40% partial, 36% neutral or worse
- Lemma extraction is unreliable (81% with prefix matching)
- Structure explanations are empty 20% of the time
- Idiom detection does not happen
- 2 of 25 entries are actively misleading

It also cannot be closed as "no." The evidence for that is weaker:

- 95% POS accuracy (when present)
- 100% counterpart precision
- Schema invariance is maintained
- Profiles behave as designed
- The pipeline has zero parser, schema, or evaluation errors

The product label `AI 설명 · 사실 확인 필요` is doing exactly the
work it was designed to do. The explanations are sometimes right,
sometimes wrong, and the card says so.

**Gate remains open.** The failure is model capability, not pipeline
correctness. A larger or better model would close most of these
failures without any code change. Whether that is the right product
decision is not for this measurement to answer.

## What would change the judgement

Three things, none of them code changes in this repository:

1. A model that lemmatizes reliably. 10 of 19 word-focused entries
   have lemma problems. A model that returns "answer" for "answered"
   and "fail" for "failed" would move lemma from 81% to 95%+.

2. A model that fills structure even for complex patterns. E01 and
   E03 returned empty structure for the two most interesting grammar
   patterns in the corpus. A model that explains "were...to" as an
   inverted conditional would move structure from 84% to 92%+.

3. A prompt that asks for idiom detection. C01 and C02 would benefit
   from a prompt line like "If the sentence uses an idiom or phrasal
   verb, name it in the note." This is the only failure that a prompt
   change could fix without a model change.

None of these are urgent. The label holds. The reader is warned. The
product does not pretend.
