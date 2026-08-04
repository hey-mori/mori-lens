//! What the probe records, and how it prints it.
//!
//! Two rules govern this file.
//!
//! **Failures are counted separately.** `unsupported`, `empty`, `timeout`,
//! `element-gone`, `protected`, and `stale` are different facts about a
//! provider, and collapsing them into "did not work" would destroy the only
//! data Milestone 1 produces (`docs/decisions/ADR-0008-probe-before-product.md`).
//!
//! **Nothing the user was reading is printed.** Lengths, counts, control ids,
//! and latencies, never the sentence or the paragraph. The one exception is the
//! opt-in `--show-word` flag, which prints the resolved word and nothing wider,
//! and announces itself while it is on (`docs/privacy-model.md` §3).

use std::collections::BTreeMap;
use std::time::Duration;

use lens_core::acquire::AcquireError;
use lens_core::geometry::ScreenPoint;
use lens_core::session::Resolution;

/// One resolved hover, reduced to what may be recorded.
#[derive(Debug)]
pub struct Outcome {
    /// `None` when the read failed before an application could be named.
    pub app: Option<String>,
    pub reason: &'static str,
    pub succeeded: bool,
    pub duplicate: bool,
    pub elapsed: Duration,
    pub control_id: Option<i32>,
    pub woke: bool,
    pub word: Option<String>,
    pub word_chars: usize,
    pub sentence_chars: usize,
    pub paragraph_chars: usize,
    pub word_rects: usize,
    pub context_rects: usize,
    /// How far the resolved text is from where the pointer actually was.
    pub pointer_distance: Option<f64>,
}

impl Outcome {
    pub fn from_resolution(resolution: &Resolution, point: ScreenPoint, elapsed: Duration) -> Self {
        match resolution {
            Resolution::Presented(target) | Resolution::Duplicate(target) => {
                let segmented = &target.segmented;
                Self {
                    app: target.app.process_name.clone(),
                    reason: "ok",
                    succeeded: true,
                    duplicate: matches!(resolution, Resolution::Duplicate(_)),
                    elapsed,
                    control_id: target.provider.control_id,
                    woke: target.provider.woke_accessibility,
                    word: segmented.word_text().map(str::to_owned),
                    word_chars: segmented
                        .word_text()
                        .map(|w| w.chars().count())
                        .unwrap_or(0),
                    sentence_chars: segmented.sentence_text().chars().count(),
                    paragraph_chars: segmented.paragraph_text().chars().count(),
                    word_rects: target.word_bounds.len(),
                    context_rects: target.context_bounds.len(),
                    pointer_distance: target.pointer_distance(point),
                }
            }
            Resolution::Stale => Self::failed(AcquireError::Stale, elapsed),
            Resolution::Failed(error) => Self::failed(*error, elapsed),
        }
    }

    fn failed(error: AcquireError, elapsed: Duration) -> Self {
        Self {
            app: None,
            reason: error.reason(),
            succeeded: false,
            duplicate: false,
            elapsed,
            control_id: None,
            woke: false,
            word: None,
            word_chars: 0,
            sentence_chars: 0,
            paragraph_chars: 0,
            word_rects: 0,
            context_rects: 0,
            pointer_distance: None,
        }
    }

