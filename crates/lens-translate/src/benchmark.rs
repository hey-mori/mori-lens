//! Benchmark result structures and reporting for P13 quality evaluation.
//!
//! This module provides infrastructure for collecting, scoring, and reporting
//! Quick and Deep benchmark results. It separates automated contract checks
//! from human quality review, and produces both machine-readable (JSON) and
//! human-readable (Markdown) reports.
//!
//! # Evidence taxonomy
//!
//! - AUTOMATED CONTRACT PASS: Deterministic checks (schema, parsing, exact match)
//! - FACTUAL OBSERVATION: Automated measurements (keyword coverage, script ratio, length ratio)
//! - HUMAN REVIEWED: Quality judgments requiring person evaluation
//! - LIVE OLLAMA MEASUREMENT: Requires running Ollama runtime
//! - LIVE LM STUDIO MEASUREMENT: Requires running LM Studio runtime
//! - NOT VERIFIED: No evidence exists
//! - NOT MEASURED: Applicable but not computed
//! - NOT REVIEWED: Applicable but human review not performed
//!
//! # Factual observations vs quality scores
//!
//! This module distinguishes between:
//! - **Factual observations** (`keyword_coverage_ratio`, `target_script_ratio`,
//!   `length_ratio`, `target_script_detected`): Deterministic measurements that
//!   can be computed automatically. They are not quality judgments.
//! - **Human quality scores** (`QualityScore`): Require person evaluation on a
//!   0-2 scale. Never conflated with factual observations.
//!
//! # Tri-state automated evidence
//!
//! Automated checks use `Option<bool>` where:
//! - `Some(true)` = actually checked and passed
//! - `Some(false)` = actually checked and failed
//! - `None` = not measured / not applicable
//!
//! A `None` value must never be counted as PASS in statistics.
//!
//! # Privacy
//!
//! All benchmark corpus text is repository-owned synthetic or curated content.
//! No captured screen text or user data appears in benchmark results.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata about a benchmark run.
///
/// Records the conditions under which measurements were taken, so results
/// can be compared across runs and reproduced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetadata {
    /// Model name (e.g., "qwen2.5:7b", "llama-3.1-8b")
    pub model: String,
    /// Provider (e.g., "Ollama", "LM Studio")
    pub provider: String,
    /// Endpoint URL (e.g., "127.0.0.1:11434")
    pub endpoint: String,
    /// ISO 8601 timestamp of when the benchmark started
    pub timestamp: String,
    /// Number of runs per corpus entry
    pub runs_per_entry: usize,
    /// Whether the model was warm (pre-loaded) or cold
    pub warm_start: bool,
    /// Benchmark corpus revision or identifier
    pub corpus_revision: String,
    /// Free-form notes about the run (hardware, Ollama version, etc.)
    pub notes: Option<String>,
}

/// Automated contract check result for a single corpus entry.
///
/// These are deterministic checks that can be asserted without human judgment.
/// Uses tri-state semantics: `Some(true)` = checked and passed, `Some(false)` =
/// checked and failed, `None` = not measured / not applicable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomatedChecks {
    /// Output is non-empty
    pub non_empty: bool,
    /// Output is in target language (Korean for this project)
    pub target_language_detected: bool,
    /// Response parses through production schema (Deep only).
    /// `Some(true)` for a successfully constructed `DeepAnalysis` because the
    /// production parser necessarily validates the schema before the type exists.
    pub schema_valid: Option<bool>,
    /// Lemma matches expected (exact match, case-insensitive)
    pub lemma_match: Option<bool>,
    /// Expected POS keyword present (containment match)
    pub pos_match: Option<bool>,
    /// Structure field populated when expected
    pub structure_present: Option<bool>,
    /// Forbidden restatement behavior not detected.
    /// `None` when not measured (e.g., bridge from production output).
    pub no_restatement: Option<bool>,
    /// Provenance/model/provider identity correct.
    /// `None` when not measured (e.g., bridge from production output).
    pub provenance_correct: Option<bool>,
    /// Timeout/error type (if any)
    pub error_type: Option<String>,
    /// Latency in milliseconds
    pub latency_ms: Option<u64>,
}

/// A validated human quality score on a 0-2 scale.
///
/// This type ensures that only valid scores (0, 1, or 2) can be constructed.
/// Scores represent:
/// - 0 (wrong): Meaning lost, major omission/addition, or factually incorrect
/// - 1 (partial): Meaning mostly preserved, minor issues, or partially correct
/// - 2 (correct): Meaning fully preserved, natural, no material issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct QualityScore(u8);

impl QualityScore {
    /// Create a new quality score. Returns `None` if value is not 0, 1, or 2.
    pub fn new(value: u8) -> Option<Self> {
        if value <= 2 {
            Some(QualityScore(value))
        } else {
            None
        }
    }

    /// Get the numeric value.
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for QualityScore {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        QualityScore::new(value).ok_or("QualityScore must be 0, 1, or 2")
    }
}

impl From<QualityScore> for u8 {
    fn from(score: QualityScore) -> u8 {
        score.0
    }
}

/// Human quality review scores for a single corpus entry.
///
/// These require person evaluation and must not be reported as automated pass.
/// Each field uses `QualityScore` (0-2 validated) or `None` if not reviewed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HumanScores {
    /// Quick translation: meaning preserved (0=wrong, 1=partial, 2=correct)
    pub meaning_preserved: Option<QualityScore>,
    /// Quick translation: Korean naturalness (0=unnatural, 1=acceptable, 2=natural)
    pub naturalness: Option<QualityScore>,
    /// Quick translation: material omission/addition (0=major, 1=minor, 2=none)
    pub completeness: Option<QualityScore>,
    /// Deep: lemma/POS factual correctness (0=wrong, 1=partial, 2=correct)
    pub factual_correctness: Option<QualityScore>,
    /// Deep: sense correctness (0=wrong, 1=partial, 2=correct)
    pub sense_correctness: Option<QualityScore>,
    /// Deep: grammar explanation correctness (0=wrong, 1=partial, 2=correct)
    pub grammar_correctness: Option<QualityScore>,
    /// Deep: structure correctness (0=wrong, 1=partial, 2=correct)
    pub structure_correctness: Option<QualityScore>,
    /// Deep: usefulness/clarity (0=useless, 1=acceptable, 2=useful)
    pub usefulness: Option<QualityScore>,
    /// Deep: misleading content flag (true = contains misleading information)
    pub misleading: Option<bool>,
    /// Free-form reviewer note
    pub note: Option<String>,
}

