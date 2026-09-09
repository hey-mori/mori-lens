//! Visible-range targeting measurement.
//!
//! Microsoft UIA provides `GetVisibleRanges` as an alternative to
//! `RangeFromPoint` for large text containers. This probe measures whether
//! that API exposes line-level geometry that can be used for point-to-line
//! targeting when the normal path fails.

use lens_platform_windows::{foreground_window, measure_visible_ranges, top_level_windows};

/// Options for the visible-ranges probe.
#[derive(Debug, Default)]
pub struct VisibleRangesOptions {
    /// Only windows whose process name contains this.
    pub only: Option<String>,
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

    println!(
        "\n  A container answering with many rectangles of a line's height is\n  the whole idea: `line_under` already turns those into an answer."
    );
}
