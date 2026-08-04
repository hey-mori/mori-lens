//! What a card says, with no idea how it is drawn.
//!
//! Pure data so the Windows and macOS overlays render the same content, and so
//! the rules about what a card must contain are testable without a screen.

use crate::acquire::AcquireError;
use crate::geometry::{ScreenPoint, ScreenRect};
use crate::translate::{QuickTranslation, TranslateError};

/// The contents of one card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// The text the user pointed at. **Never empty on a successful card.**
    ///
    /// This is a requirement rather than a nicety. A local model can be talked
    /// into emitting text a page chose instead of a translation
    /// (`docs/privacy-model.md` §1), and the only thing that lets a reader
    /// notice is seeing the original next to it. A card that shows the answer
    /// alone has removed the mitigation.
    pub original: String,
    /// The translation, or the reason there is not one.
    pub body: CardBody,
    /// Engine, model, latency. Small, and always present: the user can tell at
    /// a glance whether what they are reading left the machine.
    pub footer: String,
}

/// Either an answer or an honest failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardBody {
    Translation(String),
    /// A sentence the user can act on, never a code.
    Problem(String),
}

impl Card {
    /// A card for a finished translation.
    pub fn translated(original: impl Into<String>, translation: &QuickTranslation) -> Self {
        Self {
            original: original.into(),
            body: CardBody::Translation(translation.text.clone()),
            footer: format!(
                "{} · {} · {}ms",
                translation.model,
                if translation.engine.leaves_the_machine() {
                    "외부"
                } else {
                    "로컬"
                },
                translation.latency_ms,
            ),
        }
    }

    /// A card for an answer that is still arriving.
    ///
    /// Carries the original like any other, so the rule that a translation
    /// never appears without it holds for a half-finished one too. The footer
    /// says so rather than showing a latency, because a number that is still
    /// counting is noise and a final-looking one would be a lie.
    pub fn partial(original: impl Into<String>, so_far: impl Into<String>) -> Self {
        Self {
            original: original.into(),
            body: CardBody::Translation(so_far.into()),
            footer: "번역 중".to_owned(),
        }
    }

    /// A card for a translation that did not arrive.
    ///
    /// Still carries the original, because the user pointed at something and
    /// showing them nothing but an error tells them less than showing them the
    /// sentence they were looking at.
    pub fn not_translated(original: impl Into<String>, error: &TranslateError) -> Self {
        Self {
            original: original.into(),
            body: CardBody::Problem(error.user_message()),
            footer: error.reason().to_owned(),
        }
    }

    /// A card for text that could not be read at all.
    pub fn not_acquired(error: AcquireError) -> Self {
        Self {
            original: String::new(),
            body: CardBody::Problem(match error {
                AcquireError::Protected(_) => "보호된 입력란입니다".to_owned(),
                AcquireError::Unsupported => "이 앱은 텍스트를 제공하지 않습니다".to_owned(),
                _ => "읽을 수 없었습니다".to_owned(),
            }),
            footer: error.reason().to_owned(),
        }
    }

    /// Whether this card is safe to show.
    ///
    /// A translation with no original beside it is refused, per the note on
    /// [`Self::original`]. An error card may have no original, because there
    /// may genuinely be no text.
    pub fn is_showable(&self) -> bool {
        match &self.body {
            CardBody::Translation(text) => {
                !text.trim().is_empty() && !self.original.trim().is_empty()
            }
            CardBody::Problem(text) => !text.trim().is_empty(),
        }
    }
}

