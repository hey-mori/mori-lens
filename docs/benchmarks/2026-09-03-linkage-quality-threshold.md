# Linkage Quality Threshold (#148)

**정의일:** 2026-09-03
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

### 판정
- **Variant C가 가장 좋음** (coverage 83.3%, items with links 83.3%)
- **Average links/item이 1.5 threshold를 만족하지 못함** (1.38 < 1.5)
- **#148은 OPEN으로 유지**
- M5 regression은 acceptable (POS 94.7%, Lemma 42.1%)

## 현재 상태

**Prompt:** Variant C (maximal constituent coverage)
**Status:** #148 OPEN (threshold 미달)
**Reason:** Average verified links/item이 1.5 미만 (1.38)

## 다음 단계

1. Prompt engineering 계속 (더 많은 variant 시도)
2. 또는 threshold 조정 검토 (1.5 → 1.3으로 낮추는 것 고려)
3. 또는 rendering 개선 (현재 모든 link에 같은 underline, per-link colour 미구현)

## Validation debt

- Per-link colour system (DirectWrite custom renderer 필요)
- Subjective colour preference study
- Colour-blind user testing
