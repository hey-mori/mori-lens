//! Measure a whole window without touching the pointer.
//!
//! `ElementFromPoint` and `RangeFromPoint` take a screen coordinate as an
//! argument. Neither needs the physical cursor to be there. So acquisition can
//! be measured across a real application's entire client area, hundreds of
//! points, with the user's mouse untouched and nobody driving it by hand.
//!
//! What that buys over hovering by hand:
//!
//! - **Coverage of a real layout** rather than the four or five spots a person
//!   happens to try. Body text, headings, chrome, margins, and images all get
//!   sampled, and the failure distribution across them is the row the
//!   compatibility matrix actually wants.
//! - **A correctness check that needs no human.** `RangeFromPoint` answers with
//!   the *nearest* text, so a point in a margin still resolves to something.
//!   Whether the resolved word's rectangle **contains the sample point** is a
//!   machine-checkable proxy for "the right word", and it needs no printed
//!   text.
//! - **The wake's real effect**, by sweeping cold, waking once, and sweeping
//!   again. The per-hover retry mixes those two measurements together.
//!
//! What it does not replace: a sweep does not know that a pixel holds prose
//! rather than a button label. A person still spot-checks a few points per
//! application. The sweep makes that a spot check instead of the whole method.
//!
//! # It loads the desktop it is measuring
//!
//! `top_level_windows` says this enumeration changes nothing, and that is true
//! of window *state*. It is false of load, and the difference matters.
//!
//! Every grid point is a cross-process COM call, and the work happens on the
//! **target application's UI thread**. Sweeping fifteen windows at a few
//! hundred points each therefore makes Chrome, an Electron editor, Explorer
//! and every other window do synchronous accessibility work, all at once. The
//! machine's owner feels that as the whole desktop going sluggish, which is
//! exactly what happened on 2026-08-05.
//!
//! It compounds: as applications slow down, each call takes longer, so the
//! sweep runs longer, so the load lasts longer. Latencies during that session
//! reached 8,328ms and a single call blocked for 45 seconds. Measured
//! afterwards on the same machine at rest, the same calls run at **p50 3.9ms**.
//!
//! Two consequences worth keeping in mind:
//!
//! - **A sweep's own latency figures are measured under a sweep's load.** They
//!   are an upper bound on what a hover costs, not an estimate of it.
//! - **Sweep one application at a time**, with `--only`. Sweeping everything
//!   is what makes the desktop unusable, and the results are worse as well as
//!   more expensive.

use std::time::{Duration, Instant};

use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::Generation;
use lens_core::policy::AppPolicy;
use lens_platform_windows::{root_window_at, top_level_windows, UiaTextAcquirer, WindowInfo};

use super::report::{format_us, percentile};

/// How the grid is chosen, and how much work one window is allowed.
#[derive(Debug, Clone, Copy)]
pub struct SweepOptions {
    /// Roughly how many points to sample per window. The step is derived from
    /// this and the window's area, so a large window is not swept at a finer
    /// resolution than a small one.
    pub points_per_window: usize,
    /// Give up on a window after this long. A wedged application must not
    /// stall the whole run.
    pub window_budget: Duration,
    /// Sweep once cold, wake, then sweep again, to separate the two.
    pub measure_wake: bool,
}

impl Default for SweepOptions {
    fn default() -> Self {
        Self {
            points_per_window: 240,
            window_budget: Duration::from_secs(20),
            measure_wake: true,
        }
    }
}

/// What one pass over one window found.
#[derive(Debug, Default, Clone)]
pub struct PassStats {
    pub sampled: usize,
    pub acquired: usize,
    /// Acquisitions whose word rectangle contains the sample point.
    pub on_target: usize,
    /// Acquisitions that reported a rectangle at all. A range scrolled out of
    /// view has none, and counting those as off-target would blame the
    /// resolution for a window that is simply not showing the text.
    pub with_bounds: usize,
    pub distances: Vec<f64>,
    pub latencies_us: Vec<u128>,
    pub failures: std::collections::BTreeMap<&'static str, usize>,
    pub control_ids: std::collections::BTreeMap<i32, usize>,
    /// What the provider gave back for a word-unit expansion request.
    ///
    /// An over-expanded answer anchors an overlay to a whole line, so the
    /// distribution is a product fact rather than a curiosity.
    pub expanded_units: std::collections::BTreeMap<&'static str, usize>,
    pub truncated: bool,
    /// The shape of the misses, which is what settles question 8 in the
    /// research brief.
    pub off_target: OffTargetShape,
    /// Distinct rectangles among the hits that landed on their target.
    ///
    /// The number that answers "can it tell which message the pointer is on".
    /// A chat window whose forty on-target samples resolve to two rectangles
    /// is one blob with a pointer somewhere in it; one that resolves to twenty
    /// has twenty separable things to explain. The on-target *rate* says
    /// nothing about this: a single huge correct target scores 100%.
    pub distinct_targets: std::collections::BTreeSet<(i32, i32, i32, i32)>,
    /// Points inside the window's rectangle that another window covers.
    ///
    /// Not sampled, and not counted as anything else. Without this check the
    /// sweep measures the occluder and files the result under the occluded
    /// window: the first run of this harness reported identical statistics for
    /// a background Chrome window and the full-screen IME host on top of it,
    /// which is how the bug was found.
    pub occluded: usize,
}

