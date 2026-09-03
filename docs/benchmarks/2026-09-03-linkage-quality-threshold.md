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

## 실패 시 조치
1. 결과가 threshold 아래이면 prompt variant 2-3개 시도
2. 그래도 부족하면 #148을 OPEN으로 두고 exact numbers를 문서화
3. 수동 colour preference validation은 validation debt로 명시