/// Factual observations that can be computed automatically.
///
/// These are deterministic measurements, not quality judgments.
/// They describe observable properties of the output.
/// Uses `Option<T>` where `None` means not measured.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FactualObservations {
    /// Ratio of expected keywords found in output (0.0 to 1.0).
    /// `None` when expected keywords not provided.
    pub keyword_coverage_ratio: Option<f64>,
    /// Ratio of target script (Hangul) characters in output (0.0 to 1.0).
    /// `None` when not measured.
    pub target_script_ratio: Option<f64>,
    /// Whether target script was detected at all.
    /// `None` when not measured.
    pub target_script_detected: Option<bool>,
    /// Ratio of output length to source length.
    /// `None` when not measured.
    pub length_ratio: Option<f64>,
    /// Whether length ratio is within sanity band (0.3 to 1.5).
    /// `None` when not measured.
    pub length_ratio_within_sanity_band: Option<bool>,
}

/// Result for a single corpus entry in a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCaseResult {
    /// Corpus entry ID (e.g., "A01", "B03", "D07")
    pub entry_id: String,
    /// Category label (e.g., "general", "grammar", "developer")
    pub category: String,
    /// Lane: Quick or Deep
    pub lane: BenchmarkLane,
    /// Run index (0-based, for repeated runs)
    pub run_index: usize,
    /// Source text (repository-owned, not captured)
    pub source: String,
    /// Focused word (if any)
    pub focused_word: Option<String>,
    /// Model output text
    pub output: String,
    /// Automated contract checks
    pub automated: AutomatedChecks,
    /// Factual observations (deterministic, not quality judgments)
    pub observations: FactualObservations,
    /// Human quality scores (populated by reviewer, None = not reviewed)
    pub human: HumanScores,
}

/// Benchmark lane: Quick translation or Deep analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkLane {
    /// Quick translation path
    Quick,
    /// Deep analysis path
    Deep,
}

/// Complete benchmark run results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    /// Run metadata
    pub metadata: BenchmarkMetadata,
    /// Lane (Quick or Deep)
    pub lane: BenchmarkLane,
    /// Per-case results
    pub cases: Vec<BenchmarkCaseResult>,
}

/// Aggregate statistics for a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkStats {
    /// Total number of cases
    pub total_cases: usize,
    /// Number of cases that completed inference (no error)
    pub completed_inference_cases: usize,
    /// Number of cases with each automated check passing
    pub automated_check_pass_rates: HashMap<String, f64>,
    /// Average latency in milliseconds (if measured)
    pub avg_latency_ms: Option<f64>,
    /// Median latency in milliseconds (if measured)
    pub median_latency_ms: Option<f64>,
    /// Average human score by field (if reviewed)
    pub avg_human_scores: HashMap<String, f64>,
    /// Number of cases reviewed for misleading content (Deep only)
    pub misleading_reviewed_count: usize,
    /// Number of cases flagged as misleading (Deep only, among reviewed)
    pub misleading_count: usize,
    /// Total cases applicable for misleading review (Deep only)
    pub misleading_applicable_count: usize,
}

/// Quick translation factual observations.
///
/// Computes deterministic measurements about translation output.
/// These are observations, not quality judgments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickObservations;

impl QuickObservations {
    /// Compute keyword coverage ratio.
    ///
    /// Returns the fraction of expected keywords found in output (0.0 to 1.0).
    pub fn keyword_coverage_ratio(output: &str, expected_keywords: &[&str]) -> f64 {
        if expected_keywords.is_empty() {
            return 1.0;
        }
        let present = expected_keywords
            .iter()
            .filter(|kw| output.contains(**kw))
            .count();
        present as f64 / expected_keywords.len() as f64
    }

    /// Compute target script (Hangul) ratio.
    ///
    /// Returns the fraction of characters that are Hangul (0.0 to 1.0).
    pub fn target_script_ratio(output: &str) -> f64 {
        let hangul_count = output
            .chars()
            .filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c))
            .count();
        let total_chars = output.chars().count();
        if total_chars == 0 {
            return 0.0;
        }
        hangul_count as f64 / total_chars as f64
    }

    /// Check if target script (Hangul) was detected.
    pub fn target_script_detected(output: &str) -> bool {
        output
            .chars()
            .any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c))
    }

    /// Compute length ratio (output length / source length).
    pub fn length_ratio(source: &str, output: &str) -> f64 {
        let source_len = source.chars().count();
        let output_len = output.chars().count();
        if source_len == 0 {
            return 0.0;
        }
        output_len as f64 / source_len as f64
    }

    /// Check if length ratio is within sanity band (0.3 to 1.5).
    ///
    /// Korean text is typically shorter than English (fewer characters).
    /// A ratio between 0.3 and 1.5 is considered reasonable.
    pub fn length_ratio_within_sanity_band(source: &str, output: &str) -> bool {
        let ratio = Self::length_ratio(source, output);
        (0.3..=1.5).contains(&ratio)
    }

    /// Compute all factual observations for a Quick translation.
    ///
    /// Keyword coverage requires expected keywords; if not provided, it is `None`.
    pub fn compute(
        source: &str,
        output: &str,
        expected_keywords: Option<&[&str]>,
    ) -> FactualObservations {
        FactualObservations {
            keyword_coverage_ratio: expected_keywords
                .map(|kws| Self::keyword_coverage_ratio(output, kws)),
            target_script_ratio: Some(Self::target_script_ratio(output)),
            target_script_detected: Some(Self::target_script_detected(output)),
            length_ratio: Some(Self::length_ratio(source, output)),
            length_ratio_within_sanity_band: Some(Self::length_ratio_within_sanity_band(
                source, output,
            )),
        }
    }
}

