# P13 Human Review Packet -- Quick Lane (TEST FIXTURE)

| Field | Value |
|-------|-------|
| Canonical baseline | 20260921 |
| Corpus revision | canary-25-v1 |
| Model | qwen2.5:7b |
| Provider | Ollama |
| Endpoint | 127.0.0.1:11434 |
| Source artifact | `p13-quick-baseline-2026-09-21-054306.json` |
| Merged PR | #612 |
| Merged main SHA | `ca2b70a94820e77c6511a5f0fb0da4322ca4500b` |
| Reviewer | Test Reviewer |
| Review date | 2026-09-22 |

> **TEST FIXTURE** - Populated scores for testing aggregation script.

## Scoring rubric (Quick)

- **2 (correct):** Meaning fully preserved, natural Korean, no material issues
- **1 (partial):** Meaning mostly preserved, minor issues
- **0 (wrong):** Meaning lost, major omission/addition, or factually incorrect

## Automated summary

- Total cases: 25
- Completed inference: 24
- Inference failure: B01 `Malformed`
- Median latency: 1538ms
- non_empty: 96%
- target_language_detected: 96%

---

## A01 -- general

**Source:** The pointer rests on the sentence.

**Focused word:** rests

**Reference translation:** 포인터가 문장 위에 멈춥니다.

**Inference status:** completed
**Latency:** 14561ms

**Model output:** 포인터는 문장 위에 있습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 75%
- length_ratio: 0.47

**Human review:**

- Meaning preserved: 2 / 2
- Naturalness: 2 / 2
- Completeness: 2 / 2
- Notes: Good translation

---

## A02 -- general

**Source:** The card appears when the pointer settles, which takes a moment.

**Focused word:** settles

**Reference translation:** 포인터가 안정되면 카드가 나타나는데, 잠시 걸립니다.

**Inference status:** completed
**Latency:** 2335ms

**Model output:** 포인터가 정착할 때 카드가 나타나는데, 이 과정은 몇 초간 지속됩니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 72%
- length_ratio: 0.61

**Human review:**

- Meaning preserved: 1 / 2
- Naturalness: 1 / 2
- Completeness: 2 / 2
- Notes: Minor naturalness issues

---

## B01 -- grammar

**Source:** The model will output a card that is not relevant to the sentence.

**Focused word:** output

**Reference translation:** 모델은 문장과 관련 없는 카드를 출력합니다.

**Inference status:** failed
**Latency:** 0ms

**Model output:** (none)

**Automated evidence:**

- error_type: Malformed
- non_empty: False
- target_language_detected: False

**Human review:**

- Meaning preserved: 0 / 2
- Naturalness: 0 / 2
- Completeness: 0 / 2
- Notes: Inference failed
