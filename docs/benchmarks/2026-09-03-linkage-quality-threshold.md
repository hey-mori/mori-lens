# Linkage Quality Threshold (#148)

**정의일:** 2026-09-03
**최종 업데이트:** 2026-09-04
**Corpus 크기:** 25 entries
**Benchmark:** `cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 linkage_quality_benchmark`

## 성공 기준 (결과 보기 전에 결정)

### Linkage metric (multi-run protocol)
- **Mean avg links/item over 5 runs:** >= 1.5
- **At least 4/5 runs >= 1.4**
- **Speculative links reaching UI:** 0 (resolver guarantees this)
- **Malformed rate:** < 10%

### Resolution categories tracked
- **raw:** total proposed by model
- **unique:** resolved by unique match (substring appears exactly once)
- **context_resolved:** resolved via context disambiguation
- **missing:** dropped (substring not found in text)
- **ambiguous:** dropped (multiple matches, no usable context)
- **overlap:** dropped (overlapped a longer link)

### M5 regression check
- **Lemma accuracy:** 기존 대비 10% 이상 나빠지지 않으면 OK
- **POS accuracy:** 기존 대비 10% 이상 나빠지지 않으면 OK
- **Latency:** 추가 model call 없음 (기존 deep request에 links만 추가)

## Verification protocol

### Deterministic resolver

`resolve_with_category()` pure function:
- 0 matches -> None (dropped as missing)
- 1 match -> accept (counted as unique)
- N matches with context narrowing to exactly 1 -> accept (counted as context_resolved)
- N matches without disambiguating context -> None (dropped as ambiguous)

Context verification: the context string itself must appear exactly once in the text, and must contain exactly one occurrence of the needle within its byte range. If the context is also ambiguous, the link is dropped.

### Context-qualified JSON schema

The model now returns `sourceContext` and `targetContext` alongside `source` and `target`. When a substring appears exactly once, the context is an empty string. When it appears multiple times, the context is a longer phrase that contains the substring and appears exactly once.

### Overlap policy

When two verified links overlap in source or target range, the longer one is kept. Equal-length overlaps keep the first (stable order from model output). This prevents the renderer from painting two coloured spans over the same text.

## Correctness invariant

**If Lens cannot prove the exact source occurrence and the exact target occurrence, the link must not reach the renderer.**

- `first_match()` was removed. It arbitrarily assigned colour/link identity to an occurrence the model did not specify.
- `unique_match()` is the default: a substring must appear exactly once.
- `resolve_occurrence()` with context: when a substring appears multiple times, the model provides context to narrow to one.
- When neither unique match nor context disambiguation succeeds, the link is dropped.

## Validation algorithm history

### Phase 1: unique_match (2026-09-03)
- Average links/item: 1.38 (below 1.5 threshold)
- Coverage: 83.3%
- Problem: repeated words like "the", "model", "card" dropped too aggressively

### Phase 2: first_match (2026-09-04, REJECTED)
- Average links/item: ~1.38 (varies 1.2-1.6 due to model non-determinism)
- Correctness violation: arbitrarily assigned occurrence identity
- **This approach was reverted**

### Phase 3: context-qualified protocol (2026-09-04, current)
- Deterministic resolver with context disambiguation
- 5-run benchmark with pre-committed thresholds
- Resolution categories tracked for diagnostics
- Overlap deduplication for renderer safety

## 완료된 작업

