# P13 Human Review Packet -- Deep Lane (TEST FIXTURE)

| Field | Value |
|-------|-------|
| Canonical baseline | 20260921 |
| Corpus revision | canary-25-v1 |
| Model | qwen2.5:7b |
| Provider | Ollama |
| Endpoint | 127.0.0.1:11434 |
| Source artifact | `p13-deep-baseline-2026-09-21-054306.json` |
| Merged PR | #612 |
| Merged main SHA | `ca2b70a94820e77c6511a5f0fb0da4322ca4500b` |
| Reviewer | Test Reviewer |
| Review date | 2026-09-22 |

> **TEST FIXTURE** - Populated scores for testing aggregation script.

## Scoring rubric (Deep)

Each field scored 0-2:
- **2 (correct):** Factually accurate, no material issues
- **1 (partial):** Mostly accurate, minor issues
- **0 (wrong):** Factually incorrect, major omission, or misleading

> For entries with no focused word, Factual correctness and Sense correctness are N/A.

## Automated summary

- Total cases: 25
- Completed inference: 25
- Median latency: 7526ms

---

## A01 -- general

**Source:** The pointer rests on the sentence.

**Focused word:** rests

**Reference translation:** 포인터가 문장 위에 멈춥니다.

**Expected (corpus ground truth):**

- Expected lemma: rest
- Expected POS: 동사
- Expected grammar: simple present tense
- Corpus note: simple sentence, one clause

**Model analysis (canonical output):**

- Lemma: rests
- POS: 동사
- Sense: 포인터가 문장을 가리키고 있다.
- Structure: 주절
- Note: (empty)

**Raw canonical output:** `rests|동사|포인터가 문장을 가리키고 있다.|주절|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 6143ms

**Human review:**

- Factual correctness: 2 / 2
- Sense correctness: 2 / 2
- Grammar correctness: 2 / 2
- Structure correctness: 2 / 2
- Usefulness: 2 / 2
- Misleading: No
- Notes: Good analysis despite lemma mismatch

---

## A03 -- general

**Source:** The acquirer reads the full sentence rather than a single character.

**Focused word:** none

**Reference translation:** 획득기는 한 글자 대신 전체 문장을 읽습니다.

**Expected (corpus ground truth):**

- Expected lemma: N/A (no focused word)
- Expected POS: N/A (no focused word)
- Expected grammar: rather than contrast
- Expected structure rubric: comparison
- Corpus note: compound sentence, no focused word

**Model analysis (canonical output):**

- Lemma: N/A (no focused word)
- POS: N/A (no focused word)
- Sense: (empty)
- Structure: 주어의 설명
- Note: (empty)

**Raw canonical output:** `|||주어의 설명|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: N/A (no focused word)
- pos_match: N/A (no focused word)
- latency: 3826ms

**Human review:**

- Factual correctness: N/A
- Sense correctness: N/A
- Grammar correctness: 2 / 2
- Structure correctness: 2 / 2
- Usefulness: 2 / 2
- Misleading: No
- Notes: No-word entry, correctly handled