/// Where a card should sit, given what it is explaining.
///
/// Below the text and slightly right, which is where a reader's eye already is
/// in a left-to-right script, and far enough from the pointer that the card
/// does not sit under the cursor and swallow the next hover.
///
/// Clamped into `work_area` so a card near an edge stays on screen. The clamp
/// is the whole reason this is a function rather than an addition: an anchor at
/// the bottom right of a monitor otherwise puts the card off it.
pub fn place_card(
    anchor: ScreenRect,
    pointer: ScreenPoint,
    card: (i32, i32),
    work_area: ScreenRect,
) -> ScreenRect {
    const GAP: i32 = 12;
    let (width, height) = card;

    // Prefer below the anchor. Fall back to above when there is no room, which
    // is what happens on the last line of a window.
    let below = anchor.bottom() + GAP;
    let above = anchor.y - GAP - height;
    let y = if below + height <= work_area.bottom() {
        below
    } else if above >= work_area.y {
        above
    } else {
        // Neither fits: clamp inside rather than pick a side.
        (work_area.bottom() - height).max(work_area.y)
    };

    let preferred_x = anchor.x.max(pointer.x - width / 4);
    let x = preferred_x.min(work_area.right() - width).max(work_area.x);

    ScreenRect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::EngineId;

    fn translation(text: &str) -> QuickTranslation {
        QuickTranslation {
            text: text.to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 900,
        }
    }

    #[test]
    fn a_translation_without_its_original_is_refused() {
        // The mitigation for a model that was talked into saying something
        // else. Without the original beside it, a reader has nothing to check
        // the answer against.
        let card = Card::translated("", &translation("무엇인가"));
        assert!(!card.is_showable());
    }

    #[test]
    fn a_translation_with_its_original_is_showable() {
        let card = Card::translated("something", &translation("무엇인가"));
        assert!(card.is_showable());
        assert!(card.footer.contains("로컬"), "{}", card.footer);
        assert!(card.footer.contains("qwen2.5:3b"));
    }

    #[test]
    fn a_failure_still_shows_the_sentence_the_user_pointed_at() {
        let card = Card::not_translated(
            "The suite did not produce anything.",
            &TranslateError::Unavailable,
        );
        assert!(card.is_showable());
        assert_eq!(card.original, "The suite did not produce anything.");
        assert!(card.body != CardBody::Problem(String::new()));
    }

    #[test]
    fn an_external_engine_says_so_on_the_card() {
        let mut answer = translation("무엇인가");
        answer.engine = EngineId::External;
        let card = Card::translated("something", &answer);
        assert!(card.footer.contains("외부"), "{}", card.footer);
    }

    #[test]
    fn a_card_near_the_bottom_edge_flips_above_the_text() {
        let work = ScreenRect::new(0, 0, 1920, 1080);
        // A line of text 20px from the bottom leaves no room below.
        let placed = place_card(
            ScreenRect::new(400, 1040, 300, 20),
            ScreenPoint::new(500, 1050),
            (360, 180),
            work,
        );
        assert!(placed.bottom() <= work.bottom(), "{placed:?}");
        assert!(placed.y < 1040, "should have flipped above the text");
    }

    #[test]
    fn a_card_near_the_right_edge_stays_on_screen() {
        let work = ScreenRect::new(0, 0, 1920, 1080);
        let placed = place_card(
            ScreenRect::new(1800, 400, 100, 20),
            ScreenPoint::new(1850, 410),
            (360, 180),
            work,
        );
        assert!(placed.right() <= work.right(), "{placed:?}");
        assert!(placed.x >= work.x);
    }

    #[test]
    fn a_card_on_a_monitor_with_a_negative_origin_stays_on_it() {
        // A second monitor left of the primary. Clamping to zero here would
        // throw the card onto the wrong screen.
        let work = ScreenRect::new(-1920, -200, 1920, 1080);
        let placed = place_card(
            ScreenRect::new(-1800, 0, 200, 20),
            ScreenPoint::new(-1700, 10),
            (360, 180),
            work,
        );
        assert!(
            placed.x >= work.x && placed.right() <= work.right(),
            "{placed:?}"
        );
        assert!(placed.x < 0, "the card left its monitor: {placed:?}");
    }

    #[test]
    fn a_card_taller_than_the_screen_is_still_placed_inside_it() {
        let work = ScreenRect::new(0, 0, 800, 200);
        let placed = place_card(
            ScreenRect::new(10, 100, 100, 20),
            ScreenPoint::new(50, 110),
            (360, 400),
            work,
        );
        assert_eq!(placed.y, work.y, "{placed:?}");
    }
}
