//! Measure what OCR can read where accessibility cannot.
//!
//! Milestone 4 was moved ahead of deep analysis on one pair of numbers: VS Code
//! returned 7 acquisitions across 247 sampled points and KakaoTalk 14 across
//! 104, with **one** separable target between them. Those are a code editor and
//! a messenger, the two surfaces the first user spends the day in, and the
//! conclusion drawn was that the product needed reach before it needed depth.
//!
//! That conclusion is a hypothesis until OCR is measured on the same two
//! applications. This is the instrument that measures it, on the *same grid*
//! the accessibility sweep uses, because a number produced by a different
//! method on different points is not a comparison.
//!
//! # The two rates do not mean the same thing, and that is the trap
//!
//! The first run of this printed `OCR 6 of 260, UIA 0 of 260` and invited the
//! reading that OCR is six times better. Both figures are correct and putting
//! them side by side is wrong.
//!
//! `word_at` answers only when the point is **inside** a recognised word, so
//! OCR's rate over a uniform grid is the share of the window covered by text.
//! That first run recognised 77 words at a median 43x16px, which is 53,000 of
//! the window's 2.3 million pixels: 2.3%, exactly the rate printed. It is a
//! measurement of the layout, not of the recogniser.
//!
//! `RangeFromPoint` answers with the *nearest* text and effectively never
//! fails inside a document, from as far as 1,465px away
//! (`benchmarks/2026-08-04-acquisition-sweep.md`). Its acquisition rate is
//! therefore close to a measure of whether a provider exists at all.
//!
//! So the report pairs the two quantities that do mean the same thing:
//!
//! - **`word_at` against UIA on-target.** Both say the returned text contains
//!   the sample point.
//! - **Where OCR sees a word, what does accessibility do?** That cross-tab is
//!   the question issue #14 asks, on the only points where there is anything
//!   to read.
//!
//! And the headline for reach is neither rate: it is **how much text was
//! recovered at all**. VS Code yielded 7 acquisitions in a sweep. Whether OCR
//! yields seven words or seven hundred is the finding.
//!
//! # It measures the foreground window, and only that
//!
//! A capture reads pixels. The pixels inside a covered window's rectangle
//! belong to whatever is covering it, so an OCR measurement of a background
//! window measures the wrong application and files it under the right name. The
//! alternative is raising each window in turn, which rearranges the desktop
//! being measured, and `sweep` exists precisely because that is not acceptable.
//!
//! So the operator puts the application in front and runs this. One window per
//! run, named in the output, and no window is touched.
//!
//! # What it may print
//!
//! Counts. Lines, words, characters, rectangles, milliseconds. **Never the
//! text**, not one word of it, because the text is the user's screen and this
//! is a tool that runs against a real desktop
//! (`docs/privacy-model.md`). `--expect` exists so a correctness judgement can
//! be made without any of it being printed: the operator says what should be
//! there and the probe answers found or not found.
//!
//! Nothing is written to disk, including the capture, including for debugging.

use std::time::{Duration, Instant};

use lens_core::acquire::{AcquireContext, TextAcquirer};
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::Generation;
use lens_platform_windows::{
    capture, foreground_window, monitor_scale_at, ocr, root_window_at, UiaTextAcquirer, WindowInfo,
};

use super::report::{format_us, percentile};

/// How much of a window to sample, and how much work to allow.
#[derive(Debug, Clone)]
pub struct OcrOptions {
    /// Roughly how many grid points. The same default as a sweep, so the two
    /// rates are over comparable samples.
    pub points: usize,
    /// A string the operator says is on screen. Reported as found or not
    /// found, never echoed.
    pub expect: Option<String>,
    /// Also acquire each point through UI Automation, so the run produces both
    /// halves of the comparison at once.
    pub compare: bool,
    /// Give up on the accessibility half after this long. OCR has no budget:
    /// it is one capture and one recognition, and if either wedges there is
    /// nothing to salvage.
    pub budget: Duration,
    /// Refuse unless the window in front is this application.
    pub only: Option<String>,
    /// How long to wait before capturing, so the operator can bring the right
    /// window forward.
    pub wait: Duration,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            points: 240,
            expect: None,
            compare: true,
            budget: Duration::from_secs(20),
            only: None,
            wait: Duration::from_secs(4),
        }
    }
}

/// The most pixels a single capture may ask for, matching `capture::rectangle`.
///
/// A client area larger than this on a side is captured in tiles rather than
/// refused. Words straddling a tile edge are split by it, which is a real limit
/// and a rare one: it needs a window over 4096 physical pixels on a side.
const MAX_SIDE: i32 = 4096;

