//! What a card says, with no idea how it is drawn.
//!
//! Pure data so the Windows and macOS overlays render the same content, and so
//! the rules about what a card must contain are testable without a screen.

use crate::acquire::AcquireError;
use crate::geometry::{ScreenPoint, ScreenRect};
use crate::translate::{QuickTranslation, TranslateError};

/// A partial answer cut back to its last complete word.
///
/// A streamed translation arrives a few characters at a time, so a redraw
/// lands mid-word about as often as not: a real hover showed `삽입 스크`, which
/// is a word broken in half and reads as a rendering fault rather than as text
/// still arriving.
///
/// Only for partials. The finished answer is shown exactly as the engine gave
/// it, because cutting *that* back would silently drop the last word.
///
/// Text with no space in it yet is returned whole: two characters of an answer
/// are better than none, and a language that does not space its words would
/// otherwise never show anything until it finished.
fn whole_words(text: &str) -> String {
    match text.trim_end().rsplit_once(char::is_whitespace) {
        Some((whole, _)) => whole.to_owned(),
        None => text.trim_end().to_owned(),
    }
}

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
    /// The answer is on its way, and what is happening while it is.
    ///
    /// A quick translation measures about a second and a half from the pointer
    /// settling, and for most of that the screen shows nothing at all. A user
    /// cannot tell that from a product that did not notice them, so they move
    /// the pointer, which cancels the hover, which means it really does show
    /// nothing. This is the state that breaks that loop.
    Waiting(String),
}

