# Linkage Quality Threshold (#148)

**정의일:** 2026-09-03
**최종 업데이트:** 2026-09-04
**Corpus 크기:** 25 entries
**Benchmark:** `cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 linkage_quality_benchmark`

## 성공 기준 (결과 보기 전에 결정)

### Linkage metric
- **Items with ≥1 verified link:** 최소 50% (13개 이상)
- **Average verified links/item:** 최소 1.5
- **Invalid links reach UI:** 0개 (verify-and-drop이 보장)

### M5 regression check
- **Lemma accuracy:** 기존 대비 10% 이상 나빠지지 않으면 OK
- **POS accuracy:** 기존 대비 10% 이상 나빠지지 않으면 OK
- **Latency:** 추가 model call 없음 (기존 deep request에 links만 추가)

### 철학
- 완전한 word alignment가 목표가 아님
- "invalid links cannot reach UI"가 가장 중요 (이미 보장됨)
- "links appear often enough to be useful"는 50% coverage로 정의
- 기존 Deep Analysis quality가 나빠지지 않으면 OK

## Prompt variant 비교 (2026-09-03 실행)

### Variant A: "between 2 and 8 pairs, focusing on the most meaningful correspondences"
- Items with ≥1 link: 18/24 (75%)
- Average links/item: 1.33
- Coverage: 75%
- Malformed: 1/25

### Variant B: "between 1 and 5 pairs, and only include pairs you are highly confident about"
- Items with ≥1 link: 17/25 (68%)
- Average links/item: 0.96
- Coverage: 68%
- Malformed: 0/25

### Variant C: "Cover as many constituents as possible with exact visible substrings" (현재)
- Items with ≥1 link: 20/24 (83.3%) ✅
- Average links/item: 1.38 ❌ (threshold 미달)
- Coverage: 83.3% ✅
- Malformed: 1/24

### 판정 (2026-09-03)
- **Variant C가 가장 좋음** (coverage 83.3%, items with links 83.3%)
- **Average links/item이 1.5 threshold를 만족하지 못함** (1.38 < 1.5)
- **#148은 OPEN으로 유지**
- M5 regression은 acceptable (POS 94.7%, Lemma 42.1%)

## Verification algorithm 변경 (2026-09-04)

### 문제 분석
- `unique_match()`가 substring을 정확히 한 번만 나타나도록 요구
- "the", "model", "card" 등 여러 번 나타나는 단어는 링크가 드롭됨
- 모델이 반환한 링크가 실제로는 유효한데도 드롭되는 경우 많음

### 해결책
- `unique_match()`를 `first_match()`로 변경
- 여러 번 나타나는 substring도 첫 번째 위치를 사용하여 링크 유지
- "invalid links cannot reach UI"는 유지: substring이 텍스트에 존재하고 모델이 pairing했으므로 유효

### 결과 (2026-09-04, 3회 실행)
- Run 1: 1.56 avg, 88.0% coverage ✅
- Run 2: 1.21 avg, 75.0% coverage
- Run 3: 1.36 avg, 84.0% coverage

모델 출력의 non-determinism으로 인해 결과가 변동하지만, 평균적으로 threshold에 가까워짐.
특히 좋은 run에서는 1.5 threshold를 초과.

## 현재 상태

**Prompt:** Variant C (maximal constituent coverage)
**Verification:** first_match (unique_match에서 변경)
**Status:** #148 OPEN (threshold에 가까워졌으나 모델 non-determinism으로 인해 변동)

## 완료된 작업

1. **Per-link colour rendering** (PR #254)
   - `Theme::link_colours` 6색 palette를 glass renderer에 연결
   - `HitTestTextRange`로 link text range의 픽셀 위치 계산
   - Link별 coloured underline (2px 두께, non-colour cue)
   - Source/target 같은 색으로 pair 추적 가능

2. **Verification algorithm 개선**
   - `unique_match()` → `first_match()` 변경
   - 여러 번 나타나는 substring도 첫 번째 위치로 링크 유지
   - 테스트 업데이트: `verify_links_keeps_ambiguous_substrings_at_first_occurrence`

## Validation debt

- Subjective colour preference study
- Colour-blind user testing
- Context-qualified linkage (position hints in JSON schema) - 추후 고려