    /// One line per hover.
    pub fn line(&self, expect: &Option<String>, show_word: bool) -> String {
        let app = self.app.as_deref().unwrap_or("?");
        let mut line = format!(
            "{:<16} {:<20} {:>5}ms",
            truncate(app, 16),
            self.reason,
            self.elapsed.as_millis(),
        );

        if !self.succeeded {
            return line;
        }

        line.push_str(&format!(
            "  word={:<3} sent={:<4} para={:<5} rects={}/{}",
            self.word_chars,
            self.sentence_chars,
            self.paragraph_chars,
            self.word_rects,
            self.context_rects,
        ));

        if let Some(distance) = self.pointer_distance {
            // A large distance means `RangeFromPoint` answered with the nearest
            // text rather than the text under the pointer, which is the failure
            // mode a distance threshold has to catch.
            line.push_str(&format!("  dist={distance:.0}"));
        }
        if let Some(control) = self.control_id {
            line.push_str(&format!("  ctrl={control}"));
        }
        if self.woke {
            line.push_str("  woke");
        }
        if self.duplicate {
            line.push_str("  dup");
        }

        // A correctness judgement that costs no content: the operator says what
        // they are pointing at, the probe agrees or does not.
        if let Some(expected) = expect {
            let matched = self
                .word
                .as_deref()
                .is_some_and(|word| word.eq_ignore_ascii_case(expected));
            line.push_str(if matched { "  MATCH" } else { "  MISMATCH" });
        }

        if show_word {
            line.push_str(&format!("  [{}]", self.word.as_deref().unwrap_or("")));
        }

        line
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[derive(Debug, Default)]
struct AppStats {
    successes: usize,
    duplicates: usize,
    wakes: usize,
    /// Keyed by reason so a per-application failure profile survives to the
    /// summary. A single "failures" counter would answer none of the questions
    /// in `docs/research/hover-text-acquisition.md` §10.
    failures: BTreeMap<&'static str, usize>,
    control_ids: BTreeMap<i32, usize>,
    latencies_us: Vec<u128>,
    /// Distances from the pointer to the resolved text, to see whether a
    /// threshold is needed and where it should sit.
    distances: Vec<f64>,
}

/// Everything the run observed, grouped by application.
#[derive(Debug, Default)]
pub struct Report {
    by_app: BTreeMap<String, AppStats>,
    /// Failures that happened before an application could be named.
    unattributed: BTreeMap<&'static str, usize>,
    total: usize,
}

impl Report {
    /// `sampled_app` is the process under the pointer, which is known even when
    /// acquisition failed and the target carried no identity.
    pub fn record(&mut self, sampled_app: Option<&str>, outcome: &Outcome) {
        self.total += 1;

        let name = outcome.app.as_deref().or(sampled_app);
        let Some(name) = name else {
            *self.unattributed.entry(outcome.reason).or_default() += 1;
            return;
        };

        let stats = self.by_app.entry(name.to_owned()).or_default();
        stats.latencies_us.push(outcome.elapsed.as_micros());

        if outcome.succeeded {
            stats.successes += 1;
            if outcome.duplicate {
                stats.duplicates += 1;
            }
            if outcome.woke {
                stats.wakes += 1;
            }
            if let Some(control) = outcome.control_id {
                *stats.control_ids.entry(control).or_default() += 1;
            }
            if let Some(distance) = outcome.pointer_distance {
                stats.distances.push(distance);
            }
        } else {
            *stats.failures.entry(outcome.reason).or_default() += 1;
        }
    }

    pub fn print_summary(&self, duration: Duration) {
        println!(
            "summary · {} hovers over {:.0}s",
            self.total,
            duration.as_secs_f64()
        );

        if self.by_app.is_empty() && self.unattributed.is_empty() {
            println!("\n  Nothing resolved. If the mode line said \"hold Alt\", hold Alt.");
            return;
        }

        for (app, stats) in &self.by_app {
            let attempts = stats.successes + stats.failures.values().sum::<usize>();
            let rate = if attempts == 0 {
                0.0
            } else {
                stats.successes as f64 * 100.0 / attempts as f64
            };

            println!("\n  {app}");
            println!(
                "    acquired    {}/{} ({rate:.0}%)",
                stats.successes, attempts
            );
            println!(
                "    latency     p50 {} · p95 {}",
                format_us(percentile(&stats.latencies_us, 50.0)),
                format_us(percentile(&stats.latencies_us, 95.0)),
            );
            if !stats.distances.is_empty() {
                let mut sorted = stats.distances.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                println!(
                    "    pointer     p50 {:.0}px · max {:.0}px from the resolved text",
                    sorted[sorted.len() / 2],
                    sorted.last().copied().unwrap_or(0.0),
                );
            }
            if stats.wakes > 0 {
                println!(
                    "    wake        {} of {} reads needed the accessibility wake",
                    stats.wakes, stats.successes
                );
            }
            if stats.duplicates > 0 {
                println!("    duplicate   {}", stats.duplicates);
            }
            if !stats.control_ids.is_empty() {
                let ids: Vec<String> = stats
                    .control_ids
                    .iter()
                    .map(|(id, count)| format!("{id}x{count}"))
                    .collect();
                println!("    control     {}", ids.join(" "));
            }
            for (reason, count) in &stats.failures {
                println!("    {reason:<12}{count}");
            }
        }

        if !self.unattributed.is_empty() {
            println!("\n  (no application identified)");
            for (reason, count) in &self.unattributed {
                println!("    {reason:<12}{count}");
            }
        }

        println!("\nTranscribe this into docs/compatibility/windows-app-matrix.md with the");
        println!("date, the build sha, and the DPI scaling. A number without those is not");
        println!("a result.");
    }
}

/// Nearest-rank percentile. `None` for an empty sample rather than a zero,
/// because a zero here would read as "instant" instead of "never measured".
fn percentile(samples: &[u128], percent: f64) -> Option<u128> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((percent / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted.get(rank.saturating_sub(1)).copied()
}

fn format_us(value: Option<u128>) -> String {
    match value {
        Some(micros) => format!("{:.1}ms", micros as f64 / 1000.0),
        None => "-".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_an_empty_sample_are_absent_not_zero() {
        assert_eq!(percentile(&[], 50.0), None);
        assert_eq!(format_us(None), "-");
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let samples = [10u128, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&samples, 50.0), Some(50));
        assert_eq!(percentile(&samples, 95.0), Some(100));
        assert_eq!(percentile(&samples, 10.0), Some(10));
    }

    #[test]
    fn a_failure_is_attributed_to_the_sampled_application() {
        // Acquisition failed, so the target carried no identity, but the
        // pointer loop still knew which application it was over. Losing that
        // would make every failure unattributed, which is the one thing the
        // matrix cannot use.
        let mut report = Report::default();
        let outcome = Outcome::failed(AcquireError::Unsupported, Duration::from_millis(3));
        report.record(Some("notepad"), &outcome);

        let stats = report.by_app.get("notepad").expect("attributed to notepad");
        assert_eq!(stats.failures.get("unsupported"), Some(&1));
        assert!(report.unattributed.is_empty());
    }

    #[test]
    fn each_failure_reason_is_counted_separately() {
        let mut report = Report::default();
        for error in [
            AcquireError::Unsupported,
            AcquireError::Unsupported,
            AcquireError::Empty,
            AcquireError::Timeout,
        ] {
            report.record(
                Some("chrome"),
                &Outcome::failed(error, Duration::from_millis(1)),
            );
        }

        let stats = report.by_app.get("chrome").expect("recorded");
        assert_eq!(stats.failures.get("unsupported"), Some(&2));
        assert_eq!(stats.failures.get("empty"), Some(&1));
        assert_eq!(stats.failures.get("timeout"), Some(&1));
    }

    #[test]
    fn a_failure_line_carries_no_counts_it_did_not_measure() {
        let outcome = Outcome::failed(AcquireError::Unsupported, Duration::from_millis(7));
        let line = outcome.line(&None, false);
        assert!(line.contains("unsupported"));
        assert!(!line.contains("word="), "printed a count for a failed read");
    }

    #[test]
    fn the_default_line_prints_no_content() {
        let outcome = Outcome {
            app: Some("chrome".into()),
            reason: "ok",
            succeeded: true,
            duplicate: false,
            elapsed: Duration::from_millis(12),
            control_id: Some(50030),
            woke: true,
            word: Some("produce".into()),
            word_chars: 7,
            sentence_chars: 81,
            paragraph_chars: 240,
            word_rects: 1,
            context_rects: 2,
            pointer_distance: Some(0.0),
        };

        let line = outcome.line(&None, false);
        assert!(!line.contains("produce"), "leaked the word by default");
        assert!(line.contains("word=7"));
        assert!(line.contains("woke"));

        // Opt in, and only the word appears.
        let shown = outcome.line(&None, true);
        assert!(shown.contains("[produce]"));

        // The expectation check reports a verdict without printing anything.
        let matched = outcome.line(&Some("produce".into()), false);
        assert!(matched.contains("MATCH"));
        assert!(!matched.contains("produce"));
        let mismatched = outcome.line(&Some("consume".into()), false);
        assert!(mismatched.contains("MISMATCH"));
    }
}