impl Card {
    /// A card for a finished translation.
    pub fn translated(original: impl Into<String>, translation: &QuickTranslation) -> Self {
        Self {
            original: original.into(),
            body: CardBody::Translation(translation.text.clone()),
            footer: format!(
                "{} · {} · {}",
                translation.model,
                if translation.engine.leaves_the_machine() {
                    "외부"
                } else {
                    "로컬"
                },
                // A sentence read twice is answered from memory, and saying
                // "0ms" for that reads as a broken clock. It says so instead.
                //
                // Seconds rather than milliseconds for anything over one: a
                // reader is not timing the model, and `2779ms` is four glyphs
                // longer than `2.8초` on the line that sets the card's minimum
                // width.
                if translation.cached {
                    "캐시".to_owned()
                } else if translation.latency_ms >= 1000 {
                    format!("{:.1}초", translation.latency_ms as f64 / 1000.0)
                } else {
                    format!("{}ms", translation.latency_ms)
                },
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
            body: CardBody::Translation(whole_words(&so_far.into())),
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
            // The remedy goes in the body, beside what went wrong, rather
            // than in the footer. The footer is the quiet line carrying the
            // model and the latency; a user who cannot see a translation at
            // all is not reading the quiet line.
            body: CardBody::Problem(match error.remedy() {
                Some(remedy) => format!("{} · {remedy}", error.user_message()),
                None => error.user_message(),
            }),
            footer: error.reason().to_owned(),
        }
    }

    /// A card that says the sentence was read and the answer is coming.
    ///
    /// Deliberately carries the original and nothing else. The reader gets
    /// confirmation that Lens is looking at the sentence they meant, which is
    /// the other thing they cannot otherwise tell, and gets it about a second
    /// before the translation could arrive.
    ///
    /// It is not shown the moment a hover settles. See the delay in
    /// `apps/lens/src/lifecycle.rs`: a cached sentence answers in microseconds
    /// and flashing this first would be worse than the wait it replaces.
    pub fn waiting(original: impl Into<String>) -> Self {
        Self::in_progress(original, Self::WAITING_TEXT)
    }

    /// A waiting card for a translation that is waiting on a model load.
    pub fn loading_model(original: impl Into<String>) -> Self {
        Self::in_progress(original, Self::LOADING_TEXT)
    }

    fn in_progress(original: impl Into<String>, status: &str) -> Self {
        Self {
            original: original.into(),
            body: CardBody::Waiting(status.to_owned()),
            footer: String::new(),
        }
    }

    /// What a waiting card shows in place of a translation.
    ///
    /// Words rather than an ellipsis. A lone "…" is read as an application
    /// that has stopped responding; the point of this card is the opposite.
    ///
    /// Static, deliberately. An animated indicator would need a timer, and
    /// this product does not run one (`CLAUDE.md`); it would also be the first
    /// thing to ignore the system's reduced-motion setting.
    pub const WAITING_TEXT: &str = "번역 중";

    /// What a waiting card says while the engine is still loading a model.
    ///
    /// A separate sentence because it is a separate wait. A translation takes
    /// about a second and a half; a cold model took thirteen. The reader is
    /// owed the difference, and "번역 중" for thirteen seconds is a product
    /// that looks hung.
    pub const LOADING_TEXT: &str = "모델을 불러오는 중";

    /// A card that says something plainly, with no original to show.
    ///
    /// For the manual OCR path, where a refusal has no sentence attached to it:
    /// the whole point is that nothing was read. Every other card carries the
    /// original because there is one.
    pub fn not_readable(message: impl Into<String>) -> Self {
        Self {
            original: String::new(),
            body: CardBody::Problem(message.into()),
            footer: String::new(),
        }
    }

    /// A card for an application that has proved it exposes no text.
    ///
    /// Shown once per application per session, on the decision in
    /// [`crate::reach`], and never for a protected field. It exists because
    /// hovering an unreadable window does nothing, and nothing is also what a
    /// broken product does: without this the reader learns only that Lens does
    /// not work here, which is both discouraging and untrue.
    ///
    /// `hotkey` is `None` when OCR cannot actually be reached, because the
    /// combination is held by another application or the system has no
    /// recogniser installed. Then the card says what is wrong and stops,
    /// rather than naming a key that will do nothing.
    pub fn cannot_read_this_app(hotkey: Option<&str>) -> Self {
        match hotkey {
            Some(key) => Self {
                original: "이 앱은 글자를 내주지 않습니다".to_owned(),
                body: CardBody::Problem(format!("{key} 를 누르면 화면을 읽습니다")),
                footer: "화면 읽기".to_owned(),
            },
            None => Self {
                original: String::new(),
                body: CardBody::Problem("이 앱은 글자를 내주지 않습니다".to_owned()),
                footer: "화면 읽기를 쓸 수 없습니다".to_owned(),
            },
        }
    }

    /// The card a first-time reader needs, and the one nothing could tell them.
    ///
    /// The default tier puts reading behind a modifier, so a new user rests the
    /// pointer on a sentence and nothing happens. There is a console line for
    /// it, and a release binary is a Windows subsystem application: somebody
    /// who double-clicked it from Explorer has nowhere to read one.
    ///
    /// Shown once a session, on the decision in [`crate::reach`], and never
    /// after a card has already appeared.
    pub fn hold_the_modifier(key: &str) -> Self {
        Self {
            original: "지금은 읽지 않고 있습니다".to_owned(),
            body: CardBody::Problem(format!("{key}를 누른 채 문장 위에 올려두세요")),
            footer: "설정에서 항상 읽도록 바꿀 수 있습니다".to_owned(),
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
            // The original is the whole card, so the rule that a translation
            // never appears without one is what makes this showable at all.
            CardBody::Waiting(_) => !self.original.trim().is_empty(),
        }
    }
}

/// How far the pointer may stray from the text a card explains before the card
/// is retired.
///
/// Generous on purpose. A reader looks between the original and the
/// translation, and a card that vanishes when the eye moves is a card nobody
/// finishes reading.
pub const KEEP_CARD_WITHIN_PX: f64 = 72.0;

/// Whether the pointer has left the text a card explains.
///
/// Measured against the **anchor rectangle**, not its centre. The first version
/// compared the pointer to the centre with the hover machine's 6px movement
/// radius, which retired the card on the next 25ms tick: a pointer resting on a
/// word is rarely within 6px of that word's centre. With streaming, each
/// partial answer then re-showed a card that was hidden again immediately,
/// which is what the flicker was.
pub fn pointer_left_the_card(anchor: ScreenRect, pointer: ScreenPoint) -> bool {
    anchor.distance_to(pointer) > KEEP_CARD_WITHIN_PX
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
            cached: false,
        }
    }

    #[test]
    fn the_first_run_card_names_the_key_and_the_way_out() {
        let card = Card::hold_the_modifier("Alt");
        assert!(card.is_showable());
        match &card.body {
            CardBody::Problem(text) => assert!(text.contains("Alt"), "{text}"),
            other => panic!("expected a problem card, got {other:?}"),
        }
        // The footer says the modifier is a setting rather than the product's
        // nature, because a reader who does not want to hold a key has no
        // other way to find that out.
        assert!(card.footer.contains("설정"), "{}", card.footer);
    }

    #[test]
    fn the_offer_names_the_key_that_actually_works() {
        let offered = Card::cannot_read_this_app(Some("Ctrl+Alt+O"));
        assert!(offered.is_showable());
        match &offered.body {
            CardBody::Problem(text) => assert!(text.contains("Ctrl+Alt+O"), "{text}"),
            other => panic!("expected a problem card, got {other:?}"),
        }
    }

    #[test]
    fn an_unreachable_fallback_is_not_offered() {
        // Naming a key that is held by another application, or that has no
        // recogniser behind it, is worse than saying nothing: the reader
        // presses it, nothing happens, and now two things look broken.
        let plain = Card::cannot_read_this_app(None);
        assert!(plain.is_showable());
        match &plain.body {
            CardBody::Problem(text) => {
                assert!(!text.contains("Ctrl"), "a key was named anyway: {text}");
            }
            other => panic!("expected a problem card, got {other:?}"),
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

    /// The regression this whole rule exists for.
    #[test]
    fn a_pointer_resting_on_the_word_does_not_retire_its_card() {
        // A word rectangle, and a pointer on it but not at its centre. The
        // centre-plus-6px rule dismissed this immediately, which made the card
        // flicker on every streamed partial.
        let word = ScreenRect::new(400, 300, 120, 22);
        for pointer in [
            ScreenPoint::new(405, 305),
            ScreenPoint::new(515, 320),
            ScreenPoint::new(460, 311),
        ] {
            assert!(
                !pointer_left_the_card(word, pointer),
                "{pointer:?} is on the word and would have retired the card",
            );
        }
    }

    #[test]
    fn reading_across_to_the_card_does_not_retire_it() {
        // The card sits below the text and the reader looks at it. That must
        // not dismiss what they are reading.
        let word = ScreenRect::new(400, 300, 120, 22);
        assert!(!pointer_left_the_card(word, ScreenPoint::new(430, 380)));
    }

    #[test]
    fn a_pointer_on_another_paragraph_retires_the_card() {
        let word = ScreenRect::new(400, 300, 120, 22);
        assert!(pointer_left_the_card(word, ScreenPoint::new(400, 700)));
        assert!(pointer_left_the_card(word, ScreenPoint::new(1200, 300)));
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

#[cfg(test)]
mod waiting_tests {
    use super::*;

    #[test]
    fn a_waiting_card_shows_the_sentence_it_is_working_on() {
        let card = Card::waiting("The output files came back empty.");
        assert!(card.is_showable());
        assert_eq!(card.body, CardBody::Waiting(Card::WAITING_TEXT.to_owned()));
        // No engine has answered, so there is nothing true to put here. An
        // invented latency would be the one number on the card that lied.
        assert!(card.footer.is_empty());
    }

    /// The same rule as a translation, for the same reason: a card with no
    /// original cannot be checked by the reader against what they pointed at.
    #[test]
    fn a_waiting_card_with_no_original_is_refused() {
        assert!(!Card::waiting("   ").is_showable());
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    fn body_of(card: &Card) -> String {
        match &card.body {
            CardBody::Translation(text) => text.clone(),
            other => panic!("expected a translation, got {other:?}"),
        }
    }

    /// The half-word a real hover showed.
    #[test]
    fn a_partial_is_cut_back_to_its_last_whole_word() {
        let card = Card::partial("original", "나는 다른 다섯 개의 파일에서 삽입 스크");
        assert_eq!(body_of(&card), "나는 다른 다섯 개의 파일에서 삽입");
    }

    /// Before the first space there is nothing to cut back to, and two
    /// characters of an answer are better than none.
    #[test]
    fn a_partial_with_no_space_yet_is_shown_whole() {
        assert_eq!(body_of(&Card::partial("original", "나는")), "나는");
    }

    #[test]
    fn a_partial_that_is_only_whitespace_shows_nothing() {
        assert_eq!(body_of(&Card::partial("original", "   ")), "");
    }

    /// The finished answer is never trimmed. Doing so would drop its last
    /// word, every time, silently.
    #[test]
    fn a_finished_translation_keeps_its_last_word() {
        let finished = Card::translated(
            "original",
            &QuickTranslation {
                text: "나는 다른 파일에서 삽입 스크립트를".to_owned(),
                engine: crate::translate::EngineId::LocalOllama,
                model: "m".to_owned(),
                latency_ms: 900,
                cached: false,
            },
        );
        assert_eq!(body_of(&finished), "나는 다른 파일에서 삽입 스크립트를");
    }
}

#[cfg(test)]
mod loading_tests {
    use super::*;

    /// Thirteen seconds of "번역 중" reads as a hung application. The reader is
    /// owed the difference between a translation and a model load.
    #[test]
    fn a_model_load_says_so_rather_than_claiming_to_be_translating() {
        let card = Card::loading_model("The output files came back empty.");
        assert!(card.is_showable());
        assert_eq!(card.body, CardBody::Waiting(Card::LOADING_TEXT.to_owned()));
        assert_ne!(Card::LOADING_TEXT, Card::WAITING_TEXT);
    }

    /// Same rule as every other card: the original is what makes it showable.
    #[test]
    fn a_model_load_with_no_original_is_refused() {
        assert!(!Card::loading_model("  ").is_showable());
    }
}

#[cfg(test)]
mod footer_tests {
    use super::*;
    use crate::translate::EngineId;

    fn card_at(latency_ms: u128, cached: bool) -> Card {
        Card::translated(
            "original",
            &QuickTranslation {
                text: "번역".to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5-coder:7b".to_owned(),
                latency_ms,
                cached,
            },
        )
    }

    #[test]
    fn a_slow_answer_is_reported_in_seconds() {
        assert!(card_at(2779, false).footer.ends_with("2.8초"));
    }

    #[test]
    fn a_fast_answer_keeps_its_milliseconds() {
        assert!(card_at(840, false).footer.ends_with("840ms"));
    }

    #[test]
    fn a_cached_answer_still_says_so() {
        assert!(card_at(2779, true).footer.ends_with("캐시"));
    }

    /// The footer sets the card's minimum width, so its length is a size.
    #[test]
    fn the_footer_stays_short() {
        let footer = card_at(2779, false).footer;
        assert!(
            footer.chars().count() <= 32,
            "{footer:?} is {} chars",
            footer.chars().count()
        );
    }
}
