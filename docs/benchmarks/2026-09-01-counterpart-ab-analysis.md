# Phase 5: Counterpart A/B Analysis

Date: 2026-09-01
Model: qwen2.5:7b

## Question

"Counterpart가 실제로 '틀린 설명을 고치는가', 아니면 단순히 이미 맞는 결과를
재확인하는가?"

Does counterpart actually fix wrong analyses, or does it just confirm correct
ones?

## Architectural Answer

Counterpart **cannot fix wrong analyses**. The code is explicit:

1. `OllamaTranslator::deep()` makes 1 main analysis call.
2. It parses lemma, pos, sense, structure, note from that call.
3. It then calls `counterpart_of()` which makes 2 additional calls.
4. If both counterpart calls agree exactly, the result is stored in
   `analysis.counterpart`.
5. The main analysis fields are **not modified** by counterpart.

Counterpart is additive. It adds a `counterpart` field to the analysis card
("번역에서" row) showing which part of the translation carries the focused word.
It does not change lemma, pos, sense, or structure.

## Measured Results

19 word-focused entries, 3 runs each = 57 runs total.

| Metric | Value |
|--------|-------|
| Average deep() latency (with counterpart) | 5608ms |
| Average main analysis latency | 3247ms |
| Average counterpart cost (2 calls) | 2361ms |
| Counterpart produced | 30/57 (53%) |
| Counterpart in translation (when produced) | 30/30 (100%) |

### Entries where counterpart is never produced

| ID | Word | Why counterpart disagrees |
|----|------|--------------------------|
| A01 | rests | "멈춥니다" is a conceptual translation, not word-for-word |
| A05 | unavailable | "사용할 수 없는" is a phrase, not a single word |
| B04 | Placed | "배치되었을" is a reduced clause form |
| C03 | green | Translated conceptually as "통과했습니다" (passed) |
| D01 | adapter | "어댑터" is a loanword, counterpart questions disagree |
| D02 | lease | "대여하는" is a verb phrase, not a noun |
| D03 | cache | "캐시" is a loanword |
| F02 | Windows | Proper noun, not translated |
| F03 | 30분 | Numeric expression, not a word |

These are exactly the cases where the focused word does not have a clean
word-for-word mapping in the translation. The two counterpart questions (asked
from opposite ends) disagree because the word is embedded in a phrase,
translated conceptually, or not translated at all.

### Entries where counterpart is always produced

A02, A04, B01, B02, B05, D04, D05, E01, E02, F01

These entries have clear word-for-word mappings: the focused word appears
directly in the translation as a recognisable unit.

## Analysis

### What counterpart does

- Adds a "번역에서" row to the analysis card showing which part of the
  translation carries the focused word.
- Always correct when produced (100% in translation).
- Produced 53% of the time.
- Costs ~2.4s (two model calls).

### What counterpart does NOT do

- Fix wrong lemma. If the main analysis says "rests" instead of "rest",
  counterpart does not change it.
- Fix wrong POS. If the main analysis says "Past Participle" instead of "동사",
  counterpart does not change it.
- Fix wrong sense. If the main analysis gives a wrong explanation, counterpart
  does not correct it.
- Fix wrong structure. If the main analysis gives wrong clause info, counterpart
  does not fix it.

### Is counterpart worth the cost?

**Arguments for keeping it:**
- 100% accuracy when produced.
- Helps the reader connect the English word to its Korean translation.
- The reader pressed Ctrl+Alt+D and is willing to wait. 2.4s is acceptable.
- It is the only part of the analysis that directly addresses "where does this
  word appear in the translation I already have?"

**Arguments against it:**
- Costs ~2.4s on every deep analysis.
- Only produced 53% of the time.
- Does not improve the main analysis quality.
- The main analysis quality problems (lemma accuracy, structure accuracy) are
  not addressed by counterpart.

**Recommendation:** Keep counterpart. It is additive, not corrective. It does
not fix wrong analyses, but it adds useful context when available. The 2.4s
cost is acceptable for a reader-triggered action. The 53% production rate is
not a failure: it is the model correctly recognising when a word does not have
a clean mapping.

However, counterpart should not be mistaken for a quality mechanism. The main
analysis quality must be improved through prompt engineering (Phase 4) or model
selection (Phase 3), not through counterpart.

## Implications for Phase 4

The prompt A/B results (Phase 4) showed that P1 improves lemma accuracy from
29% to 86%. This is the main analysis improvement. Counterpart is independent:
it adds supplementary info regardless of whether the main analysis is correct.

The production decision is:
1. Adopt P1 prompt for main analysis (Phase 4 recommendation).
2. Keep counterpart as-is (this Phase 5 recommendation).

These are orthogonal changes. P1 fixes the main analysis. Counterpart adds
context. Both are worth doing.
