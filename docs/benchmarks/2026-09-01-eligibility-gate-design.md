# Pre-Analysis Eligibility Gate — Design

Date: 2026-09-01

## Problem Statement

Phase 13 showed that no-word hallucination persists across all prompts (P0, P1, P2, P3). The model cannot reliably follow "leave fields empty" instructions. This is a model capability limitation, not a prompt limitation.

The current pipeline calls Deep Analysis even when there's no focused word, and relies on the model to leave lemma/pos/sense empty. This fails.

## Solution

Instead of asking the model to leave fields empty, don't call the model for word-specific fields when there's no word.

## Eligibility Contract

### Valid word → Deep Analysis with word fields
```text
sentence: "The server has expired."
word: "expired"
```
→ Deep Analysis runs, asks for lemma/pos/sense/structure/note

### No focused word → Deep Analysis without word fields (or skip entirely)
```text
sentence: "The server has expired."
word: None (pointer on whitespace/punctuation)
```
→ Option A: Deep Analysis runs, but does NOT ask for lemma/pos/sense
→ Option B: Deep Analysis is not called at all

### Invalid word → No Deep Analysis
```text
word: ""
```
→ No Deep Analysis

### Word outside sentence → No Deep Analysis
```text
sentence: "The server has expired."
word: "adapter"
```
→ No Deep Analysis (already handled by `about_the_word()` which checks if word restates sentence)

### Ambiguous selection → Existing word resolution
→ Use existing `SegmentedText::word_text()` which returns `None` for whitespace/punctuation

## Implementation Options

### Option A: Prompt-level gating (no production code change)

Modify the prompt to NOT ask for lemma/pos/sense when `word` is `None`:

```rust
if answerable.the_word {
    // ask for lemma/pos/sense
} else {
    // do NOT ask for lemma/pos/sense, only ask for structure/note
}
```

**Pros:** No pipeline change, only prompt change
**Cons:** Model still called, still might hallucinate structure/note

### Option B: Pipeline-level gating (skip Deep Analysis entirely)

Don't call `translator.deep()` when `word` is `None`:

```rust
if job.focused_word.is_none() {
    // skip Deep Analysis, show card without analysis
    continue;
}
```

**Pros:** No model call, no hallucination possible
**Cons:** Loses sentence-level analysis (structure, idiom detection)

### Option C: Hybrid (skip word fields, keep sentence fields)

Call Deep Analysis but with a modified request that only asks for structure/note:

```rust
let request = if job.focused_word.is_none() {
    DeepRequest::about_what_they_can_read(&job.original, &job.language)
        // don't call about_the_word()
} else {
    DeepRequest::new(&job.original, translation, &job.language)
        .about_the_word(word)
};
```

**Pros:** Keeps sentence-level analysis, no word-field hallucination
**Cons:** Need to verify model doesn't hallucinate word fields anyway

## Recommendation

**Option C: Hybrid approach.**

Rationale:
1. The existing `DeepRequest` API already supports no-word requests
2. The prompt already has a no-word branch (line 105 in analyse.rs)
3. The problem is the model doesn't follow the "leave empty" instruction
4. Option C keeps the sentence-level analysis (structure, note) which is useful
5. Option C eliminates the word-field hallucination risk

But we need to verify: does the model still hallucinate lemma/pos/sense when the prompt says "this is a sentence, not a word"?

If yes → Option B (skip entirely)
If no → Option C (hybrid)

## Testing Plan

1. Run corpus_structural_invariants on C01/C02/E03 with current prompt
2. Check if model fills lemma/pos/sense
3. If yes → pipeline must prevent the call
4. If no → prompt is sufficient

## False Positive / False Negative

### False positive (gate says yes, should say no)
→ Model called with no word → hallucination possible
→ Must be ≈ 0

### False negative (gate says no, should say yes)
→ Deep Analysis not called when it should be
→ User loses functionality
→ Acceptable if rare

## Metrics

```text
eligibility_precision = true_positives / (true_positives + false_positives)
eligibility_recall = true_positives / (true_positives + false_negatives)
false_positive_rate = false_positives / (false_positives + true_negatives)
false_negative_rate = false_negatives / (false_negatives + true_positives)
```

Target: `false_positive_rate ≈ 0`