/// What one run found. Counts only.
#[derive(Debug)]
pub struct OcrRow {
    pub process: Option<String>,
    pub client: ScreenRect,
    pub scale: f32,
    pub tiles: usize,
    pub capture_us: u128,
    pub recognise_us: u128,
    pub lines: usize,
    pub words: usize,
    pub characters: usize,
    /// Grid points that landed inside a recognised word.
    pub sampled: usize,
    pub ocr_hits: usize,
    /// Grid points where the pointer was over this window at all. A point under
    /// a covering window is not this window's failure.
    pub visible: usize,
    /// The accessibility half, or `None` when it was not run. Two passes,
    /// because a Chromium provider wakes on first contact.
    pub cold: Option<UiaPass>,
    pub woken: Option<UiaPass>,
    /// Median word box, which says whether a hit is a word or a whole line.
    pub median_word: Option<(i32, i32)>,
    /// `Some(true)` when `--expect` was given and the string was recognised.
    pub expected_found: Option<bool>,
}

impl OcrRow {
    /// A row for a window whose geometry is already known. `ScreenRect` has no
    /// `Default`, on the grounds that a rectangle at the origin with no size is
    /// not a sensible zero for a screen coordinate, so this takes the one fact
    /// that has no neutral value.
    fn for_window(client: ScreenRect) -> Self {
        Self {
            process: None,
            client,
            scale: 1.0,
            tiles: 0,
            capture_us: 0,
            recognise_us: 0,
            lines: 0,
            words: 0,
            characters: 0,
            sampled: 0,
            ocr_hits: 0,
            visible: 0,
            cold: None,
            woken: None,
            median_word: None,
            expected_found: None,
        }
    }
}

/// Split a rectangle into pieces no larger than a capture allows.
fn tiles(region: ScreenRect) -> Vec<ScreenRect> {
    let mut pieces = Vec::new();
    let mut y = region.y;
    while y < region.bottom() {
        let height = (region.bottom() - y).min(MAX_SIDE);
        let mut x = region.x;
        while x < region.right() {
            let width = (region.right() - x).min(MAX_SIDE);
            pieces.push(ScreenRect::new(x, y, width, height));
            x += width;
        }
        y += height;
    }
    pieces
}

