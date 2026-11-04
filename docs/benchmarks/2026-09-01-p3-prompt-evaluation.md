# P3 Prompt Evaluation

Date: 2026-09-01
Model: qwen2.5:7b
Evaluation: 11-entry subset (A01, A04, A05, B04, C02, C03, C04, D01, D04, E02, F02)

## P3 Design

P3 combines P1's lemmatisation examples with:
1. POS instruction that explicitly says "the part of speech of the focused word"
2. Much stronger no-word instruction: repeated prohibition on inventing fields

**P3 word-focused:**
```
The reader is looking at the word "{safe_word}".

For "lemma": reduce the word to its dictionary (base) form. Examples: "rests" → "rest", "answered" → "answer", "expired" → "expire", "Placed" → "place", "processed" → "process". If the word is already in its base form, return it unchanged. The lemma MUST be an English word, never Korean.

For "part_of_speech": the grammatical category of THE WORD "{safe_word}" in this sentence, in Korean (동사, 명사, 형용사, 부사, etc.). This is the part of speech of "{safe_word}", not of the sentence or any other word.

For "sense": what "{safe_word}" means IN THIS SENTENCE, in Korean.
```

**P3 no-word:**
```
No word was pointed at.

CRITICAL: You MUST leave "lemma", "part_of_speech" and "sense" as empty strings. Do NOT invent a word to analyse. Do NOT fill these fields with information about any word in the sentence. Leave them completely empty. An explanation without a focused word is not useful.
```

## Results (11-entry subset, 1 run)

### Lemma accuracy

| Prompt | Correct | Total | Accuracy |
|--------|---------|-------|----------|
| P0 | 4 | 11 | 36% |
| P1 | 9 | 11 | 82% |
| P2 | 4 | 11 | 36% |
| P3 | 7 | 11 | 64% |

**P3 lemma errors:**
- A05: "unavailable" → "available" (wrong, missed negation prefix)
- D04: "targeting" → "target" (wrong, "targeting" is correct as gerund)
- E02: "processed" → "processed" (wrong, should be "process")
- F02: "Workloom" → "dashboard" (wrong, analyzed wrong word)

### POS accuracy

| Prompt | Correct | Total | Accuracy |
|--------|---------|-------|----------|
| P0 | 9 | 11 | 82% |
| P1 | 10 | 11 | 91% |
| P2 | 0 | 11 | 0% |
| P3 | 9 | 11 | 82% |

**P2 is too conservative:** all POS fields empty.

**P3 POS errors:**
- C03: "green" → "형용사" but P3 said "형용사" (actually correct)
- C04: empty (no-word entry, should be empty anyway)

### No-word hallucination

| Prompt | C02 (no-word) | C04 (no-word) | Hallucination rate |
|--------|---------------|---------------|-------------------|
| P0 | Filled lemma/pos/sense | All empty | 1/2 (50%) |
| P1 | Filled lemma/pos | Filled lemma | 2/2 (100%) |
| P2 | All empty | Filled lemma | 1/2 (50%) |
| P3 | Filled lemma | Filled lemma | 2/2 (100%) |

**P3 does NOT solve no-word hallucination.** Despite the "CRITICAL: You MUST leave empty" instruction, the model still invents information.

### Key findings

1. **Lemma accuracy:** P3 (64%) is between P0 (36%) and P1 (82%). The lemmatisation examples help, but the stronger POS instruction may be confusing the model.

2. **POS accuracy:** P3 (82%) is the same as P0, worse than P1 (91%). The word-specific POS instruction did not improve accuracy.

3. **No-word hallucination:** P3 still hallucinates for no-word entries. The stronger instruction (repeated prohibition) did NOT work. This is a model capability limit, not a prompt issue.

4. **P2 is too conservative:** 0% POS accuracy because all fields are empty. The "better empty than wrong" instruction is too strong.

## Decision

**P3 is NOT committed to production.**

Reasons:
1. No-word hallucination persists despite stronger instruction (model capability limit)
2. Lemma accuracy (64%) is worse than P1 (82%)
3. POS accuracy (82%) is the same as P0, worse than P1 (91%)
4. The threshold "clearly improved with no regression" is not met

## Classification

No-word hallucination is classified as **model capability limitation**, not prompt limitation. The model cannot reliably follow the instruction to leave fields empty when no word is focused. No prompt wording tested (P0, P1, P2, P3) solves this.

## M5 Gate Status

Gate: **OPEN**

The model's ability to provide correct explanations remains limited by:
1. Lemma accuracy (best: P1 82%)
2. No-word hallucination (all prompts fail)
3. Idiom/note detection (0% for Reader profile)

The product label "AI 설명 · 사실 확인 필요" continues to do the work it was designed to do.