impl PassStats {
    fn record(&mut self, outcome: Result<PointOutcome, AcquireError>, elapsed: Duration) {
        self.sampled += 1;
        self.latencies_us.push(elapsed.as_micros());
        match outcome {
            Ok(point) => {
                self.acquired += 1;
                if let Some(control) = point.control_id {
                    *self.control_ids.entry(control).or_default() += 1;
                }
                *self.expanded_units.entry(point.expanded_unit).or_default() += 1;
                if let Some(distance) = point.distance {
                    self.with_bounds += 1;
                    self.distances.push(distance);
                    if distance == 0.0 {
                        self.on_target += 1;
                        if let Some(rect) = point.rect {
                            self.distinct_targets
                                .insert((rect.x, rect.y, rect.width, rect.height));
                        }
                    } else if let (Some(rect), Some(sample)) = (point.rect, point.sample) {
                        self.off_target.record(sample, rect, point.within_gate);
                    }
                }
            }
            Err(error) => {
                *self.failures.entry(error.reason()).or_default() += 1;
            }
        }
    }

    fn rate(hit: usize, total: usize) -> f64 {
        if total == 0 {
            0.0
        } else {
            hit as f64 * 100.0 / total as f64
        }
    }
}

struct PointOutcome {
    control_id: Option<i32>,
    /// `None` when the resolved range has no visible rectangle.
    distance: Option<f64>,
    /// Where the resolved text actually is, when it is anywhere.
    rect: Option<ScreenRect>,
    /// The point that was sampled, carried so the miss's direction can be
    /// worked out where the outcome is recorded.
    sample: Option<ScreenPoint>,
    /// Whether the product gate would treat this as pointing at the word.
    within_gate: bool,
    /// What a word-unit expansion actually returned.
    expanded_unit: &'static str,
}

/// Which side of the resolved rectangle the sample point fell on.
///
/// This exists to settle one question: when acquisition resolves text far from
/// the point, is that the *nearest line above an empty region*, or is
/// resolution inside a document simply inaccurate? The two call for opposite
/// products. Empty space means suppress the card, because there is nothing
/// there to explain. Inaccurate resolution means the text the user pointed at
/// exists and was missed, and suppressing would throw it away.
///
/// The signatures differ and are machine-checkable:
///
/// - **Empty space** concentrates. A handful of rectangles act as attractors,
///   the last rendered line catching every point below it, so the points sit
///   predominantly *below* what they resolved to.
/// - **Inaccurate resolution** scatters. Offsets point every way and each
///   sample lands on its own rectangle.
#[derive(Debug, Default, Clone)]
pub struct OffTargetShape {
    pub below: usize,
    pub above: usize,
    pub left: usize,
    pub right: usize,
    /// How many samples resolved to each distinct rectangle. A small map
    /// against a large sample count is the attractor signature.
    pub rects: std::collections::BTreeMap<(i32, i32, i32, i32), usize>,
    /// Misses the product gate would nonetheless accept as slack.
    ///
    /// Tracked so every later run re-tests ADR-0009's threshold rather than
    /// leaving it justified only by the run that set it.
    pub within_gate: usize,
}

impl OffTargetShape {
    fn record(&mut self, point: ScreenPoint, rect: ScreenRect, within_gate: bool) {
        if within_gate {
            self.within_gate += 1;
        }
        // Vertical first: a text line is wide and short, so "below the last
        // line" is the signature worth reading, and a point below a line is
        // usually also within its horizontal span.
        if point.y >= rect.bottom() {
            self.below += 1;
        } else if point.y < rect.y {
            self.above += 1;
        } else if point.x < rect.x {
            self.left += 1;
        } else {
            self.right += 1;
        }
        *self
            .rects
            .entry((rect.x, rect.y, rect.width, rect.height))
            .or_default() += 1;
    }

