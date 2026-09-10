//! What a pinned detail shows, and what it refuses to.
//!
//! [ADR-0004](../../../docs/decisions/ADR-0004-native-overlay.md) named this
//! surface on 2026-08-04, *a real window the user pins or opens*, alongside the
//! passive tooltip. It has never been built, and four capabilities became
//! keyboard chords in the meantime because a button inside a click-through
//! window cannot be clicked.
//!
//! # What it is for, from a reader's own session
//!
//! A `--auto` log on 2026-08-18 shows one 264-character sentence producing
//! **eight cards**, alternating between its translation and its explanation,
//! four of them served from cache in microseconds.
//!
//! **Not because a card shows one or the other.** It shows both: the analysis
//! card carries the translation it explains, and `CardBody::Analysis` says why
//! at length. The first reading of that log said otherwise and was wrong.
//!
//! The reason is that a card is **transient**. It goes when the pointer leaves
//! ([ADR-0009](../../../docs/decisions/ADR-0009-pointer-distance-gate.md)), so a
//! reader who looks away and comes back gets the quick translation again and has
//! to ask for the explanation a second time. The explanation itself is cached,
//! and one of those in the log came back from memory: what was thrown away was
//! not the answer, it was the card.
//!
//! So the rule here is the one thing a card cannot be:
//!
//! > **A card goes when the pointer does. A detail stays until it is closed.**
//!
//! # What it deliberately does not do
//!
//! **It is not a bigger card**, and the card is not going away. The card
//! answers in about a second and a half and never takes focus, which is the
//! product ([ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md)).
//!
//! **It never opens by itself.** A dwell that produced a window would make
//! every hover a window, which is the failure the passive card exists to avoid.
//! `lens-core` cannot enforce that, because opening is the caller's, so this
//! module records it and `Detail::from_card` takes what a reader was already
//! looking at rather than a point on the screen.
//!
//! **It truncates nothing.** The card cuts its footer from the end and holds
//! 300 logical pixels; that is what a tooltip is. The whole reason to open this
//! is the answer that did not fit, and the reader's own session had
//! `일부만 번역됨` on that sentence every time it appeared.

use crate::analyse::DeepAnalysis;
use crate::card::{Card, CardBody};
use crate::panel::{Block, Panel};

/// Everything a card had to choose between, in one place.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Detail {
    /// The sentence the reader pointed at, whole.
    ///
    /// Never empty on a detail worth opening, for the reason `Card::original`
    /// is never empty: seeing the source beside the answer is the only way a
    /// reader notices an answer that is not a translation of it.
    pub original: String,
    /// The translation, when there is one.
    ///
    /// `None` is a real state and not a missing value: text already in the
    /// reader's language is never translated, and the explanation is the whole
    /// of what this panel has to show for it.
    pub translation: Option<String>,
    /// The explanation, as the labelled rows the analysis card draws.
    ///
    /// Owned rather than borrowed, because that is how a card carries them and
    /// this takes a card.
    ///
    /// Empty when nobody has asked for one. The panel says so rather than
    /// showing headings over blanks, which is the same rule `DeepAnalysis`
    /// follows for a field.
    pub rows: Vec<(String, String)>,
    /// Engine, model and latency, exactly as the card's footer says them.
    pub footer: String,
}

impl Detail {
    /// Open on what the reader is already looking at.
    ///
    /// **Not on a point.** Taking a card rather than a screen position is what
    /// makes it impossible for this to be opened by a dwell: there is no path
    /// from a settled hover to a `Card` that the reader has not already been
    /// shown.
    pub fn from_card(card: &Card) -> Option<Self> {
        if card.original.trim().is_empty() {
            return None;
        }
        let (translation, rows) = match &card.body {
            CardBody::Translation(text) => (Some(text.clone()), Vec::new()),
            CardBody::Analysis {
                translation, rows, ..
            } => (translation.clone(), rows.clone()),
            // A failure and a half-arrived answer are both states a reader
            // would be opening a window onto nothing for.
            CardBody::Problem(_) | CardBody::Waiting(_) => return None,
        };
        Some(Self {
            original: card.original.clone(),
            translation,
            rows,
            footer: card.footer.clone(),
        })
    }

    /// The explanation, arriving after the panel is already open.
    ///
    /// The reason the panel exists: the reader had to close one card to see the
    /// other. Here the translation stays where it is and the rows appear beside
    /// it.
    pub fn explained_by(&mut self, analysis: &DeepAnalysis) {
        self.rows = analysis
            .lines()
            .into_iter()
            .map(|(label, value)| (label.to_owned(), value))
            .collect();
        // The translation is kept, not replaced. An analysis carries the
        // translation it was grounded on, and taking it from there would be
        // taking the same string by a longer route; where it carries none, the
        // reader's own is still the one on screen.
        if self.translation.is_none() {
            self.translation = analysis.translation.clone();
        }
    }

