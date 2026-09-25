//! What a window exposes, asked of the window rather than of the screen.
//!
//! The sweep samples a grid of coordinates and can only measure a window that
//! is in front, because `ElementFromPoint` answers with whatever is topmost.
//! That made the compatibility matrix a manual exercise: raise an application,
//! sweep it, raise the next.
//!
//! It was never a limit of UI Automation. `ElementFromHandle` takes a window
//! handle and no coordinate, so a window can be inventoried while completely
//! covered. Three things follow, and all three are improvements:
//!
//! - **No one has to arrange the desktop.** Every window at once, in seconds.
//! - **Far less load.** The sweep's cost is a call per grid point across every
//!   window, which made a machine unusable on 2026-08-05. This walks each
//!   provider's tree once.
//! - **Ground truth**, which the sweep never had. A grid guesses where text
//!   might be. A tree walk *knows* which elements hold text and where each one
//!   is, so a resolution can be checked against the element it should have
//!   hit rather than against a hopeful rectangle.
//!
//! It does not replace the sweep. The sweep exercises the product's real entry
//! point, `ElementFromPoint` at a pointer position, and this deliberately does
//! not. What this answers is the prior question: **is there anything there to
//! find, and is it in pieces a pointer could aim at.**

use std::time::Instant;

use lens_platform_windows::{text_inventory, WindowTextInventory};

/// Ask each matching window's tree whether it holds these words.
///
/// **The decisive question the counts cannot answer.** An inventory saying a
/// window holds 8,131 characters does not say whether those are the document
/// or the file tree beside it, and 107 elements is consistent with both.
/// Naming a word that can only be in the document settles it.
///
/// Printing is allowed here and only here: the caller supplies the words, so
/// what is reported is **whether something the operator already knew was
/// found**, which is the shape `docs/privacy-model.md` names as the exception.
/// Nothing that was read is printed.
pub fn expect(only: Option<&str>, wanted: &[String]) {
    let needle = only.map(str::to_lowercase);
    for window in lens_platform_windows::top_level_windows() {
        let name = window.process_name.clone().unwrap_or_else(|| "?".into());
        if let Some(needle) = &needle {
            if !name.to_lowercase().contains(needle.as_str()) {
                continue;
            }
        }
        let held = lens_platform_windows::text_words_of(window.id);
        if held.is_empty() {
            println!("{name:<22} nothing in its tree");
            continue;
        }
        let found: Vec<&String> = wanted
            .iter()
            .filter(|word| held.contains(&word.to_lowercase()))
            .collect();
        println!(
            "{name:<22} {} of {} expected words, {} distinct words in all",
            found.len(),
            wanted.len(),
            held.len(),
        );
        for word in wanted {
            println!(
                "    {:<28} {}",
                word,
                if held.contains(&word.to_lowercase()) {
                    "in the tree"
                } else {
                    "NOT in the tree"
                },
            );
        }
    }
    println!(
        "\nA word that can only be in the document, found in the tree, means the\ndocument is exposed and a pointer failing to reach it is a targeting\nproblem rather than a reach one. A word that is not there proves less:\nit may be scrolled out of view, or spelt differently by the provider."
    );
}

/// Inventory every window, or those matching `only`.
pub fn run(only: Option<&str>) -> Vec<WindowTextInventory> {
    let needle = only.map(str::to_lowercase);
    let started = Instant::now();

    let mut found = text_inventory(|name| match &needle {
        None => true,
        Some(needle) => name.to_lowercase().contains(needle.as_str()),
    });

    // Most text first: it is the order somebody reading the output cares
    // about, and a window with nothing is the least interesting row.
    found.sort_by_key(|window| std::cmp::Reverse(window.characters));
    println!("inventoried in {}ms\n", started.elapsed().as_millis());
    found
}

pub fn print_report(windows: &[WindowTextInventory], titles: bool) {
    if windows.is_empty() {
        println!("No window matched.");
        return;
    }

    println!(
        "{:<20}{:>9}{:>10}{:>9}{:>10}{:>11}",
        "application", "elements", "withtext", "chars", "smallest", "on-target",
    );

    for window in windows {
        let on_target = if window.probed == 0 {
            "-".to_owned()
        } else {
            format!("{}/{}", window.resolved_into_element, window.probed)
        };
        let mut notes = Vec::new();
        if window.unlocated > 0 {
            notes.push(format!("{} resolved but not rendered", window.unlocated));
        }
        let misses = if window.miss_distances.is_empty() {
            String::new()
        } else {
            let mut sorted = window.miss_distances.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            format!(
                "{} mislocated by {:.0} to {:.0}px",
                sorted.len(),
                sorted[0],
                sorted[sorted.len() - 1],
            )
        };
        if !misses.is_empty() {
            notes.push(misses);
        }
        println!(
            "{:<20}{:>9}{:>10}{:>9}{:>10}{:>11}",
            truncate(&window.process_name, 19),
            window.text_elements,
            window.with_content,
            window.characters,
            window
                .smallest_height
                .map_or("?".to_owned(), |h| format!("{h}px")),
            on_target,
        );
        if titles {
            // Opt-in. Two Edge windows are two different results, and the
            // matrix cannot record which is which without this. Off by
            // default because a title is a document name.
            println!(
                "{:<20}{}",
                "",
                truncate(window.title.as_deref().unwrap_or("(untitled)"), 70),
            );
        }
        if !notes.is_empty() {
            println!("{:<20}{}", "", notes.join(" · "));
        }
    }

    println!();
    println!("elements   descendants advertising a text pattern");
    println!("withtext   how many of those actually returned characters");
    println!("smallest   the shortest text element, in pixels. A window whose");
    println!("           smallest is the height of the whole window holds its text");
    println!("           in one piece, and a pointer cannot aim inside it.");
    println!("           A probe that resolved to text with no rectangle is");
    println!("           counted apart, rather than as a miss: unrendered text is");
    println!("           scrolled out of view, not mislocated, and the product");
    println!("           treats it the same way.");
    println!("on-target  the check the grid sweep could never make: for each");
    println!("           sampled element, a point at its own centre is given to");
    println!("           its own text pattern, and the answer is correct only if");
    println!("           it lands back inside that element. Ground truth, because");
    println!("           the element is known before the question is asked.");
    println!();
    println!("This asks what a window holds. It does not exercise the product's");
    println!("entry point, which is ElementFromPoint at a pointer position, and a");
    println!("window covered by another still fails there. Use the sweep for that.");
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars().take(width - 1).collect::<String>() + "…"
}

