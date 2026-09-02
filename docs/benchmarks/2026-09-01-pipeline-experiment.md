# Pipeline-Level Hallucination Experiment

Date: 2026-09-01

## Experiment Design

Compare two configurations with P0 prompt fixed:

### Baseline (current)
```text
card → DeepRequest(word: None or Some) → P0 prompt → model
```

### Gate (proposed)
```text
card → eligibility check
  ↓
  if valid word:
    DeepRequest(word: Some) → P0 prompt → model
  else:
    no DeepRequest (or sentence-only analysis)
```

## Expected Results (based on Phase 13 data)

### No-word hallucination

**Baseline:** C01, C02, E03 all hallucinate lemma/pos/sense (Phase 13 P0/P1/P2/P3 all failed)

**Gate:** C01, C02, E03 don't trigger word-field analysis → 0 hallucination

### Misleading rate

**Baseline:** 6/25 misleading (A04, B03, C01, C02, E02, E03)

**Gate:** 4 pipeline-preventable (B03, C01, C02, E03) + 2 model-dependent (A04, E02)
→ Misleading rate drops from 24% to 8% (2/25)

### Deep calls

**Baseline:** 25 entries × 3 runs = 75 calls

**Gate:** 19 valid-word entries × 3 runs = 57 calls (24% reduction)

### Latency

**Baseline:** ~3.2s per call (P0 warm)

**Gate:** 19 calls instead of 25 → 24% fewer calls → faster overall

## Simulation Method

Since production code is not changed yet, simulate by:
1. Running `corpus_structural_invariants` on all 25 entries (baseline)
2. Filtering results by `eligibility_class(entry) == "valid-word"` (gate)
3. Comparing metrics

## Success Criteria

Gate is successful if:
1. No-word hallucination = 0 (C01, C02, E03 not called)
2. Misleading rate < 10% (2/25 = 8% ✓)
3. No regression on valid-word entries (A04, E02 still analysed)

## Product Behavior (Gate = false)

When gate is false (no word), what does the user see?

**Option A: No card**
- User presses Ctrl+Alt+D, nothing happens
- **Rejected:** silent failure, user doesn't know why

**Option B: Card saying "No word selected"**
- User presses Ctrl+Alt+D, card says "선택된 단어가 없습니다"
- **Acceptable:** clear feedback

**Option C: Sentence-only analysis**
- User presses Ctrl+Alt+D, card shows structure/note only (no lemma/pos/sense)
- **Acceptable:** provides value without hallucination risk

**Recommendation:** Option C (sentence-only analysis) if the model can do it without hallucinating word fields. Otherwise Option B.

## Next Steps

1. Verify model can do sentence-only analysis without hallucinating word fields
2. If yes → implement Option C
3. If no → implement Option B
4. Add regression test: no-word → no word-field analysis
