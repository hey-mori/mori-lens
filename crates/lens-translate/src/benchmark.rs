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
//! - HUMAN REVIEWED: Quality judgments requiring person evaluation
//! - LIVE OLLAMA MEASUREMENT: Requires running Ollama runtime
//! - LIVE LM STUDIO MEASUREMENT: Requires running LM Studio runtime
//! - NOT VERIFIED: No evidence exists
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomatedChecks {
    /// Output is non-empty
    pub non_empty: bool,
    /// Output is in target language (Korean for this project)
    pub target_language_detected: bool,
    /// Response parses through production schema (Deep only)
    pub schema_valid: Option<bool>,
    /// Lemma matches expected (exact match, case-insensitive)
    pub lemma_match: Option<bool>,
    /// Expected POS keyword present (containment match)
    pub pos_match: Option<bool>,
    /// Structure field populated when expected
    pub structure_present: Option<bool>,
    /// Forbidden restatement behavior not detected
    pub no_restatement: bool,
    /// Provenance/model/provider identity correct
    pub provenance_correct: bool,
    /// Timeout/error type (if any)
    pub error_type: Option<String>,
    /// Latency in milliseconds
    pub latency_ms: Option<u64>,
}

/// Human quality review scores for a single corpus entry.
///
/// These require person evaluation and must not be reported as automated pass.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HumanScores {
    /// Quick translation: meaning preserved (0=wrong, 1=partial, 2=correct)
    pub meaning_preserved: Option<u8>,
    /// Quick translation: Korean naturalness (0=unnatural, 1=acceptable, 2=natural)
    pub naturalness: Option<u8>,
    /// Quick translation: material omission/addition (0=major, 1=minor, 2=none)
    pub completeness: Option<u8>,
    /// Deep: lemma/POS factual correctness (0=wrong, 1=partial, 2=correct)
    pub factual_correctness: Option<u8>,
    /// Deep: sense correctness (0=wrong, 1=partial, 2=correct)
    pub sense_correctness: Option<u8>,
    /// Deep: grammar explanation correctness (0=wrong, 1=partial, 2=correct)
    pub grammar_correctness: Option<u8>,
    /// Deep: structure correctness (0=wrong, 1=partial, 2=correct)
    pub structure_correctness: Option<u8>,
    /// Deep: usefulness/clarity (0=useless, 1=acceptable, 2=useful)
    pub usefulness: Option<u8>,
    /// Deep: misleading content flag (true = contains misleading information)
    pub misleading: Option<bool>,
    /// Free-form reviewer note
    pub note: Option<String>,
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
    /// Human quality scores (populated by reviewer)
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
    /// Number of successful cases (no error)
    pub successful_cases: usize,
    /// Number of cases with each automated check passing
    pub automated_check_pass_rates: HashMap<String, f64>,
    /// Average latency in milliseconds (if measured)
    pub avg_latency_ms: Option<f64>,
    /// Median latency in milliseconds (if measured)
    pub median_latency_ms: Option<f64>,
    /// Average human score by field (if reviewed)
    pub avg_human_scores: HashMap<String, f64>,
    /// Number of cases flagged as misleading (Deep only)
    pub misleading_count: Option<usize>,
}

/// Quick translation quality rubric.
///
/// Scores translation quality on a 0-2 scale:
/// - 0 (wrong): Meaning lost, major omission/addition, or not Korean
/// - 1 (acceptable): Meaning mostly preserved, minor awkwardness or omission
/// - 2 (correct): Meaning fully preserved, natural Korean, no material issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickQualityRubric;

impl QuickQualityRubric {
    /// Score meaning preservation.
    ///
    /// - 2: All key concepts and relationships preserved
    /// - 1: Most meaning preserved, minor details lost or added
    /// - 0: Major meaning lost, or output is not a translation
    pub fn score_meaning(output: &str, expected_keywords: &[&str]) -> u8 {
        if output.trim().is_empty() {
            return 0;
        }
        // Check if expected keywords are present in output
        let present = expected_keywords
            .iter()
            .filter(|kw| output.contains(**kw))
            .count();
        let ratio = if expected_keywords.is_empty() {
            1.0
        } else {
            present as f64 / expected_keywords.len() as f64
        };
        if ratio >= 0.8 {
            2
        } else if ratio >= 0.5 {
            1
        } else {
            0
        }
    }