/// How much of a window a grid would land inside a text element.
///
/// **The measurement `ElementFromPoint` cannot be asked about a covered
/// window.** That call answers with the deepest element in the global z-order,
/// so over a buried window it answers about whatever is on top, and a sweep
/// reports `0/0 (N occluded)`.
///
/// A rectangle does not care what is in front. So this lays the sweep's own
/// grid over a window's client area and asks how many points fall inside
/// something that holds text.
///
/// The two numbers side by side are the point of it. If a large share of a
/// window's points are inside a text element and `ElementFromPoint` still
/// resolves 2.8% of them, then the text is there, it is where a pointer is,
/// and the gap is in turning a coordinate into an element (#14).
///
/// Layout only: rectangles, never a character.
pub fn coverage(only: Option<&str>) {
    /// The tallest an element can be and still be a line of text.
    ///
    /// Forty pixels. A line of code renders at 16 to 19 and a heading at up to
    /// about 40; a container is hundreds. The figure only has to separate
    /// those two populations and it does not need to be delicate.
    const LINE_SHAPED: i32 = 40;

    let needle = only.map(str::to_lowercase);
    println!("window                 client        points  on a line              covered");

    for window in lens_platform_windows::top_level_windows() {
        let name = window.process_name.clone().unwrap_or_else(|| "?".into());
        if let Some(needle) = &needle {
            if !name.to_lowercase().contains(needle.as_str()) {
                continue;
            }
        }
        let bounds = window.client_bounds;
        if bounds.width <= 0 || bounds.height <= 0 {
            continue;
        }
        let boxes = lens_platform_windows::text_rectangles_of(window.id);
        if boxes.is_empty() {
            println!(
                "{name:<22} {:>5}x{:<7} no text elements",
                bounds.width, bounds.height
            );
            continue;
        }

        // The same 24px step the selftest and the sweep use, so the figures
        // are comparable to the rows already recorded.
        let step = 24;
        let (mut points, mut on_a_line, mut occluded) = (0usize, 0usize, 0usize);
        let mut y = bounds.y + 8;
        while y < bounds.bottom() - 8 {
            let mut x = bounds.x + 8;
            while x < bounds.right() - 8 {
                let point = lens_core::geometry::ScreenPoint::new(x, y);
                points += 1;
                if lens_platform_windows::root_window_at(point) != Some(window.id) {
                    occluded += 1;
                }
                // **The smallest containing element, not any.** The first
                // version of this asked whether a point was inside any text
                // element and every window answered 100%, because one element
                // covers the whole client area and containment is then
                // trivially true. That measured nothing.
                //
                // A point is on a line when the smallest thing containing it
                // is line-shaped. A point inside only a window-sized container
                // is inside a document, which is not the same as being on a
                // sentence and is exactly the difference #14 turns on.
                let smallest = boxes
                    .iter()
                    .filter(|box_| box_.contains(point))
                    .min_by_key(|box_| (box_.width as i64) * (box_.height as i64));
                if smallest.is_some_and(|box_| box_.height <= LINE_SHAPED) {
                    on_a_line += 1;
                }
                x += step;
            }
            y += step;
        }
        let share = if points == 0 {
            0.0
        } else {
            on_a_line as f64 * 100.0 / points as f64
        };
        println!(
            "{name:<22} {:>5}x{:<7} {points:>6}  {on_a_line:>6} ({share:>4.1}%)        {occluded}",
            bounds.width, bounds.height,
        );
    }

    // One string per line. A `\` continuing a string literal is what CONTRIBUTING.md
    // bans and what broke this file's first draft: rustfmt rejoins such a
    // literal and leaves the indentation inside it.
    for line in [
        "",
        "on a line             the smallest text element containing the point is",
        "                       line-shaped rather than a container. A rectangle",
        "                       test, so it does not care what is in front.",
        "                       Compare it against the acquisition rate in the",
        "                       matrix, which is ElementFromPoint and does care.",
        "                       Asking whether a point is inside *any* text",
        "                       element answers 100% everywhere and means",
        "                       nothing: one element covers the client area.",
        "covered                points another window is over. They are counted",
        "                       rather than skipped here, because the rectangle",
        "                       test is unaffected by them and the difference",
        "                       between the two columns is the finding.",
    ] {
        println!("{line}");
    }
}