    pub fn total(&self) -> usize {
        self.below + self.above + self.left + self.right
    }

    /// Samples per distinct rectangle. High means a few attractors caught
    /// everything, which is the empty-space signature.
    pub fn concentration(&self) -> f64 {
        if self.rects.is_empty() {
            return 0.0;
        }
        self.total() as f64 / self.rects.len() as f64
    }
}

/// One window's row.
#[derive(Debug)]
pub struct WindowRow {
    pub process_name: String,
    pub title: Option<String>,
    pub bounds: ScreenRect,
    pub cold: PassStats,
    /// `None` when the wake was not measured, or when the window was already
    /// woken by an earlier pass in the same run.
    pub woken: Option<PassStats>,
}

/// Sample a grid inside `bounds`, inset so the outermost pixels of a client
/// area do not dominate a small window.
fn grid(bounds: ScreenRect, target_points: usize) -> Vec<ScreenPoint> {
    let inset = 4;
    let width = (bounds.width - inset * 2).max(1);
    let height = (bounds.height - inset * 2).max(1);

    // Choose a step so the point count lands near the target regardless of
    // window size. Clamped: finer than 8px measures the same glyph repeatedly,
    // coarser than 96px walks straight past a line of text.
    let area = f64::from(width) * f64::from(height);
    let step = (area / target_points.max(1) as f64).sqrt().round() as i32;
    let step = step.clamp(8, 96);

    let mut points = Vec::new();
    let mut y = bounds.y + inset;
    while y < bounds.y + inset + height {
        let mut x = bounds.x + inset;
        while x < bounds.x + inset + width {
            points.push(ScreenPoint::new(x, y));
            x += step;
        }
        y += step;
    }
    points
}

fn sample(
    acquirer: &UiaTextAcquirer,
    window_id: i64,
    points: &[ScreenPoint],
    budget: Duration,
) -> PassStats {
    let mut stats = PassStats::default();
    let started = Instant::now();
    // The wake is driven by the sweep, not by the acquirer, so the two passes
    // measure what they claim to.
    let context = AcquireContext {
        allow_accessibility_wake: false,
        ..AcquireContext::new(Generation(0))
    };

    for point in points {
        if started.elapsed() > budget {
            stats.truncated = true;
            break;
        }
        // Checked per point rather than per window, because the covering
        // window is usually only over part of this one.
        if root_window_at(*point) != Some(window_id) {
            stats.occluded += 1;
            continue;
        }
        let began = Instant::now();
        let result = acquirer.acquire(*point, &context);
        let elapsed = began.elapsed();

        let outcome = result.map(|target| PointOutcome {
            control_id: target.provider.control_id,
            distance: target.pointer_distance(*point),
            rect: target.nearest_rect_to(*point),
            sample: Some(*point),
            within_gate: target.is_on_target(*point),
            expanded_unit: target.provider.expanded_unit.as_str(),
        });
        stats.record(outcome, elapsed);
    }
    stats
}

/// Sweep every visible window the policy allows.
pub fn run(
    acquirer: &UiaTextAcquirer,
    policy: &AppPolicy,
    options: SweepOptions,
    only: Option<&str>,
) -> Vec<WindowRow> {
    let mut rows = Vec::new();

    for window in targets(policy, only) {
        let process_name = window
            .process_name
            .clone()
            .unwrap_or_else(|| "?".to_owned());
        let points = grid(window.client_bounds, options.points_per_window);

        println!(
            "  sweeping {:<16} {:>4} points  {}x{}",
            truncate(&process_name, 16),
            points.len(),
            window.client_bounds.width,
            window.client_bounds.height,
        );

        let cold = sample(acquirer, window.id, &points, options.window_budget);

        let woken = if options.measure_wake {
            // Returns false when this window was already woken, in which case
            // the second pass would measure nothing new and is skipped rather
            // than reported as a wake that changed nothing.
            acquirer
                .wake_window(window.id)
                .then(|| sample(acquirer, window.id, &points, options.window_budget))
        } else {
            None
        };

        rows.push(WindowRow {
            process_name,
            title: window.title.clone(),
            bounds: window.client_bounds,
            cold,
            woken,
        });
    }

    rows
}

