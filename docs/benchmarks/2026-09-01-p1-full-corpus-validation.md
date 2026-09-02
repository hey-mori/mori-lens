# P1 Full Corpus Validation Report

Date: 2026-09-01
Model: qwen2.5:7b
Evaluation: 25-entry corpus, 3 runs per entry for invariants, 1 run for full output

## Summary

The P1 prompt (lemmatisation examples) was tested on the full 25-entry corpus
after showing 86% lemma accuracy on an 8-entry subset. The full corpus results
show that P1 improves lemma accuracy (+23pp) but causes POS regression (-8pp)
and does not reduce misleading rate (24%). The threshold for production commit
is NOT met.

## Baseline Production Prompt (P0)

```
The reader is looking at the word "{safe}". Give its dictionary form in
"lemma", its part of speech in "part_of_speech", and what it means in this
particular sentence in "sense".
```

## P1 Prompt (tested, not committed)

```
The reader is looking at the word "{safe}". Reduce it to its dictionary form
in "lemma": "rests" becomes "rest", "answered" becomes "answer", "Placed"
becomes "place", "processed" becomes "process". A word already in its base
form stays as it is. The lemma is English, never {target}. Its part of speech
goes in "part_of_speech", and what it means in this particular sentence in
"sense".
```

## Quantitative Comparison

| Metric | P0 Baseline | P1 (avg of 3 runs) | Delta |
|--------|-------------|---------------------|-------|
| Lemma accuracy | 40% (23/57) | 63% (36/57) | **+23pp** |
| POS accuracy | 95% (54/57) | 87% (50/57) | **-8pp** |
| Structure accuracy | 85% (64/75) | 82% (62/75) | **-3pp** |
| Note/idiom | 0% (0/12) | 0% (0/12) | 0 |
| Language leaks | 1 (A05) | 0 | **-1** |

### P1 Run Variance

| Run | Lemma | POS | Structure |
|-----|-------|-----|-----------|
| 1 | 65% | 89% | 84% |
| 2 | 63% | 84% | 77% |
| 3 | 61% | 88% | 84% |
| avg | 63% | 87% | 82% |

The model shows significant run-to-run variance even with temperature=0.

## Human Evaluation (P1, single run)

| id | lemma | pos | sense | struct | note | cp | overall |
|----|-------|-----|-------|--------|------|----|---------|
| A01 | I | C | C | P | - | - | P |
| A02 | I | C | C | C | - | C | P |
| A03 | - | - | - | P | - | - | N |
| A04 | I | I | P | I | - | C | **M** |
| A05 | C | C | C | C | - | - | U |
| B01 | C | C | C | C | - | C | U |
| B02 | C | C | C | C | - | C | U |
| B03 | - | - | - | I | - | - | **M** |
| B04 | C | P | C | I | - | - | P |
| B05 | I | P | C | I | - | C | P |
| C01 | - | I | I | P | I | - | **M** |
| C02 | - | I | I | P | I | - | **M** |
| C03 | C | I | C | P | I | - | P |
| C04 | - | - | - | C | I | - | N |
| D01 | C | C | P | P | - | - | N |
| D02 | I | C | C | P | - | - | P |
| D03 | C | C | C | I | - | - | P |
| D04 | C | C | C | P | - | C | U |
| D05 | C | C | C | P | - | C | U |
| E01 | C | C | C | C | - | C | U |
| E02 | I | I | C | I | - | C | **M** |
| E03 | - | I | I | I | - | - | **M** |
| F01 | C | I | C | - | - | C | P |
| F02 | C | C | C | I | - | - | P |
| F03 | C | C | C | C | - | C | U |

**Summary:**
- U (useful): 7/25 (28%)
- P (partial): 10/25 (40%)
- M (misleading): 6/25 (24%)
- N (neutral): 3/25 (12%)

**Misleading rate: 24%** (6/25 entries)

**NOTE:** The P0 baseline used a different rubric (separated "incorrect" from
"misleading"). Under the P0 rubric, the problematic rate was 20% (5/25), but
the classification was more lenient for no-word hallucination. This comparison
is NOT directly comparable. See [`2026-09-01-evaluation-integrity-audit.md`]
(2026-09-01-evaluation-integrity-audit.md) for details.