1. **Per-link colour rendering** (PR #254)
   - `Theme::link_colours` 6색 palette를 glass renderer에 연결
   - `HitTestTextRange`로 link text range의 픽셀 위치 계산
   - Link별 coloured underline (2px 두께, non-colour cue)
   - Source/target 같은 색으로 pair 추적 가능

2. **Deterministic resolution** (PR #256)
   - `first_match()` 제거, `unique_match()` 복원
   - `RawLink`에 `source_context`, `target_context` 필드 추가
   - JSON schema와 prompt에 context fields 추가
   - `resolve_with_category()` pure function 구현
   - `LinkResolutionStats` 구조체로 resolution 카테고리 추적
   - Overlap deduplication 구현
   - 5-run multi-run benchmark 프로토콜 구현

3. **Accessible pair identity** (PR #257, fix PR pending)
   - `FillRectangle` → `DrawLine` + `ID2D1StrokeStyle` 전환
   - 6개 slot 모두 고유한 dash pattern 보유 (5 built-in + 1 custom)
   - Slot 0~4: solid, dash, dot, dash-dot, dash-dot-dot
   - Slot 5: custom long-dash-dot [6, 2, 1, 2] (built-in 5개로는 6번째 고유 패턴 불가)
   - Source/target 동일 link_idx → 동일 colour + 동일 dash pattern
   - `colour_index = link_idx % 6`, `dash_index = link_idx % 6`

## Remaining work

- Subjective colour preference study
- Colour-blind user testing

## 5-run benchmark 결과 (2026-09-04)

**모델:** qwen2.5:7b (Ollama v0.30.7, 로컬)
**Corpus:** 24 entries (A01~F03)
**소요 시간:** ~1765초 (약 29분)

### Per-run 결과

| Run | Avg links/item | Coverage | Malformed |
|-----|---------------|----------|-----------|
| 1   | 0.96          | 70.8%    | 1 (E03)   |
| 2   | 0.75          | 70.8%    | 1 (E03)   |
| 3   | 0.83          | 66.7%    | 1 (E03)   |
| 4   | 1.00          | 75.0%    | 1 (E03)   |
| 5   | 1.04          | 79.2%    | 1 (C01)   |

### Aggregate

| Metric | Value | Gate | Status |
|--------|-------|------|--------|
| Mean avg links/item | 0.92 | >= 1.5 | **FAIL** |
| Runs >= 1.4 | 0/5 | >= 4/5 | **FAIL** |
| Median | 0.96 | - | - |
| Min | 0.75 | - | - |
| Max | 1.04 | - | - |
| Malformed rate | 4.0% | < 10% | PASS |
| Speculative to UI | 0 | = 0 | PASS |

### Resolution categories (전체 5 runs 합계)

| Category | Count |
|----------|-------|
| Raw proposed | 155 |
| Unique match | 114 |
| Context resolved | 0 |
| Missing dropped | 40 |
| Ambiguous dropped | 0 |
| Overlap dropped | 4 |

### 분석

- **Context resolved = 0**: 모델이 context field에 유용한 값을 넣지 못함. 모델이 substring을 정확히 반환하면 unique match로 해결되고, 못 찾으면 missing으로 drop됨. Context disambiguation path가 실제로 작동한 적 없음.
- **Missing dropped = 40**: 모델이 반환한 substring이 원문에서 실제로 존재하지 않는 경우가 가장 큰 손실 원인. 모델이 번역의 일부를 substring으로 반환하지만 원문의 해당 부분을 정확히 매칭하지 못함.
- **E03 malformed**: 5 runs 중 4 runs에서 malformed. 이 entry는 모델이 JSON schema를 따르지 못하는 경우.
- **Mean 0.92 vs gate 1.5**: 모델 quality가 threshold에 미달. Resolver는 올바르게 동작하고 있으며, speculative link를 drop하는 것이 correctness invariant를 유지함.

### 결론 (baseline)

**IMPLEMENTATION: COMPLETE** (resolver, schema, renderer, dash patterns 모두 정상 동작)
**QUALITY GATE: OPEN** (모델 quality가 threshold 미달)

Threshold는 낮추지 않음. 모델 개선(qwen2.5:7b → 더 큰 모델, prompt 개선, 또는 fine-tuning)이 필요.

## 5-run benchmark 결과 (2026-09-12, prompt 개선 후)

**모델:** qwen2.5:7b (Ollama v0.30.7, 로컬)
**Corpus:** 25 entries (A01~F03)
**변경:** link prompt 개선 (Candidate A)
- "substring" → "word or short phrase" (입자 granularity 명시)
- "Copy character-for-character from the fenced text" (명시적 copy 지시)
- "Never link the entire sentence" (whole-sentence link 금지)
- "at most six words" (길이 제한)
- "Give two to five links when you can" (복수 link 유도)

### Per-run 결과

| Run | Avg links/item | Coverage | Malformed |
|-----|---------------|----------|-----------|
| 1   | 1.76          | 96.0%    | 0         |
| 2   | 1.64          | 92.0%    | 0         |
| 3   | 1.88          | 95.8%    | 1 (E03)   |
| 4   | 1.75          | 95.8%    | 1 (E03)   |
| 5   | 1.83          | 91.7%    | 1 (E03)   |

### Aggregate

| Metric | Value | Gate | Status |
|--------|-------|------|--------|
| Mean avg links/item | 1.77 | >= 1.5 | **PASS** |
| Runs >= 1.4 | 5/5 | >= 4/5 | **PASS** |
| Median | 1.76 | - | - |
| Min | 1.64 | - | - |
| Max | 1.88 | - | - |
| Malformed rate | 2.4% | < 10% | PASS |
| Speculative to UI | 0 | = 0 | PASS |

### Resolution categories (전체 5 runs 합계)

| Category | Count |
|----------|-------|
| Raw proposed | 257 |
| Unique match | 216 |
| Context resolved | 0 |
| Missing dropped | 40 |
| Ambiguous dropped | 1 |
| Overlap dropped | 0 |

### 분석

- **Whole-sentence links 소멸**: baseline에서는 A01, C02, C03, D05 등이 항상 전체 문장을 하나의 link로 반환._prompt 개선 후_ word-level 및 short-phrase link가 일관적으로 나옴.
- **Coverage 75% → 94%**: F02, F03 등 항상 0 links를 받던 entry들도 link를 받기 시작.
- **Mean 0.97 → 1.77**: +82% 개선. 모든 run이 1.4 이상.
- **Context resolved = 0**: 여전히 context disambiguation은 작동하지 않음. 하지만 unique match가 충분히 많아서 gate 통과에 문제없음.
- **Resolver 변경 없음**: correctness invariant 유지. Prompt만 변경.

### 결론

**QUALITY GATE: MET** (#148 CLOSED)
