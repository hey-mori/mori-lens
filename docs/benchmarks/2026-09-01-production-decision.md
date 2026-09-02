# Production Change Decision

Date: 2026-09-01
Evaluation period: 2026-09-01
Model: qwen2.5:7b (primary), translategemma:latest (secondary)
**Status: SUPERSEDED by [`P1 full corpus validation`](2026-09-01-p1-full-corpus-validation.md). P1 was NOT committed.**

## Status

**This decision is SUPERSEDED.** P1 was tested on the full 25-entry corpus and
found to cause POS regression (-8pp) and not reduce misleading rate (24%).

The original "P1 dominates P0" claim was based on an 8-entry subset and a
misleading rate comparison that is NOT directly comparable (P0 used a more
lenient rubric). See [`2026-09-01-evaluation-integrity-audit.md`]
(2026-09-01-evaluation-integrity-audit.md).

P1 was NOT committed. The gate remains OPEN.

### Phase 1: Evaluation harness verification
- Lemma scoring: exact case-insensitive match (not prefix)
- Idiom detection: IS part of production contract (prompt line 116)
- Structure scoring: keyword-based, not exact match

### Phase 2: Failure taxonomy
- 32 instances of F5 (model capability): dominant failure
- 7 instances of F2 (prompt instruction): mainly idiom suppression
- 1 instance of F4 (language filtering): A05 lemma language leak
- 0 instances of F3, F7: pipeline is correct

### Phase 3: Model comparison
- qwen2.5:7b and translategemma:latest have identical 42% lemma accuracy
- Suggests fundamental small-model limitation, not model-specific issue
- translategemma:latest is slower (4.5s vs 3.2s)

### Phase 4: Prompt A/B
- P0 (production): 29% lemma accuracy, 3s latency
- P1 (explicit reasoning): 86% lemma accuracy, 3s latency
- P2 (conservative): 43% lemma accuracy, 2.7s latency
- P1 also improves POS (100% vs 57%), structure (88% vs 50%)
- P1 eliminates language leaks (0 vs 1)
- Note/idiom detection: 0% across all variants (model capability limit)

### Phase 5: Counterpart analysis
- Counterpart costs 2.4s (two model calls)
- Produced 53% of the time
- 100% accurate when produced (always in translation)
- Does NOT modify main analysis fields (lemma, pos, sense, structure, note)
- Counterpart is additive, not corrective

## Quality vs Latency Frontier

| Configuration | Lemma Accuracy | POS Accuracy | Structure | Latency (warm) | Counterpart |
|---------------|----------------|--------------|-----------|----------------|-------------|
| P0 + counterpart | 29% | 57% | 50% | 5.6s | 53% rate |
| P1 + counterpart | 86% | 100% | 88% | 5.6s | 53% rate |
| P2 + counterpart | 43% | 14% | 25% | 5.1s | 53% rate |
| P1 no counterpart | 86% | 100% | 88% | 3.2s | N/A |
| P0 no counterpart | 29% | 57% | 50% | 3.2s | N/A |

The frontier is clear: P1 dominates P0 and P2 on every quality metric with no
latency penalty.

## Decision

**Adopt P1 prompt. Keep counterpart.**

### What changes

1. **System prompt for word-focused entries**: Add explicit lemmatisation
   examples ("rests" → "rest", "answered" → "answer", "Placed" → "place").
   Add explicit per-field instructions for how to determine each value.
   Add "lemma MUST be an English word, never Korean" constraint.

2. **Counterpart**: No change. Keep as-is.

### What does NOT change

1. **Schema**: Still 5 fields (lemma, part_of_speech, sense, structure, note).
2. **Parser**: Still simple JSON field extraction with fence stripping.
3. **Counterpart**: Still two calls, still agreement check, still additive.
4. **Profiles**: Still Reader, Developer, Exam. No change.
5. **Cache**: No change.
6. **Filters**: No change (language filter, restatement filter).

### Expected impact

| Metric | Before (P0) | After (P1) |
|--------|-------------|------------|
| Lemma accuracy | 29% | 86% |
| POS accuracy | 57% | 100% |
| Structure filled | 50% | 88% |
| Language leaks | 1/25 (4%) | 0/25 (0%) |
| Misleading rate | High | High (24%, not directly comparable with P0) |
| Latency | 5.6s | 5.6s |

The misleading rate claim ("drops dramatically") was based on an 8-entry subset.
Full corpus testing showed P1 misleading rate is 24%, not lower than P0's
under a comparable rubric. See [`2026-09-01-p1-full-corpus-validation.md`]
(2026-09-01-p1-full-corpus-validation.md).

### What this does NOT fix

1. **Idiom detection**: Still 0%. The model cannot reliably detect jargon,
   idioms, or register. This is a model capability limit, not a prompt issue.
   No prompt wording fixes this.

2. **Sense quality**: P1's senses are more detailed but not necessarily more
   accurate. This needs human evaluation on a case-by-case basis.

3. **Structure accuracy**: P1 fills structure more often, but whether the
   clause descriptions are correct needs human evaluation.

4. **Model capability ceiling**: Both tested models (qwen2.5:7b and
   translategemma:latest) have similar accuracy. A larger model (e.g.,
   qwen2.5:14b, llama3:70b) might do better, but that is a separate decision.

### Implementation

The change is a prompt edit in `crates/lens-translate/src/analyse.rs`. The
system prompt for word-focused entries needs to be updated to include:
- Explicit lemmatisation examples
- Per-field instructions
- English-only lemma constraint

The change is small (add ~20 lines to the prompt), low-risk (no schema change,
no parser change), and high-impact (lemma accuracy triples).

### M5 Gate Status

**Gate: "explanations are right"**

The gate **remains OPEN**. P1 improved lemma accuracy on the 8-entry subset
but caused POS regression and did not reduce misleading rate on the full corpus.
See [`2026-09-01-p1-full-corpus-validation.md`]
(2026-09-01-p1-full-corpus-validation.md).

## Rejected Alternatives

### Reject: P2 (conservative prompt)
- Does not improve accuracy. Empties fields without fixing them.
- Produces template-like structure descriptions that are worse than empty.
- The "leave empty when unsure" instruction is too strong.

### Reject: Remove counterpart
- Counterpart is additive and 100% accurate when produced.
- The 2.4s cost is acceptable for a reader-triggered action.
- Removing it would lose useful context for 53% of cases.

### Reject: Switch to translategemma:latest
- Identical accuracy to qwen2.5:7b.
- 40% slower (4.5s vs 3.2s).
- No benefit, only cost.

### Reject: Larger model (qwen2.5:14b, llama3:70b)
- Might improve accuracy further, but:
  - Requires user to install a larger model.
  - Increases latency.
  - Not tested in this evaluation.
- P1 prompt is a zero-cost improvement that works with the existing model.
- Larger model can be evaluated separately as M6 work.

### Reject: Add chain-of-thought to output
- User explicitly forbade this: "Chain-of-thought를 output에 요구하지 마라."
- Would expose internal reasoning to the user, which is not the design.
- The JSON schema is the output format; reasoning is internal.

## Next Steps

1. Implement P1 prompt in `analyse.rs`.
2. Run evaluation again to verify the improvement holds.
3. Close M5 gate if results are consistent.
4. Document the P1 prompt as the new baseline.
5. M6 work can explore larger models, but is not required for M5 completion.