/// Deep analysis factual observations.
///
/// Computes deterministic measurements about analysis output.
/// These are observations, not quality judgments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepObservations;

impl DeepObservations {
    /// Check if lemma matches expected (exact match, case-insensitive).
    pub fn lemma_matches(actual: &str, expected: &str) -> bool {
        actual.trim().eq_ignore_ascii_case(expected.trim())
    }

    /// Check if lemma partially matches (one contains the other).
    pub fn lemma_partially_matches(actual: &str, expected: &str) -> bool {
        let actual_lower = actual.trim().to_lowercase();
        let expected_lower = expected.trim().to_lowercase();
        actual_lower.contains(&expected_lower) || expected_lower.contains(&actual_lower)
    }

    /// Compute POS keyword coverage ratio.
    pub fn pos_coverage_ratio(actual: &str, expected_keywords: &[&str]) -> f64 {
        if expected_keywords.is_empty() {
            return 1.0;
        }
        let present = expected_keywords
            .iter()
            .filter(|kw| actual.contains(**kw))
            .count();
        present as f64 / expected_keywords.len() as f64
    }
}

/// Bridge to production types.
///
/// Provides conversion from production Quick/Deep results to benchmark structures.
pub mod bridge {
    use super::*;
    use lens_core::analyse::DeepAnalysis;
    use lens_core::translate::QuickTranslation;

    /// Create a BenchmarkCaseResult from a QuickTranslation.
    ///
    /// Extracts automated facts from the production result.
    /// Human scores and observations must be filled separately.
    ///
    /// `run_index` must be provided explicitly for repeated runs.
    /// `expected_keywords` is optional; if `None`, keyword coverage is not measured.
    pub fn from_quick_translation(
        entry_id: &str,
        category: &str,
        source: &str,
        focused_word: Option<&str>,
        translation: &QuickTranslation,
        run_index: usize,
        expected_keywords: Option<&[&str]>,
    ) -> BenchmarkCaseResult {
        BenchmarkCaseResult {
            entry_id: entry_id.to_string(),
            category: category.to_string(),
            lane: BenchmarkLane::Quick,
            run_index,
            source: source.to_string(),
            focused_word: focused_word.map(|s| s.to_string()),
            output: translation.text.clone(),
            automated: AutomatedChecks {
                non_empty: !translation.text.trim().is_empty(),
                target_language_detected: QuickObservations::target_script_detected(
                    &translation.text,
                ),
                schema_valid: None,
                lemma_match: None,
                pos_match: None,
                structure_present: None,
                no_restatement: None,     // Not measured by bridge
                provenance_correct: None, // Not measured by bridge
                error_type: None,
                latency_ms: Some(translation.latency_ms as u64),
            },
            observations: QuickObservations::compute(source, &translation.text, expected_keywords),
            human: HumanScores::default(),
        }
    }

    /// Create a BenchmarkCaseResult from a DeepAnalysis.
    ///
    /// Extracts automated facts from the production result.
    /// Human scores and observations must be filled separately.
    ///
    /// `run_index` must be provided explicitly for repeated runs.
    /// `schema_valid` is set to `Some(true)` because the production parser
    /// necessarily validates the schema before `DeepAnalysis` can exist.
    pub fn from_deep_analysis(
        entry_id: &str,
        category: &str,
        source: &str,
        focused_word: Option<&str>,
        analysis: &DeepAnalysis,
        run_index: usize,
    ) -> BenchmarkCaseResult {
        BenchmarkCaseResult {
            entry_id: entry_id.to_string(),
            category: category.to_string(),
            lane: BenchmarkLane::Deep,
            run_index,
            source: source.to_string(),
            focused_word: focused_word.map(|s| s.to_string()),
            output: format!(
                "{}|{}|{}|{}|{}",
                analysis.lemma,
                analysis.part_of_speech,
                analysis.sense,
                analysis.structure,
                analysis.note
            ),
            automated: AutomatedChecks {
                non_empty: !analysis.lemma.trim().is_empty(),
                target_language_detected: QuickObservations::target_script_detected(
                    &analysis.part_of_speech,
                ),
                schema_valid: Some(true), // Production parser validates schema
                lemma_match: None,
                pos_match: None,
                structure_present: Some(!analysis.structure.trim().is_empty()),
                no_restatement: None,     // Not measured by bridge
                provenance_correct: None, // Not measured by bridge
                error_type: None,
                latency_ms: Some(analysis.latency_ms),
            },
            observations: FactualObservations::default(), // Deep does not inherit Quick observations
            human: HumanScores::default(),
        }
    }
}

impl BenchmarkRun {
    /// Create a new benchmark run with the given metadata and lane.
    pub fn new(metadata: BenchmarkMetadata, lane: BenchmarkLane) -> Self {
        Self {
            metadata,
            lane,
            cases: Vec::new(),
        }
    }

    /// Add a case result to this run.
    pub fn add_case(&mut self, case: BenchmarkCaseResult) {
        self.cases.push(case);
    }

