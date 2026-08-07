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
//! # Two instruments on one pixel
//!
//! The oldest open condition on the M1 gate is that **no person has confirmed
//! that a resolved word is the word they were pointing at**. Everything
//! measured so far uses one proxy: whether the resolved word's rectangle
//! contains the sample point. That checks a position, not a word. A provider
//! that returns the right rectangle around the wrong text passes it.
//!
//! OCR is a second, independent answer to the same question, arrived at from
//! pixels rather than from a provider. Where both resolve a word at one point,
//! they can be compared, and agreement between two methods that share no code
//! and no assumptions is much stronger evidence than either alone.
//!
//! It does not close the gate. Both could be wrong the same way, and only a
//! person can say that the word either of them found is the word they meant.
//! What it does is make the human pass cheap: instead of judging every point,
//! there are the disagreements, which is where a wrong answer must be if there
//! is one.
//!
//! **The words are compared, never printed.** The report is counts.
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
    capture, foreground_window, monitor_scale_at, ocr, root_window_at, top_level_windows,
    UiaTextAcquirer, WindowInfo,
};

use super::printed;
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
    /// Points where OCR and accessibility both resolved a word.
    pub both_read_a_word: usize,
    /// Of those, how many resolved the **same** word.
    pub agreed: usize,
    /// Of those, how many resolved words where one contains the other. A
    /// provider expanding to a larger unit than asked for produces this, and it
    /// is neither agreement nor a wrong word.
    pub overlapped: usize,
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
            both_read_a_word: 0,
            agreed: 0,
            overlapped: 0,
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
    /// Points where this pass and OCR both resolved a word, and what came of
    /// comparing them.
    pub both_read_a_word: usize,
    pub agreed: usize,
    pub overlapped: usize,
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

/// Two words, as they compare for the purpose of agreeing.
///
/// Lower-cased and stripped of anything that is not a letter, a digit or an
/// underscore, because `Word,` and `word` are the same word and a provider that
/// includes the trailing comma is not wrong about which word it is.
fn same_word(left: &str, right: &str) -> (bool, bool) {
    let normalise = |text: &str| -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .flat_map(char::to_lowercase)
            .collect()
    };
    let (left, right) = (normalise(left), normalise(right));
    if left.is_empty() || right.is_empty() {
        return (false, false);
    }
    if left == right {
        return (true, false);
    }
    // One inside the other. A provider that expanded to a line where a word was
    // asked for lands here, and calling that a disagreement would report the
    // unit as a wrong word.
    (false, left.contains(&right) || right.contains(&left))
}

fn uia_pass(
    acquirer: &UiaTextAcquirer,
    window: i64,
    points: &[(ScreenPoint, bool)],
    read: &ocr::Recognised,
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
                // The second instrument. Both resolved a word here, so they can
                // be asked whether it is the same one.
                if let (Some(theirs), Some((_, ours))) =
                    (target.segmented.word_text(), read.word_at(*point))
                {
                    pass.both_read_a_word += 1;
                    let (agreed, overlapped) = same_word(theirs, &ours.text);
                    if agreed {
                        pass.agreed += 1;
                    } else if overlapped {
                        pass.overlapped += 1;
                    }
                }
            }
        }
    }
    pass
}