    /// Whether there is an explanation to show, or only a translation.
    pub fn is_explained(&self) -> bool {
        self.rows.iter().any(|(_, value)| !value.trim().is_empty())
    }

    /// The blocks a window draws for this, in reading order.
    ///
    /// The original is quiet and the translation is not: the reader already
    /// looked at the source, and what they opened the window for is the answer.
    ///
    /// `hint` fills the explanation block when there is no explanation, so the
    /// panel names the key rather than leaving a gap where a row would be.
    pub fn as_panel(&self, hint: &str) -> Panel {
        let explanation = if self.is_explained() {
            self.rows
                .iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(label, value)| format!("{label}   {value}"))
                .collect::<Vec<_>>()
                .join(
                    "
",
                )
        } else {
            hint.to_owned()
        };
        Panel::new(
            [
                Block::quiet("원문", self.original.clone()),
                Block::loud("번역", self.translation.clone().unwrap_or_default()),
                Block::loud("설명", explanation),
            ],
            self.footer.clone(),
        )
    }

    /// What the panel says where the explanation would be, before there is one.
    ///
    /// Names the key rather than describing a capability, for the reason
    /// `lens_core::reach` exists: a capability nobody can discover is a silent
    /// product.
    pub fn nothing_explained_yet(key: &str) -> String {
        format!("{key} 를 누르면 이 문장을 설명합니다")
    }
}

#[cfg(test)]
mod tests {
    use super::Detail;
    use crate::analyse::DeepAnalysis;
    use crate::card::{Card, CardBody};
    use crate::translate::{EngineId, QuickTranslation};

    fn a_translation() -> Card {
        Card::translated(
            "The provider returns one rectangle per rendered line.",
            &QuickTranslation {
                text: "제공자는 각 렌더링된 줄당 하나의 직사각형을 반환합니다.".to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 1500,
                cached: false,
                part_was_removed: false,
                name_changed: false,
            },
        )
    }

    /// A detail opens on what the reader is already looking at.
    #[test]
    fn a_translation_card_becomes_a_detail() {
        let detail = Detail::from_card(&a_translation()).expect("a detail");
        assert_eq!(
            detail.original,
            "The provider returns one rectangle per rendered line."
        );
        assert!(detail.translation.is_some());
        assert!(!detail.is_explained());
    }

    /// The translation stays when the explanation arrives.
    ///
    /// This is the whole point of the surface. A card goes when the pointer
    /// does; here the answer that was already read is still on screen when the
    /// account of it appears.
    #[test]
    fn an_explanation_arrives_beside_the_translation() {
        let mut detail = Detail::from_card(&a_translation()).expect("a detail");
        let before = detail.translation.clone();
        detail.explained_by(&DeepAnalysis {
            lemma: "rectangle".to_owned(),
            part_of_speech: "명사".to_owned(),
            ..DeepAnalysis::default()
        });
        assert!(detail.is_explained());
        assert_eq!(detail.translation, before, "the translation was replaced");
    }

    /// Nothing to open a window onto is not a window.
    #[test]
    fn a_failure_or_a_half_answer_opens_nothing() {
        assert!(Detail::from_card(&Card::explaining("a sentence")).is_none());
        let empty = Card {
            original: "   ".to_owned(),
            body: CardBody::Translation("무언가".to_owned()),
            footer: String::new(),
            metadata: None,
        };
        assert!(Detail::from_card(&empty).is_none());
    }

    /// An analysis card carries its translation, so a detail made from one has
    /// both without being told twice.
    #[test]
    fn an_analysis_card_brings_its_translation_with_it() {
        let card = Card {
            original: "The lease is renewed.".to_owned(),
            body: CardBody::Analysis {
                translation: Some("임대는 갱신됩니다.".to_owned()),
                word_at: None,
                counterpart_at: None,
                links: vec![],
                rows: vec![("원형".to_owned(), "lease".to_owned())],
            },
            footer: "AI 설명 · 사실 확인 필요".to_owned(),
            metadata: None,
        };
        let detail = Detail::from_card(&card).expect("a detail");
        assert_eq!(detail.translation.as_deref(), Some("임대는 갱신됩니다."));
        assert!(detail.is_explained());
    }

    /// A row of empty values is not an explanation.
    #[test]
    fn headings_over_blanks_are_not_an_explanation() {
        let mut detail = Detail::from_card(&a_translation()).expect("a detail");
        detail.explained_by(&DeepAnalysis::default());
        assert!(!detail.is_explained());
    }

    /// The placeholder names the key, rather than describing a capability.
    #[test]
    fn the_placeholder_names_the_key() {
        let said = Detail::nothing_explained_yet("Ctrl+Alt+D");
        assert!(said.contains("Ctrl+Alt+D"), "{said}");
    }
}