    /// Compute aggregate statistics for this run.
    pub fn compute_stats(&self) -> BenchmarkStats {
        let completed_inference_cases = self
            .cases
            .iter()
            .filter(|c| c.automated.error_type.is_none())
            .count();

        let mut stats = BenchmarkStats {
            total_cases: self.cases.len(),
            completed_inference_cases,
            ..Default::default()
        };

        // Compute automated check pass rates (only for measured checks)
        let mut check_counts: HashMap<String, (usize, usize)> = HashMap::new();
        for case in &self.cases {
            let checks: [(&str, Option<bool>); 6] = [
                ("non_empty", Some(case.automated.non_empty)),
                (
                    "target_language_detected",
                    Some(case.automated.target_language_detected),
                ),
                ("no_restatement", case.automated.no_restatement),
                ("provenance_correct", case.automated.provenance_correct),
                ("schema_valid", case.automated.schema_valid),
                ("lemma_match", case.automated.lemma_match),
            ];
            for (name, value) in checks {
                if let Some(passed) = value {
                    let entry = check_counts.entry(name.to_string()).or_insert((0, 0));
                    entry.0 += if passed { 1 } else { 0 };
                    entry.1 += 1;
                }
            }
            // Additional optional checks
            if let Some(pos_match) = case.automated.pos_match {
                let entry = check_counts
                    .entry("pos_match".to_string())
                    .or_insert((0, 0));
                entry.0 += if pos_match { 1 } else { 0 };
                entry.1 += 1;
            }
            if let Some(structure_present) = case.automated.structure_present {
                let entry = check_counts
                    .entry("structure_present".to_string())
                    .or_insert((0, 0));
                entry.0 += if structure_present { 1 } else { 0 };
                entry.1 += 1;
            }
        }
        for (name, (passed, total)) in check_counts {
            if total > 0 {
                stats
                    .automated_check_pass_rates
                    .insert(name, passed as f64 / total as f64);
            }
        }

        // Compute latency statistics
        let latencies: Vec<u64> = self
            .cases
            .iter()
            .filter_map(|c| c.automated.latency_ms)
            .collect();
        if !latencies.is_empty() {
            let sum: u64 = latencies.iter().sum();
            stats.avg_latency_ms = Some(sum as f64 / latencies.len() as f64);
            let mut sorted = latencies.clone();
            sorted.sort_unstable();
            // Correct median calculation for even/odd sample counts
            let n = sorted.len();
            stats.median_latency_ms = if n % 2 == 0 {
                // Even: average of two middle values
                Some((sorted[n / 2 - 1] as f64 + sorted[n / 2] as f64) / 2.0)
            } else {
                // Odd: middle value
                Some(sorted[n / 2] as f64)
            };
        }

        // Compute human score averages
        let mut score_sums: HashMap<String, (f64, usize)> = HashMap::new();
        for case in &self.cases {
            let scores: [(&str, Option<QualityScore>); 8] = [
                ("meaning_preserved", case.human.meaning_preserved),
                ("naturalness", case.human.naturalness),
                ("completeness", case.human.completeness),
                ("factual_correctness", case.human.factual_correctness),
                ("sense_correctness", case.human.sense_correctness),
                ("grammar_correctness", case.human.grammar_correctness),
                ("structure_correctness", case.human.structure_correctness),
                ("usefulness", case.human.usefulness),
            ];
            for (name, score) in scores {
                if let Some(val) = score {
                    let entry = score_sums.entry(name.to_string()).or_insert((0.0, 0));
                    entry.0 += val.value() as f64;
                    entry.1 += 1;
                }
            }
        }
        for (name, (sum, count)) in score_sums {
            if count > 0 {
                stats.avg_human_scores.insert(name, sum / count as f64);
            }
        }

        // Count misleading cases (Deep only)
        // Track reviewed count separately from misleading count
        if self.lane == BenchmarkLane::Deep {
            let applicable = self.cases.len();
            let reviewed = self
                .cases
                .iter()
                .filter(|c| c.human.misleading.is_some())
                .count();
            let misleading = self
                .cases
                .iter()
                .filter(|c| c.human.misleading == Some(true))
                .count();
            stats.misleading_applicable_count = applicable;
            stats.misleading_reviewed_count = reviewed;
            stats.misleading_count = misleading;
        }

        stats
    }
}

