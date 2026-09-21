# P13 Human Review Packet -- Deep Lane

| Field | Value |
|-------|-------|
| Canonical baseline | 2026-09-21-054306 |
| Corpus revision | canary-25-v1 |
| Model | qwen2.5:7b |
| Provider | Ollama |
| Endpoint | 127.0.0.1:11434 |
| Source artifact | `p13-deep-baseline-2026-09-21-054306.json` |
| Merged PR | #612 |
| Merged main SHA | `d3e0e34caaceb1a1208f390f83733fb08a506fae` |
| Reviewer | _________________ |
| Review date | _________________ |

> **Human judgment fields are intentionally blank at generation time.**
> Do not use automated evidence to pre-fill human scores.
> A lemma exact-match failure does not automatically mean the whole Deep explanation deserves a zero.

## Scoring rubric (Deep)

Each field scored 0-2:
- **2 (correct):** Factually accurate, no material issues
- **1 (partial):** Mostly accurate, minor issues
- **0 (wrong):** Factually incorrect, major omission, or misleading

## Automated summary

- Total cases: 25
- Completed inference: 25 (zero inference failures)
- Median latency: 7526ms
- non_empty: 100%, schema_valid: 100%, target_language_detected: 100%
- structure_present: 96% (24/25)
- lemma_match: 42.1% (8/19 eligible)
- pos_match: 84.2% (16/19 eligible)

---

## A01 -- general

**Source:** The pointer rests on the sentence.

**Focused word:** rests

**Reference translation:** 포인터가 문장 위에 멈추니다.

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

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## A02 -- general

**Source:** The card appears when the pointer settles, which takes a moment.

**Focused word:** settles

**Reference translation:** 포인터가 안정되면 카드가 나타나는데, 잠시 걸립니다.

**Expected (corpus ground truth):**

- Expected lemma: settle
- Expected POS: 동사
- Expected grammar: complex sentence with temporal clause
- Expected structure rubric: subordinate clause
- Corpus note: two clauses: temporal + relative

**Model analysis (canonical output):**

- Lemma: settles
- POS: 동사
- Sense: 포인터가 안정되다
- Structure: 주절
- Note: (empty)

**Raw canonical output:** `settles|동사|포인터가 안정되다|주절|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 8278ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

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

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## A04 -- general

**Source:** A card that the model has already answered is served from cache.

**Focused word:** answered

**Reference translation:** 모델이 이미 답한 카드는 캐시에서 제공됩니다.

**Expected (corpus ground truth):**

- Expected lemma: answer
- Expected POS: 동사
- Expected grammar: present perfect in relative clause
- Expected structure rubric: relative clause
- Corpus note: passive main clause with relative clause

**Model analysis (canonical output):**

- Lemma: answered
- POS: (empty)
- Sense: 모델이 이미 답변한
- Structure: 주어 + 동사被动语态结构
- Note: (empty)

**Raw canonical output:** `answered||모델이 이미 답변한|주어 + 동사被动语态结构|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: FAIL
- latency: 8668ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## A05 -- general

**Source:** If the model is unavailable, the card says so.

**Focused word:** unavailable

**Reference translation:** 모델을 사용할 수 없으면 카드가 그렇게 알립니다.

**Expected (corpus ground truth):**

- Expected lemma: unavailable
- Expected POS: 형용사
- Expected grammar: zero conditional
- Expected structure rubric: conditional
- Corpus note: if-then structure

**Model analysis (canonical output):**

- Lemma: 불可用
- POS: 형용사
- Sense: 사용할 수 없는 상태
- Structure: 만약 모델이 사용할 수 없으면, 카드는 그 사실을 알려줍니다.
- Note: (empty)

**Raw canonical output:** `불可用|형용사|사용할 수 없는 상태|만약 모델이 사용할 수 없으면, 카드는 그 사실을 알려줍니다.|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 7526ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## B01 -- grammar

**Source:** The lease has expired and the model is no longer resident.

**Focused word:** expired

**Reference translation:** 리스가 만료되어 모델이 더 이상 상주하지 않습니다.

**Expected (corpus ground truth):**

- Expected lemma: expire
- Expected POS: 동사
- Expected grammar: present perfect
- Expected structure rubric: compound
- Corpus note: present perfect with 'and' coordination

**Model analysis (canonical output):**

- Lemma: expired
- POS: 동사
- Sense: 렌트 계약이 만료되었다.
- Structure: 서두르는 동사와 현재 분사가 연결되어 있는 복합 구조입니다.
- Note: (empty)

**Raw canonical output:** `expired|동사|렌트 계약이 만료되었다.|서두르는 동사와 현재 분사가 연결되어 있는 복합 구조입니다.|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 6772ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## B02 -- grammar

