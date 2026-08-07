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
    /// What a model said the sentence is doing, one field to a line.
    ///
    /// **The one body a reader sees that carries newlines**, and the copy
    /// tests check it line by line for that reason. Every other card says one
    /// thing and lets the renderer wrap it; this one is a list of five
    /// headings, and joining them into a paragraph would make the shortest
    /// answer the hardest to read.
    ///
    /// A string rather than the pairs it was built from, so both renderers
    /// keep laying out one measured block of text. Ordering and headings are
    /// decided once, in [`crate::analyse::DeepAnalysis::lines`].
    Analysis(String),
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

    /// The waiting card for an explanation, which takes seconds rather than
    /// one second and so needs its own word for what is happening.
    pub fn explaining(original: impl Into<String>) -> Self {
        Self::in_progress(original, Self::EXPLAINING_TEXT)
    }

    /// A card for a finished analysis.
    ///
    /// The source stays on it, exactly as a translation's does and for the
    /// same reason: it is the only way to notice that what is being explained
    /// is not what the reader pointed at.
    ///
    /// The label goes in the **footer**, beside the model and the time, which
    /// is where a card already says how it came by what it is showing. It is
    /// first in that line rather than last, because the shell of a card cuts
    /// from the end and this is the part
    /// [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md) requires.
    pub fn analysed(original: impl Into<String>, analysis: &crate::analyse::DeepAnalysis) -> Self {
        let body = analysis
            .lines()
            .into_iter()
            .map(|(heading, text)| format!("{heading} · {text}"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        Self {
            original: original.into(),
            body: CardBody::Analysis(body),
            footer: format!(
                "{} · {} · {}",
                crate::analyse::DeepAnalysis::LABEL,
                analysis.model,
                if analysis.cached {
                    // The same word the translation card uses. A remembered
                    // explanation costs microseconds, and printing the four
                    // seconds the first one took would be a number about a
                    // press the reader has forgotten making.
                    "캐시".to_owned()
                } else if analysis.latency_ms >= 1000 {
                    format!("{:.1}초", analysis.latency_ms as f64 / 1000.0)
                } else {
                    format!("{}ms", analysis.latency_ms)
                },
            ),
        }
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

    /// What a waiting card says while an explanation is being written.
    ///
    /// Its own sentence for the same reason as the one above. A deep analysis
    /// measured 2.6 to 4.9 seconds warm against a translation's 1.6, and
    /// "번역 중" would be wrong twice: it is not a translation, and it is not
    /// over in the second that word has taught the reader to expect.
    pub const EXPLAINING_TEXT: &str = "설명 만드는 중";

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

    /// Text the reader can already read, which is not a dead end any more.
    ///
    /// This card used to say `이미 읽을 수 있는 언어입니다` and stop, and that
    /// was the whole of what Lens did with somebody's own language. A word you
    /// can read and do not know is the commonest thing on a technical screen,
    /// and translating it is not the answer: explaining it is.
    ///
    /// So the sentence stayed true and the card stopped ending there.
    pub fn already_readable(key: &str) -> Self {
        Self {
            original: String::new(),
            body: CardBody::Problem("읽을 수 있는 언어라 번역하지 않았습니다".to_owned()),
            footer: format!("{key} 를 누르면 뜻을 설명합니다"),
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

    /// The same card, marked as built on a reading that may be wrong.
    ///
    /// **On the card the reader ends up looking at**, not on one shown and then
    /// replaced. A warning that appears for a second and is covered by the
    /// answer is a warning nobody sees, and the answer is what they will act
    /// on.
    ///
    /// In the footer, beside the model and the time, because that line is
    /// already where a card says how it came by what it is showing. It is a
    /// caveat, not the message: the reading is still the best one available,
    /// and refusing to translate it would leave somebody who pressed a key
    /// with nothing at all.
    pub fn built_on_a_doubtful_reading(mut self) -> Self {
        if !self.footer.is_empty() {
            self.footer.push_str(" · ");
        }
        self.footer.push_str("읽기 불확실");
        self
    }

    /// The same card with its answer cut to roughly `keep` characters.
    ///
    /// **The answer, never the original.** A card taller than the screen it is
    /// on gets clipped by the window, and what vanishes is the end of the
    /// answer with nothing to say so: measured at 269 pixels on a text with no
    /// sentence boundary in it, which is what the segmenter returns for
    /// KakaoTalk's single region. Cutting deliberately and saying so is the
    /// same loss with the reader told about it.
    ///
    /// The original stays whole for the reason it always does. It is the only
    /// way to notice an answer that is not a translation, and an original cut
    /// to fit beside a cut answer once put a reader in front of two different
    /// halves of one sentence.
    ///
    /// Cut at a word boundary, so the last thing shown is a word rather than
    /// part of one, and marked, because an answer that merely stops reads as a
    /// model that gave up.
    pub fn with_the_answer_cut_to(mut self, keep: usize) -> Self {
        /// Fewest characters worth showing as an answer.
        ///
        /// Below this the card would say `한…`, which tells a reader less than
        /// a clipped card does: a clipped one at least shows the beginning of
        /// a sentence. A cut that leaves nothing is not a cut, it is a
        /// deletion with a marker on it.
        const WORTH_SHOWING: usize = 4;

        if let CardBody::Translation(text) = &self.body {
            let cut: String = text.chars().take(keep).collect();
            let mut cut = whole_words(&cut);
            if cut.chars().count() < WORTH_SHOWING {
                return self;
            }
            cut.push('…');
            self.body = CardBody::Translation(cut);
            // Once. A fitting loop calls this more than once and a footer
            // reading `잘림 · 잘림 · 잘림` says three times over what it needed
            // to say once.
            if !self.footer.contains(Self::CUT) {
                if !self.footer.is_empty() {
                    self.footer.push_str(" · ");
                }
                self.footer.push_str(Self::CUT);
            }
        }
        self
    }

    /// What a footer says when the answer was cut to fit.
    pub const CUT: &str = "잘림";

    /// A card for a passage too long to show honestly.
    ///
    /// **Not a clipped card.** The layout is original, then answer, then
    /// footer, so a card taller than the screen loses the answer first: a
    /// reader hovering an unsegmented blob would get their own source text
    /// back and nothing else. Cutting the original instead is the one thing
    /// this file forbids, because an original cut to fit beside a translation
    /// of the whole thing puts two different halves of one sentence in front
    /// of somebody.
    ///
    /// So neither. A passage this long is one the segmenter could not break
    /// up, which is what KakaoTalk's single region produces (issue #34), and
    /// saying that is more use than most of a source with the answer missing.
    pub fn too_long_to_show() -> Self {
        Self {
            original: String::new(),
            body: CardBody::Problem("문장이 너무 길어 카드에 담을 수 없습니다".to_owned()),
            footer: "문장 나누기 실패".to_owned(),
        }
    }

    /// A card for a setting that was changed and cannot take effect yet.
    ///
    /// Some settings are read once, when the thing that uses them is built:
    /// the model and the address belong to the translation worker, and the
    /// OCR mode decides whether the acquisition thread is given a capture path
    /// at all. Changing one of those while Lens runs does nothing.
    ///
    /// Saying nothing is the failure this repository keeps meeting. A reader
    /// who edits a file, saves it, and sees no change concludes the file does
    /// not work, and there was a console line for exactly this that a binary
    /// started from Explorer has nowhere to print.
    ///
    /// `keys` is the settings-file spelling, in English, because that is what
    /// they typed and what they will look for again.
    pub fn needs_a_restart(keys: &str) -> Self {
        Self {
            original: keys.to_owned(),
            body: CardBody::Problem("다시 실행해야 적용됩니다".to_owned()),
            footer: "설정 변경".to_owned(),
        }
    }

    /// A card for a second copy of Lens, which is about to exit.
    ///
    /// Lens opens no window on launch, so somebody who double-clicks it and
    /// sees nothing double-clicks it again. Until this, the second copy ran to
    /// the end and reported both hotkeys as taken by another application,
    /// which was a sentence about Lens naming something else.
    pub fn already_running() -> Self {
        Self {
            original: String::new(),
            body: CardBody::Problem("이미 실행 중입니다".to_owned()),
            footer: "알림 영역에 아이콘이 있습니다".to_owned(),
        }
    }

    /// What this card puts on the clipboard, or `None` when there is nothing
    /// worth keeping.
    ///
    /// A reader handed a translation has no way to keep it: the overlay passes
    /// clicks through ([ADR-0004](../../../docs/decisions/ADR-0004-passive-overlay.md)),
    /// so there is nothing to select and nothing to drag. This is the whole of
    /// the answer to that.
    ///
    /// # The source comes with it, always
    ///
    /// For the reason [`Self::original`] exists at all. A translation pasted
    /// into somebody's notes on its own can never be checked against anything
    /// again, and this product's own benchmarks are a list of confident
    /// answers that were wrong. What is kept has to carry what it is a
    /// translation **of**.
    ///
    /// # The footer does not, except the part that is a warning
    ///
    /// `qwen2.5:7b · 로컬 · 2.6초` is about a press the reader made and will
    /// not remember. [`crate::analyse::DeepAnalysis::LABEL`] is not: it says a
    /// model wrote the grammar below it and may be wrong, and a learner
    /// pasting an explanation into their notes without that is exactly the
    /// failure [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md)
    /// requires the label for.
    ///
    /// # Nothing is copied from a card that has no answer on it
    ///
    /// A problem or a waiting card yields `None`. Replacing whatever somebody
    /// had on their clipboard with 번역 중 is worse than doing nothing, and
    /// doing nothing is what a key with nothing to do should do.
    pub fn for_the_clipboard(&self) -> Option<String> {
        let original = self.original.trim();
        match &self.body {
            CardBody::Translation(text) if !text.trim().is_empty() && !original.is_empty() => {
                Some(format!("{original}\n{}", text.trim()))
            }
            CardBody::Analysis(text) if !text.trim().is_empty() && !original.is_empty() => {
                Some(format!(
                    "{original}\n{}\n{}",
                    text.trim(),
                    crate::analyse::DeepAnalysis::LABEL
                ))
            }
            _ => None,
        }
    }

    /// The same card, marked as having been copied.
    ///
    /// In the footer, beside the model, because that line is already where a
    /// card says how it came by what it is showing.
    ///
    /// **The card is redrawn rather than replaced.** A confirmation card would
    /// take the text off the screen at the moment somebody decided to keep it,
    /// which is the one moment they are still reading it.
    pub fn copied(mut self) -> Self {
        if !self.footer.is_empty() {
            self.footer.push_str(" · ");
        }
        self.footer.push_str(Self::COPIED);
        self
    }

    /// What a copied card says. One place, because a test checks for it.
    pub const COPIED: &str = "복사함";

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
            // The same rule as a translation, and for the same reason. An
            // explanation of a sentence, shown without the sentence, cannot be
            // checked against anything: it is a model's account of text the
            // reader would have to remember.
            CardBody::Analysis(text) => !text.trim().is_empty() && !self.original.trim().is_empty(),
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

#[cfg(test)]
mod copy_tests {
    use super::*;
    use crate::acquire::{AcquireError, ProtectedReason};
    use crate::translate::TranslateError;

    /// Every sentence a reader can be shown, gathered so one test can read
    /// them all.
    ///
    /// A list rather than a scan: there is no way to enumerate string literals,
    /// and a test that checked only what somebody remembered to add would be
    /// worth less than one that says plainly what it covers. Adding a card
    /// constructor without adding it here leaves it unchecked, and the comment
    /// is the only thing standing between that and the next reader.
    fn everything_a_reader_can_be_shown() -> Vec<(&'static str, String)> {
        let mut copy: Vec<(&'static str, String)> = Vec::new();

        fn parts(what: &'static str, card: &Card, into: &mut Vec<(&'static str, String)>) {
            let (CardBody::Translation(text)
            | CardBody::Problem(text)
            | CardBody::Waiting(text)
            | CardBody::Analysis(text)) = &card.body;
            // An analysis is the one body with lines in it, so it is checked a
            // line at a time. Pushing it whole would fail the newline rule for
            // the one card where a newline is the point.
            for line in text.lines() {
                into.push((what, line.to_owned()));
            }
            into.push((what, card.original.clone()));
            into.push((what, card.footer.clone()));
        }
        let mut card = |what, made: Card| parts(what, &made, &mut copy);

        card("hold_the_modifier", Card::hold_the_modifier("Alt"));
        card(
            "cannot_read_this_app",
            Card::cannot_read_this_app(Some("Ctrl+Alt+O")),
        );
        card(
            "cannot_read_this_app none",
            Card::cannot_read_this_app(None),
        );
        card("not_readable", Card::not_readable("읽지 못했습니다"));
        card("explaining", Card::explaining("a sentence"));
        card("already_readable", Card::already_readable("Ctrl+Alt+D"));
        card(
            "analysed",
            Card::analysed(
                "a sentence",
                &crate::analyse::DeepAnalysis {
                    lemma: "rest".to_owned(),
                    part_of_speech: "동사".to_owned(),
                    sense: "포인터를 멈추게 하는 것".to_owned(),
                    structure: "제한 조건을 설정하는 구문".to_owned(),
                    note: String::new(),
                    model: "qwen2.5-coder:7b".to_owned(),
                    latency_ms: 3670,
                    cached: false,
                },
            ),
        );
        card(
            "not_acquired protected",
            Card::not_acquired(AcquireError::Protected(ProtectedReason::Declared)),
        );
        card(
            "not_acquired unsupported",
            Card::not_acquired(AcquireError::Unsupported),
        );
        card("already_running", Card::already_running());
        card("needs_a_restart", Card::needs_a_restart("model, ocr"));
        card("too_long_to_show", Card::too_long_to_show());
        card("waiting", Card::waiting("a sentence"));
        card("loading_model", Card::loading_model("a sentence"));

        for error in [
            TranslateError::Unavailable,
            TranslateError::ModelMissing("qwen2.5:7b".to_owned()),
            TranslateError::Timeout,
            TranslateError::Malformed,
            TranslateError::NotTranslated,
            TranslateError::AlreadyReadable,
            TranslateError::Stale,
            TranslateError::NotWorthTranslating,
        ] {
            copy.push(("user_message", error.user_message()));
            if let Some(remedy) = error.remedy() {
                copy.push(("remedy", remedy));
            }
            parts(
                "not_translated",
                &Card::not_translated("a sentence", &error),
                &mut copy,
            );
        }

        copy
    }

    /// No sentence a reader sees has a run of spaces in the middle of it.
    ///
    /// **This has reached a user twice.** A `\`-continuation inside a Rust
    /// string literal is meant to swallow the newline and the indentation after
    /// it, and `rustfmt` has twice rejoined such a literal onto one line with
    /// the indentation left in. What arrived was
    ///
    /// > Lens has no way to                      send text off this machine
    ///
    /// in a refusal, and the same shape in a banner line. Both were found by
    /// reading the output rather than by anything in the suite, which is the
    /// part worth fixing.
    #[test]
    fn no_sentence_shown_to_a_reader_has_a_run_of_spaces_in_it() {
        for (what, sentence) in everything_a_reader_can_be_shown() {
            assert!(
                !sentence.contains("  "),
                "{what} has a run of spaces in it, from a line continuation \
                 rustfmt rejoined with its indentation: {sentence:?}",
            );
            assert_eq!(
                sentence.trim(),
                sentence,
                "{what} has space at one end: {sentence:?}",
            );
            assert!(
                !sentence.contains('\n'),
                "{what} has a newline in it, and a card lays out its own lines: \
                 {sentence:?}",
            );
        }
    }

    /// An analysis is a list, and the list is what the card shows.
    ///
    /// One line per field the model filled in, headed, in the order
    /// `DeepAnalysis::lines` decides. Asserted here rather than left to the
    /// renderer, because a body joined into a paragraph would still fit on the
    /// card and would still be readable, and nobody would notice the shortest
    /// answer had become the hardest one.
    #[test]
    fn an_analysis_is_one_headed_line_per_field_it_has() {
        let card = Card::analysed(
            "The pointer must rest before anything is read.",
            &crate::analyse::DeepAnalysis {
                lemma: "rest".to_owned(),
                part_of_speech: "동사".to_owned(),
                structure: "제한 조건을 설정하는 구문".to_owned(),
                model: "qwen2.5-coder:7b".to_owned(),
                latency_ms: 3670,
                ..crate::analyse::DeepAnalysis::default()
            },
        );
        let CardBody::Analysis(body) = &card.body else {
            panic!("{card:?}");
        };
        assert_eq!(
            body.lines().collect::<Vec<_>>(),
            vec![
                "원형 · rest",
                "품사 · 동사",
                "구조 · 제한 조건을 설정하는 구문"
            ],
        );
        assert!(card.is_showable());
    }

    /// The label ADR-0005 requires, on the card, where it is not cut off.
    ///
    /// First in the footer rather than last: the fitting loop and the shell
    /// both take from the end, and this is the part that must survive.
    #[test]
    fn an_analysis_says_a_model_wrote_it_before_it_says_which() {
        let card = Card::analysed(
            "a sentence",
            &crate::analyse::DeepAnalysis {
                sense: "뜻".to_owned(),
                model: "qwen2.5-coder:7b".to_owned(),
                latency_ms: 3670,
                ..crate::analyse::DeepAnalysis::default()
            },
        );
        assert!(
            card.footer.starts_with(crate::analyse::DeepAnalysis::LABEL),
            "{}",
            card.footer,
        );
        assert!(card.footer.contains("3.7초"), "{}", card.footer);
    }

    /// An explanation with no sentence beside it cannot be checked against
    /// anything, so it is not shown at all.
    #[test]
    fn an_analysis_with_no_original_is_not_showable() {
        let card = Card::analysed(
            "",
            &crate::analyse::DeepAnalysis {
                sense: "뜻".to_owned(),
                ..crate::analyse::DeepAnalysis::default()
            },
        );
        assert!(!card.is_showable());
    }

    /// And the em dash, which this repository bans everywhere.
    ///
    /// Checked here as well as by eye because a UI string is the one place it
    /// would be least noticed and most seen.
    #[test]
    fn no_sentence_shown_to_a_reader_contains_an_em_dash() {
        for (what, sentence) in everything_a_reader_can_be_shown() {
            assert!(
                !sentence.contains('\u{2014}'),
                "{what} contains an em dash: {sentence:?}",
            );
        }
    }
}

#[cfg(test)]
mod cutting_tests {
    use super::*;
    use crate::translate::{EngineId, QuickTranslation};

    fn card(text: &str) -> Card {
        Card::translated(
            "The original, which stays whole because it is how a reader checks the answer.",
            &QuickTranslation {
                text: text.to_owned(),
                engine: EngineId::LocalOllama,
                model: "fixture".to_owned(),
                latency_ms: 900,
                cached: false,
            },
        )
    }

    #[test]
    fn the_answer_is_cut_at_a_word_and_marked() {
        let cut = card("하나 둘 셋 넷 다섯 여섯 일곱").with_the_answer_cut_to(10);
        match &cut.body {
            CardBody::Translation(text) => {
                assert!(text.ends_with('…'), "{text:?}");
                assert!(!text.contains("여섯"), "it kept too much: {text:?}");
                // Cut at a word, so the last thing shown is a whole one.
                let last = text.trim_end_matches('…');
                assert!(!last.ends_with(' '), "{text:?}");
            }
            other => panic!("expected a translation, got {other:?}"),
        }
        assert!(cut.footer.contains("잘림"), "{}", cut.footer);
    }

    /// The original is untouched, which is the whole reason this cuts the
    /// answer rather than the card.
    #[test]
    fn the_original_is_never_cut() {
        let whole = card("하나 둘 셋 넷 다섯 여섯 일곱");
        let cut = whole.clone().with_the_answer_cut_to(6);
        assert_eq!(cut.original, whole.original);
    }

    /// A cut so short that nothing survives leaves the card alone.
    ///
    /// Better a clipped card than an empty one: an answer of `…` says less
    /// than the first half of a sentence does.
    #[test]
    fn a_cut_with_nothing_left_changes_nothing() {
        let whole = card("한 단어");
        let cut = whole.clone().with_the_answer_cut_to(0);
        assert_eq!(cut.body, whole.body);
        assert_eq!(cut.footer, whole.footer);
    }

    /// A problem card has no answer to cut.
    #[test]
    fn only_a_translation_is_cut() {
        let problem = Card::not_readable("읽지 못했습니다");
        let after = problem.clone().with_the_answer_cut_to(2);
        assert_eq!(after.body, problem.body);
        assert_eq!(after.footer, problem.footer);
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use crate::analyse::DeepAnalysis;
    use crate::translate::{EngineId, QuickTranslation};

    fn an_answer() -> QuickTranslation {
        QuickTranslation {
            text: "포인터가 멈춘 뒤에야 무엇이든 읽습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:7b".to_owned(),
            latency_ms: 2644,
            cached: false,
        }
    }

    /// What a reader pastes into their notes: the sentence and its answer.
    #[test]
    fn a_translation_is_copied_with_the_sentence_it_translates() {
        let card = Card::translated(
            "The pointer must rest before anything is read.",
            &an_answer(),
        );
        assert_eq!(
            card.for_the_clipboard().as_deref(),
            Some(
                "The pointer must rest before anything is read.\n포인터가 멈춘 뒤에야 무엇이든 읽습니다."
            ),
        );
    }

    /// The model and the milliseconds are about a press nobody will remember.
    #[test]
    fn the_engine_and_the_timing_are_not_copied() {
        let card = Card::translated("a sentence", &an_answer());
        let copied = card.for_the_clipboard().expect("something");
        assert!(!copied.contains("qwen2.5"), "{copied}");
        assert!(!copied.contains("2.6"), "{copied}");
        assert!(!copied.contains("로컬"), "{copied}");
    }

    /// The one part of a footer that is a warning goes with the text.
    ///
    /// A learner pasting a model's grammar claim into their notes without the
    /// caveat is the failure ADR-0005 requires the label for, and a note
    /// outlives the card it was read from.
    #[test]
    fn an_analysis_is_copied_with_the_warning_that_a_model_wrote_it() {
        let card = Card::analysed(
            "The pointer must rest.",
            &DeepAnalysis {
                lemma: "rest".to_owned(),
                part_of_speech: "동사".to_owned(),
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 3670,
                ..DeepAnalysis::default()
            },
        );
        let copied = card.for_the_clipboard().expect("something");
        assert!(copied.starts_with("The pointer must rest."), "{copied}");
        assert!(copied.contains("원형 · rest"), "{copied}");
        assert!(copied.ends_with(DeepAnalysis::LABEL), "{copied}");
        assert!(!copied.contains("3.7초"), "{copied}");
    }

    /// A key with nothing to do does nothing, rather than replacing whatever
    /// somebody had on their clipboard with a status message.
    #[test]
    fn there_is_nothing_to_copy_from_a_card_with_no_answer_on_it() {
        assert_eq!(Card::waiting("a sentence").for_the_clipboard(), None);
        assert_eq!(Card::explaining("a sentence").for_the_clipboard(), None);
        assert_eq!(Card::too_long_to_show().for_the_clipboard(), None);
        assert_eq!(
            Card::not_acquired(AcquireError::Unsupported).for_the_clipboard(),
            None,
        );
        assert_eq!(Card::already_running().for_the_clipboard(), None);
        // The one the loop draws most often for a Korean reader. Before the
        // card on screen was recorded, pressing the copy key here copied a
        // translation of a different sentence.
        assert_eq!(
            Card::already_readable("Ctrl+Alt+D").for_the_clipboard(),
            None
        );
    }

    /// A translation with no original is refused by `is_showable` and must be
    /// refused here too, for the same reason: nothing to check it against.
    #[test]
    fn a_translation_with_no_sentence_beside_it_is_not_copied() {
        let card = Card::translated("", &an_answer());
        assert!(!card.is_showable());
        assert_eq!(card.for_the_clipboard(), None);
    }

    /// The confirmation keeps the text on screen.
    #[test]
    fn a_copied_card_says_so_without_losing_its_answer() {
        let card = Card::translated("a sentence", &an_answer());
        let after = card.clone().copied();
        assert_eq!(after.body, card.body);
        assert_eq!(after.original, card.original);
        assert!(after.footer.ends_with(Card::COPIED), "{}", after.footer);
        assert!(after.footer.starts_with("qwen2.5:7b"), "{}", after.footer);
    }
}
