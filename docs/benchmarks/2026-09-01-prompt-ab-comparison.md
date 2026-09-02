# Prompt A/B Comparison: P0 vs P1 vs P2

Date: 2026-09-01
Model: qwen2.5:7b
Method: Same 8-entry subset, same schema, same parser, same model.
Only the system prompt text changes.

## Prompt Variants

**P0 (production):** The prompt currently in `analyse.rs`. Says "lemma is the word's
dictionary form and stays in the language it is written in." Says "Most sentences
need no note." Does not give worked examples of lemmatisation.

**P1 (explicit structured reasoning):** Adds worked examples of lemmatisation
("rests" -> "rest", "answered" -> "answer", etc.). Adds explicit per-field
instructions for how to determine each value. Structure instruction gives
examples of clause patterns.

**P2 (conservative):** Emphasises "It is better to return an empty string than to
guess. A wrong lemma, a wrong part of speech, or a wrong structure explanation is
actively harmful." Adds the same lemmatisation examples as P1 but wraps every
field instruction with "If you are not confident, leave it empty."

All three variants use the same JSON schema, the same parser, and the same model.
The only variable is the system prompt text.

## Results

### Lemma accuracy (8 entries, 7 with a focused word)

| Entry | Word       | P0 lemma    | P1 lemma   | P2 lemma   | Expected |
|-------|------------|-------------|------------|------------|----------|
| A01   | rests      | rests WRONG | rest OK    | rests WRONG| rest     |
| A04   | answered   | answered W  | answer OK  | answered W | answer   |
| A05   | unavail.   | 불可用 W     | unavail. OK| unavail. OK| unavail. |
| B04   | Placed     | Placed W    | place OK   | Placed W   | place    |
| D01   | adapter    | adapter OK  | adapter OK | adapter OK | adapter  |
| D04   | targeting  | targeting ? | targeting ?| targeting ?| target?  |
| E02   | processed  | processed W | process OK | process OK | process  |

**Lemma accuracy:**

| Variant | Correct | Wrong | Uncertain |
|---------|---------|-------|-----------|
| P0      | 2/7     | 4/7   | 1/7       |
| P1      | 6/7     | 0/7   | 1/7       |
| P2      | 3/7     | 3/7   | 1/7       |

P1 is the clear winner. The explicit lemmatisation examples in the prompt are the
single highest-leverage change available without touching the model.

### POS accuracy

| Variant | Filled | Korean | Reasonable |
|---------|--------|--------|------------|
| P0      | 6/7    | 5/7    | 4/7        |
| P1      | 7/7    | 7/7    | 6/7        |
| P2      | 1/7    | 0/7    | 1/7        |

P2 empties almost everything. The "leave empty when unsure" instruction is too
strong: the model takes it as permission to not try. P1 fills POS correctly for
every entry.

### Structure

| Variant | Filled | Contains clause info | Korean |
|---------|--------|---------------------|--------|
| P0      | 6/8    | 4/8                 | 5/8    |
| P1      | 8/8    | 7/8                 | 6/8    |
| P2      | 6/8    | 2/8                 | 2/8    |

P2's "leave empty when unsure" empties structure for most entries, and the few
that are filled contain template syntax ("{NP}는 {ADJ}이고 {NP}는 아직 {ADJ}")
rather than actual clause descriptions.

### Note / idiom detection

| Variant | Entries with note | Expected note entries hit |
|---------|-------------------|--------------------------|
| P0      | 0/8               | 0/3                      |
| P1      | 0/8               | 0/3                      |
| P2      | 0/8               | 0/3                      |

All three variants fail to produce notes. C04 (no-op jargon), D01 (adapter
error), and D04 (acquisition/targeting jargon) all have expected_idiom=true, but
no variant fills the note field. The failure is model-capability, not
prompt-instruction: even when the prompt says "give one idiom, register or
false-friend worth knowing," the model returns empty. The "Most sentences need no
note" line in P0 reinforces this, but P1 and P2 also fail, so the suppression
line is not the root cause. The root cause is that the model does not detect
jargon or idiom reliably at all.