**Source:** I would rather it failed loudly than passed silently.

**Focused word:** failed

**Reference translation:** 조용히 통과하는 것보다 크게 실패하는 것이 낫겠습니다.

**Expected (corpus ground truth):**

- Expected lemma: fail
- Expected POS: 동사
- Expected grammar: subjunctive with would rather
- Expected structure rubric: comparison
- Corpus note: subjunctive mood after 'would rather'

**Model analysis (canonical output):**

- Lemma: 실패
- POS: 동사
- Sense: 명확하게 실패하는 것 대신 조용히 통과하는 것을 선호합니다.
- Structure: 비교절: '보다'로 연결된 두 동사 구조
- Note: (empty)

**Raw canonical output:** `실패|동사|명확하게 실패하는 것 대신 조용히 통과하는 것을 선호합니다.|비교절: '보다'로 연결된 두 동사 구조|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 7581ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## B03 -- grammar

**Source:** Not only does the model translate, it also explains.

**Focused word:** none

**Reference translation:** 모델은 번역할 뿐만 아니라 설명도 합니다.

**Expected (corpus ground truth):**

- Expected lemma: N/A (no focused word)
- Expected POS: N/A (no focused word)
- Expected grammar: negative inversion with 'not only'
- Expected structure rubric: inversion
- Corpus note: fronted negative triggers subject-auxiliary inversion

**Model analysis (canonical output):**

- Lemma: N/A (no focused word)
- POS: N/A (no focused word)
- Sense: (empty)
- Structure: 복합명사 + 부사 + 동사 + 부사 + 동사
- Note: (empty)

**Raw canonical output:** `|||복합명사 + 부사 + 동사 + 부사 + 동사|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: N/A (no focused word)
- pos_match: N/A (no focused word)
- latency: 4514ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## B04 -- grammar

**Source:** Placed on the wrong sentence, the card is worse than none.

**Focused word:** Placed

**Reference translation:** 잘못된 문장에 배치되면 카드는 없는 것만 못합니다.

**Expected (corpus ground truth):**

- Expected lemma: place
- Expected POS: 분사
- Expected grammar: past participle as reduced adverbial
- Expected structure rubric: reduced clause
- Corpus note: reduced adverbial clause

**Model analysis (canonical output):**

- Lemma: Placed
- POS: 분사
- Sense: 잘못 배치되었을 때
- Structure: 주어 + 부정부사 + 보다 + 대명사
- Note: (empty)

**Raw canonical output:** `Placed|분사|잘못 배치되었을 때|주어 + 부정부사 + 보다 + 대명사|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 7839ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## B05 -- grammar

**Source:** The reader asked for explaining the sentence.

**Focused word:** explaining

**Reference translation:** 독자가 문장 설명을 요청했습니다.

**Expected (corpus ground truth):**

- Expected lemma: explain
- Expected POS: 동명사
- Expected grammar: gerund as object of preposition
- Corpus note: gerund after preposition 'for'

**Model analysis (canonical output):**

- Lemma: explaining
- POS: 동사
- Sense: 해당 문장을 설명하는 것
- Structure: 주절
- Note: (empty)

**Raw canonical output:** `explaining|동사|해당 문장을 설명하는 것|주절|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: FAIL
- latency: 7175ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## C01 -- idiom

**Source:** The output files came back empty.

**Focused word:** none

**Reference translation:** 출력 파일이 비어 있었습니다.

**Expected (corpus ground truth):**

- Expected lemma: N/A (no focused word)
- Expected POS: N/A (no focused word)
- Expected grammar: phrasal verb 'come back'
- Expected idiom/register note: yes
- Corpus note: phrasal verb: come back = return (with result)

**Model analysis (canonical output):**

- Lemma: N/A (no focused word)
- POS: N/A (no focused word)
- Sense: (empty)
- Structure: 주절
- Note: (empty)