/// Whether the words on a covered window's pixels are in its accessibility
/// tree at all.
///
/// **The question a covered run can answer, and the one issue #14 is open on.**
/// A sweep of a covered window measures nothing, because `ElementFromPoint`
/// answers about whatever is on top. The tree can still be asked *by handle*,
/// which is what `inventory` does, and comparing that against what the pixels
/// say separates two very different findings:
///
/// - a **low** share: the text is not in the tree, and no amount of getting
///   targeting right would reach it
/// - a **high** share: the text is there and a pointer cannot get to it, which
///   is a different problem with a different answer
///
/// Counts only. Neither reading is printed, and the tree's words are held as a
/// set so they cannot reconstruct a document (`docs/privacy-model.md`).
fn report_the_overlap(id: i64, lines: &[ocr::OcrLine]) {
    let in_the_tree = lens_platform_windows::text_words_of(id);
    let seen: std::collections::HashSet<String> = lines
        .iter()
        .flat_map(|line| line.text.split_whitespace())
        .map(comparable_word)
        .filter(|word| !word.is_empty())
        .collect();
    let shared = seen
        .iter()
        .filter(|word| in_the_tree.contains(*word))
        .count();
    let share = if seen.is_empty() {
        0.0
    } else {
        shared as f64 * 100.0 / seen.len() as f64
    };
    println!(
        "  by handle {} distinct words on the pixels, {shared} of them also in the\n               accessibility tree ({share:.0}%). The tree holds {} distinct words in all.\n               A low share means the text is not in the tree. A high one means it\n               is there and a pointer cannot reach it. OCR misreads pull this\n               down, so a low number is weaker evidence than a high one.",
        seen.len(),
        in_the_tree.len(),
    );
}