/// Windows worth sweeping: visible, allowed by policy, and not our own.
fn targets(policy: &AppPolicy, only: Option<&str>) -> Vec<WindowInfo> {
    let needle = only.map(str::to_lowercase);
    top_level_windows()
        .into_iter()
        .filter(|window| {
            // A denied application is never read, and a sweep is a read. The
            // policy is checked with the modifier held, because a sweep is a
            // deliberate act: the question is only whether this application is
            // off limits entirely.
            if !policy.allows(window.process_name.as_deref(), true) {
                return false;
            }
            match &needle {
                None => true,
                Some(needle) => {
                    let process = window.process_name.as_deref().unwrap_or("").to_lowercase();
                    let title = window.title.as_deref().unwrap_or("").to_lowercase();
                    process.contains(needle) || title.contains(needle)
                }
            }
        })
        .collect()
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

/// Print the rows in the shape the compatibility matrix wants.
pub fn print_report(rows: &[WindowRow]) {
    if rows.is_empty() {
        println!("\nNo window matched. Nothing was swept, and that is not a result.");
        return;
    }

    println!("\nsweep · {} windows\n", rows.len());

    for row in rows {
        // The window title routinely carries a document name, which is the
        // user's content. Only its presence is reported, never the string.
        //
        // The client size is reported because a row is not comparable without
        // it: the same application swept at 400x300 and at 2560x1400 is two
        // different layouts, and the second says far more about the provider.
        println!(
            "  {}   {}x{} at ({}, {}){}",
            row.process_name,
            row.bounds.width,
            row.bounds.height,
            row.bounds.x,
            row.bounds.y,
            if row.title.is_some() {
                ""
            } else {
                "   (untitled)"
            },
        );
        print_pass("cold", &row.cold);
        if let Some(woken) = &row.woken {
            print_pass("woken", woken);
            print_wake_verdict(&row.cold, woken);
        }
        println!();
    }

    println!("How to read this, so a row is not over-claimed:");
    println!();
    println!("  acquired   is NOT a provider quality score. A uniform grid lands mostly on");
    println!("             chrome, margins, and images, where `unsupported` is the correct");
    println!("             answer. It measures how much of this window is text.");
    println!(
        "  separable  how many distinct things the window exposes. A window whose
             on-target hits all resolve to one rectangle is a single blob:
             the pointer is in it, but there is no telling which sentence.
             The on-target rate cannot see this, because one huge correct
             target scores 100%.
  on-target  is the one to watch. It is the share of acquisitions whose word"
    );
    println!("             rectangle contains the sample point. A low rate with a large");
    println!("             distance means RangeFromPoint answered with the nearest text");
    println!("             rather than the text under the point, which is what a distance");
    println!("             threshold has to catch.");
    println!("  occluded   points another window covers. Skipped, not counted, because");
    println!("             sampling them would measure the window on top.");
    println!();
    println!(
        "Built from {}. Compare that against `git rev-parse --short HEAD`: cargo test",
        super::build_stamp(),
    );
    println!("does not rebuild the binary, so a stale one measures old code and says");
    println!("nothing about it.");
    println!();
    println!("Transcribe into docs/compatibility/windows-app-matrix.md with the date, the");
    println!("build sha above, and the DPI scaling. A number without those is not a result.");
}

fn print_pass(label: &str, stats: &PassStats) {
    let acquired = PassStats::rate(stats.acquired, stats.sampled);
    println!(
        "    {label:<6} acquired {}/{} ({acquired:.0}%)   latency p50 {} · p95 {}{}{}",
        stats.acquired,
        stats.sampled,
        format_us(percentile(&stats.latencies_us, 50.0)),
        format_us(percentile(&stats.latencies_us, 95.0)),
        if stats.occluded > 0 {
            format!("  ({} occluded)", stats.occluded)
        } else {
            String::new()
        },
        if stats.truncated { "  [budget]" } else { "" },
    );

    if stats.with_bounds > 0 {
        let on_target = PassStats::rate(stats.on_target, stats.with_bounds);
        let mut sorted = stats.distances.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        println!(
            "           on-target {}/{} ({on_target:.0}%)   distance p50 {:.0}px · p95 {:.0}px · max {:.0}px",
            stats.on_target,
            stats.with_bounds,
            sorted[sorted.len() / 2],
            sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)],
            sorted.last().copied().unwrap_or(0.0),
        );
    }
    if stats.on_target > 0 {
        println!(
            "           separable {} distinct targets among {} on-target hits ({:.1} hits each)",
            stats.distinct_targets.len(),
            stats.on_target,
            stats.on_target as f64 / stats.distinct_targets.len().max(1) as f64,
        );
    }

    let shape = &stats.off_target;
    if shape.total() > 0 {
        println!(
            "           off-target {} points → {} distinct rects ({:.1} each)   below {} · above {} · left {} · right {}",
            shape.total(),
            shape.rects.len(),
            shape.concentration(),
            shape.below,
            shape.above,
            shape.left,
            shape.right,
        );
        // The product gate is more lenient than the strict containment this
        // report measures, because slack on a label is not a wrong target.
        // Printing what the gate would do keeps ADR-0009 falsifiable on every
        // later run instead of frozen at the numbers that justified it.
        println!(
            "           gate would accept {}/{} of those as slack, at one line height",
            shape.within_gate,
            shape.total(),
        );
    }
    if stats.acquired > stats.with_bounds {
        println!(
            "           {} acquired with no visible rectangle (scrolled out of view)",
            stats.acquired - stats.with_bounds,
        );
    }
    if !stats.expanded_units.is_empty() {
        let rendered: Vec<String> = stats
            .expanded_units
            .iter()
            .map(|(unit, count)| format!("{unit} {count}"))
            .collect();
        println!("           word unit {}", rendered.join(" · "));
    }
    if !stats.control_ids.is_empty() {
        let mut ids: Vec<_> = stats.control_ids.iter().collect();
        ids.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        let rendered: Vec<String> = ids
            .iter()
            .take(4)
            .map(|(id, count)| format!("{id}x{count}"))
            .collect();
        println!("           control {}", rendered.join(" "));
    }
    if !stats.failures.is_empty() {
        let rendered: Vec<String> = stats
            .failures
            .iter()
            .map(|(reason, count)| format!("{reason} {count}"))
            .collect();
        println!("           {}", rendered.join(" · "));
    }
}