    /// Score Korean naturalness (requires human review or heuristic).
    ///
    /// This is a placeholder that detects Hangul presence.
    /// Real naturalness scoring requires human judgment.
    pub fn score_naturalness_heuristic(output: &str) -> u8 {
        let hangul_count = output
            .chars()
            .filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c))
            .count();
        let total_chars = output.chars().count();
        if total_chars == 0 {
            return 0;
        }
        let hangul_ratio = hangul_count as f64 / total_chars as f64;
        if hangul_ratio >= 0.5 {
            2 // Mostly Korean
        } else if hangul_ratio >= 0.2 {
            1 // Some Korean
        } else {
            0 // Not Korean
        }
    }

    /// Score completeness (no major omission/addition).
    ///
    /// This is a heuristic based on output length relative to source.
    /// Real completeness scoring requires human judgment.
    pub fn score_completeness_heuristic(source: &str, output: &str) -> u8 {
        let source_len = source.chars().count();
        let output_len = output.chars().count();
        if source_len == 0 || output_len == 0 {
            return 0;
        }
        // Korean text is typically shorter than English (fewer characters)
        // Accept 0.3x to 1.5x length ratio
        let ratio = output_len as f64 / source_len as f64;
        if (0.3..=1.5).contains(&ratio) {
            2 // Reasonable length
        } else if (0.2..=2.0).contains(&ratio) {
            1 // Acceptable length
        } else {
            0 // Major length mismatch
        }
    }
}

/// Deep analysis quality rubric.
///
/// Scores analysis fields on a 0-2 scale:
/// - 0 (wrong): Factually incorrect or missing
/// - 1 (partial): Partially correct or incomplete
/// - 2 (correct): Fully correct and useful
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepQualityRubric;

impl DeepQualityRubric {
    /// Score lemma accuracy (exact match, case-insensitive).
    pub fn score_lemma(actual: &str, expected: &str) -> u8 {
        if actual.trim().eq_ignore_ascii_case(expected.trim()) {
            2
        } else if actual.trim().is_empty() {
            0
        } else {
            // Partial credit if they share a common root (heuristic)
            let actual_lower = actual.trim().to_lowercase();
            let expected_lower = expected.trim().to_lowercase();
            if actual_lower.contains(&expected_lower) || expected_lower.contains(&actual_lower) {
                1
            } else {
                0
            }
        }
    }

    /// Score part-of-speech accuracy (keyword containment).
    pub fn score_pos(actual: &str, expected_keywords: &[&str]) -> u8 {
        if actual.trim().is_empty() {
            return 0;
        }
        let present = expected_keywords
            .iter()
            .filter(|kw| actual.contains(**kw))
            .count();
        if present == expected_keywords.len() {
            2
        } else if present > 0 {
            1
        } else {
            0
        }
    }

    /// Score sense correctness (requires human review).
    ///
    /// This is a placeholder. Real sense scoring requires checking if the
    /// explanation is grounded in the sentence context, not generic dictionary.
    pub fn score_sense_placeholder(_actual: &str, _sentence: &str, _word: &str) -> u8 {
        // Real implementation would check if sense mentions the word and relates
        // to the sentence context. For now, return None to indicate human review needed.
        1 // Placeholder: assume partial correctness
    }

    /// Score structure correctness (requires human review).
    ///
    /// This is a placeholder. Real structure scoring requires checking if the
    /// clause analysis is correct.
    pub fn score_structure_placeholder(_actual: &str, _sentence: &str) -> u8 {
        // Real implementation would parse the structure and verify clause relationships.
        // For now, return None to indicate human review needed.
        1 // Placeholder: assume partial correctness
    }

    /// Score grammar explanation correctness (requires human review).
    ///
    /// This is a placeholder. Real grammar scoring requires checking if the
    /// explanation is accurate and not misleading.
    pub fn score_grammar_placeholder(_actual: &str, _expected_grammar: &str) -> u8 {
        // Real implementation would check if the explanation correctly identifies
        // the grammar phenomenon. For now, return None to indicate human review needed.
        1 // Placeholder: assume partial correctness
    }

