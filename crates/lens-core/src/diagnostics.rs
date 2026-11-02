//! Is this working for me, in a window rather than in a console nobody has.
//!
//! [#165](https://github.com/hey-mori/mori-lens/issues/165) puts it plainly: a
//! release binary is a Windows-subsystem application, so **everything the
//! banner prints is invisible to the reader who most needs it**, which is the
//! engine, the address, whether OCR has a recogniser, which chords failed to
//! register, and what the session has actually done.
//!
//! # What is here and what is not
//!
//! The configuration lines come in as strings, because they are the app's:
//! `apps/lens` already builds every one of them for the banner and a second
//! copy of that logic would be a second thing to keep right.
//!
//! **The arithmetic is here**, because a rule about a ratio written inside a
//! `println!` is a rule nobody can test, and because [`crate::tally::Tally`]
//! already owns the counting for the same reason.
//!
//! # Reading only
//!
//! Nothing here writes a settings file. #165's writing half is a real decision
//! problem, with two writers, atomic replacement, comment preservation, unknown
//! keys and a schema version, and none of it is answered by a window that
//! happens to know the values. This is the half that can be built without
//! deciding any of that.

use crate::panel::{Block, Panel};
use crate::session::OcrMode;
use crate::tally::Tally;

/// The window a reader opens to find out whether Lens is working.
pub fn diagnostics_panel(
    configuration: &[(String, String)],
    tally: &Tally,
    ocr: OcrMode,
    build: &str,
) -> Panel {
    let settings = configuration
        .iter()
        .map(|(label, value)| format!("{label}   {value}"))
        .collect::<Vec<_>>()
        .join("\n");

    Panel::new(
        [
            Block::loud("이번 세션", session_lines(tally)),
            // Last, because a reader whose session is going badly wants the
            // sentence about it and not a list of settings above it.
            Block::loud("제안", tally.advice(ocr).unwrap_or_default()),
            Block::quiet("설정", settings),
        ],
        build.to_owned(),
    )
}

/// What the session did, as lines a reader can read.
///
/// **Says the hover count even when it is zero.** A panel opened in the first
/// minute that showed nothing at all would read as a product that is not
/// running, which is the failure this whole surface exists to prevent.
fn session_lines(tally: &Tally) -> String {
    let settled = tally.settled();
    let mut lines = vec![format!("카드 {} / 정착된 호버 {}", tally.cards(), settled)];
    for (reason, count) in tally.refusals() {
        let share = if settled == 0 {
            0.0
        } else {
            100.0 * f64::from(count) / f64::from(settled)
        };
        lines.push(format!("{reason}   {count}회, {share:.0}%"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::diagnostics_panel;
    use crate::session::OcrMode;
    use crate::tally::Tally;

    fn configuration() -> Vec<(String, String)> {
        vec![
            ("engine".to_owned(), "qwen2.5:7b".to_owned()),
            ("address".to_owned(), "127.0.0.1:11434".to_owned()),
        ]
    }

    fn a_session() -> Tally {
        let mut tally = Tally::default();
        for _ in 0..20 {
            tally.card();
        }
        for _ in 0..42 {
            tally.refusal("unsupported");
        }
        for _ in 0..12 {
            tally.refusal("timeout");
        }
        tally
    }

    /// A reader's own session, in the window that would have told them.
    #[test]
    fn the_panel_carries_the_counts_and_the_advice() {
        let panel = diagnostics_panel(&configuration(), &a_session(), OcrMode::Manual, "b1a7f1d");
        let said = panel
            .blocks
            .iter()
            .map(|block| format!("{}\n{}", block.heading, block.body))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(said.contains("카드 20"), "{said}");
        assert!(said.contains("unsupported"), "{said}");
        assert!(said.contains("57%"), "{said}");
        assert!(said.contains("automatic"), "{said}");
        assert_eq!(panel.footer, "b1a7f1d");
    }

    /// Nothing to suggest is no block, rather than a heading over a blank.
    #[test]
    fn a_session_with_no_advice_has_no_advice_block() {
        let mut working = Tally::default();
        for _ in 0..40 {
            working.card();
        }
        let panel = diagnostics_panel(&configuration(), &working, OcrMode::Manual, "b1a7f1d");
        assert!(
            !panel.blocks.iter().any(|block| block.heading == "제안"),
            "{:?}",
            panel.blocks,
        );
    }

    /// Opened in the first minute, it still says the product is running.
    #[test]
    fn an_empty_session_still_says_something() {
        let panel = diagnostics_panel(
            &configuration(),
            &Tally::default(),
            OcrMode::Manual,
            "b1a7f1d",
        );
        assert!(panel.says_anything());
        let first = &panel.blocks[0];
        assert_eq!(first.heading, "이번 세션");
        assert!(first.body.contains("카드 0"), "{}", first.body);
    }

    /// The settings a reader cannot see in a release binary are in it.
    #[test]
    fn the_configuration_is_carried_through() {
        let panel = diagnostics_panel(&configuration(), &a_session(), OcrMode::Manual, "b1a7f1d");
        let settings = panel
            .blocks
            .iter()
            .find(|block| block.heading == "설정")
            .expect("a settings block");
        assert!(settings.body.contains("qwen2.5:7b"), "{}", settings.body);
        assert!(
            settings.quiet,
            "the settings are reference, not the reading"
        );
    }
}
