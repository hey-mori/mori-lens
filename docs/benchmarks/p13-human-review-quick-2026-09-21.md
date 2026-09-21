# P13 Human Review Packet -- Quick Lane

| Field | Value |
|-------|-------|
| Canonical baseline | 20260921 |
| Corpus revision | canary-25-v1 |
| Model | qwen2.5:7b |
| Provider | Ollama |
| Endpoint | 127.0.0.1:11434 |
| Source artifact | `p13-quick-baseline-2026-09-21-054306.json` |
| Merged PR | #612 |
| Merged main SHA | `d3e0e34caaceb1a1208f390f83733fb08a506fae` |
| Reviewer | _________________ |
| Review date | _________________ |

> **Human judgment fields are intentionally blank at generation time.**
> Do not use automated evidence to pre-fill human scores.

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

> The first measured request (A01, 14561ms) includes cold-start cost. Median represents steady-state.

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

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

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

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## A03 -- general

**Source:** The acquirer reads the full sentence rather than a single character.

**Reference translation:** 획득기는 한 글자 대신 전체 문장을 읽습니다.

**Inference status:** completed
**Latency:** 1766ms

**Model output:** 매수자는 전체 문장을 읽습니다. 한 글자만 읽지 않습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 72%
- length_ratio: 0.47

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## A04 -- general

**Source:** A card that the model has already answered is served from cache.

**Focused word:** answered

**Reference translation:** 모델이 이미 답한 카드는 캐시에서 제공됩니다.

**Inference status:** completed
**Latency:** 1672ms

**Model output:** 모델이 이미 답변한 카드는 캐시에서 제공됩니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 77%
- length_ratio: 0.41

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## A05 -- general

**Source:** If the model is unavailable, the card says so.

**Focused word:** unavailable

**Reference translation:** 모델을 사용할 수 없으면 카드가 그렇게 알립니다.

**Inference status:** completed
**Latency:** 3680ms

**Model output:** 모델이 unavailable한 경우, 카드는 vậy 말합니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 38%
- length_ratio: 0.74

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## B01 -- grammar

**Source:** The lease has expired and the model is no longer resident.

**Focused word:** expired

**Reference translation:** 리스가 만료되어 모델이 더 이상 상주하지 않습니다.

**Inference status:** FAILED (`Malformed`)
**Latency:** 1423ms

> No usable translation output. The model returned a response that could not be parsed into a valid translation object.

**Automated evidence:**

- non_empty: False
- target_language_detected: False

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## B02 -- grammar

**Source:** I would rather it failed loudly than passed silently.

**Focused word:** failed

**Reference translation:** 조용히 통과하는 것보다 크게 실패하는 것이 낫겠습니다.

**Inference status:** completed
**Latency:** 2382ms

**Model output:** 보다 좋다. 조용히 통과시킨 것보다는 나쁘다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 72%
- length_ratio: 0.47

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## B03 -- grammar

**Source:** Not only does the model translate, it also explains.

**Reference translation:** 모델은 번역할 뿐만 아니라 설명도 합니다.

**Inference status:** completed
**Latency:** 1334ms

**Model output:** 모델은 번역뿐만 아니라 설명도 합니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 76%
- length_ratio: 0.40

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## B04 -- grammar

**Source:** Placed on the wrong sentence, the card is worse than none.

**Focused word:** Placed

**Reference translation:** 잘못된 문장에 배치되면 카드는 없는 것만 못합니다.

**Inference status:** completed
**Latency:** 1861ms

**Model output:** 잘못된 문장에 카드가 배치되면, 그 카드는 존재하지 않는 것보다 더 나쁘다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 71%
- length_ratio: 0.72

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## B05 -- grammar

**Source:** The reader asked for explaining the sentence.

**Focused word:** explaining

**Reference translation:** 독자가 문장 설명을 요청했습니다.

**Inference status:** completed
**Latency:** 1544ms

**Model output:** 독자는 문장을 설명해달라고 요청했습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 82%
- length_ratio: 0.49

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## C01 -- idiom

**Source:** The output files came back empty.

**Reference translation:** 출력 파일이 비어 있었습니다.

**Inference status:** completed
**Latency:** 1388ms

**Model output:** 출력 파일은 비어 있었습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 75%
- length_ratio: 0.48

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## C02 -- idiom

**Source:** The model is up to date with the latest release.

**Reference translation:** 모델이 최신 릴리스에 맞게 업데이트되어 있습니다.

**Inference status:** completed
**Latency:** 1538ms

**Model output:** 모델은 최신 릴리스에 맞춥니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 76%
- length_ratio: 0.35

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## C03 -- idiom

**Source:** The build is green.

**Focused word:** green

**Reference translation:** 빌드가 성공입니다.

**Inference status:** completed
**Latency:** 1268ms

**Model output:** 빌드는 초록색입니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 82%
- length_ratio: 0.58

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## C04 -- idiom

**Source:** This is a no-op for existing sessions.

**Reference translation:** 기존 세션에 대해 이것은 아무 작업도 하지 않습니다.