**Raw canonical output:** `|||주절|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: N/A (no focused word)
- pos_match: N/A (no focused word)
- latency: 5252ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## C02 -- idiom

**Source:** The model is up to date with the latest release.

**Focused word:** none

**Reference translation:** 모델이 최신 릴리즈에 맞게 업데이트되어 있습니다.

**Expected (corpus ground truth):**

- Expected lemma: N/A (no focused word)
- Expected POS: N/A (no focused word)
- Expected grammar: adjective idiom 'up to date'
- Expected idiom/register note: yes
- Corpus note: compound adjective: up to date = current

**Model analysis (canonical output):**

- Lemma: N/A (no focused word)
- POS: N/A (no focused word)
- Sense: (empty)
- Structure: 주어+동사구
- Note: (empty)

**Raw canonical output:** `|||주어+동사구|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: N/A (no focused word)
- pos_match: N/A (no focused word)
- latency: 8400ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## C03 -- idiom

**Source:** The build is green.

**Focused word:** green

**Reference translation:** 빌드가 성공입니다.

**Expected (corpus ground truth):**

- Expected lemma: green
- Expected POS: 형용사
- Expected grammar: domain-specific adjective use
- Expected idiom/register note: yes
- Corpus note: CI/CD jargon: green = all tests passed

**Model analysis (canonical output):**

- Lemma: green
- POS: 형용사
- Sense: 빌드가 성공했음을 나타내는 색상 코드입니다.
- Structure: 단일 문장: 주어 + 동사
- Note: (empty)

**Raw canonical output:** `green|형용사|빌드가 성공했음을 나타내는 색상 코드입니다.|단일 문장: 주어 + 동사|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 8589ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## C04 -- idiom

**Source:** This is a no-op for existing sessions.

**Focused word:** none

**Reference translation:** 기존 세션에 대해 이것은 아무 작업도 하지 않습니다.

**Expected (corpus ground truth):**

- Expected lemma: N/A (no focused word)
- Expected POS: N/A (no focused word)
- Expected grammar: technical compound noun
- Expected idiom/register note: yes
- Corpus note: tech jargon: no-op = no operation

**Model analysis (canonical output):**

- Lemma: N/A (no focused word)
- POS: N/A (no focused word)
- Sense: (empty)
- Structure: 이 문장은 '기존 세션'에 대해 설명하며, '이것은 아무 작업도 하지 않습니다'라는 의미를 전달합니다.
- Note: (empty)

**Raw canonical output:** `|||이 문장은 '기존 세션'에 대해 설명하며, '이것은 아무 작업도 하지 않습니다'라는 의미를 전달합니다.|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: N/A (no focused word)
- pos_match: N/A (no focused word)
- latency: 5275ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## D01 -- developer

**Source:** Error: the adapter returned null for a non-optional field.

**Focused word:** adapter

**Reference translation:** 오류: 어댑터가 필수 필드에 null을 반환했습니다.

**Expected (corpus ground truth):**

- Expected lemma: adapter
- Expected POS: 명사
- Expected grammar: error message structure
- Corpus note: developer context: adapter pattern

**Model analysis (canonical output):**

- Lemma: adapter
- POS: 명사
- Sense: 오류 메시지에서 사용된 어댑터
- Structure: 주어 + 목적어
- Note: (empty)

**Raw canonical output:** `adapter|명사|오류 메시지에서 사용된 어댑터|주어 + 목적어|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 7331ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## D02 -- developer

**Source:** Returns the nearest text range to a screen point.

**Focused word:** nearest

**Reference translation:** 화면 지점에 가장 가까운 텍스트 범위를 반환합니다.

**Expected (corpus ground truth):**

- Expected lemma: near
- Expected POS: 형용사
- Expected grammar: superlative adjective
- Corpus note: API documentation style

**Model analysis (canonical output):**

- Lemma: nearest
- POS: 형용사
- Sense: 화면의 특정 위치에서 가장 가까이 있는 텍스트 범위를 의미합니다.
- Structure: 단일 명사절: 'nearest text range'
- Note: (empty)

**Raw canonical output:** `nearest|형용사|화면의 특정 위치에서 가장 가까이 있는 텍스트 범위를 의미합니다.|단일 명사절: 'nearest text range'|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 8951ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## D03 -- developer

**Source:** This change removes an assumption the code was built on.

**Focused word:** assumption