    /// Check if explanation is misleading.
    ///
    /// Returns true if the explanation contains information that would confuse
    /// or mislead the reader. Requires human review.
    pub fn is_misleading_placeholder(_actual: &str) -> Option<bool> {
        // Real implementation would check for common misleading patterns:
        // - Wrong word sense
        // - Incorrect grammar explanation
        // - Hallucinated information
        // For now, return None to indicate human review needed.
        None
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
        let successful_cases = self
            .cases
            .iter()
            .filter(|c| c.automated.error_type.is_none())
            .count();

        let mut stats = BenchmarkStats {
            total_cases: self.cases.len(),
            successful_cases,
            ..Default::default()
        };

        // Compute automated check pass rates
        let mut check_counts: HashMap<String, (usize, usize)> = HashMap::new();
        for case in &self.cases {
            let checks = [
                ("non_empty", case.automated.non_empty),
                (
                    "target_language_detected",
                    case.automated.target_language_detected,
                ),
                ("no_restatement", case.automated.no_restatement),
                ("provenance_correct", case.automated.provenance_correct),
            ];
            for (name, passed) in checks {
                let entry = check_counts.entry(name.to_string()).or_insert((0, 0));
                entry.0 += if passed { 1 } else { 0 };
                entry.1 += 1;
            }
            // Optional checks
            if let Some(schema_valid) = case.automated.schema_valid {
                let entry = check_counts
                    .entry("schema_valid".to_string())
                    .or_insert((0, 0));
                entry.0 += if schema_valid { 1 } else { 0 };
                entry.1 += 1;
            }
            if let Some(lemma_match) = case.automated.lemma_match {
                let entry = check_counts
                    .entry("lemma_match".to_string())
                    .or_insert((0, 0));
                entry.0 += if lemma_match { 1 } else { 0 };
                entry.1 += 1;
            }
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
            stats.median_latency_ms = Some(sorted[sorted.len() / 2] as f64);
        }

        // Compute human score averages
        let mut score_sums: HashMap<String, (f64, usize)> = HashMap::new();
        for case in &self.cases {
            let scores = [
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
                    entry.0 += val as f64;
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
        if self.lane == BenchmarkLane::Deep {
            stats.misleading_count = Some(
                self.cases
                    .iter()
                    .filter(|c| c.human.misleading == Some(true))
                    .count(),
            );
        }

        stats
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
                no_restatement: true,
                provenance_correct: true,
                error_type: None,
                latency_ms: latency,
            },
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
    fn compute_stats_counts_successful_cases() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        run.add_case(test_case("A01", Some(100)));
        run.add_case(test_case("A02", Some(200)));
        let mut error_case = test_case("A03", None);
        error_case.automated.error_type = Some("Timeout".to_string());
        run.add_case(error_case);

        let stats = run.compute_stats();
        assert_eq!(stats.total_cases, 3);
        assert_eq!(stats.successful_cases, 2);
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
    fn compute_stats_handles_human_scores() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Quick);
        let mut case1 = test_case("A01", Some(100));
        case1.human.meaning_preserved = Some(2);
        case1.human.naturalness = Some(1);
        run.add_case(case1);

        let mut case2 = test_case("A02", Some(200));
        case2.human.meaning_preserved = Some(1);
        case2.human.naturalness = Some(2);
        run.add_case(case2);

        let stats = run.compute_stats();
        assert_eq!(stats.avg_human_scores.get("meaning_preserved"), Some(&1.5));
        assert_eq!(stats.avg_human_scores.get("naturalness"), Some(&1.5));
    }