/// Count applicable human review fields by lane.
///
/// Quick: meaning_preserved, naturalness, completeness (3 fields)
/// Deep: factual_correctness, sense_correctness, grammar_correctness,
///       structure_correctness, usefulness (5 fields)
/// misleading is a separate boolean field, not counted here.
pub fn count_applicable_human_fields(lane: BenchmarkLane) -> usize {
    match lane {
        BenchmarkLane::Quick => 3,
        BenchmarkLane::Deep => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> BenchmarkMetadata {
        BenchmarkMetadata {
            model: "test-model".to_string(),
            provider: "TestProvider".to_string(),
            endpoint: "127.0.0.1:9999".to_string(),
            timestamp: "2026-09-21T12:00:00Z".to_string(),
            runs_per_entry: 3,
            warm_start: true,
            corpus_revision: "v1".to_string(),
            notes: Some("Test run".to_string()),
        }
    }

    fn test_case(entry_id: &str, latency: Option<u64>) -> BenchmarkCaseResult {
        BenchmarkCaseResult {
            entry_id: entry_id.to_string(),
            category: "general".to_string(),
            lane: BenchmarkLane::Quick,
            run_index: 0,
            source: "Test sentence.".to_string(),
            focused_word: None,
            output: "테스트 문장.".to_string(),
            automated: AutomatedChecks {
                non_empty: true,
                target_language_detected: true,
                schema_valid: None,
                lemma_match: None,
                pos_match: None,
                structure_present: None,
                no_restatement: None,
                provenance_correct: None,
                error_type: None,
                latency_ms: latency,
            },
            observations: FactualObservations::default(),
            human: HumanScores::default(),
        }
    }

    #[test]
    fn benchmark_run_tracks_cases() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        assert_eq!(run.cases.len(), 0);

        run.add_case(test_case("A01", Some(100)));
        assert_eq!(run.cases.len(), 1);
        assert_eq!(run.cases[0].entry_id, "A01");
    }

    #[test]
    fn compute_stats_counts_completed_inference_cases() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        run.add_case(test_case("A01", Some(100)));
        run.add_case(test_case("A02", Some(200)));
        let mut error_case = test_case("A03", None);
        error_case.automated.error_type = Some("Timeout".to_string());
        run.add_case(error_case);

        let stats = run.compute_stats();
        assert_eq!(stats.total_cases, 3);
        assert_eq!(stats.completed_inference_cases, 2);
    }

    #[test]
    fn compute_stats_calculates_pass_rates() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        run.add_case(test_case("A01", Some(100)));
        run.add_case(test_case("A02", Some(200)));

        let stats = run.compute_stats();
        assert_eq!(
            stats.automated_check_pass_rates.get("non_empty"),
            Some(&1.0)
        );
        assert_eq!(
            stats
                .automated_check_pass_rates
                .get("target_language_detected"),
            Some(&1.0)
        );
    }

    #[test]
    fn compute_stats_does_not_count_none_as_pass() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        let mut case = test_case("A01", Some(100));
        case.automated.no_restatement = None; // Not measured
        run.add_case(case);

        let stats = run.compute_stats();
        // no_restatement should not appear in pass rates when None
        assert!(!stats
            .automated_check_pass_rates
            .contains_key("no_restatement"));
    }

    #[test]
    fn compute_stats_calculates_latency() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        run.add_case(test_case("A01", Some(100)));
        run.add_case(test_case("A02", Some(200)));
        run.add_case(test_case("A03", Some(300)));

        let stats = run.compute_stats();
        assert_eq!(stats.avg_latency_ms, Some(200.0));
        assert_eq!(stats.median_latency_ms, Some(200.0));
    }

    #[test]
    fn compute_stats_calculates_median_for_even_count() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        run.add_case(test_case("A01", Some(100)));
        run.add_case(test_case("A02", Some(200)));
        run.add_case(test_case("A03", Some(300)));
        run.add_case(test_case("A04", Some(400)));

        let stats = run.compute_stats();
        // Even count: median is average of two middle values (200 and 300)
        assert_eq!(stats.median_latency_ms, Some(250.0));
    }

    #[test]
    fn compute_stats_handles_human_scores() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        let mut case1 = test_case("A01", Some(100));
        case1.human.meaning_preserved = QualityScore::new(2);
        case1.human.naturalness = QualityScore::new(1);
        run.add_case(case1);

        let mut case2 = test_case("A02", Some(200));
        case2.human.meaning_preserved = QualityScore::new(1);
        case2.human.naturalness = QualityScore::new(2);
        run.add_case(case2);

        let stats = run.compute_stats();
        assert_eq!(stats.avg_human_scores.get("meaning_preserved"), Some(&1.5));
        assert_eq!(stats.avg_human_scores.get("naturalness"), Some(&1.5));
    }

    #[test]
    fn compute_stats_distinguishes_misleading_reviewed_from_count() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Deep);
        let mut case1 = test_case("A01", Some(100));
        case1.lane = BenchmarkLane::Deep;
        case1.human.misleading = Some(true);
        run.add_case(case1);

        let mut case2 = test_case("A02", Some(200));
        case2.lane = BenchmarkLane::Deep;
        case2.human.misleading = Some(false);
        run.add_case(case2);

        let case3 = test_case("A03", Some(300));
        // Not reviewed: misleading is None
        run.add_case(case3);

        let stats = run.compute_stats();
        assert_eq!(stats.misleading_applicable_count, 3);
        assert_eq!(stats.misleading_reviewed_count, 2);
        assert_eq!(stats.misleading_count, 1);
    }

    #[test]
    fn quality_score_rejects_invalid_values() {
        assert!(QualityScore::new(0).is_some());
        assert!(QualityScore::new(1).is_some());
        assert!(QualityScore::new(2).is_some());
        assert!(QualityScore::new(3).is_none());
        assert!(QualityScore::new(255).is_none());
    }

    #[test]
    fn benchmark_metadata_serializes_to_json() {
        let metadata = test_metadata();
        let json = serde_json::to_string(&metadata).expect("serialize failed");
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"provider\":\"TestProvider\""));
    }

    #[test]
    fn benchmark_case_result_serializes_to_json() {
        let case = test_case("A01", Some(100));
        let json = serde_json::to_string(&case).expect("serialize failed");
        assert!(json.contains("\"entry_id\":\"A01\""));
        assert!(json.contains("\"lane\":\"quick\""));
    }

    #[test]
    fn benchmark_run_serializes_to_json() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        run.add_case(test_case("A01", Some(100)));
        let json = serde_json::to_string(&run).expect("serialize failed");
        assert!(json.contains("\"lane\":\"quick\""));
        assert!(json.contains("\"entry_id\":\"A01\""));
    }

    #[test]
    fn factual_observations_default_to_none() {
        let obs = FactualObservations::default();
        assert!(obs.keyword_coverage_ratio.is_none());
        assert!(obs.target_script_ratio.is_none());
        assert!(obs.target_script_detected.is_none());
        assert!(obs.length_ratio.is_none());
        assert!(obs.length_ratio_within_sanity_band.is_none());
    }

    #[test]
    fn quick_observations_compute_keyword_coverage() {
        let output = "포인터가 문장 위에 멈춥니다.";
        assert_eq!(
            QuickObservations::keyword_coverage_ratio(output, &["포인터", "문장"]),
            1.0
        );
        assert_eq!(
            QuickObservations::keyword_coverage_ratio(output, &["포인터"]),
            1.0
        );
        assert_eq!(
            QuickObservations::keyword_coverage_ratio(output, &["없는키워드"]),
            0.0
        );
        assert_eq!(
            QuickObservations::keyword_coverage_ratio("", &["포인터"]),
            0.0
        );
    }

    #[test]
    fn quick_observations_compute_target_script_ratio() {
        assert_eq!(QuickObservations::target_script_ratio("테스트"), 1.0);
        // "test 테스트" has 8 total chars (4 ASCII + 1 space + 3 Hangul), so ratio = 3/8 = 0.375
        assert!((QuickObservations::target_script_ratio("test 테스트") - 0.375).abs() < 0.01);
        assert_eq!(QuickObservations::target_script_ratio("test"), 0.0);
        assert_eq!(QuickObservations::target_script_ratio(""), 0.0);
    }

    #[test]
    fn quick_observations_detect_target_script() {
        assert!(QuickObservations::target_script_detected("테스트"));
        assert!(QuickObservations::target_script_detected("test 테스트"));
        assert!(!QuickObservations::target_script_detected("test"));
        assert!(!QuickObservations::target_script_detected(""));
    }

    #[test]
    fn quick_observations_compute_length_ratio() {
        assert_eq!(QuickObservations::length_ratio("", "output"), 0.0);
        let ratio = QuickObservations::length_ratio(
            "The pointer rests on the sentence.",
            "포인터가 문장 위에 멈춥니다.",
        );
        // 16 Korean chars / 34 English chars ≈ 0.47
        assert!((ratio - 0.47).abs() < 0.01);
    }

    #[test]
    fn quick_observations_check_length_sanity_band() {
        assert!(QuickObservations::length_ratio_within_sanity_band(
            "The pointer rests on the sentence.",
            "포인터가 문장 위에 멈춥니다."
        ));
        assert!(!QuickObservations::length_ratio_within_sanity_band(
            "Short",
            "Very very very long output that is way too long"
        ));
    }

    #[test]
    fn quick_observations_compute_with_keywords() {
        let obs = QuickObservations::compute("source", "테스트", Some(&["테스트"]));
        assert_eq!(obs.keyword_coverage_ratio, Some(1.0));
        assert!(obs.target_script_ratio.is_some());
        assert!(obs.target_script_detected.is_some());
    }

    #[test]
    fn quick_observations_compute_without_keywords() {
        let obs = QuickObservations::compute("source", "테스트", None);
        assert!(obs.keyword_coverage_ratio.is_none());
        assert!(obs.target_script_ratio.is_some());
    }

    #[test]
    fn deep_observations_check_lemma_match() {
        assert!(DeepObservations::lemma_matches("rest", "rest"));
        assert!(DeepObservations::lemma_matches("REST", "rest"));
        assert!(!DeepObservations::lemma_matches("resting", "rest"));
        assert!(!DeepObservations::lemma_matches("run", "rest"));
        assert!(!DeepObservations::lemma_matches("", "rest"));
    }

    #[test]
    fn deep_observations_check_lemma_partial_match() {
        assert!(DeepObservations::lemma_partially_matches("rest", "rest"));
        assert!(DeepObservations::lemma_partially_matches("resting", "rest"));
        assert!(!DeepObservations::lemma_partially_matches("run", "rest"));
    }

    #[test]
    fn deep_observations_compute_pos_coverage() {
        assert_eq!(DeepObservations::pos_coverage_ratio("동사", &["동사"]), 1.0);
        assert_eq!(
            DeepObservations::pos_coverage_ratio("동사, 자동사", &["동사"]),
            1.0
        );
        assert_eq!(
            DeepObservations::pos_coverage_ratio("형용사", &["동사"]),
            0.0
        );
        assert_eq!(DeepObservations::pos_coverage_ratio("", &["동사"]), 0.0);
    }

    #[test]
    fn count_applicable_human_fields_quick() {
        assert_eq!(count_applicable_human_fields(BenchmarkLane::Quick), 3);
    }

    #[test]
    fn count_applicable_human_fields_deep() {
        assert_eq!(count_applicable_human_fields(BenchmarkLane::Deep), 5);
    }
}