**Reference translation:** 이 변경은 코드가 기반으로 했던 가정을 제거합니다.

**Expected (corpus ground truth):**

- Expected lemma: assumption
- Expected POS: 명사
- Expected grammar: reduced relative clause
- Expected structure rubric: relative clause
- Corpus note: code review context, reduced relative 'the code was built on'

**Model analysis (canonical output):**

- Lemma: assumption
- POS: 명사
- Sense: 코드가 설계될 때 기초로 삼았던 전제
- Structure: 이 문장은 동사 'removes'와 연결詞汇，使得句子更为通顺：这个变化移除了代码所依赖的前提。
- Note: (empty)

**Raw canonical output:** `assumption|명사|코드가 설계될 때 기초로 삼았던 전제|이 문장은 동사 'removes'와 연결詞汇，使得句子更为通顺：这个变化移除了代码所依赖的前提。|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 8317ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## D04 -- developer

**Source:** Acquisition is solved and targeting is not.

**Focused word:** targeting

**Reference translation:** 획득은 해결되었고 타기팅은 아직 아닙니다.

**Expected (corpus ground truth):**

- Expected lemma: targeting
- Expected POS: 명사
- Expected grammar: parallel structure with ellipsis
- Expected structure rubric: ellipsis
- Corpus note: architecture discussion, gapped coordination

**Model analysis (canonical output):**

- Lemma: targeting
- POS: 명사
- Sense: 목표를 정하는 과정; 공격 대상 선택
- Structure: 두 개의 부정문이 연결되어 있으며, 각각의 부정문은 별개의 개념을 나타냅니다.
- Note: (empty)

**Raw canonical output:** `targeting|명사|목표를 정하는 과정; 공격 대상 선택|두 개의 부정문이 연결되어 있으며, 각각의 부정문은 별개의 개념을 나타냅니다.|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 8499ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## D05 -- developer

**Source:** The token is valid but the lease has expired.

**Focused word:** lease

**Reference translation:** 토큰은 유효하지만 리스가 만료되었습니다.

**Expected (corpus ground truth):**

- Expected lemma: lease
- Expected POS: 명사
- Expected grammar: present perfect with contrast
- Expected structure rubric: contrast
- Corpus note: debugging context: 'lease' is a model retention term

**Model analysis (canonical output):**

- Lemma: lease
- POS: 명사
- Sense: 임대 계약
- Structure: 주어 + 부정부사 + 동사 + 서술어
- Note: (empty)

**Raw canonical output:** `lease|명사|임대 계약|주어 + 부정부사 + 동사 + 서술어|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 7870ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## E01 -- exam

**Source:** Were the model to fail, the fallback would engage immediately.

**Focused word:** fail

**Reference translation:** 모델이 실패했다면 폴백이 즉시 작동했을 것입니다.

**Expected (corpus ground truth):**

- Expected lemma: fail
- Expected POS: 동사
- Expected grammar: inverted second conditional (were...to)
- Expected structure rubric: inverted conditional
- Corpus note: formal conditional without 'if', common in exam passages

**Model analysis (canonical output):**

- Lemma: fail
- POS: 동사
- Sense: 모델이 실패하는 경우
- Structure: 만약 조건절 + 결과절
- Note: (empty)

**Raw canonical output:** `fail|동사|모델이 실패하는 경우|만약 조건절 + 결과절|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 7039ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## E02 -- exam

**Source:** The data, having been processed, was written to the output.

**Focused word:** processed

**Reference translation:** 데이터는 처리된 후 출력에 기록되었습니다.

**Expected (corpus ground truth):**

- Expected lemma: process
- Expected POS: 분사
- Expected grammar: perfect participle passive (having been + pp)
- Expected structure rubric: perfect participle
- Corpus note: formal perfect participle, common in exam reading

**Model analysis (canonical output):**

- Lemma: processed
- POS: 과거분사
- Sense: 처리되었음
- Structure: 본문은 '데이터'가 '처리된 후'라는 조건을 받고, 그 결과로 '출력에 기록되었습니다.'
- Note: (empty)