/// Sample a grid inside `bounds`, by the same rule the accessibility sweep
/// uses. Duplicated deliberately rather than shared: if that one changes, this
/// comparison must be re-measured rather than silently follow it.
fn grid(bounds: ScreenRect, target_points: usize) -> Vec<ScreenPoint> {
    let inset = 4;
    let width = (bounds.width - inset * 2).max(1);
    let height = (bounds.height - inset * 2).max(1);
    let area = f64::from(width) * f64::from(height);
    let step = ((area / target_points.max(1) as f64).sqrt().round() as i32).clamp(8, 96);

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

/// Capture and recognise the whole of `region`, in tiles if it is large.
///
/// The captures are dropped as soon as each is recognised, so at most one
/// window's worth of pixels is held at a time and none of it outlives this
/// function.
fn recognise(region: ScreenRect) -> Option<(Vec<ocr::OcrLine>, usize, u128, u128)> {
    let pieces = tiles(region);
    let mut lines = Vec::new();
    let mut capture_us = 0;
    let mut recognise_us = 0;

    for piece in &pieces {
        let began = Instant::now();
        let shot = capture::rectangle(*piece)?;
        capture_us += began.elapsed().as_micros();

        let began = Instant::now();
        let read = ocr::read(&shot, ScreenPoint::new(piece.x, piece.y))?;
        recognise_us += began.elapsed().as_micros();
        lines.extend(read.lines);
    }
    Some((lines, pieces.len(), capture_us, recognise_us))
}

/// One accessibility pass over the grid.
#[derive(Debug, Default, Clone)]
pub struct UiaPass {
    pub acquired: usize,
    pub on_target: usize,
    /// Of the points inside a recognised word, how many acquired anything.
    pub on_words_acquired: usize,
    /// Of the points inside a recognised word, how many came back with text
    /// whose rectangle also contains the point. This is the number that says
    /// whether accessibility could have served the hover OCR just served.
    pub on_words_on_target: usize,
    pub latencies_us: Vec<u128>,
    pub truncated: bool,
}

fn uia_pass(
    acquirer: &UiaTextAcquirer,
    window: i64,
    points: &[(ScreenPoint, bool)],
    budget: Duration,
) -> UiaPass {
    // The wake is allowed, unlike in a sweep. A sweep separates cold from
    // woken because the cost of waking is what it measures. The question here
    // is whether accessibility can read this surface at all, so it gets the
    // same chance the product gives it.
    let context = AcquireContext {
        allow_accessibility_wake: true,
        ..AcquireContext::new(Generation(0))
    };
    let started = Instant::now();
    let mut pass = UiaPass::default();

    for (point, on_a_word) in points {
        if started.elapsed() > budget {
            pass.truncated = true;
            break;
        }
        if root_window_at(*point) != Some(window) {
            continue;
        }
        let began = Instant::now();
        let acquired = acquirer.acquire(*point, &context);
        pass.latencies_us.push(began.elapsed().as_micros());
        if let Ok(target) = acquired {
            pass.acquired += 1;
            let on_target = target.is_on_target(*point);
            if on_target {
                pass.on_target += 1;
            }
            if *on_a_word {
                pass.on_words_acquired += 1;
                if on_target {
                    pass.on_words_on_target += 1;
                }
            }
        }
    }
    pass
}

/// Run the probe against whatever is in front.
pub fn run(acquirer: &UiaTextAcquirer, options: &OcrOptions) -> Result<OcrRow, String> {
    if !ocr::can_recognise("en") && !ocr::can_recognise("ko") {
        return Err(format!(
            "this machine recognises neither English nor Korean; installed: {:?}",
            ocr::available_languages(),
        ));
    }
    let WindowInfo {
        id,
        process_name,
        client_bounds,
        ..
    } = foreground_window().map_err(|problem| problem.explain())?;
    if let Some(wanted) = options.only.as_deref() {
        // A named check, because the commonest way to get a wrong number here
        // is to measure the terminal the probe was launched from and never
        // notice: it has a client area, it has text, and the row looks
        // entirely ordinary under the wrong application's name.
        let actual = process_name.as_deref().unwrap_or("unknown");
        if !actual.to_lowercase().contains(&wanted.to_lowercase()) {
            return Err(format!(
                "the window in front is {actual}, not {wanted}. Nothing was captured.",
            ));
        }
    }
    if client_bounds.width <= 0 || client_bounds.height <= 0 {
        return Err("the foreground window has an empty client area".to_owned());
    }

    let Some((lines, tile_count, capture_us, recognise_us)) = recognise(client_bounds) else {
        return Err("the capture or the recognition failed".to_owned());
    };

    let read = ocr::Recognised { lines };
    let words: Vec<&ocr::OcrWord> = read.lines.iter().flat_map(|line| &line.words).collect();
    let characters: usize = read
        .lines
        .iter()
        .map(|line| line.text.chars().count())
        .sum();

    let mut widths: Vec<i32> = words.iter().map(|word| word.bounds.width).collect();
    let mut heights: Vec<i32> = words.iter().map(|word| word.bounds.height).collect();
    widths.sort_unstable();
    heights.sort_unstable();
    let median_word =
        (!widths.is_empty()).then(|| (widths[widths.len() / 2], heights[heights.len() / 2]));

    let mut row = OcrRow {
        process: process_name,
        scale: monitor_scale_at(client_bounds.center()),
        tiles: tile_count,
        capture_us,
        recognise_us,
        lines: read.lines.len(),
        words: words.len(),
        characters,
        median_word,
        // A substring test on text this process recognised, whose result is a
        // boolean. The operator supplied the needle, so answering does not
        // disclose anything they did not already type.
        expected_found: options
            .expect
            .as_deref()
            .map(|needle| read.text().contains(needle)),
        ..OcrRow::for_window(client_bounds)
    };

    // Each point paired with whether OCR sees a word there, so both passes
    // score the same cross-tab without re-running the recogniser.
    let points: Vec<(ScreenPoint, bool)> = grid(client_bounds, options.points)
        .into_iter()
        .map(|point| (point, read.word_at(point).is_some()))
        .collect();
    row.sampled = points.len();
    for (point, on_a_word) in &points {
        if root_window_at(*point) != Some(id) {
            continue;
        }
        row.visible += 1;
        if *on_a_word {
            row.ocr_hits += 1;
        }
    }

    if options.compare {
        // **Twice.** A Chromium provider wakes on first contact, so a single
        // pass over a browser or an Electron editor measures the waking rather
        // than the reading. The first run of this recorded Chrome at 0 of 260,
        // which is a fact about a cold provider and not about Chrome. The
        // second pass is the fair number, and both are printed because the gap
        // between them is worth having on its own.
        row.cold = Some(uia_pass(acquirer, id, &points, options.budget));
        row.woken = Some(uia_pass(acquirer, id, &points, options.budget));
    }
    Ok(row)
}

/// Print a row, in the shape the compatibility matrix wants.
pub fn print(row: &OcrRow) {
    let rate = |hits: usize| -> String {
        if row.visible == 0 {
            "n/a".to_owned()
        } else {
            format!("{:.1}%", hits as f64 * 100.0 / row.visible as f64)
        }
    };
    let of_words = |count: usize| -> String {
        if row.ocr_hits == 0 {
            "n/a".to_owned()
        } else {
            format!("{:.1}%", count as f64 * 100.0 / row.ocr_hits as f64)
        }
    };

    println!(
        "\n{} · client {}x{} at ({}, {}) · scale {:.2}",
        row.process.as_deref().unwrap_or("unknown"),
        row.client.width,
        row.client.height,
        row.client.x,
        row.client.y,
        row.scale,
    );
    println!(
        "  capture      {} tile(s) in {}",
        row.tiles,
        format_us(Some(row.capture_us)),
    );
    println!(
        "  recognition  {} lines · {} words · {} characters in {}",
        row.lines,
        row.words,
        row.characters,
        format_us(Some(row.recognise_us)),
    );
    if let Some((width, height)) = row.median_word {
        println!("  median word  {width}x{height}px");
    }
    println!(
        "  grid         {} points, {} over this window, {} of those on a word",
        row.sampled, row.visible, row.ocr_hits,
    );
    println!(
        "               OCR text covers {} of the window",
        rate(row.ocr_hits),
    );

    let Some(woken) = &row.woken else {
        println!("\n  accessibility  not measured");
        return;
    };
    let cold = row.cold.clone().unwrap_or_default();

    println!("\n  accessibility, over the same points     cold    woken");
    println!(
        "    acquired anything                  {:>6}  {:>6}",
        cold.acquired, woken.acquired,
    );
    println!(
        "    the point was inside what it read  {:>6}  {:>6}",
        cold.on_target, woken.on_target,
    );
    println!(
        "\n  where OCR found a word ({} points)       cold    woken",
        row.ocr_hits,
    );
    println!(
        "    acquired anything                  {:>6}  {:>6}",
        cold.on_words_acquired, woken.on_words_acquired,
    );
    println!(
        "    the point was inside what it read  {:>6}  {:>6}   {} woken",
        cold.on_words_on_target,
        woken.on_words_on_target,
        of_words(woken.on_words_on_target),
    );

    if !woken.latencies_us.is_empty() {
        let mut sorted = woken.latencies_us.clone();
        sorted.sort_unstable();
        println!(
            "\n  latency      p50 {} · p95 {}",
            format_us(percentile(&sorted, 50.0)),
            format_us(percentile(&sorted, 95.0)),
        );
    }
    if let Some(found) = row.expected_found {
        println!(
            "  expected     {}",
            if found {
                "found"
            } else {
                "NOT FOUND in what was recognised"
            },
        );
    }
    if cold.truncated || woken.truncated {
        println!("  note         a pass ran out of budget, so its counts are floors");
    }

    // Said in words, because two tables invite the reading that the OCR figure
    // and the acquisition figure are the same measurement. They are not.
    println!("\n  Read the second table. The OCR figure above it is how much of this window");
    println!("  is text; it is not a success rate and it does not compare to acquisition.");
    println!("  What compares is: on the points where there is something to read, could");
    println!("  accessibility read it.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_region_is_one_tile() {
        let region = ScreenRect::new(100, 200, 1920, 1080);
        assert_eq!(tiles(region), vec![region]);
    }

    #[test]
    fn a_region_too_tall_to_capture_is_split_and_covers_all_of_it() {
        let region = ScreenRect::new(0, 0, 2000, 5000);
        let pieces = tiles(region);
        assert_eq!(pieces.len(), 2);
        // No gap and no overlap: a gap is text nobody read and an overlap is
        // the same word counted twice.
        assert_eq!(pieces[0].bottom(), pieces[1].y);
        assert_eq!(pieces[1].bottom(), region.bottom());
        let covered: i32 = pieces.iter().map(|piece| piece.height).sum();
        assert_eq!(covered, region.height);
    }

    #[test]
    fn a_region_too_large_in_both_directions_tiles_in_both() {
        let pieces = tiles(ScreenRect::new(0, 0, 9000, 5000));
        assert_eq!(pieces.len(), 6, "three across and two down");
        let covered: i64 = pieces
            .iter()
            .map(|piece| i64::from(piece.width) * i64::from(piece.height))
            .sum();
        assert_eq!(covered, 9000 * 5000);
    }

    #[test]
    fn the_grid_matches_the_sweeps_own_rule() {
        // Not a copy of the sweep's numbers: the same window must produce the
        // same count, or the two rates in one row are over different samples
        // and the comparison this whole mode exists for is not one.
        let bounds = ScreenRect::new(0, 0, 1600, 900);
        assert_eq!(
            grid(bounds, 240).len(),
            super::super::sweep::grid_len(bounds, 240)
        );
    }
}
