//! What any window that is not the card shows, as blocks of text.
//!
//! [ADR-0012](../../../docs/decisions/ADR-0012-three-product-surfaces.md) names
//! three surfaces. The card is its own thing and always will be: 300 logical
//! pixels, click-through, never activating. The other two are ordinary windows
//! a reader opens on purpose, and what they have in common is that they are a
//! heading, some text, a heading, some text.
//!
//! So the renderer takes this and neither surface owns a window of its own. The
//! second one paid for the generalisation, which is the right time to do it:
//! [`crate::detail::Detail`] had a window written for it, and rather than a
//! second four hundred lines for diagnostics, both produce a `Panel`.
//!
//! # What a panel is not
//!
//! **Not a layout.** There are no widths here, no coordinates and no fonts. A
//! block is a heading and a body, and where those land is the renderer's
//! business, which is what keeps this in `lens-core`
//! ([ADR-0003](../../../docs/decisions/ADR-0003-rust-core-platform-adapters.md)).
//!
//! **Not scrollable, and not a table.** Both are real and neither is here yet.
//! A panel that needed either would be asking for a widget toolkit, and the
//! first thing to find out is whether a reader opens these at all.

/// One heading and the text under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The heading, in the reader's language. Short: it is a label, not a
    /// sentence.
    pub heading: String,
    /// What sits under it. May carry newlines; the renderer wraps and does not
    /// truncate.
    pub body: String,
    /// Whether this is supporting rather than the thing being read.
    ///
    /// The original on a detail is quiet and its translation is not. A renderer
    /// that ignores this draws a correct panel; one that honours it draws the
    /// one a reader's eye lands on in the right place.
    pub quiet: bool,
}

impl Block {
    /// A block a reader is meant to read.
    pub fn loud(heading: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            body: body.into(),
            quiet: false,
        }
    }

    /// A block that is there for reference.
    pub fn quiet(heading: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            body: body.into(),
            quiet: true,
        }
    }
}

/// A window's worth of text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Panel {
    /// The blocks, in the order they are read.
    ///
    /// Empty ones are dropped on the way in rather than drawn as a heading over
    /// a blank, which is the rule `DeepAnalysis` follows for a field and the
    /// card follows for a row.
    pub blocks: Vec<Block>,
    /// The line along the bottom. Engine, model, latency, or nothing.
    pub footer: String,
}

impl Panel {
    /// Build one, dropping any block with nothing under it.
    pub fn new(blocks: impl IntoIterator<Item = Block>, footer: impl Into<String>) -> Self {
        Self {
            blocks: blocks
                .into_iter()
                .filter(|block| !block.body.trim().is_empty())
                .collect(),
            footer: footer.into(),
        }
    }

    /// Whether there is anything to open a window onto.
    pub fn says_anything(&self) -> bool {
        !self.blocks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Block, Panel};

    /// A heading over a blank is not a block.
    #[test]
    fn an_empty_block_is_dropped() {
        let panel = Panel::new(
            [
                Block::quiet("원문", "The lease is renewed."),
                Block::loud("번역", "   "),
                Block::loud("설명", ""),
            ],
            "qwen2.5:7b",
        );
        assert_eq!(panel.blocks.len(), 1);
        assert_eq!(panel.blocks[0].heading, "원문");
        assert!(panel.says_anything());
    }

    /// Nothing to show is not a window.
    #[test]
    fn a_panel_with_no_blocks_says_nothing() {
        let panel = Panel::new([Block::loud("설명", "")], "");
        assert!(!panel.says_anything());
    }

    /// Order is the order it was given, because it is reading order.
    #[test]
    fn the_blocks_keep_their_order() {
        let panel = Panel::new(
            [
                Block::quiet("원문", "a"),
                Block::loud("번역", "b"),
                Block::loud("설명", "c"),
            ],
            "",
        );
        let headings: Vec<&str> = panel
            .blocks
            .iter()
            .map(|block| block.heading.as_str())
            .collect();
        assert_eq!(headings, ["원문", "번역", "설명"]);
    }

    /// Quiet and loud survive the trip, since the renderer reads them.
    #[test]
    fn a_block_remembers_whether_it_is_quiet() {
        let panel = Panel::new([Block::quiet("원문", "a"), Block::loud("번역", "b")], "");
        assert!(panel.blocks[0].quiet);
        assert!(!panel.blocks[1].quiet);
    }
}