**Inference status:** completed
**Latency:** 1519ms

**Model output:** 이것은 기존 세션에 영향을 미치지 않습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 75%
- length_ratio: 0.63

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## D01 -- developer

**Source:** Error: the adapter returned null for a non-optional field.

**Focused word:** adapter

**Reference translation:** 오류: 어댑터가 필수 필드에 null을 반환했습니다.

**Inference status:** completed
**Latency:** 1498ms

**Model output:** 오류: 어댑터가 선택되지 않은 필드에 대해 null을 반환했습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 65%
- length_ratio: 0.64

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## D02 -- developer

**Source:** Returns the nearest text range to a screen point.

**Focused word:** nearest

**Reference translation:** 화면 지점에 가장 가까운 텍스트 범위를 반환합니다.

**Inference status:** completed
**Latency:** 1647ms

**Model output:** 화면 포인트에 가장 가까운 텍스트 범위를 반환합니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 76%
- length_ratio: 0.59

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## D03 -- developer

**Source:** This change removes an assumption the code was built on.

**Focused word:** assumption

**Reference translation:** 이 변경은 코드가 기반으로 했던 가정을 제거합니다.

**Inference status:** completed
**Latency:** 1605ms

**Model output:** 이 변경 사항은 코드가 기반으로 한 가정을 제거합니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 73%
- length_ratio: 0.54

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## D04 -- developer

**Source:** Acquisition is solved and targeting is not.

**Focused word:** targeting

**Reference translation:** 획득은 해결되었고 타기팅은 아직 아닙니다.

**Inference status:** completed
**Latency:** 1552ms

**Model output:** 획득은 해결되었지만, 공격 대상은 아직 해결되지 않았습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 76%
- length_ratio: 0.77

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## D05 -- developer

**Source:** The token is valid but the lease has expired.

**Focused word:** lease

**Reference translation:** 토큰은 유효하지만 리스가 만료되었습니다.

**Inference status:** completed
**Latency:** 1415ms

**Model output:** 토큰은 유효하지만 레이스는 만료되었습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 83%
- length_ratio: 0.51

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## E01 -- exam

**Source:** Were the model to fail, the fallback would engage immediately.

**Focused word:** fail

**Reference translation:** 모델이 실패했다면 폴백이 즉시 작동했을 것입니다.

**Inference status:** completed
**Latency:** 1383ms

**Model output:** 모델이 실패할 경우 대체 시스템은 즉시 작동합니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 75%
- length_ratio: 0.45

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## E02 -- exam

**Source:** The data, having been processed, was written to the output.

**Focused word:** processed

**Reference translation:** 데이터는 처리된 후 출력에 기록되었습니다.

**Inference status:** completed
**Latency:** 1238ms

**Model output:** 데이터가 처리된 후 출력에 씌여졌습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 77%
- length_ratio: 0.37

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## E03 -- exam

**Source:** So quickly did the model respond that nobody noticed the latency.

**Reference translation:** 모델이 너무나 빨리 응답해서 아무도 지연을 눈치채지 못했습니다.

**Inference status:** completed
**Latency:** 1467ms

**Model output:** 모델이 그렇게 빨리 응답하여 지연 시간을 알아차릴 수 없었습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 75%
- length_ratio: 0.55

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## F01 -- edge

**Source:** vacuously

**Focused word:** vacuously

**Reference translation:** 공허하게

**Inference status:** completed
**Latency:** 835ms

**Model output:** 허상적으로

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 100%
- length_ratio: 0.56

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## F02 -- edge

**Source:** The Workloom dashboard shows one row per session.

**Focused word:** Workloom

**Reference translation:** Workloom 대시보드는 세션당 한 줄을 표시합니다.

**Inference status:** completed
**Latency:** 1383ms

**Model output:** 워크 lynx 대시보드는 한 줄씩 세션을 표시합니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 62%
- length_ratio: 0.59

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## F03 -- edge

**Source:** The timeout is 404ms, which exceeds the 200ms budget.

**Focused word:** timeout

**Reference translation:** 타임아웃은 404ms로, 200ms 예산을 초과합니다.

**Inference status:** completed
**Latency:** 1709ms

**Model output:** 타임아웃은 404밀리세컨드로, 200밀리세컨드 예산을 초과했습니다.

**Automated evidence:**

- non_empty: True
- target_language_detected: True
- target_script_ratio: 68%
- length_ratio: 0.70

**Human review:**

- Meaning preserved: __ / 2
- Naturalness: __ / 2
- Completeness: __ / 2
- Notes:

---

## Summary (to be filled after review)

| Metric | Value |
|--------|-------|
| Meaning preservation avg | __ / 2 |
| Naturalness avg | __ / 2 |
| Completeness avg | __ / 2 |
| Cases reviewed | __ / 25 |
| Inference failures | 1 (B01: Malformed) |

> B01 remains part of the canonical review population. The reviewer should apply the rubric to the observed user-facing result. Do not silently remove it from product-quality conclusions.