/// The same normalisation the tree side uses, so the two sets are comparable.
fn comparable_word(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

/// Which window is actually in front, for the row to say whether these pixels
/// came off the screen or out of the window.
fn front_id_of(only: &Option<String>, chosen: i64) -> i64 {
    match only {
        // Nothing was named, so the chosen window *is* the foreground one.
        None => chosen,
        Some(_) => foreground_window().map(|window| window.id).unwrap_or(-1),
    }
}

/// Run the probe against the window in front, or a named one wherever it is.
pub fn run(acquirer: &UiaTextAcquirer, options: &OcrOptions) -> Result<OcrRow, String> {
    if !ocr::can_recognise("en") && !ocr::can_recognise("ko") {
        return Err(format!(
            "this machine recognises neither English nor Korean; installed: {:?}",
            ocr::available_languages(),
        ));
    }
    // The window in front, unless a name was given and something else is
    // wearing it. `--only` used to be a check on the foreground window and
    // nothing more, so a named application that was not in front produced a
    // refusal and a session of moving windows around.
    let front = foreground_window().map_err(|problem| problem.explain())?;
    let chosen = match options.only.as_deref() {
        None => front,
        Some(wanted) => {
            let matches = |window: &WindowInfo| {
                window
                    .process_name
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_lowercase()
                    .contains(&wanted.to_lowercase())
            };
            if matches(&front) {
                front
            } else {
                // The largest match, because an application often owns several
                // windows and the one worth measuring is the one with the text
                // in it rather than a tooltip or a hidden host.
                top_level_windows()
                    .into_iter()
                    .filter(matches)
                    .max_by_key(|window| {
                        (window.client_bounds.width as i64) * (window.client_bounds.height as i64)
                    })
                    .ok_or_else(|| {
                        format!("no window belongs to {wanted}. Nothing was captured.")
                    })?
            }
        }
    };
    let WindowInfo {
        id,
        process_name,
        client_bounds,
        ..
    } = chosen;
    if client_bounds.width <= 0 || client_bounds.height <= 0 {
        return Err("the chosen window has an empty client area".to_owned());
    }

    // In front: read the screen, which is what the product does and therefore
    // the number that belongs in the matrix.
    //
    // Behind: ask the window to draw itself. `ElementFromPoint` still answers
    // with whatever is on top, so this changes the OCR half only, and that is
    // stated in the row rather than left for a reader to infer.
    let in_front = id == front_id_of(&options.only, id);
    let printed_instead = !in_front;
    let captured = if printed_instead {
        printed::window(id, client_bounds).map(|shot| {
            let began = Instant::now();
            let read = ocr::read(&shot, ScreenPoint::new(client_bounds.x, client_bounds.y));
            let elapsed = began.elapsed().as_micros();
            (
                read.map(|read| read.lines).unwrap_or_default(),
                1,
                0,
                elapsed,
                printed::ink(&shot),
            )
        })
    } else {
        recognise(client_bounds).map(|(lines, tiles, capture_us, recognise_us)| {
            (lines, tiles, capture_us, recognise_us, f64::NAN)
        })
    };

    let Some((lines, tile_count, capture_us, recognise_us, ink)) = captured else {
        return Err(if printed_instead {
            format!(
                "{} would not render itself. It is behind another window, and \
                 PrintWindow declined.",
                process_name.as_deref().unwrap_or("the window"),
            )
        } else {
            "the capture or the recognition failed".to_owned()
        });
    };
    if printed_instead {
        println!(
            "  rendered  the window is behind another; it drew itself, {ink:.1}% ink.\n               Accessibility by point still answers about whatever is on top,\n               so those columns below are empty by construction."
        );
        report_the_overlap(id, &lines);
    }

    let read = ocr::Recognised {
        lines,
        // The client area, which is what `recognise` tiled and read.
        covered: client_bounds,
    };
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
        row.cold = Some(uia_pass(acquirer, id, &points, &read, options.budget));
        let woken = uia_pass(acquirer, id, &points, &read, options.budget);
        row.both_read_a_word = woken.both_read_a_word;
        row.agreed = woken.agreed;
        row.overlapped = woken.overlapped;
        row.woken = Some(woken);
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

    // The M1 gate's oldest open row, narrowed. Two instruments that share no
    // code arriving at the same word is far better evidence than one
    // instrument's rectangle containing a point, and the disagreements are
    // where a person should look rather than at all of it.
    if row.both_read_a_word > 0 {
        let share = |count: usize| -> String {
            format!("{:.1}%", count as f64 * 100.0 / row.both_read_a_word as f64)
        };
        let disagreed = row
            .both_read_a_word
            .saturating_sub(row.agreed)
            .saturating_sub(row.overlapped);
        println!(
            "\n  both instruments resolved a word at {} points:",
            row.both_read_a_word,
        );
        println!(
            "    the same word          {:>4}  {}",
            row.agreed,
            share(row.agreed)
        );
        println!(
            "    one inside the other   {:>4}  {}",
            row.overlapped,
            share(row.overlapped),
        );
        println!(
            "    different              {:>4}  {}",
            disagreed,
            share(disagreed)
        );
        println!(
            "  The words are compared and never printed. Two methods agreeing is not a\n  \
             person confirming, and does not close that gate row: both could be wrong the\n  \
             same way. It says where to look."
        );
    }

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

#[cfg(test)]
mod agreement_tests {
    use super::same_word;

    #[test]
    fn the_same_word_agrees_through_case_and_punctuation() {
        assert_eq!(same_word("Word", "word"), (true, false));
        assert_eq!(same_word("word,", "word"), (true, false));
        assert_eq!(same_word("\"quoted\"", "quoted"), (true, false));
    }

    #[test]
    fn an_expanded_unit_is_an_overlap_rather_than_a_disagreement() {
        // A provider asked for a word that hands back a line is not wrong about
        // which word the pointer is on, and counting it as a wrong answer would
        // put the unit's behaviour into the correctness figure.
        assert_eq!(same_word("the whole line of text", "line"), (false, true));
        assert_eq!(same_word("line", "the whole line of text"), (false, true));
    }

    #[test]
    fn two_different_words_are_a_disagreement() {
        assert_eq!(same_word("pointer", "printer"), (false, false));
        assert_eq!(same_word("output", "input"), (false, false));
    }

    #[test]
    fn a_word_with_nothing_readable_in_it_agrees_with_nothing() {
        // Punctuation on its own normalises to an empty string, and an empty
        // string is inside every string. Without this it would agree with
        // everything and inflate the figure the whole measure exists to
        // produce.
        assert_eq!(same_word("---", "word"), (false, false));
        assert_eq!(same_word("word", "  "), (false, false));
        assert_eq!(same_word("", ""), (false, false));
    }

    #[test]
    fn korean_and_latin_normalise_the_same_way() {
        assert_eq!(same_word("포인터", "포인터"), (true, false));
        assert_eq!(same_word("포인터,", "포인터"), (true, false));
        assert_eq!(same_word("포인터", "프린터"), (false, false));
    }
}