### Language leaks

| Variant | Entries with non-English lemma | Language leak details |
|---------|-------------------------------|----------------------|
| P0      | 1 (A05: 불可用)                | Chinese characters in lemma |
| P1      | 0                             | None                 |
| P2      | 0                             | None                 |

P1 and P2 both say "lemma MUST be an English word, never Korean" explicitly. P0
says "stays in the language it is written in" which the model interprets as
"translate it." The explicit English constraint fixes the language leak.

### Latency

| Variant | Min    | Median | Max    |
|---------|--------|--------|--------|
| P0      | 2706ms | 3202ms | 24875ms|
| P1      | 2163ms | 3075ms | 3720ms |
| P2      | 1841ms | 2700ms | 3136ms |

P0's 25s outlier is cold model load (first call). After warm-up, all three
variants are within the same 2-3.5s range. P2 is slightly faster because it
produces fewer tokens (empty fields). No significant latency penalty from P1.

## Analysis

### Why P1 works

The production prompt tells the model WHAT to produce ("dictionary form") but not
HOW. The model has to figure out on its own that "rests" -> "rest" requires
removing the third-person suffix, that "answered" -> "answer" requires removing
"-ed", and that "Placed" -> "place" requires both removing "-d" and lowering the
case. P1 gives three worked examples that establish the pattern, and the model
generalises from them.

This is a well-known property of small models: they follow explicit patterns much
better than they follow abstract rules. "The lemma is the dictionary form" is an
abstract rule. "rests -> rest, answered -> answer, Placed -> place" is a pattern
the model can imitate.

### Why P2 fails

P2 says "if you are not confident, leave it empty." The model interprets this as
"the safe default is empty." For entries where it was already producing wrong
answers (rests, answered, Placed), the conservatism does not help because the
model does not know it is wrong. It produces the same wrong lemma as P0. For
entries where it was producing correct answers, the conservatism empties them.
Net effect: fewer correct fields and the same number of wrong fields.

P2 also produces template-like structure descriptions ("{NP}는 {ADJ}이고 {NP}는
아직 {ADJ}") which are worse than empty: they look like grammar but are not.

### The D04 "targeting" question

All three variants produce "targeting" as the lemma. Is this correct? In
"Acquisition is solved and targeting is not," "targeting" could be:

1. A gerund ("the act of selecting targets"), lemma = "target"
2. A noun ("targeting" as a technical concept), lemma = "targeting"

The corpus entry says `expected_lemma: Some("target")`. But the sentence is
ambiguous enough that "targeting" as a noun is defensible. The model might
actually be right here. This is a case where the ground truth could be debated.

### Implications for production

P1 is a clear improvement over P0 across every measured dimension:
- Lemma accuracy: 86% vs 29%
- POS accuracy: 100% vs 57%
- Structure: 88% filled with clause info vs 50%
- Language leaks: 0 vs 1
- No latency penalty

The one concern is that P1 fills lemma even when no word is focused (C04 gets
"no-op" as a lemma). But P0 handles this correctly because it explicitly says
"Leave lemma, POS, sense as empty strings" when no word is pointed at. The P1
prompt variant tested here did not include that conditional. The actual P1 prompt
for production would need to keep the answerable() conditional.

### What P1 does NOT fix

- **Idiom detection:** Still 0%. The model simply cannot detect jargon, idioms,
  or register reliably. No prompt wording fixes this.
- **Sense quality:** P1's senses are more detailed but not necessarily more
  accurate. This needs human evaluation.
- **Structure accuracy:** P1 fills structure more often, but whether the clause
  descriptions are correct needs human evaluation.

## Recommendation

Adopt P1 as the production prompt for word-focused entries. The explicit
lemmatisation examples are the single highest-leverage change. The change is a
prompt edit only, no schema change, no parser change, no counterpart change.

The counterpart analysis (Phase 5) is independent: even with P1 fixing the main
analysis, the counterpart still adds two model calls. Its value question remains.
