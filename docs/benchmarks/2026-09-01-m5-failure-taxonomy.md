# Phase 2: Failure Taxonomy

모든 25 corpus entry의 실패를 8가지 기준으로 분류.
한 entry가 여러 failure type에 해당할 수 있음.

## Failure Types

| Code | Name | Meaning |
|------|------|---------|
| F1 | acquisition | 텍스트를 잘못 읽음 (deep eval에서는 N/A) |
| F2 | prompt instruction | 프롬프트 지시가 모호하거나 상충 |
| F3 | schema/parser | 스키마가 잘못된 형태를 허용하거나 파서가 잘못 읽음 |
| F4 | language filtering | 출력에 잘못된 언어가 포함 |
| F5 | model capability | 모델이 이 작업을 수행 불가 |
| F6 | profile behavior | 프로필 지시가 기본 프롬프트와 상충 |
| F7 | evaluation expectation | ground truth가 잘못됨 |
| F8 | latency/cost | 성능 문제 |

## Entry-by-entry Classification

### A01 · "The pointer rests on the sentence." · word: "rests"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | rest | rests | F5 | 모델이 영어 lemmatization을 수행하지 않음. "rests"를 그대로 반환 |

- POS: 동사 ✓
- Structure: "단순 현재 시제의 동사 구조" - 충분하지만 generic
- Overall: Partial

### A02 · "The card appears when the pointer settles, which takes a moment." · word: "settles"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | settle | settles | F5 | A01과 동일 |
| structure | subordinate clause | "주절" | F5 | 복합문 구조를 설명하지 못함. 한 단어만 반환 |

- POS: 동사 ✓
- Overall: Neutral

### A03 · "The acquirer reads the full sentence rather than a single character." · no word

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| structure | comparison | "주어가 있는 무단절 구문" | F5 | "rather than" 대조를 식별하지 못함 |

### A04 · "A card that the model has already answered is served from cache." · word: "answered"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | answer | answered | F5 | lemmatization 실패 |
| pos | 동사 | (empty) | F5 | 모델이 필드를 채우지 않음 |
| structure | relative clause | (empty) | F5 | 관계절 설명 누락 |

- Overall: **Misleading** - 대부분의 필드가 비어있음

### A05 · "If the model is unavailable, the card says so." · word: "unavailable"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | unavailable | 불可用 | F4 | 한/중 혼용 스크립트. lemma 필드가 언어 필터에서 면제되어 카드에 도달 |
| (lemma) | | | F2 | 프롬프트가 "stays in the language it is written in"이라고 하지만 모델이 번역함 |

- POS: 형용사 ✓
- Structure: conditional ✓
- Overall: Partial (lemma만 문제)

### B01 · "The lease has expired and the model is no longer resident." · word: "expired"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | expire | expired | F5 | lemmatization 실패 |
| structure | compound | (empty) | F5 | compound sentence 구조 설명 누락 |

### B02 · "I would rather it failed loudly than passed silently." · word: "failed"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | fail | 실패 | F5 | 영어 lemma 대신 한국어로 번역. "dictionary form" 지시를 번역으로 오해 |
| structure | comparison | (문장 전체 번역) | F5 | 구조 분석 대신 문장 번역 반환 |

- Overall: **Misleading** - 한국어 lemma는 능동적으로 잘못된 정보

### B03 · "Not only does the model translate, it also explains." · no word

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| pos | (n/a, no word) | 동사 | F5 | 단어.focus가 없는데 POS를 생성. 환각 |
| structure | inversion | (generic 설명) | F5 | "Not only...but also" 역전 패턴을 식별하지 못함 |

### B04 · "Placed on the wrong sentence, the card is worse than none." · word: "Placed"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | place | Placed | F5 | lemmatization 실패 (대소문자만 다름) |
| pos | 분사 | 동사 | F5 | 과거분사라는 구체적 형태를 식별하지 못함 |
| structure | reduced clause | "주어+목적어+부정문" | F5 | wrong analysis - reduced adverbial을 식별하지 못함 |

### B05 · "The reader asked for explaining the sentence." · word: "explaining"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | explain | explaining | F5 | lemmatization 실패 |
| pos | 동명사 | 동사 | F5 | 동명사(gerund)라는 구체적 형태를 식별하지 못함 |

### C01 · "The output files came back empty." · no word

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| pos | (n/a, no word) | 명사 | F5 | 단어.focus가 없는데 POS를 생성 |
| note | idiom (come back) | (empty) | F2 | "Most sentences need no note"가 idiom 지시를 억제 |
| structure | phrasal verb | "주절" | F5 | too terse |

### C02 · "The model is up to date with the latest release." · no word

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| pos | (n/a, no word) | 명사 | F5 | 환각 POS |
| sense | up to date 설명 | "모델, 업데이트" | F5 | 명사 나열. 설명이 아님 |
| structure | adjective idiom | "주어+이동사" | F5 | 틀린 분석 |
| note | idiom (up to date) | (empty) | F2 | idiom 지시 억제 |

- Overall: **Incorrect**

### C03 · "The build is green." · word: "green"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| note | jargon (green=success) | (empty) | F2 | idiom/register 지시 억제. 단, sense에서 CI 의미를 correctly 설명 |

- Lemma: "green" ✓, POS: 형용사 ✓, Sense: correct ✓
- Overall: Useful (note가 비어있어도 sense에서 의미 전달)

### C04 · "This is a no-op for existing sessions." · no word

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| note | jargon (no-op) | (empty) | F2 | idiom 지시 억제. (개발자 프로필에서는 채워짐) |

