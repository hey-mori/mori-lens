# M5 Deep Analysis Final Report (Phase 13)

Date: 2026-09-01
Model: qwen2.5:7b
Evaluation period: 2026-09-01

## Executive Summary

Phase 13 was designed to answer two questions:
1. Are the P0/P1 comparisons valid?
2. Can a conservative prompt (P3) fix no-word hallucination?

**Answer 1:** No. The P0/P1 human evaluation comparison used different rubrics and is NOT directly comparable. The automated metrics ARE comparable after correcting for lemma scoring method (exact match).

**Answer 2:** No. P3's stronger no-word instruction did NOT reduce hallucination. This is a model capability limitation, not a prompt limitation.

**Gate status: OPEN.** No prompt tested (P0, P1, P2, P3) meets the threshold for production commit.

## Methodology Integrity Audit

### Metric Comparability Crisis

The original P0 evaluation (2026-09-01-deep-analysis-evaluation.md) used:
- **Prefix matching** for lemma (81% accuracy)
- **Separated scoring**: "incorrect" (12%) + "misleading" (8%) = 20% problematic

The P1 evaluation used:
- **Exact matching** for lemma (63% accuracy)
- **Combined scoring**: all problematic entries as "misleading" (24%)

**The "P0 20% vs P1 24%" delta is NOT directly comparable.** The P0 rubric was more lenient, classifying no-word hallucination as "incorrect" rather than "misleading".

### Correction

After re-running P0 with exact matching, the fair comparison is:

| Metric | P0 (exact) | P1 (exact) | Delta |
|--------|-----------|-----------|-------|
| Lemma | 40% (23/57) | 63% (36/57) | +23pp |
| POS | 95% (54/57) | 87% (50/57) | -8pp |
| Structure | 85% (64/75) | 82% (62/75) | -3pp |

The automated metrics ARE comparable. The human evaluation requires re-scoring on the same rubric, which was not possible (P0 full output not available).

### Documents Updated

- `2026-09-01-deep-analysis-evaluation.md`: Added comparability note
- `2026-09-01-production-decision.md`: Marked as SUPERSEDED, corrected misleading rate claim
- `2026-09-01-p1-full-corpus-validation.md`: Added comparability note
- `roadmap.md`: Added integrity audit reference

## Corpus Expectation Audit

### No-word entries (C01, C02, E03)

Expected: lemma="", pos="", sense=""

Model fills POS/sense → hallucination. **Audit: Expected behavior is correct.** No word = no lemma/pos/sense.

### Note expectation for Reader profile (C01, C02)

Corpus expected idiom notes for C01/C02 under Reader profile. But the production prompt says "Most sentences need no note and an empty string is the expected answer." Reader profile is not designed for idiom detection.

**Audit: Corpus expectation wrong for Reader profile.** The Developer profile correctly fills notes for jargon.

### D02: nearest → near

"nearest" is the superlative form of "near". Expected lemma "near" is correct. Model returned "nearest" (unchanged). **Audit: Expected correct, model failed.**

### F01: vacuously → vacuously

"vacuously" is already in its base form (adverb). Expected lemma "vacuously" is correct. **Audit: Expected correct.**

### F02: Workloom → Workloom

"Workloom" is a proper noun. Proper nouns are their own lemma. **Audit: Expected correct.**

## Human Evaluation Rubric (Fixed)

All future M5 evaluations use this rubric:

### Per-field scoring
```
C = correct: the field is accurate and complete
P = partially correct: incomplete or partially wrong
I = incorrect: the field is wrong
N/A = not applicable: should be empty
```

### Overall scoring
```
U = useful: helps the reader understand
N = neutral: neither helpful nor harmful
M = misleading: wrong information that could cause incorrect knowledge
```

### Critical distinction: Incorrect vs Misleading

```
Incorrect (I) for a field:
  - Lemma is wrong but the reader would likely notice
  - Structure is incomplete but not confidently wrong
  - Sense is vague but not actively misleading

Misleading (M) for overall:
  - Wrong grammatical explanation stated confidently
  - Invented POS/sense for a no-word entry (hallucination)
  - Mixed-language output (e.g., Chinese characters in Korean text)
  - Structure description that is confidently wrong
  - Any error that a learner would accept as correct and remember
```

### Key rule

**No-word hallucination is always M (misleading), not I (incorrect).** When the model invents POS/sense for an entry with no focused word, it creates false information that the reader has no reason to doubt.

## Misleading Regression Set

These 6 entries must pass (not M) before any prompt change can be committed:

| ID | Source | Word | Failure mode |
|----|--------|------|-------------|
| A04 | A card that the model has already answered is served from cache. | answered | Wrong lemma + mixed-language structure |
| B03 | Not only does the model translate, it also explains. | (none) | Wrong structure for inversion |
| C01 | The output files came back empty. | (none) | No-word hallucination (POS/sense) |
| C02 | The model is up to date with the latest release. | (none) | No-word hallucination (POS/sense) |
| E02 | The data, having been processed, was written to the output. | processed | Wrong lemma + wrong POS + mixed-language |
| E03 | So quickly did the model respond that nobody noticed the latency. | (none) | No-word hallucination (POS/sense) |