/// State plainly whether the wake earned its cost, so the number is not left
/// for a reader to work out from two tables.
fn print_wake_verdict(cold: &PassStats, woken: &PassStats) {
    let gained = woken.acquired as i64 - cold.acquired as i64;
    let verdict = match gained {
        g if g > 0 => format!("+{g} acquisitions, so the wake earned its cost here"),
        0 => "no change, so this provider was never cold".to_owned(),
        g => format!("{g} acquisitions, which means the window changed under the sweep").to_owned(),
    };
    println!("           wake: {verdict}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_lands_near_the_requested_point_count() {
        let bounds = ScreenRect::new(0, 0, 1600, 900);
        let points = grid(bounds, 240);
        // Nearest-step rounding cannot hit the target exactly; within a factor
        // of two is what makes windows of different sizes comparable.
        assert!(
            (120..=480).contains(&points.len()),
            "got {} points",
            points.len()
        );
    }

    #[test]
    fn the_grid_stays_inside_the_window() {
        let bounds = ScreenRect::new(100, 200, 400, 300);
        for point in grid(bounds, 100) {
            assert!(bounds.contains(point), "{point:?} escaped {bounds:?}");
        }
    }

    #[test]
    fn a_negative_origin_window_is_swept_like_any_other() {
        // A monitor left of the primary. This is where unsigned arithmetic and
        // clamped-to-zero grids quietly sweep the wrong screen.
        let bounds = ScreenRect::new(-1920, -100, 800, 600);
        let points = grid(bounds, 100);
        assert!(!points.is_empty());
        assert!(points.iter().all(|point| bounds.contains(*point)));
        assert!(points.iter().any(|point| point.x < 0 && point.y < 0));
    }

    #[test]
    fn a_tiny_window_still_yields_at_least_one_point() {
        let points = grid(ScreenRect::new(0, 0, 10, 10), 240);
        assert_eq!(points.len(), 1);
    }

    #[test]
    fn a_point_inside_the_word_rectangle_counts_as_on_target() {
        let mut stats = PassStats::default();
        stats.record(
            Ok(PointOutcome {
                control_id: Some(50030),
                distance: Some(0.0),
                rect: None,
                sample: None,
                within_gate: true,
                expanded_unit: "word",
            }),
            Duration::from_millis(4),
        );
        stats.record(
            Ok(PointOutcome {
                control_id: Some(50030),
                distance: Some(37.0),
                rect: Some(ScreenRect::new(0, 0, 10, 10)),
                sample: Some(ScreenPoint::new(50, 50)),
                within_gate: false,
                expanded_unit: "word",
            }),
            Duration::from_millis(4),
        );
        assert_eq!(stats.acquired, 2);
        assert_eq!(stats.with_bounds, 2);
        assert_eq!(stats.on_target, 1);
    }

    #[test]
    fn an_acquisition_with_no_rectangle_is_not_counted_against_on_target() {
        // Text scrolled out of view resolves fine and has no rectangle.
        // Counting it as off-target would blame the resolution for a window
        // that is simply not showing the text.
        let mut stats = PassStats::default();
        stats.record(
            Ok(PointOutcome {
                control_id: None,
                distance: None,
                rect: None,
                sample: None,
                within_gate: true,
                expanded_unit: "word",
            }),
            Duration::from_millis(2),
        );
        assert_eq!(stats.acquired, 1);
        assert_eq!(stats.with_bounds, 0);
        assert_eq!(stats.on_target, 0);
        assert!(stats.distances.is_empty());
    }

    #[test]
    fn an_occluded_point_is_not_counted_as_a_sample() {
        // The bug this guards: sampling a covered point measures the window on
        // top and files the answer under the window underneath. The first run
        // of this harness reported byte-identical statistics for a background
        // Chrome window and the full-screen IME host covering it.
        let mut stats = PassStats::default();
        stats.occluded += 1;
        stats.record(
            Ok(PointOutcome {
                control_id: Some(50020),
                distance: Some(0.0),
                rect: Some(ScreenRect::new(0, 0, 10, 10)),
                sample: Some(ScreenPoint::new(5, 5)),
                within_gate: true,
                expanded_unit: "word",
            }),
            Duration::from_millis(3),
        );

        assert_eq!(stats.sampled, 1, "an occluded point was counted as sampled");
        assert_eq!(stats.occluded, 1);
        assert_eq!(stats.latencies_us.len(), 1, "timed a point it never read");
    }

    /// Points scattered below one line of text, which is what an empty region
    /// under the last rendered line looks like.
    #[test]
    fn empty_space_below_a_line_concentrates_on_one_attractor() {
        let last_line = ScreenRect::new(100, 200, 400, 18);
        let mut shape = OffTargetShape::default();
        for step in 0..20 {
            shape.record(
                ScreenPoint::new(120 + step * 15, 300 + step * 10),
                last_line,
                false,
            );
        }

        assert_eq!(shape.total(), 20);
        assert_eq!(shape.rects.len(), 1, "one line should catch them all");
        assert_eq!(shape.concentration(), 20.0);
        assert_eq!(shape.below, 20, "every point was below the line");
        assert_eq!(shape.within_gate, 0, "82px and up is not slack");
    }

    /// Each point resolving to its own rectangle, in every direction, which is
    /// what inaccurate resolution would look like.
    #[test]
    fn scattered_resolution_leaves_a_rect_per_sample() {
        let mut shape = OffTargetShape::default();
        for step in 0..20i32 {
            shape.record(
                ScreenPoint::new(500, 500),
                ScreenRect::new(100 + step * 40, 100 + step * 37, 30, 18),
                false,
            );
        }

        assert_eq!(shape.rects.len(), 20, "no attractor should emerge");
        assert_eq!(shape.concentration(), 1.0);
        assert!(
            shape.below > 0 && shape.above > 0,
            "a scatter should land on both sides, got below {} above {}",
            shape.below,
            shape.above,
        );
    }

    #[test]
    fn an_on_target_point_is_not_recorded_as_a_miss() {
        let mut stats = PassStats::default();
        stats.record(
            Ok(PointOutcome {
                control_id: Some(50020),
                distance: Some(0.0),
                rect: Some(ScreenRect::new(0, 0, 10, 10)),
                sample: Some(ScreenPoint::new(5, 5)),
                within_gate: true,
                expanded_unit: "word",
            }),
            Duration::from_millis(1),
        );
        assert_eq!(stats.on_target, 1);
        assert_eq!(stats.off_target.total(), 0);
    }

    #[test]
    fn failures_are_counted_by_reason_not_lumped_together() {
        let mut stats = PassStats::default();
        for error in [
            AcquireError::Unsupported,
            AcquireError::Unsupported,
            AcquireError::Empty,
        ] {
            stats.record(Err(error), Duration::from_millis(1));
        }
        assert_eq!(stats.failures.get("unsupported"), Some(&2));
        assert_eq!(stats.failures.get("empty"), Some(&1));
        assert_eq!(stats.acquired, 0);
        assert_eq!(stats.sampled, 3);
    }
}