/// Report generation for benchmark results.
///
/// Produces both machine-readable (JSON) and human-readable (Markdown) reports
/// from benchmark run data.
pub mod report {
    use super::*;
    use std::fmt::Write;

    /// Generate a JSON report from a benchmark run.
    ///
    /// The JSON report includes all case results, metadata, and computed statistics.
    /// Suitable for programmatic analysis and comparison across runs.
    pub fn generate_json_report(run: &BenchmarkRun) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(run)
    }

    /// Generate a Markdown report from a benchmark run.
    ///
    /// The Markdown report is human-readable and suitable for documentation.
    /// It separates automated contract checks from human review fields.
    pub fn generate_markdown_report(run: &BenchmarkRun) -> String {
        let mut md = String::new();
        let stats = run.compute_stats();

        // Header
        writeln!(md, "# P13 Benchmark Report").unwrap();
        writeln!(md).unwrap();
        writeln!(md, "**Lane:** {:?}", run.lane).unwrap();
        writeln!(md, "**Model:** {}", run.metadata.model).unwrap();
        writeln!(md, "**Provider:** {}", run.metadata.provider).unwrap();
        writeln!(md, "**Endpoint:** {}", run.metadata.endpoint).unwrap();
        writeln!(md, "**Timestamp:** {}", run.metadata.timestamp).unwrap();
        writeln!(md, "**Runs per entry:** {}", run.metadata.runs_per_entry).unwrap();
        writeln!(md, "**Warm start:** {}", run.metadata.warm_start).unwrap();
        writeln!(md, "**Corpus revision:** {}", run.metadata.corpus_revision).unwrap();
        if let Some(notes) = &run.metadata.notes {
            writeln!(md, "**Notes:** {}", notes).unwrap();
        }
        writeln!(md).unwrap();

        // Summary statistics
        writeln!(md, "## Summary").unwrap();
        writeln!(md).unwrap();
        writeln!(md, "- **Total cases:** {}", stats.total_cases).unwrap();
        writeln!(
            md,
            "- **Completed inference cases:** {}",
            stats.completed_inference_cases
        )
        .unwrap();
        if let Some(avg_latency) = stats.avg_latency_ms {
            writeln!(md, "- **Average latency:** {:.1}ms", avg_latency).unwrap();
        }
        if let Some(median_latency) = stats.median_latency_ms {
            writeln!(md, "- **Median latency:** {:.1}ms", median_latency).unwrap();
        }
        // Misleading review coverage (Deep only)
        if run.lane == BenchmarkLane::Deep {
            if stats.misleading_reviewed_count == 0 {
                writeln!(md, "- **Misleading content review:** NOT REVIEWED").unwrap();
            } else if stats.misleading_reviewed_count == stats.misleading_applicable_count {
                writeln!(
                    md,
                    "- **Misleading content:** {} flagged / {} reviewed",
                    stats.misleading_count, stats.misleading_reviewed_count
                )
                .unwrap();
            } else {
                writeln!(
                    md,
                    "- **Misleading content:** {} flagged / {} reviewed / {} total cases",
                    stats.misleading_count,
                    stats.misleading_reviewed_count,
                    stats.misleading_applicable_count
                )
                .unwrap();
            }
        }
        writeln!(md).unwrap();

        // Automated check pass rates
        if !stats.automated_check_pass_rates.is_empty() {
            writeln!(md, "## Automated Contract Checks").unwrap();
            writeln!(md).unwrap();
            writeln!(md, "| Check | Pass Rate |").unwrap();
            writeln!(md, "|-------|-----------|").unwrap();
            let mut checks: Vec<_> = stats.automated_check_pass_rates.iter().collect();
            checks.sort_by_key(|(name, _)| (*name).clone());
            for (name, rate) in checks {
                writeln!(md, "| {} | {:.1}% |", name, rate * 100.0).unwrap();
            }
            writeln!(md).unwrap();
        }

        // Human review scores
        if !stats.avg_human_scores.is_empty() {
            writeln!(md, "## Human Quality Review").unwrap();
            writeln!(md).unwrap();
            writeln!(md, "| Field | Average Score |").unwrap();
            writeln!(md, "|-------|---------------|").unwrap();
            let mut scores: Vec<_> = stats.avg_human_scores.iter().collect();
            scores.sort_by_key(|(name, _)| (*name).clone());
            for (name, avg) in scores {
                writeln!(md, "| {} | {:.2}/2 |", name, avg).unwrap();
            }
            writeln!(md).unwrap();
            writeln!(
                md,
                "*Note: Human scores require person evaluation. Factual observations (keyword coverage, target script ratio, length ratio) are automated measurements, not quality judgments.*"
            )
            .unwrap();
            writeln!(md).unwrap();
        }

        // Per-case results
        writeln!(md, "## Per-Case Results").unwrap();
        writeln!(md).unwrap();
        writeln!(
            md,
            "| ID | Category | Latency | Automated | Observations | Human |"
        )
        .unwrap();
        writeln!(
            md,
            "|----|----------|---------|-----------|--------------|-------|"
        )
        .unwrap();
        for case in &run.cases {
            let latency = case
                .automated
                .latency_ms
                .map(|l| format!("{}ms", l))
                .unwrap_or_else(|| "-".to_string());
            let automated_count = count_automated_passes(&case.automated);
            let automated_total = count_automated_checks(&case.automated);
            let automated_str = format!("{}/{}", automated_count, automated_total);
            let observations_str = format_observations(&case.observations);
            let human_count = count_human_scores(&case.human);
            let human_total = count_applicable_human_fields(case.lane);
            let human_str = if human_total > 0 {
                format!("{}/{}", human_count, human_total)
            } else {
                "-".to_string()
            };
            writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} |",
                case.entry_id, case.category, latency, automated_str, observations_str, human_str
            )
            .unwrap();
        }
        writeln!(md).unwrap();

        // Evidence labels
        writeln!(md, "## Evidence Labels").unwrap();
        writeln!(md).unwrap();
        writeln!(
            md,
            "- **AUTOMATED CONTRACT PASS:** Deterministic checks (schema, parsing, exact match)"
        )
        .unwrap();
        writeln!(
            md,
            "- **FACTUAL OBSERVATION:** Automated measurements (keyword coverage, script ratio, length ratio). These are not quality judgments."
        )
        .unwrap();
        writeln!(
            md,
            "- **HUMAN REVIEWED:** Quality judgments requiring person evaluation"
        )
        .unwrap();
        writeln!(
            md,
            "- **LIVE OLLAMA MEASUREMENT:** Requires running Ollama runtime"
        )
        .unwrap();
        writeln!(
            md,
            "- **LIVE LM STUDIO MEASUREMENT:** Requires running LM Studio runtime"
        )
        .unwrap();
        writeln!(md, "- **NOT VERIFIED:** No evidence exists").unwrap();
        writeln!(md, "- **NOT MEASURED:** Applicable but not computed").unwrap();
        writeln!(
            md,
            "- **NOT REVIEWED:** Applicable but human review not performed"
        )
        .unwrap();
        writeln!(md).unwrap();

        md
    }

    /// Format factual observations for display in report table.
    fn format_observations(obs: &FactualObservations) -> String {
        let mut parts = Vec::new();
        if let Some(kw) = obs.keyword_coverage_ratio {
            parts.push(format!("kw={:.0}%", kw * 100.0));
        }
        if let Some(scr) = obs.target_script_ratio {
            parts.push(format!("scr={:.0}%", scr * 100.0));
        }
        if let Some(len) = obs.length_ratio {
            parts.push(format!("len={:.2}", len));
        }
        if parts.is_empty() {
            "NOT MEASURED".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Count how many automated checks passed (only measured checks).
    fn count_automated_passes(checks: &AutomatedChecks) -> usize {
        let mut count = 0;
        if checks.non_empty {
            count += 1;
        }
        if checks.target_language_detected {
            count += 1;
        }
        if checks.schema_valid == Some(true) {
            count += 1;
        }
        if checks.lemma_match == Some(true) {
            count += 1;
        }
        if checks.pos_match == Some(true) {
            count += 1;
        }
        if checks.structure_present == Some(true) {
            count += 1;
        }
        if checks.no_restatement == Some(true) {
            count += 1;
        }
        if checks.provenance_correct == Some(true) {
            count += 1;
        }
        count
    }

    /// Count how many automated checks are applicable (measured).
    fn count_automated_checks(checks: &AutomatedChecks) -> usize {
        let mut count = 2; // non_empty, target_language_detected (always measured)
        if checks.schema_valid.is_some() {
            count += 1;
        }
        if checks.lemma_match.is_some() {
            count += 1;
        }
        if checks.pos_match.is_some() {
            count += 1;
        }
        if checks.structure_present.is_some() {
            count += 1;
        }
        if checks.no_restatement.is_some() {
            count += 1;
        }
        if checks.provenance_correct.is_some() {
            count += 1;
        }
        count
    }

    /// Count how many human scores are present.
    fn count_human_scores(scores: &HumanScores) -> usize {
        let mut count = 0;
        if scores.meaning_preserved.is_some() {
            count += 1;
        }
        if scores.naturalness.is_some() {
            count += 1;
        }
        if scores.completeness.is_some() {
            count += 1;
        }
        if scores.factual_correctness.is_some() {
            count += 1;
        }
        if scores.sense_correctness.is_some() {
            count += 1;
        }
        if scores.grammar_correctness.is_some() {
            count += 1;
        }
        if scores.structure_correctness.is_some() {
            count += 1;
        }
        if scores.usefulness.is_some() {
            count += 1;
        }
        count
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn test_run() -> BenchmarkRun {
            let mut run = BenchmarkRun::new(
                BenchmarkMetadata {
                    model: "test-model".to_string(),
                    provider: "TestProvider".to_string(),
                    endpoint: "127.0.0.1:9999".to_string(),
                    timestamp: "2026-09-21T12:00:00Z".to_string(),
                    runs_per_entry: 3,
                    warm_start: true,
                    corpus_revision: "v1".to_string(),
                    notes: Some("Test run".to_string()),
                },
                BenchmarkLane::Quick,
            );
            let case = BenchmarkCaseResult {
                entry_id: "A01".to_string(),
                category: "general".to_string(),
                lane: BenchmarkLane::Quick,
                run_index: 0,
                source: "Test sentence.".to_string(),
                focused_word: None,
                output: "테스트 문장.".to_string(),
                automated: AutomatedChecks {
                    non_empty: true,
                    target_language_detected: true,
                    schema_valid: None,
                    lemma_match: None,
                    pos_match: None,
                    structure_present: None,
                    no_restatement: None,
                    provenance_correct: None,
                    error_type: None,
                    latency_ms: Some(100),
                },
                observations: FactualObservations {
                    keyword_coverage_ratio: Some(1.0),
                    target_script_ratio: Some(1.0),
                    target_script_detected: Some(true),
                    length_ratio: Some(0.5),
                    length_ratio_within_sanity_band: Some(true),
                },
                human: HumanScores {
                    meaning_preserved: QualityScore::new(2),
                    naturalness: QualityScore::new(1),
                    ..Default::default()
                },
            };
            run.add_case(case);
            run
        }

        #[test]
        fn generate_json_report_succeeds() {
            let run = test_run();
            let json = generate_json_report(&run).expect("JSON generation failed");
            assert!(json.contains("\"model\": \"test-model\""));
            assert!(json.contains("\"entry_id\": \"A01\""));
        }

        #[test]
        fn generate_markdown_report_includes_metadata() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("# P13 Benchmark Report"));
            assert!(md.contains("**Model:** test-model"));
            assert!(md.contains("**Provider:** TestProvider"));
        }

        #[test]
        fn generate_markdown_report_includes_summary() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("## Summary"));
            assert!(md.contains("**Total cases:** 1"));
            assert!(md.contains("**Completed inference cases:** 1"));
        }

        #[test]
        fn generate_markdown_report_includes_automated_checks() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("## Automated Contract Checks"));
            assert!(md.contains("| non_empty | 100.0% |"));
        }

        #[test]
        fn generate_markdown_report_includes_human_scores() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("## Human Quality Review"));
            assert!(md.contains("| meaning_preserved | 2.00/2 |"));
        }

        #[test]
        fn generate_markdown_report_includes_observations_column() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("| Observations |"));
            assert!(md.contains("kw=100%"));
        }

        #[test]
        fn generate_markdown_report_shows_not_measured_for_empty_observations() {
            let mut run = test_run();
            run.cases[0].observations = FactualObservations::default();
            let md = generate_markdown_report(&run);
            assert!(md.contains("NOT MEASURED"));
        }

        #[test]
        fn generate_markdown_report_includes_per_case_results() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("## Per-Case Results"));
            assert!(md.contains("| A01 | general | 100ms |"));
        }

        #[test]
        fn generate_markdown_report_includes_evidence_labels() {
            let run = test_run();
            let md = generate_markdown_report(&run);
            assert!(md.contains("## Evidence Labels"));
            assert!(md.contains("AUTOMATED CONTRACT PASS"));
            assert!(md.contains("FACTUAL OBSERVATION"));
            assert!(md.contains("HUMAN REVIEWED"));
            assert!(md.contains("NOT MEASURED"));
            assert!(md.contains("NOT REVIEWED"));
        }

        #[test]
        fn generate_markdown_report_shows_misleading_not_reviewed() {
            let mut run = BenchmarkRun::new(
                BenchmarkMetadata {
                    model: "test-model".to_string(),
                    provider: "TestProvider".to_string(),
                    endpoint: "127.0.0.1:9999".to_string(),
                    timestamp: "2026-09-21T12:00:00Z".to_string(),
                    runs_per_entry: 3,
                    warm_start: true,
                    corpus_revision: "v1".to_string(),
                    notes: None,
                },
                BenchmarkLane::Deep,
            );
            let case = BenchmarkCaseResult {
                entry_id: "A01".to_string(),
                category: "general".to_string(),
                lane: BenchmarkLane::Deep,
                run_index: 0,
                source: "Test.".to_string(),
                focused_word: None,
                output: "출력".to_string(),
                automated: AutomatedChecks::default(),
                observations: FactualObservations::default(),
                human: HumanScores::default(),
            };
            run.add_case(case);
            let md = generate_markdown_report(&run);
            assert!(md.contains("**Misleading content review:** NOT REVIEWED"));
        }
    }
}