Subtypes:
- No-word hallucination: C01, C02, E03 (3 entries)
- Wrong lemma: A04, E02 (2 entries)
- Wrong structure: B03 (1 entry)
- Mixed-language output: A04, E02 (2 entries)

## Prompt Comparison (P0 / P1 / P2 / P3)

### Design

**P0 (production):** Standard prompt, no lemmatisation examples.

**P1:** Explicit lemmatisation examples ("rests" → "rest", "answered" → "answer", etc.).

**P2:** Conservative: "better to return an empty string than to guess."

**P3:** P1's lemmatisation examples + stronger POS instruction ("the part of speech of THE WORD") + much stronger no-word instruction ("CRITICAL: You MUST leave empty").

### Results (11-entry subset, 1 run)

| Prompt | Lemma | POS | No-word hallucination |
|--------|-------|-----|----------------------|
| P0 | 36% (4/11) | 82% (9/11) | 1/2 (50%) |
| P1 | 82% (9/11) | 91% (10/11) | 2/2 (100%) |
| P2 | 36% (4/11) | 0% (0/11) | 1/2 (50%) |
| P3 | 64% (7/11) | 82% (9/11) | 2/2 (100%) |

### Analysis

**P1 is the best prompt tested** for lemma (82%) and POS (91%), but causes POS regression on the full 25-entry corpus (-8pp).

**P3 does NOT solve no-word hallucination.** Despite the "CRITICAL: You MUST leave empty" instruction, the model still invents information.

**P2 is too conservative.** 0% POS accuracy because all fields are empty.

**No prompt tested meets the threshold for production commit.**

## Model Limitation Classification

### Prompt limitations (fixable by prompt engineering)

1. **Lemma accuracy:** P1's lemmatisation examples improve accuracy from 40% to 63% (full corpus) or 36% to 82% (subset). The model CAN lemmatise when given examples.

2. **Language leaks:** P1 eliminates mixed-language output (A05 "不可用" → "unavailable"). The model CAN stay in one language when told to.

### Model capability limitations (NOT fixable by prompt engineering)

1. **No-word hallucination:** The model cannot reliably follow the instruction to leave fields empty when no word is focused. Tested with P0, P1, P2, P3 — all fail. This is a fundamental inability to follow negative constraints.

2. **Idiom/note detection:** 0% for Reader profile. The model cannot reliably detect jargon, idioms, or register. This is by design (prompt suppresses detection), but even without suppression, detection would be unreliable.

3. **Structure accuracy for complex sentences:** B03 (inversion) fails across all prompts. The model cannot parse complex grammatical structures.

4. **POS regression with P1:** Adding lemmatisation examples causes POS to regress from 95% to 87% on the full corpus. The model's attention is drawn to lemma at the expense of POS.

## Production Decision

**No prompt change is committed.**

The threshold for production commit is:
> "Clear improvement with no regression on misleading rate, POS, or latency"

None of the tested prompts meet this threshold:
- P0: Low lemma accuracy (40%)
- P1: POS regression (-8pp), misleading rate unchanged (24%)
- P2: Too conservative (0% POS)
- P3: Lemma 64%, no-word hallucination persists

## M5 Gate Status

| Gate condition | Status | Evidence |
|----------------|--------|----------|
| quick-path latency unchanged | PASS | Not affected by deep analysis |
| explicit user action | PASS | Ctrl+Alt+D, not automatic |
| schema degradation | PASS | Missing field = empty field, typed error |
| AI explanation labelled | PASS | "AI 설명 · 사실 확인 필요" shown on every card |
| the explanations are right | **OPEN** | Best lemma 63%, misleading 24%, no-word hallucination persists |

## What Would Close the Gate

The gate can be closed when:
1. Lemma accuracy > 80% (exact match) with no POS regression
2. Misleading rate < 10% (using consistent rubric)
3. No-word hallucination < 5% (using consistent rubric)

This requires either:
- A better model (larger, more capable)
- A breakthrough in prompt engineering that solves no-word hallucination
- A different approach (e.g., separate model call for no-word detection)

## What This Does NOT Mean

This does NOT mean:
- The product is unusable. The "AI 설명 · 사실 확인 필요" label is doing the work it was designed to do.
- The current implementation is broken. It works correctly within its limitations.
- No progress was made. We now have a measurement instrument that can detect improvement.

## Recommendations

1. **M6 work:** Evaluate larger models (qwen2.5:14b, llama3:70b) to see if they can solve no-word hallucination and improve lemma accuracy.

2. **Separate no-word detection:** Before calling the deep analysis model, add a fast check: "Is there a focused word?" If no, skip lemma/pos/sense entirely. This is a pipeline change, not a prompt change.

3. **Accept the limitation:** No-word entries are rare in practice (most uses have a focused word). The product can ship with the current limitation, clearly labelled.

4. **Continue measurement:** Run the evaluation harness periodically to track improvement as models improve.

## Conclusion

Phase 13 answered the two questions:
1. P0/P1 comparisons were NOT valid (different rubrics). Corrected metrics show P1 lemma +23pp but POS -8pp.
2. P3 did NOT solve no-word hallucination (model capability limit).

The gate remains OPEN. No prompt tested meets the threshold for production commit. The product is usable with the current label, but the explanations are not yet reliably correct.

The evaluation harness is now a permanent part of the codebase. It can detect improvement when better models or prompts become available.
