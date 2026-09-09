//! Visible-range targeting measurement.
//!
//! Microsoft UIA provides `GetVisibleRanges` as an alternative to
//! `RangeFromPoint` for large text containers. This probe measures whether
//! that API exposes line-level geometry that can be used for point-to-line
//! targeting when the normal path fails.

use lens_core::geometry::ScreenPoint;
use lens_platform_windows::{
    foreground_window, measure_ancestor_chain, measure_visible_ranges, top_level_windows,
    WindowInfo,
};

/// Options for the visible-ranges probe.
#[derive(Debug, Default)]
pub struct VisibleRangesOptions {
    /// Only windows whose process name contains this.
    pub only: Option<String>,
    /// Also walk the ancestor chain from probe points.
    pub ancestors: bool,
}

/// Run the visible-ranges probe.
pub fn run(options: VisibleRangesOptions) {
    let windows = if let Some(ref only) = options.only {
        let needle = only.to_lowercase();
        let matches: Vec<_> = top_level_windows()
            .into_iter()
            .filter(|w| {
                w.process_name
                    .as_deref()
                    .is_some_and(|name| name.to_lowercase().contains(&needle))
            })
            .collect();
        if matches.is_empty() {
            eprintln!("visible-ranges  no visible window matching {only:?}");
            return;
        }
        matches
    } else {
        match foreground_window() {
            Ok(f) => vec![f],
            Err(_) => {
                eprintln!("visible-ranges  no foreground window");
                return;
            }
        }
    };

    for window in &windows {
        run_container_scan(window);
        if options.ancestors {
            run_ancestor_scan(window);
        }
    }

    println!(
        "\n  A container answering with many rectangles of a line's height is\n  the whole idea: `line_under` already turns those into an answer."
    );
}

fn run_container_scan(window: &WindowInfo) {
    println!(
        "visible-ranges · {} · {}x{}",
        window.process_name.as_deref().unwrap_or("?"),
        window.client_bounds.width,
        window.client_bounds.height,
    );
    println!(
        "  element        ranges  rects  median height  tallest  grid coverage   point-at-centre"
    );

    let samples = measure_visible_ranges(window.id);

    if samples.is_empty() {
        println!("    (no text-pattern containers found)");
    }

    for (index, sample) in samples.iter().enumerate() {
        let mut heights: Vec<i32> = sample.rects.iter().map(|rect| rect.height).collect();
        heights.sort_unstable();

        let hit = if sample.point_at_centre_total == 0 {
            0.0
        } else {
            sample.point_at_centre_hits as f64 * 100.0 / sample.point_at_centre_total as f64
        };

        let share = if sample.grid_points == 0 {
            0.0
        } else {
            sample.grid_points_on_a_line as f64 * 100.0 / sample.grid_points as f64
        };

        println!(
            "  {:<14} {:>6}  {:>5}  {:>13}  {:>7}  {:>5}/{:<5} ({share:>3.0}%)  {:>3}/{:<3} ({hit:>3.0}%)",
            format!("#{} h={}", index, sample.container_height),
            sample.range_count,
            sample.rects.len(),
            heights.get(heights.len() / 2).copied().unwrap_or(0),
            heights.last().copied().unwrap_or(0),
            sample.grid_points_on_a_line,
            sample.grid_points,
            sample.point_at_centre_hits,
            sample.point_at_centre_total,
        );
        println!(
            "    GetVisibleRanges: {:.1}ms",
            sample.get_visible_ranges_time.as_secs_f64() * 1000.0
        );
    }
}

fn run_ancestor_scan(window: &WindowInfo) {
    let area = &window.client_bounds;
    // Lay a sparse grid across the client area.
    let mut points = Vec::new();
    let step_x = (area.width / 5).max(1);
    let step_y = (area.height / 5).max(1);
    let mut y = area.y + step_y / 2;
    while y < area.y + area.height - step_y / 2 {
        let mut x = area.x + step_x / 2;
        while x < area.x + area.width - step_x / 2 {
            points.push(ScreenPoint::new(x, y));
            x += step_x;
        }
        y += step_y;
    }

    println!("\n  ancestor chain · {} probe points", points.len());
    println!("  point       depth  ct     has_tp  vr_count  line_rects  gvr_ms  coverage");

    let samples = measure_ancestor_chain(window.id, &points);

    for sample in &samples {
        // Find the deepest ancestor with a useful TextPattern.
        let best = sample
            .ancestors
            .iter()
            .filter(|a| a.has_text_pattern)
            .filter(|a| a.line_rect_count.is_some_and(|n| n > 0))
            .max_by_key(|a| a.depth);

        let (depth_str, ct_str, tp_str, vr_str, lr_str, gvr_str, cov_str) = if let Some(b) = best {
            let cov = b.grid_coverage.unwrap_or((0, 0));
            let share = if cov.1 == 0 {
                "  -".to_string()
            } else {
                format!("{:>3.0}%", cov.0 as f64 * 100.0 / cov.1 as f64)
            };
            (
                format!("{}", b.depth),
                format!("{:?}", b.control_type),
                "yes".to_string(),
                format!("{:?}", b.visible_range_count),
                format!("{:?}", b.line_rect_count),
                format!(
                    "{:.1}",
                    b.get_visible_ranges_time
                        .map(|d| d.as_secs_f64() * 1000.0)
                        .unwrap_or(0.0)
                ),
                format!("{}/{} {share}", cov.0, cov.1),
            )
        } else {
            // Show the first ancestor with TextPattern, even if no line rects.
            let first_tp = sample.ancestors.iter().find(|a| a.has_text_pattern);
            if let Some(tp) = first_tp {
                (
                    format!("{}", tp.depth),
                    format!("{:?}", tp.control_type),
                    "yes".to_string(),
                    format!("{:?}", tp.visible_range_count),
                    format!("{:?}", tp.line_rect_count),
                    format!(
                        "{:.1}",
                        tp.get_visible_ranges_time
                            .map(|d| d.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0)
                    ),
                    "no line rects".to_string(),
                )
            } else {
                (
                    "-".to_string(),
                    format!("{:?}", sample.start_control_type),
                    "no".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                )
            }
        };

        println!(
            "  ({:>4},{:>4})  {:>5}  {:<6}  {:<5}  {:>8}  {:>10}  {:>6}  {}",
            sample.point.x,
            sample.point.y,
            depth_str,
            ct_str,
            tp_str,
            vr_str,
            lr_str,
            gvr_str,
            cov_str
        );
    }

    // Summary: how many points found a useful ancestor.
    let total = samples.len();
    let with_useful = samples
        .iter()
        .filter(|s| {
            s.ancestors
                .iter()
                .any(|a| a.has_text_pattern && a.line_rect_count.is_some_and(|n| n > 0))
        })
        .count();
    let chain_p50 = {
        let mut times: Vec<_> = samples.iter().map(|s| s.chain_time).collect();
        times.sort();
        times.get(times.len() / 2).copied()
    };

    println!("\n  summary: {with_useful}/{total} points found a useful text-pattern ancestor");
    if let Some(p50) = chain_p50 {
        println!("  chain walk p50: {:.1}ms", p50.as_secs_f64() * 1000.0);
    }
}
