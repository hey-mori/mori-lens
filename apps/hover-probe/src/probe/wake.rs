//! What the Chromium accessibility wake actually costs, and how often.
//!
//! Chromium and Electron build no accessibility tree until an assistive client
//! asks for one. Lens asks with `SendMessageTimeoutW(WM_GETOBJECT,
//! OBJID_CLIENT)` and then waits 140ms before reading again. That 140ms came
//! from Mori as a starting value, and Mori pays it **once per deliberate
//! keypress** where Lens would pay it on a hover budget of 320ms.
//!
//! Issue #4 is that nobody has measured it here. The number that decides
//! whether hovering in a browser is viable is not the cost of one wake, it is
//! **how often the wake recurs**. Paid once per window, 340ms is a first hover
//! that is slightly slow. Paid per hover, it is more than the whole dwell
//! budget and the browser is where most people meet a second language.
//!
//! # What this measures, and what it cannot
//!
//! - **The wake message on its own.** Timed separately from the settle, so the
//!   two are not reported as one number ever again.
//! - **How long until the first successful read.** Read every 10ms after the
//!   wake rather than sleeping a fixed 140ms and declaring success, which is
//!   what makes 140 an answer instead of an assumption.
//! - **Whether a woken window goes cold again.** A window that answers is left
//!   alone for a chosen interval and asked once more, without a wake.
//!
//! It cannot put an awake window back to sleep. A desktop where everything has
//! already been read is a desktop with no wakes left to measure, and that is a
//! finding rather than a failed run: it means the tree persists, which is the
//! answer to the question that matters.

use std::time::{Duration, Instant};

use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
use lens_core::geometry::ScreenPoint;
use lens_core::hover::Generation;
use lens_platform_windows::{root_window_at, top_level_windows, UiaTextAcquirer, WindowInfo};

use super::report::format_us;

#[derive(Debug, Clone)]
pub struct WakeOptions {
    /// Only windows whose process or title contains this.
    pub only: Option<String>,
    /// How long to leave a woken window alone before asking whether it is
    /// still awake.
    pub quiet: Duration,
    /// How long to keep re-reading after a wake before giving up on it.
    pub patience: Duration,
}

impl Default for WakeOptions {
    fn default() -> Self {
        Self {
            only: None,
            quiet: Duration::from_secs(30),
            patience: Duration::from_millis(600),
        }
    }
}

/// How often to re-read while waiting for a woken tree to answer.
///
/// 10ms. Finer would measure this loop's own overhead against a call whose p50
/// is a few milliseconds; coarser could not tell 140ms from 100ms, which is the
/// distinction the whole run is for.
const STEP: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct WakeRow {
    pub process: Option<String>,
    /// Whether a read succeeded before any wake was sent.
    pub already_awake: bool,
    /// The point that was measured, kept only to say one was found.
    pub found_a_point: bool,
    /// The wake message alone, without the settle.
    pub message_us: Option<u128>,
    /// Time from the wake message returning to the first successful read.
    pub settle_us: Option<u128>,
    /// The failure a cold read gave, when it failed.
    pub cold_error: Option<&'static str>,
    /// Whether a read still worked after the quiet interval, with no wake.
    pub still_awake: Option<bool>,
}

/// What a coarse scan of one window found.
///
/// `examined` is the number of points that were actually this window's to ask
/// about, and it carries the difference between two things that look identical
/// from the result alone: a window that answered nothing, and a window that was
/// covered so completely there was nothing to ask. Reporting the second as the
/// first is how a scan says a browser went cold when somebody put a window in
/// front of it.
struct Scan {
    examined: usize,
    found: Option<(ScreenPoint, Result<(), AcquireError>)>,
}

/// A point inside this window that has text under it, or the failure it gives.
///
/// Sampled coarsely: this is looking for somewhere worth measuring rather than
/// covering the window, and a fine grid over a browser is a sweep, which this
/// deliberately is not.
fn find_point(acquirer: &UiaTextAcquirer, window: &WindowInfo, context: &AcquireContext) -> Scan {
    let bounds = window.client_bounds;
    let step = 48;
    let mut best: Option<(ScreenPoint, Result<(), AcquireError>)> = None;
    let mut examined = 0usize;

    let mut y = bounds.y + 16;
    while y < bounds.bottom() - 16 {
        let mut x = bounds.x + 16;
        while x < bounds.right() - 16 {
            let point = ScreenPoint::new(x, y);
            if root_window_at(point) == Some(window.id) {
                examined += 1;
                match acquirer.acquire(point, context) {
                    // Text, now, with no wake. Nothing better to find.
                    Ok(_) => {
                        return Scan {
                            examined,
                            found: Some((point, Ok(()))),
                        }
                    }
                    Err(error) => {
                        // Remember the first wake-repairable failure and keep
                        // looking, because a point that a wake could fix is the
                        // one worth measuring and a margin is not.
                        if best.is_none()
                            && matches!(error, AcquireError::Unsupported | AcquireError::Empty)
                        {
                            best = Some((point, Err(error)));
                        }
                    }
                }
            }
            x += step;
        }
        y += step;
    }
    Scan {
        examined,
        found: best,
    }
}