- Structure: good ✓
- Overall: Neutral

### D01 · "Error: the adapter returned null for a non-optional field." · word: "adapter"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| (all fields) | | | | 모든 필드 정확 ✓ |

- Overall: **Useful**

### D02 · "Returns the nearest text range to a screen point." · word: "nearest"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | near | nearest | F5 | 최고급 형용사를 원형으로 환원하지 못함 |

- Overall: Useful

### D03 · "This change removes an assumption the code was built on." · word: "assumption"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| structure | relative clause | (문장 의역) | F5 | 구조 분석 대신 의역 반환 |

- Lemma, POS, Sense 모두 정확 ✓
- Overall: Useful

### D04 · "Acquisition is solved and targeting is not." · word: "targeting"

- 모든 필드 정확 ✓
- Overall: **Useful**

### D05 · "The token is valid but the lease has expired." · word: "lease"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| sense | model retention period | "임대 계약이 만료되었다" | F5 | 도메인 특화 의미를 모름. "lease"를 일반 영어로 해석 |

- Overall: Partial

### E01 · "Were the model to fail, the fallback would engage immediately." · word: "fail"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| structure | inverted conditional | (empty) | F5 | 복잡한 문법 패턴에서 구조 설명 누락 |

- Lemma: "fail" ✓, POS: 동사 ✓, Sense: good ✓
- Overall: Partial

### E02 · "The data, having been processed, was written to the output." · word: "processed"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | process | processed | F5 | lemmatization 실패 |
| pos | 분사 | 과거 시제 동사 | F5 | 완료분사(perfect participle)를 식별하지 못함 |

### E03 · "So quickly did the model respond that nobody noticed the latency." · no word

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| pos | (n/a, no word) | 형용사 | F5 | 환각 POS |
| sense | so...that 설명 | "빠르게" | F5 | 한 단어. 설명이 아님 |
| structure | so...that inversion | (empty) | F5 | 복잡한 문법 패턴에서 구조 설명 누락 |

- Overall: **Incorrect**

### F01 · "vacuously" · word: "vacuously"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| pos | 부사 | 형용사 | F5 | 부사를 형용사로 잘못 분류 |

- Lemma: "vacuously" ✓, Sense: correct ✓

### F02 · "The Workloom dashboard shows one row per session." · word: "Workloom"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| sense | proper noun | "워크룸 시스템의 관리자 도구" | F5 | 알려지지 않은 고유명사의 의미를 환각 |
| structure | (something) | (empty) | F5 | multi-word 문장에서 구조 누락 |

- Overall: Neutral (고유명사 처리로는 합리적)

### F03 · "The timeout is 404ms, which exceeds the 200ms budget." · word: "timeout"

| field | expected | actual | type | cause |
|-------|----------|--------|------|-------|
| lemma | timeout | 타임아웃 | F5 | 영어 lemma 대신 한국어 음역 반환 |

## Summary by Failure Type

| Type | Count | Affected entries |
|------|-------|-----------------|
| F2 prompt instruction | 7 | A05, B02, C01, C02, C03, C04, F03 |
| F4 language filtering | 1 | A05 |
| F5 model capability | 32 | A01-A05, B01-B05, C01-C03, D02, D03, D05, E02, E03, F01, F02, F03 |
| F7 evaluation expectation | 0 | - |

### F5 Sub-categories

| Sub-type | Count | Entries |
|----------|-------|---------|
| Lemma not lemmatized (inflected form echoed) | 8 | A01, A02, A04, B01, B04, B05, D02, E02 |
| Lemma translated to Korean | 2 | B02, F03 |
| Lemma mixed script | 1 | A05 |
| POS empty when expected | 1 | A04 |
| POS wrong classification | 1 | F01 |
| POS spurious (no word focused) | 4 | B03, C01, C02, E03 |
| Structure empty when expected | 5 | A04, B01, E01, E03, F02 |
| Structure wrong or paraphrase | 6 | A02, B02, B04, C01, C02, D03 |
| Sense wrong or unhelpful | 4 | C02, D05, E03, F02 |

## Key Findings

### 1. F5 (model capability)가 압도적

32건으로 전체 실패의 대다수. 특히:
- **Lemmatisation 불가**: 11건 (inflected form echo 8 + translation 2 + mixed 1). 모델이 영어 단어의 사전형을 추출하지 못함.
- **Structure empty/wrong**: 11건. 특히 exam 문법(E01, E03)에서 구조 설명이 완전히 누락.

### 2. F2 (prompt instruction)가 idiom에서 체계적 실패

C01-C04에서 note가 모두 비어있음 (0/12 runs).
원인: prompt line 116의 두 지시가 상충:
- "give one idiom, register or false-friend worth knowing"
- "Most sentences need no note and an empty string is the expected answer"

두 번째 지시가 첫 번째를 완전히 억제. 모델은 "empty string is the expected answer"를 듣고 note를 거의 항상 비움.

### 3. F4 (language filtering)는 1건

lemma 필드가 언어 필터에서 면제되어서 "불可用"이 카드에 도달.
이는 설계상 의도(lemma는 영어여야 함)이지만, 모델이 영어 대신 혼합 스크립트를 반환하면 필터가 잡지 못함.

### 4. F3, F7 제로

schema/parser 실패 없음. evaluation expectation도 모두 정확.
파이프라인은 정상. 문제는 전적으로 모델과 프롬프트.
