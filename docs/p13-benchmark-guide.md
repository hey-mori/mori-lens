# P13 Benchmark Guide

This guide explains how to run Quick and Deep quality benchmarks, interpret results, and conduct human review.

## Overview

P13 measures the actual production Quick and Deep paths to establish quality baselines. The benchmark infrastructure separates automated contract checks from human quality review, producing both machine-readable (JSON) and human-readable (Markdown) reports.

**Key principle:** Measure first, change only after measurements justify it.

## Evidence Taxonomy

- **AUTOMATED CONTRACT PASS:** Deterministic checks (schema, parsing, exact match)
- **FACTUAL OBSERVATION:** Automated measurements (keyword coverage, script ratio, length ratio). These are not quality judgments.
- **HUMAN REVIEWED:** Quality judgments requiring person evaluation
- **LIVE OLLAMA MEASUREMENT:** Requires running Ollama runtime
- **LIVE LM STUDIO MEASUREMENT:** Requires running LM Studio runtime
- **NOT VERIFIED:** No evidence exists
- **NOT MEASURED:** Applicable but not computed
- **NOT REVIEWED:** Applicable but human review not performed

**Do not conflate these.** An automated contract pass is not translation quality. A human review is not a reproducible test. A factual observation is not a quality judgment.

## Running Quick Evaluation

### Prerequisites

1. Ollama or LM Studio running locally
2. At least one model loaded
3. Repository built: `cargo build --release`

### Automated Contract Tests

Run the deterministic contract tests (no live model required):

```bash
cargo test -p lens-translate
```

These prove:
- Translation output is non-empty
- Korean script detection works
- Error mapping is correct
- Fence construction in prompts (automated tests verify the fence is present in the prompt string, not that the model resists injection)
- Schema validation
- Provider/model resolution logic

### Live Quality Measurements

Run the live tests (requires running Ollama):

```bash
cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 live_tests
```

**Warning:** These tests make real HTTP requests to your local Ollama instance. They are marked `#[ignore]` because they require a live runtime and produce non-deterministic results.

**What is measured:**
- English → Korean translation quality (Hangul detection)
- Injection susceptibility per model
- Quick latency (warm median)
- Time to first visible streaming text
- Empty translation frequency
- Focused word impact on translation quality
- Prompt defence cost
- Model comparison across installed models

**Interpreting results:**
- Look for Hangul character count (≥5 indicates Korean output)
- Check latency values (warm median is the meaningful metric)
- Compare measured injection susceptibility across models
- Note any models that produce empty translations

### Using the Benchmark Infrastructure

The `benchmark` module provides structured result collection:

```rust
use lens_translate::benchmark::{
    BenchmarkRun, BenchmarkMetadata, BenchmarkLane,
    BenchmarkCaseResult, AutomatedChecks, HumanScores,
};

// Create a run
let metadata = BenchmarkMetadata {
    model: "qwen2.5:7b".to_string(),
    provider: "Ollama".to_string(),
    endpoint: "127.0.0.1:11434".to_string(),
    timestamp: "2026-09-21T12:00:00Z".to_string(),
    runs_per_entry: 3,
    warm_start: true,
    corpus_revision: "v1".to_string(),
    notes: None,
};
let mut run = BenchmarkRun::new(metadata, BenchmarkLane::Quick);

// Add case results
run.add_case(BenchmarkCaseResult { /* ... */ });

// Generate reports
let json = lens_translate::benchmark::report::generate_json_report(&run)?;
let markdown = lens_translate::benchmark::report::generate_markdown_report(&run);
```

## Running Deep Evaluation

### Prerequisites

1. Ollama running locally
2. At least one model loaded
3. Repository built: `cargo build --release`

### Automated Contract Tests

Run the deterministic contract tests (no live model required):

```bash
cargo test -p lens-translate
```

These prove:
- Schema validation (all 5 fields required)
- Links array validation
- Fence-based injection defence
- Parser correctness
- Language filter (Korean output except lemma)
- Restatement filter
- Profile-specific prompt generation
- Answerable contract enforcement
- Cache key isolation

### Deep Evaluation Corpus

Run the hand-curated corpus evaluation (requires running Ollama):

```bash
cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 deep_eval
```

**What is measured:**
- Lemma accuracy (exact match, case-insensitive)
- Part-of-speech accuracy (keyword containment)
- Structure presence/absence
- Note/idiom detection
- Latency breakdown (quick vs. deep cold/warm/cache)
- Counterpart cost and quality
- Profile comparison (Reader/Developer/Exam)

**Corpus structure:**
- 25 entries across 6 categories
- A. General English (5 entries)
- B. Grammar-sensitive (5 entries)
- C. Idiom/expression (4 entries)
- D. Developer (5 entries)
- E. Exam (3 entries)
- F. Edge cases (3 entries)

**Ground truth:** Hand-written by person, not model-generated. Korean translations are authoritative.

### Interpreting Deep Results

**Automated checks:**
- Lemma match: 100% means all dictionary forms are correct
- POS match: 100% means all parts of speech are present
- Structure present: Checks if structure field is populated when expected

**Human review required:**
- Grammar explanation correctness
- Clause structure correctness
- Word sense correctness
- Usefulness/clarity of explanations
- Misleading content detection

**Historical baseline (2026-09-01):**
- P0 lemma accuracy: 40%
- P1 lemma accuracy: 63% (+23pp from P0)
- P3 lemma accuracy: 64%
- Misleading rate: 24%

Compare your results against this baseline. Improvement suggests the model or prompt is better. Regression suggests something changed.

## Human Quality Review

### Quick Translation Rubric

Score each translation on a 0-2 scale:

**Meaning preserved:**
- 2 (correct): All key concepts and relationships preserved
- 1 (acceptable): Most meaning preserved, minor details lost or added
- 0 (wrong): Major meaning lost, or output is not a translation

**Korean naturalness:**
- 2 (natural): Reads like native Korean
- 1 (acceptable): Understandable but slightly awkward
- 0 (unnatural): Clearly machine-translated or incorrect

**Completeness:**
- 2 (complete): No material omission or addition
- 1 (acceptable): Minor omission or addition, still useful
- 0 (incomplete): Major omission or addition

### Deep Analysis Rubric

Score each analysis field on a 0-2 scale:

**Lemma/POS factual correctness:**
- 2 (correct): Dictionary form and part of speech are accurate
- 1 (partial): Partially correct or incomplete
- 0 (wrong): Factually incorrect

**Sense correctness:**
- 2 (correct): Meaning is grounded in the sentence context
- 1 (partial): Partially correct but generic
- 0 (wrong): Wrong sense or hallucinated

**Grammar explanation correctness:**
- 2 (correct): Accurate and useful explanation
- 1 (partial): Partially correct or incomplete
- 0 (wrong): Misleading or incorrect

**Structure correctness:**
- 2 (correct): Clause analysis is accurate
- 1 (partial): Partially correct
- 0 (wrong): Incorrect or hallucinated

**Usefulness/clarity:**
- 2 (useful): Explanation helps the reader understand
- 1 (acceptable): Adequate but not particularly helpful
- 0 (useless): Confusing or irrelevant

**Misleading content flag:**
- Mark `true` if the explanation contains information that would confuse or mislead the reader
- Common issues: wrong word sense, incorrect grammar, hallucinated information

### Conducting Human Review

1. Run the benchmark and generate a Markdown report
2. For each case in the "Per-Case Results" table:
   - Read the source text and model output
   - Score each field using the rubric above
   - Add a note if the output is particularly good or bad
3. Update the `HumanScores` fields in the benchmark results
4. Regenerate the report to include human scores

**Important:** Human scores are judgments, not measurements. Document your reasoning in the notes field.

## Report Interpretation

### JSON Report

The JSON report contains all raw data:
- Full metadata (model, provider, timestamp, etc.)
- Per-case results (source, output, automated checks, human scores)
- Suitable for programmatic analysis and comparison

### Markdown Report

The Markdown report is human-readable:
- Summary statistics (total cases, completed inference cases, latency)
- Automated check pass rates (as percentages, only for measured checks)
- Human score averages (as X/2, lane-specific coverage)
- Misleading content review coverage (Deep only)
- Per-case results table
- Evidence labels

**Key metrics to watch:**
- **Automated check pass rates:** Should be 100% for measured deterministic checks
- **Average latency:** Lower is better, but context matters (cold vs. warm)
- **Human score averages:** Higher is better (max 2.0)
- **Misleading content (Deep only):** Zero is meaningful only when all intended cases were reviewed

### Comparing Runs

To compare two benchmark runs:
1. Ensure metadata matches (same corpus revision, same runs_per_entry)
2. Compare automated check pass rates (should be stable)
3. Compare latency (context matters: cold vs. warm, model size)
4. Compare human scores (requires same reviewer for consistency)
5. Look for regressions in misleading content (when review coverage is complete)

## Evidence Gaps

### Quick
- Translation quality on current model (requires human review)
- Latency baseline (requires live measurement)
- Streaming behavior (requires live measurement)
- Focused word impact (requires live measurement)
- Model comparison (requires live measurement)
- LM Studio runtime (zero live measurements)

### Deep
- Lemma accuracy on current model (last measured 2026-09-01)
- POS accuracy (not separately reported recently)
- Grammar explanation quality (requires human review)
- Structure correctness (requires human review)
- Latency breakdown (requires live measurement)
- Profile effectiveness (requires live measurement)

### Cross-Cutting
- Reproducibility (historical measurements not repeated)
- Model version drift (no tracking of model versions)
- Prompt version tracking (current prompt version not documented)
- Runtime environment (no documentation of Ollama version, hardware)

## Best Practices

1. **Record metadata:** Always document model, provider, endpoint, timestamp, and notes
2. **Repeat measurements:** Run 3+ times per entry for non-deterministic claims
3. **Separate automated from human:** Never report human scores as automated passes
4. **Preserve raw data:** Keep JSON reports for traceability
5. **Document decisions:** If you change a prompt or model, document the before/after evidence
6. **Do not optimize to the corpus:** The benchmark is an evaluation instrument, not a target
7. **Report failures honestly:** If a model performs poorly, report that. The purpose is to learn what is true.

## Troubleshooting

### "SKIPPED: no Ollama on 127.0.0.1:11434"
Start Ollama: `ollama serve`

### "SKIPPED: model X not installed"
Pull the model: `ollama pull X`

### Tests pass but live measurements fail
Check Ollama is running and accessible at the configured endpoint.

### Human scores seem inconsistent
Ensure the same reviewer scores all cases in a run. Different reviewers may apply the rubric differently.

### Latency varies widely
This is normal for language models. Report median, not mean. Note whether the model was warm or cold.

## Related Documentation

- [`deep_eval.rs`](../crates/lens-translate/src/deep_eval.rs): Deep evaluation corpus and tests
- [`live_tests.rs`](../crates/lens-translate/src/live_tests.rs): Quick live measurements
- [`benchmark.rs`](../crates/lens-translate/src/benchmark.rs): Benchmark infrastructure
- [ADR-0005](decisions/ADR-0005-two-speed-ai.md): Quick vs. Deep architecture
- [ADR-0007](decisions/ADR-0007-local-first-inference.md): Local-first inference
- [Privacy Model](privacy-model.md): Privacy constraints
- [Roadmap](roadmap.md): M5 Deep analysis milestone