pub fn run(acquirer: &UiaTextAcquirer, options: &WakeOptions) -> Vec<WakeRow> {
    // No waking from the acquirer's own retry: this measures the wake, so the
    // wake has to be the only thing that sends one.
    let context = AcquireContext {
        allow_accessibility_wake: false,
        ..AcquireContext::new(Generation(0))
    };
    let mut rows = Vec::new();
    // Which rows to re-read after the shared quiet interval, and where.
    let mut answering: Vec<(usize, WindowInfo, bool)> = Vec::new();

    for window in top_level_windows() {
        if let Some(only) = options.only.as_deref() {
            let matches = |value: Option<&String>| {
                value.is_some_and(|value| value.to_lowercase().contains(&only.to_lowercase()))
            };
            if !matches(window.process_name.as_ref()) && !matches(window.title.as_ref()) {
                continue;
            }
        }

        let Some((point, cold)) = find_point(acquirer, &window, &context).found else {
            rows.push(WakeRow {
                process: window.process_name.clone(),
                already_awake: false,
                found_a_point: false,
                message_us: None,
                settle_us: None,
                cold_error: None,
                still_awake: None,
            });
            continue;
        };

        let mut row = WakeRow {
            process: window.process_name.clone(),
            already_awake: cold.is_ok(),
            found_a_point: true,
            message_us: None,
            settle_us: None,
            cold_error: cold.as_ref().err().map(|error| error.reason()),
            still_awake: None,
        };

        if cold.is_err() {
            // The message on its own. `wake_window` sleeps the settle inside
            // it, so this sends the message directly and times the settle by
            // reading rather than by trusting a constant.
            acquirer.forget_woken();
            let began = Instant::now();
            lens_platform_windows::wake_accessibility_for(window.id);
            row.message_us = Some(began.elapsed().as_micros());

            let settling = Instant::now();
            while settling.elapsed() < options.patience {
                if acquirer.acquire(point, &context).is_ok() {
                    row.settle_us = Some(settling.elapsed().as_micros());
                    break;
                }
                std::thread::sleep(STEP);
            }
        }

        answering.push((
            rows.len(),
            window.clone(),
            row.already_awake || row.settle_us.is_some(),
        ));
        rows.push(row);
    }

    // **One wait, not one per window.** Whether a tree persists is a question
    // about elapsed silence, and every window is silent during the same
    // silence. Sleeping per window made a six-window desktop take six times as
    // long and measured the last window over six times the interval the first
    // one got.
    let watching: Vec<(usize, WindowInfo)> = answering
        .into_iter()
        .filter_map(|(index, window, answering)| answering.then_some((index, window)))
        .collect();
    if !watching.is_empty() {
        println!(
            "  waiting {}s in silence, then asking {} window(s) again",
            options.quiet.as_secs(),
            watching.len(),
        );
        std::thread::sleep(options.quiet);
        for (index, window) in watching {
            // **The window, not the point that worked before.** Re-reading one
            // coordinate cannot tell a tree that went away from text that
            // moved, and the first run of this reported a terminal as cold when
            // what had changed was its contents. A tree is present if anything
            // in the window still reads.
            let scan = find_point(acquirer, &window, &context);
            rows[index].still_awake = if scan.examined == 0 {
                // Covered, moved, or closed. Not an answer about its tree.
                None
            } else {
                Some(matches!(scan.found, Some((_, Ok(())))))
            };
        }
    }
    rows
}

pub fn print(rows: &[WakeRow], options: &WakeOptions) {
    println!(
        "\n{:<20} {:>10} {:>10} {:>10}  state",
        "process", "message", "settle", "cold",
    );
    for row in rows {
        if !row.found_a_point {
            println!(
                "{:<20} {:>10} {:>10} {:>10}  no point with text or a repairable failure",
                row.process.as_deref().unwrap_or("?"),
                "-",
                "-",
                "-",
            );
            continue;
        }
        let state = match (row.already_awake, row.settle_us, row.still_awake) {
            (true, _, Some(true)) => "was awake, still awake".to_owned(),
            (true, _, Some(false)) => "was awake, WENT COLD".to_owned(),
            (true, _, None) => "was awake, could not re-check".to_owned(),
            (false, Some(_), Some(true)) => "woken, still awake".to_owned(),
            (false, Some(_), Some(false)) => "woken, WENT COLD".to_owned(),
            (false, Some(_), None) => "woken".to_owned(),
            (false, None, _) => format!(
                "the wake did not repair it within {}ms",
                options.patience.as_millis(),
            ),
        };
        println!(
            "{:<20} {:>10} {:>10} {:>10}  {state}",
            row.process.as_deref().unwrap_or("?"),
            format_us(row.message_us),
            format_us(row.settle_us),
            row.cold_error.unwrap_or("ok"),
        );
    }

    let settles: Vec<u128> = rows.iter().filter_map(|row| row.settle_us).collect();
    println!(
        "\n  {} window(s) needed a wake, {} were already awake",
        settles.len(),
        rows.iter().filter(|row| row.already_awake).count(),
    );
    if settles.is_empty() {
        println!(
            "  Nothing to wake is a result, not a failed run: every window here had a tree\n  \
             already, so the wake is not being paid per hover on this desktop."
        );
    } else {
        let slowest = settles.iter().max().copied().unwrap_or(0);
        println!(
            "  slowest settle {}, against the {}ms this crate sleeps",
            format_us(Some(slowest)),
            140,
        );
    }
    let cold_again = rows
        .iter()
        .filter(|row| row.still_awake == Some(false))
        .count();
    let rechecked = rows.iter().filter(|row| row.still_awake.is_some()).count();
    println!(
        "  after {}s of no assistive traffic, {} of {} re-checkable windows went cold",
        options.quiet.as_secs(),
        cold_again,
        rechecked,
    );
    let uncheckable = rows
        .iter()
        .filter(|row| row.found_a_point && row.still_awake.is_none())
        .count();
    if uncheckable > 0 {
        println!(
            "  {uncheckable} could not be re-checked: nothing in them was still this window's\n  \
             to ask about, which is a covered or closed window and not a cold tree.",
        );
    }
}