**Raw canonical output:** `processed|과거분사|처리되었음|본문은 '데이터'가 '처리된 후'라는 조건을 받고, 그 결과로 '출력에 기록되었습니다.'|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 7401ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## E03 -- exam

**Source:** So quickly did the model respond that nobody noticed the latency.

**Focused word:** none

**Reference translation:** 모델이 너무나 빨리 응답해서 아무도 지연을 눈치찰 못했습니다.

**Expected (corpus ground truth):**

- Expected lemma: N/A (no focused word)
- Expected POS: N/A (no focused word)
- Expected grammar: so...that with fronted adverb and inversion
- Expected structure rubric: so...that inversion
- Corpus note: emphatic inversion pattern, exam favourite

**Model analysis (canonical output):**

- Lemma: N/A (no focused word)
- POS: N/A (no focused word)
- Sense: (empty)
- Structure: 주절
- Note: (empty)

**Raw canonical output:** `|||주절|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: N/A (no focused word)
- pos_match: N/A (no focused word)
- latency: 4832ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## F01 -- edge

**Source:** vacuously

**Focused word:** vacuously

**Reference translation:** 공허하게

**Expected (corpus ground truth):**

- Expected lemma: vacuously
- Expected POS: 부사
- Expected grammar: single adverb, no structure
- Corpus note: single word: structure must be empty, lemma is the word itself

**Model analysis (canonical output):**

- Lemma: vacuously
- POS: 형용사
- Sense: 비어 있고, 내용이 없음을 나타내는 상태로 행동하거나 말하는 것
- Structure: (empty)
- Note: 공허하게는 주로 사람의 태도나 행동을 묘사할 때 사용됩니다.

**Raw canonical output:** `vacuously|형용사|비어 있고, 내용이 없음을 나타내는 상태로 행동하거나 말하는 것||공허하게는 주로 사람의 태도나 행동을 묘사할 때 사용됩니다.`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: False
- lemma_match: PASS
- pos_match: FAIL
- latency: 7433ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## F02 -- edge

**Source:** The Worklynx dashboard shows one row per session.

**Focused word:** Worklynx

**Reference translation:** Worklynx 대시보드는 세션당 한 줄을 표시합니다.

**Expected (corpus ground truth):**

- Expected lemma: Worklynx
- Expected POS: 명사
- Expected grammar: proper noun
- Corpus note: proper noun: lemma should not change the form

**Model analysis (canonical output):**

- Lemma: Worklynx
- POS: 명사
- Sense: 워크라이너스 시스템의 관리자 인터페이스
- Structure: 주어+동사+보어
- Note: (empty)

**Raw canonical output:** `Worklynx|명사|워크라이너스 시스템의 관리자 인터페이스|주어+동사+보어|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: PASS
- pos_match: PASS
- latency: 7620ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## F03 -- edge

**Source:** The timeout is 404ms, which exceeds the 200ms budget.

**Focused word:** timeout

**Reference translation:** 타임아웃은 404ms로, 200ms 예산을 초과합니다.

**Expected (corpus ground truth):**

- Expected lemma: timeout
- Expected POS: 명사
- Expected grammar: numeric expression with relative clause
- Expected structure rubric: relative clause
- Corpus note: numbers and units in technical context

**Model analysis (canonical output):**

- Lemma: 타임아웃
- POS: 명사
- Sense: 실행 시간이 제한된 작업이나 프로세스가 일정 시간 동안 중지되는 기간
- Structure: 주어 + 숫자 + 단위 + , + which + 서술절 + 비교구
- Note: (empty)

**Raw canonical output:** `타임아웃|명사|실행 시간이 제한된 작업이나 프로세스가 일정 시간 동안 중지되는 기간|주어 + 숫자 + 단위 + , + which + 서술절 + 비교구|`

**Automated evidence:**

- non_empty: True
- schema_valid: True
- target_language_detected: True
- structure_present: True
- lemma_match: FAIL
- pos_match: PASS
- latency: 12419ms

**Human review:**

- Factual correctness: __ / 2
- Sense correctness: __ / 2
- Grammar correctness: __ / 2
- Structure correctness: __ / 2
- Usefulness: __ / 2
- Misleading: Yes / No
- Notes:

---

## Summary (to be filled after review)

| Metric | Value |
|--------|-------|
| Factual correctness avg | __ / 2 |
| Sense correctness avg | __ / 2 |
| Grammar correctness avg | __ / 2 |
| Structure correctness avg | __ / 2 |
| Usefulness avg | __ / 2 |
| Misleading count | __ |
| Cases reviewed | __ / 25 |
| Inference failures | 0 |