    #[test]
    fn compute_stats_counts_misleading_for_deep() {
        let mut run = BenchmarkRun::new(test_metadata(), BenchmarkLane::Deep);
        let mut case1 = test_case("A01", Some(100));
        case1.lane = BenchmarkLane::Deep;
        case1.human.misleading = Some(true);
        run.add_case(case1);

        let mut case2 = test_case("A02", Some(200));
        case2.lane = BenchmarkLane::Deep;
        case2.human.misleading = Some(false);
        run.add_case(case2);

        let stats = run.compute_stats();
        assert_eq!(stats.misleading_count, Some(1));
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
    fn quick_rubric_scores_meaning_correctly() {
        let output = "포인터가 문장 위에 멈춥니다.";
        assert_eq!(
            QuickQualityRubric::score_meaning(output, &["포인터", "문장"]),
            2
        );
        assert_eq!(QuickQualityRubric::score_meaning(output, &["포인터"]), 2);
        assert_eq!(
            QuickQualityRubric::score_meaning(output, &["없는키워드"]),
            0
        );
        assert_eq!(QuickQualityRubric::score_meaning("", &["포인터"]), 0);
    }

    #[test]
    fn quick_rubric_scores_naturalness_heuristic() {
        assert_eq!(QuickQualityRubric::score_naturalness_heuristic("테스트"), 2);
        assert_eq!(
            QuickQualityRubric::score_naturalness_heuristic("test 테스트"),
            1
        );
        assert_eq!(QuickQualityRubric::score_naturalness_heuristic("test"), 0);
        assert_eq!(QuickQualityRubric::score_naturalness_heuristic(""), 0);
    }

    #[test]
    fn quick_rubric_scores_completeness_heuristic() {
        assert_eq!(
            QuickQualityRubric::score_completeness_heuristic(
                "The pointer rests on the sentence.",
                "포인터가 문장 위에 멈춥니다."
            ),
            2
        );
        assert_eq!(
            QuickQualityRubric::score_completeness_heuristic(
                "Short",
                "Very very very long output that is way too long"
            ),
            0
        );
        assert_eq!(
            QuickQualityRubric::score_completeness_heuristic("", "output"),
            0
        );
    }

    #[test]
    fn deep_rubric_scores_lemma_correctly() {
        assert_eq!(DeepQualityRubric::score_lemma("rest", "rest"), 2);
        assert_eq!(DeepQualityRubric::score_lemma("REST", "rest"), 2);
        assert_eq!(DeepQualityRubric::score_lemma("resting", "rest"), 1);
        assert_eq!(DeepQualityRubric::score_lemma("run", "rest"), 0);
        assert_eq!(DeepQualityRubric::score_lemma("", "rest"), 0);
    }

    #[test]
    fn deep_rubric_scores_pos_correctly() {
        assert_eq!(DeepQualityRubric::score_pos("동사", &["동사"]), 2);
        assert_eq!(DeepQualityRubric::score_pos("동사, 자동사", &["동사"]), 2);
        assert_eq!(DeepQualityRubric::score_pos("형용사", &["동사"]), 0);
        assert_eq!(DeepQualityRubric::score_pos("", &["동사"]), 0);
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
        writeln!(md, "- **Successful cases:** {}", stats.successful_cases).unwrap();
        if let Some(avg_latency) = stats.avg_latency_ms {
            writeln!(md, "- **Average latency:** {:.1}ms", avg_latency).unwrap();
        }
        if let Some(median_latency) = stats.median_latency_ms {
            writeln!(md, "- **Median latency:** {:.1}ms", median_latency).unwrap();
        }
        if let Some(misleading_count) = stats.misleading_count {
            writeln!(md, "- **Misleading cases:** {}", misleading_count).unwrap();
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
                "*Note: Human scores require person evaluation. Automated heuristics may not reflect true quality.*"
            )
            .unwrap();
            writeln!(md).unwrap();
        }

        // Per-case results
        writeln!(md, "## Per-Case Results").unwrap();
        writeln!(md).unwrap();
        writeln!(md, "| ID | Category | Latency | Automated | Human |").unwrap();
        writeln!(md, "|----|----------|---------|-----------|-------|").unwrap();
        for case in &run.cases {
            let latency = case
                .automated
                .latency_ms
                .map(|l| format!("{}ms", l))
                .unwrap_or_else(|| "-".to_string());
            let automated_count = count_automated_passes(&case.automated);
            let automated_total = count_automated_checks(&case.automated);
            let automated_str = format!("{}/{}", automated_count, automated_total);
            let human_count = count_human_scores(&case.human);
            let human_total = count_human_fields(&case.human);
            let human_str = if human_total > 0 {
                format!("{}/{}", human_count, human_total)
            } else {
                "-".to_string()
            };
            writeln!(
                md,
                "| {} | {} | {} | {} | {} |",
                case.entry_id, case.category, latency, automated_str, human_str
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
        writeln!(md).unwrap();

        md
    }

    /// Count how many automated checks passed.
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
        if checks.no_restatement {
            count += 1;
        }
        if checks.provenance_correct {
            count += 1;
        }
        count
    }

    /// Count how many automated checks are applicable.
    fn count_automated_checks(checks: &AutomatedChecks) -> usize {
        let mut count = 4; // non_empty, target_language_detected, no_restatement, provenance_correct
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

    /// Count how many human score fields are applicable.
    fn count_human_fields(_scores: &HumanScores) -> usize {
        // For simplicity, count all fields as applicable
        // Real implementation would check lane-specific relevance
        8
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
                    no_restatement: true,
                    provenance_correct: true,
                    error_type: None,
                    latency_ms: Some(100),
                },
                human: HumanScores {
                    meaning_preserved: Some(2),
                    naturalness: Some(1),
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
            assert!(md.contains("**Successful cases:** 1"));
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
            assert!(md.contains("HUMAN REVIEWED"));
        }
    }
}
