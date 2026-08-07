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
        "\nA word that can only be in the document, found in the tree, means the\n\
         document is exposed and a pointer failing to reach it is a targeting\n\
         problem rather than a reach one. A word that is not there proves less:\n\
         it may be scrolled out of view, or spelt differently by the provider."
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
