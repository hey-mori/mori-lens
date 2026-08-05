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

pub fn print_report(windows: &[WindowTextInventory]) {
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
    }

    println!();
    println!("elements   descendants advertising a text pattern");
    println!("withtext   how many of those actually returned characters");
    println!("smallest   the shortest text element, in pixels. A window whose");
    println!("           smallest is the height of the whole window holds its text");
    println!("           in one piece, and a pointer cannot aim inside it.");
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