## Misleading Entry Analysis

| Entry | Why misleading |
|-------|---------------|
| A04 | Wrong lemma "answered" + POS empty + structure has Chinese characters ("宾语") |
| B03 | Structure says "복합명사" (compound noun) for an inversion pattern |
| C01 | POS/sense filled for no-word entry (inventing information) |
| C02 | POS/sense filled for no-word entry (inventing information) |
| E02 | Wrong lemma "processed" + wrong POS "과거 시제 동사" + structure has Chinese ("主句") |
| E03 | POS/sense filled for no-word entry + structure empty |

Root causes:
1. No-word entries where model invents POS/sense (C01, C02, E03) - pre-existing
2. Wrong lemma leading to wrong explanation (A04, E02) - P1 did not fully fix
3. Wrong structure description (B03) - model capability limit

## Counterpart Effect with P1

| Metric | P0 | P1 |
|--------|----|----|
| Production rate | 53% | 54% |
| Accuracy (when produced) | 100% | 100% |
| Avg counterpart cost | 2.4s | 4.2s |
| Avg main analysis | 3.2s | 3.9s |

Counterpart quality is unchanged. Latency is higher with P1 (+1.8s counterpart,
+0.7s main).

## Profile Regression

All three profiles (Reader, Developer, Exam) were tested. Key findings:
- Developer profile correctly identifies jargon in notes (C01: "API", C04: "no-op")
- Profile-specific behavior is preserved
- Lemma accuracy issue is consistent across all profiles
- No schema breakage

## Idiom/Note Analysis

Note/idiom detection remains 0% for Reader profile. This is by design: the
prompt says "Most sentences need no note and an empty string is the expected
answer." The Developer profile correctly fills notes when jargon is present.

Classification:
- **Prompt instruction**: "Most sentences need no note" suppresses detection
- **Model capability**: Even without suppression, detection is unreliable
- **Corpus expectation**: C01/C02 expect notes for Reader, but Reader is not
  designed for jargon

## Latency

| Condition | P0 | P1 |
|-----------|----|----|
| Main analysis (warm) | 3.2s | 3.9s |
| Counterpart cost | 2.4s | 4.2s |
| Total deep (with cp) | 5.6s | 8.1s |

P1 increases latency due to longer prompt.

## Decision

**P1 prompt is NOT committed to production.**

Reasons:
1. POS accuracy regressed (-8pp)
2. Misleading rate did not decrease (24% vs 20%)
3. Latency increased (+2.5s total)
4. The threshold "clearly improved with no regression" is not met

The lemma improvement (+23pp) is real but is offset by POS regression and
unchanged misleading rate. The P1 prompt needs revision to fix the POS
regression before it can be committed.

## What P1 Does Fix

- Language leak: A05 "不可用" → "unavailable" (fixed)
- Some lemma errors: "expired" → "expire", "failed" → "fail", "Placed" → "place"

## What P1 Does NOT Fix

- No-word entries inventing POS/sense (C01, C02, E03)
- Idiom/note detection (still 0% for Reader)
- Structure accuracy for complex sentences
- Misleading rate (24%)

## Next Steps

1. Revise P1 prompt to preserve POS accuracy while keeping lemma improvement
2. Consider separate instruction for no-word entries
3. Evaluate larger models (M6 work)
4. Gate stays OPEN until misleading rate decreases and POS regression is fixed

## M5 Gate Status

| Gate | Before | After P1 Validation |
|------|--------|---------------------|
| quick-path latency unchanged | PASS | PASS |
| explicit user action | PASS | PASS |
| schema degradation | PASS | PASS |
| AI explanation labelled | PASS | PASS |
| explanation correctness | OPEN | **OPEN** |

The gate stays OPEN. The P1 prompt improved lemma accuracy but caused POS
regression and did not reduce misleading rate. A revised prompt that improves
lemma without regressing POS or increasing misleading rate is needed.
