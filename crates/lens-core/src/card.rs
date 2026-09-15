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

/// Whether the text on a card was read from pixels rather than handed over
/// by the operating system.
///
/// Typed, because the persistence layer used to derive this by searching the
/// footer for the Korean string `Card::FROM_PIXELS`. If UI copy changed, the
/// database provenance changed with it. The type is the invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardProvenance {
    /// The operating system's accessibility tree handed the text over.
    Accessibility,
    /// The text was read off a screenshot.
    Ocr,
}

/// Whether the engine that produced the answer runs on this machine or
/// across the network.
///
/// Typed, for the same reason as [`CardProvenance`]: the footer used to be
/// parsed for "외부" and "로컬", and a UI reword would have rewritten the
/// database column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardProviderScope {
    /// The engine runs on this machine.
    Local,
    /// The engine runs across the network.
    External,
}

/// Typed provenance for the persistence layer.
///
/// Carries the information that `record_presented_reading` writes to the
/// history database. Set by card constructors from the structured types
/// ([`QuickTranslation`], [`AcquisitionSource`]) that carry the same facts
/// before the footer renders them into a display string.
///
/// `None` on cards that were not produced by a translation or analysis:
/// error cards, waiting cards, refusal cards. These never reach the
/// persistence path (`PresentationKind::Feedback` is not recorded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardMetadata {
    pub provenance: CardProvenance,
    pub provider_scope: Option<CardProviderScope>,
    pub model: Option<String>,
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
    /// Typed provenance for the persistence layer. `None` on cards that never
    /// reach history (error cards, waiting cards, feedback cards). Read by
    /// `record_presented_reading` instead of parsing the footer string.
    pub(crate) metadata: Option<CardMetadata>,
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
    /// What a model said the sentence is doing, as `(label, value)` rows.
    ///
    /// **The one body that is not a piece of prose.** Every other card says one
    /// thing and lets the renderer wrap it; this one is a list of up to five
    /// headed rows, and a renderer that draws it as a paragraph makes the
    /// shortest answer the hardest to read.
    ///
    /// It was a single string with newlines in it until 2026-08-11, which is
    /// why both renderers still measure it as one block: they ask
    /// [`CardBody::rows_as_text`] for the paragraph they used to be handed.
    /// Drawing the rows as rows is the change that string was in the way of.
    ///
    /// Ordering, headings, and which fields are worth showing at all are
    /// decided once, in [`crate::analyse::DeepAnalysis::lines`].
    Analysis {
        /// The translation the explanation is about, when there was one.
        ///
        /// **`Ctrl+Alt+D` used to take the answer off the screen.** A reader
        /// pressed it to understand a translation and the translation was
        /// replaced by an account of it, so the two things they needed to hold
        /// side by side were never on the card at the same time.
        ///
        /// `None` is a real case: text already in the reader's language is
        /// never translated, and it is still explained.
        translation: Option<String>,
        /// Where the focused word sits in [`Card::original`], for a renderer
        /// that can mark part of a line.
        ///
        /// Byte offsets, because that is what the string is indexed by. A
        /// renderer that needs another unit converts, and the one that does is
        /// documented where it does it.
        word_at: Option<(usize, usize)>,
        /// And where its counterpart sits in `translation`.
        counterpart_at: Option<(usize, usize)>,
        /// Verified span pairs linking source to translation.
        ///
        /// Each link says "this part of the original corresponds to that part of
        /// the translation". The renderer marks both spans with the same visual
        /// identity (colour + underline) so the reader sees the correspondence
        /// at a glance. Empty when the model produced no links, all links
        /// failed verification, or the prompt variant does not request links.
        /// An empty list is not an error: the card works identically without it.
        links: Vec<crate::analyse::TextLink>,
        rows: Vec<(String, String)>,
    },
}

impl CardBody {
    /// Where `needle` sits inside `text`, if it sits there exactly once.
    ///
    /// **Once, and that is the whole rule.** A word that appears twice in a
    /// sentence has no single place the pointer was on, and marking the first
    /// one is a guess dressed as an answer. The reader who hovered the second
    /// `card` in `A card on the wrong sentence is worse than no card` would be
    /// shown a mark on the first, and nothing about the card would say it had
    /// chosen.
    ///
    /// Nothing is marked instead, which is what this product does everywhere
    /// else it cannot prove which thing it is looking at.
    pub fn mark_of(text: &str, needle: &str) -> Option<(usize, usize)> {
        let needle = needle.trim();
        if needle.is_empty() {
            return None;
        }
        let mut found = text.match_indices(needle);
        let (at, _) = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some((at, at + needle.len()))
    }

    /// The rows of an analysis, or nothing for any other body.
    pub fn analysis_rows(&self) -> &[(String, String)] {
        match self {
            Self::Analysis { rows, .. } => rows,
            _ => &[],
        }
    }

    /// The analysis as the block of text it used to be.
    ///
    /// For the clipboard, the saved file, and any renderer that has not yet
    /// learnt to draw two columns. One place decides how a row reads as a line,
    /// so what a reader copies and what they see cannot drift apart.
    pub fn rows_as_text(rows: &[(String, String)]) -> String {
        rows.iter()
            .map(|(label, value)| format!("{label} · {value}"))
            .collect::<Vec<_>>()
            // `"\n"`, not a newline typed inside the quotes. rustfmt keeps the
            // literal one and it reads as an accident every time somebody
            // opens this file, which is how `CLAUDE.md`'s string rule was
            // earned.
            .join("\n")
    }
}

impl Card {
    /// A card for a finished translation.
    pub fn translated(original: impl Into<String>, translation: &QuickTranslation) -> Self {
        let provider_scope = match translation.engine {
            crate::translate::EngineId::Local => Some(CardProviderScope::Local),
            crate::translate::EngineId::External => Some(CardProviderScope::External),
        };
        let card = Self {
            original: original.into(),
            body: CardBody::Translation(translation.text.clone()),
            // **Where it ran, then how long, then which model.**
            //
            // The model name led this line until 2026-08-11, and it is the one
            // token on it a reader has no use for. `privacy-model.md` says what
            // this line is for: it is small and incidental and it carries one
            // thing that is not, which is whether the answer left the machine.
            // That token now comes first, where a line nobody finishes reading
            // puts the thing they must not miss.
            footer: format!(
                "{} · {} · {}",
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
                translation.model,
            ),
            metadata: Some(CardMetadata {
                provenance: CardProvenance::Accessibility,
                provider_scope,
                model: Some(translation.model.clone()),
            }),
        };
        // Said on the card the reader ends up looking at, for the same reason
        // the other two caveats are: a warning that is not on the answer is a
        // warning nobody reads.
        let card = if translation.part_was_removed {
            card.say(Self::PART_REMOVED)
        } else {
            card
        };
        if translation.name_changed {
            card.say(Self::NAME_CHANGED)
        } else {
            card
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
            metadata: None,
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
            metadata: None,
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
        let original = original.into();
        // **Pairs, not a paragraph.** This joined its rows into one string and
        // handed the renderers a block of text with newlines in it, so a list
        // of five headed rows was drawn as five sentences. The rows travel as
        // rows now, and anything that wants the old block asks for it.
        let body = analysis
            .lines()
            .into_iter()
            .map(|(heading, text)| (heading.to_owned(), text.to_owned()))
            .collect();
        Self {
            body: CardBody::Analysis {
                word_at: analysis
                    .word
                    .as_deref()
                    .and_then(|word| CardBody::mark_of(&original, word)),
                counterpart_at: analysis
                    .translation
                    .as_deref()
                    .zip(analysis.counterpart.as_deref())
                    .and_then(|(text, span)| CardBody::mark_of(text, span)),
                translation: analysis.translation.clone(),
                links: analysis.links.clone(),
                rows: body,
            },
            original,
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
            metadata: Some(CardMetadata {
                provenance: CardProvenance::Accessibility,
                provider_scope: None,
                model: Some(analysis.model.clone()),
            }),
        }
    }

    fn in_progress(original: impl Into<String>, status: &str) -> Self {
        Self {
            original: original.into(),
            body: CardBody::Waiting(status.to_owned()),
            footer: String::new(),
            metadata: None,
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
            metadata: None,
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
                metadata: None,
            },
            None => Self {
                original: String::new(),
                body: CardBody::Problem("이 앱은 글자를 내주지 않습니다".to_owned()),
                footer: "화면 읽기를 쓸 수 없습니다".to_owned(),
                metadata: None,
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
            metadata: None,
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
            metadata: None,
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
            metadata: None,
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
    pub fn built_on_a_doubtful_reading(self) -> Self {
        self.say(Self::DOUBTFUL)
    }

    /// What a card says when its text was recognised from pixels.
    pub const FROM_PIXELS: &str = "화면 읽기";
    /// And when that reading also looks wrong.
    pub const DOUBTFUL: &str = "읽기 불확실";
    /// What a card says when part of the answer was taken off it.
    ///
    /// A model that switches language mid-sentence gives back a correct first
    /// half and a second half in a third language.
    /// [`crate::language::without_the_third_language`] keeps the first half,
    /// which is right, and the reader then saw a sentence ending mid-phrase
    /// under a footer saying it took 3.9 seconds. Nothing said the end was
    /// missing, so the card read as a complete answer that simply stopped
    /// making sense.
    pub const PART_REMOVED: &str = "일부만 번역됨";
    /// What a card says when an identifier did not survive the translation.
    ///
    /// Short, because it is the fourth of these and they accumulate. The thing
    /// a reader has to know is that a name on this card may not be the name:
    /// `windows-rs` came back as `windows-r스`, which looks almost right and
    /// matches nothing.
    pub const NAME_CHANGED: &str = "이름 바뀜";

    /// The same card, marked as built on characters read off the screen.
    ///
    /// **The certain fact, where `is_misread` is a guess.** That rule is a
    /// shape heuristic and says so: it catches misreadings whose shape is
    /// impossible and leaves the ones whose shape is merely unlikely, and its
    /// own comment lists `5h0l` for `show` and `于or'` for `for` as ones it
    /// cannot see. A reader's card carried
    /// `/// Print the resolved word. 0千f by default` with nothing on it to say
    /// the characters had been guessed at.
    ///
    /// Whether the text came from pixels is not a guess. It is the difference
    /// between a sentence a provider handed over and one this machine read off
    /// a screenshot, and it belongs on the line that already says how the card
    /// came by what it is showing.
    ///
    /// **Never both.** `읽기 불확실` already means the characters were read
    /// from pixels *and* look wrong, so a card that says both spends the
    /// footer's width saying one thing twice, on the line that sets the card's
    /// minimum width.
    pub fn read_from_the_screen(mut self) -> Self {
        if self.footer.contains(Self::DOUBTFUL) {
            return self;
        }
        if let Some(ref mut meta) = self.metadata {
            meta.provenance = CardProvenance::Ocr;
        }
        self.say(Self::FROM_PIXELS)
    }

    /// Every caveat this line can carry, so one can be told from a fact.
    ///
    /// The footer is a `·` separated list ordered by usefulness, and the rule
    /// below has to know which entries are caveats. Nothing else does.
    const CAVEATS: [&str; 4] = [
        Self::FROM_PIXELS,
        Self::DOUBTFUL,
        Self::PART_REMOVED,
        Self::NAME_CHANGED,
    ];

    /// Put a caveat on the quiet line, in the model's place.
    ///
    /// **The footer is a list ordered by usefulness and it has a width.** It
    /// sets the card's minimum width, so a fourth entry widens every card that
    /// carries one. The model name is last because it is the entry a reader has
    /// no use for, and a caveat is the entry they must not miss, so the caveat
    /// takes its place rather than queueing behind it.
    ///
    /// This was found by adding one: `로컬 · 2.8초 · qwen2.5-coder:7b · 화면
    /// 읽기` is 36 characters against a 32 ceiling. **`읽기 불확실` had been
    /// over it since the day it landed** and nothing noticed, because the
    /// length test only ever measured a card with no caveat on it.
    ///
    /// Appends instead when there is nothing to displace: a footer of one or
    /// two entries is a failure card or a status, and neither has a model name
    /// in it to give up.
    fn say(mut self, what: &str) -> Self {
        if self.footer.is_empty() {
            self.footer = what.to_owned();
            return self;
        }
        // **Caveats never displace each other**, and the first version of this
        // let them. A card read from pixels whose answer had a foreign tail
        // removed said `화면 읽기` and silently dropped `일부만 번역됨`: the
        // reader was told where the characters came from and not that half the
        // answer was missing, which is the more important of the two.
        //
        // So the list is split. Facts yield from the end, model first and then
        // the time, until the line fits; caveats are kept and appended in the
        // order they were applied.
        let is_caveat = |part: &str| Self::CAVEATS.contains(&part);
        let mut facts: Vec<String> = Vec::new();
        let mut caveats: Vec<String> = Vec::new();
        for part in self.footer.split(" · ") {
            if is_caveat(part) {
                caveats.push(part.to_owned());
            } else {
                facts.push(part.to_owned());
            }
        }
        if !caveats.iter().any(|held| held == what) {
            caveats.push(what.to_owned());
        }
        let joined = |facts: &[String], caveats: &[String]| {
            facts
                .iter()
                .chain(caveats.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        };
        // The first fact is the one thing on this line a reader must not miss,
        // so it is never dropped however many caveats arrive.
        while facts.len() > 1 && joined(&facts, &caveats).chars().count() > Self::FOOTER_LIMIT {
            facts.pop();
        }
        self.footer = joined(&facts, &caveats);
        self
    }

    /// How long the quiet line may be, in characters.
    ///
    /// It sets the card's minimum width, so this is a size rather than a
    /// preference. `the_footer_stays_short` measures every shape against it.
    pub const FOOTER_LIMIT: usize = 32;

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
            metadata: None,
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
            metadata: None,
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
            metadata: None,
        }
    }

    /// What this card puts on the clipboard, or `None` when there is nothing
    /// worth keeping.
    ///
    /// A reader handed a translation has no way to keep it: the overlay passes
    /// clicks through ([ADR-0004](../../../docs/decisions/ADR-0004-native-overlay.md)),
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
        let (answer, caveat) = self.answer_worth_keeping()?;
        Some(match caveat {
            Some(caveat) => format!("{}\n{answer}\n{caveat}", self.original.trim()),
            None => format!("{}\n{answer}", self.original.trim()),
        })
    }

    /// The same card as something to write down.
    ///
    /// The application comes from the caller, because a card does not know
    /// where it was read. Everything else is what is on it.
    ///
    /// Whether there is anything at all is [`Self::answer_worth_keeping`],
    /// shared with the clipboard deliberately. Two rules for what counts as an
    /// answer would eventually let a card be worth copying and not worth
    /// keeping, and a reader would have no way to guess which.
    pub fn kept_as(&self, app: Option<&str>) -> Option<crate::keep::Entry> {
        let (answer, caveat) = self.answer_worth_keeping()?;
        Some(crate::keep::Entry {
            original: self.original.trim().to_owned(),
            answer: answer.to_owned(),
            caveat: caveat.map(str::to_owned),
            app: app
                .map(str::to_owned)
                .filter(|name| !name.trim().is_empty()),
            source_language: None,
            target_language: None,
            model: self.metadata.as_ref().and_then(|m| m.model.clone()),
        })
    }

    /// The answer on this card and the caveat it carries, or nothing.
    ///
    /// Nothing for a problem or a waiting card: replacing somebody's clipboard
    /// or their notes with 번역 중 is worse than doing nothing. Nothing without
    /// an original either, for the reason [`Self::original`] exists at all, and
    /// which a file makes sharper than a clipboard does: an answer kept alone
    /// can never be checked against anything again.
    /// Owned rather than borrowed, because an analysis is now rows and the
    /// block a reader copies is made here rather than stored anywhere.
    fn answer_worth_keeping(&self) -> Option<(String, Option<&str>)> {
        if self.original.trim().is_empty() {
            return None;
        }
        let (text, caveat) = match &self.body {
            CardBody::Translation(text) => (text.clone(), None),
            CardBody::Analysis { rows, .. } => (
                CardBody::rows_as_text(rows),
                Some(crate::analyse::DeepAnalysis::LABEL),
            ),
            _ => return None,
        };
        (!text.trim().is_empty()).then(|| (text.trim().to_owned(), caveat))
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

    /// The same card, marked as written down.
    ///
    /// Beside the model in the footer, exactly as [`Self::copied`] is, and for
    /// the same reason: the card stays up, because a confirmation card would
    /// take the text away at the moment somebody decided to keep it.
    pub fn kept(mut self) -> Self {
        if !self.footer.is_empty() {
            self.footer.push_str(" · ");
        }
        self.footer.push_str(Self::KEPT);
        self
    }

    /// What a saved card says.
    pub const KEPT: &str = "저장함";

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
            CardBody::Analysis { rows, .. } => !rows.is_empty() && !self.original.trim().is_empty(),
        }
    }

    /// Typed provenance for the persistence layer.
    ///
    /// `Some` on cards produced by a translation or analysis, `None` on every
    /// other card. Read by `record_presented_reading` instead of parsing the
    /// footer string.
    pub fn metadata(&self) -> Option<&CardMetadata> {
        self.metadata.as_ref()
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
            engine: EngineId::Local,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 900,
            cached: false,
            part_was_removed: false,
            name_changed: false,
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
    fn a_translation_carries_typed_metadata() {
        let card = Card::translated("something", &translation("무엇인가"));
        let meta = card.metadata().expect("translation carries metadata");
        assert_eq!(meta.provenance, CardProvenance::Accessibility);
        assert_eq!(meta.provider_scope, Some(CardProviderScope::Local));
        assert_eq!(meta.model.as_deref(), Some("qwen2.5:3b"));
    }

    #[test]
    fn read_from_the_screen_sets_ocr_provenance_in_metadata() {
        let card = Card::translated("something", &translation("무엇인가")).read_from_the_screen();
        let meta = card
            .metadata()
            .expect("still carries metadata after OCR mark");
        assert_eq!(meta.provenance, CardProvenance::Ocr);
        assert_eq!(meta.model.as_deref(), Some("qwen2.5:3b"));
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
                engine: crate::translate::EngineId::Local,
                model: "m".to_owned(),
                latency_ms: 900,
                cached: false,
                part_was_removed: false,
                name_changed: false,
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
                part_was_removed: false,
                name_changed: false,
                text: "번역".to_owned(),
                engine: EngineId::Local,
                model: "qwen2.5-coder:7b".to_owned(),
                latency_ms,
                cached,
            },
        )
    }

    // These three are about how a duration is written, not where it sits.
    // They said `ends_with` while the latency happened to be last, and pinned
    // an ordering they were not about.
    #[test]
    fn a_slow_answer_is_reported_in_seconds() {
        assert!(card_at(2779, false).footer.contains("2.8초"));
    }

    #[test]
    fn a_fast_answer_keeps_its_milliseconds() {
        assert!(card_at(840, false).footer.contains("840ms"));
    }

    #[test]
    fn a_cached_answer_still_says_so() {
        assert!(card_at(2779, true).footer.contains("캐시"));
    }

    /// Where it ran comes first, and that is the point of the line.
    ///
    /// `privacy-model.md` says the footer is small and incidental and carries
    /// one thing that is not: whether the answer left the machine. A reader who
    /// stops partway through this line must have read that much, so it goes
    /// where a line is started rather than where it is finished.
    #[test]
    fn the_footer_says_where_it_ran_before_anything_else() {
        assert!(
            card_at(2779, false).footer.starts_with("로컬"),
            "{}",
            card_at(2779, false).footer
        );
    }

    /// And the model, which a reader has no use for, comes last.
    #[test]
    fn the_model_is_the_last_thing_on_the_line() {
        assert!(
            card_at(2779, false).footer.ends_with("qwen2.5-coder:7b"),
            "{}",
            card_at(2779, false).footer
        );
    }

    /// A card whose characters were guessed at says so.
    ///
    /// The card that prompted this carried
    /// `/// Print the resolved word. 0千f by default; see `5h0누』 word banner&`
    /// with a footer reading `로컬 · 캐시 · qwen2.5:7b`, which is what a
    /// sentence a provider handed over looks like.
    #[test]
    fn a_card_read_off_the_screen_says_where_its_characters_came_from() {
        let card = card_at(2779, false).read_from_the_screen();
        assert!(card.footer.contains(Card::FROM_PIXELS), "{}", card.footer);
        // And still leads with the one thing on the line that is not
        // incidental.
        assert!(card.footer.starts_with("로컬"), "{}", card.footer);
    }

    /// **Never both.** `읽기 불확실` already means read from pixels and wrong.
    #[test]
    fn a_doubtful_card_does_not_also_say_it_was_read_from_the_screen() {
        let card = card_at(2779, false)
            .built_on_a_doubtful_reading()
            .read_from_the_screen();
        assert!(card.footer.contains(Card::DOUBTFUL), "{}", card.footer);
        assert!(!card.footer.contains(Card::FROM_PIXELS), "{}", card.footer);
    }

    /// And the order the two arrive in must not change the answer.
    #[test]
    fn the_stronger_caveat_wins_whichever_is_applied_first() {
        let first = card_at(2779, false)
            .read_from_the_screen()
            .built_on_a_doubtful_reading();
        assert!(first.footer.contains(Card::DOUBTFUL), "{}", first.footer);
    }

    /// A card whose answer had a foreign tail cut off it says so.
    ///
    /// The card that prompted this ended at `두 번째 테스트의`, mid-phrase,
    /// under a footer reading `로컬 · 3.9초 · qwen2.5:7b`. The model had
    /// switched to Chinese partway through, the third-language rule removed the
    /// Chinese, and what was left was a correct Korean translation of the first
    /// half of the sentence presented as the whole answer.
    #[test]
    fn a_card_missing_part_of_its_answer_says_so() {
        // Through `Card::translated`, so this checks the path the reader's
        // card took rather than the builder it ends with.
        let card = Card::translated(
            "original",
            &QuickTranslation {
                text: "번역".to_owned(),
                engine: EngineId::Local,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 3900,
                cached: false,
                part_was_removed: true,
                name_changed: false,
            },
        );
        assert!(card.footer.contains(Card::PART_REMOVED), "{}", card.footer);
        assert!(card.footer.starts_with("로컬"), "{}", card.footer);
    }

    /// And an ordinary answer does not.
    #[test]
    fn an_answer_that_kept_all_of_itself_says_nothing() {
        let card = card_at(2779, false);
        assert!(!card.footer.contains(Card::PART_REMOVED), "{}", card.footer);
    }

    /// **Two caveats both survive, and the model yields to make room.**
    ///
    /// The first version of this rule let a later caveat replace an earlier
    /// one. A card read from pixels whose answer had a foreign tail removed
    /// said `화면 읽기` and silently dropped `일부만 번역됨`, so the reader was
    /// told where the characters came from and not that half the answer was
    /// gone.
    #[test]
    fn a_second_caveat_does_not_replace_the_first() {
        let card = Card::translated(
            "original",
            &QuickTranslation {
                text: "번역".to_owned(),
                engine: EngineId::Local,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 3900,
                cached: false,
                part_was_removed: true,
                name_changed: false,
            },
        )
        .read_from_the_screen();

        assert!(card.footer.contains(Card::PART_REMOVED), "{}", card.footer);
        assert!(card.footer.contains(Card::FROM_PIXELS), "{}", card.footer);
        assert!(card.footer.starts_with("로컬"), "{}", card.footer);
        // The model is what yields, because it is the entry a reader has no
        // use for and the caveats are the entries they must not miss.
        assert!(!card.footer.contains("qwen"), "{}", card.footer);
        assert!(
            card.footer.chars().count() <= Card::FOOTER_LIMIT,
            "{} is {} chars",
            card.footer,
            card.footer.chars().count()
        );
    }

    /// And the same caveat twice is still one caveat.
    #[test]
    fn saying_the_same_thing_twice_says_it_once() {
        let card = card_at(2779, false)
            .read_from_the_screen()
            .read_from_the_screen();
        assert_eq!(
            card.footer.matches(Card::FROM_PIXELS).count(),
            1,
            "{}",
            card.footer
        );
    }

    /// A caveat takes the model's place rather than queueing behind it.
    #[test]
    fn a_caveat_displaces_the_model_and_not_the_privacy_signal() {
        let card = card_at(2779, false).read_from_the_screen();
        assert!(card.footer.starts_with("로컬"), "{}", card.footer);
        assert!(card.footer.contains("2.8초"), "{}", card.footer);
        assert!(!card.footer.contains("qwen"), "{}", card.footer);
        assert!(card.footer.ends_with(Card::FROM_PIXELS), "{}", card.footer);
    }

    /// A footer with nothing to displace keeps what it has.
    ///
    /// A failure card's footer is one entry, the reason, and losing it to a
    /// caveat would replace what went wrong with a note about how it was read.
    #[test]
    fn a_short_footer_is_appended_to_rather_than_overwritten() {
        let card = Card::not_readable("이 부분에서 글자를 찾지 못했습니다").read_from_the_screen();
        assert_eq!(card.footer, Card::FROM_PIXELS);

        let failed = Card {
            original: "x".to_owned(),
            body: CardBody::Translation("y".to_owned()),
            footer: "timeout".to_owned(),
            metadata: None,
        }
        .read_from_the_screen();
        assert_eq!(failed.footer, format!("timeout · {}", Card::FROM_PIXELS));
    }

    /// The footer sets the card's minimum width, so its length is a size.
    /// Every shape of it, not only the one with no caveat on it.
    ///
    /// The version that measured a plain card let `읽기 불확실` sit at 33
    /// characters for as long as it existed.
    #[test]
    fn the_footer_stays_short() {
        for footer in [
            card_at(2779, false).footer,
            card_at(2779, true).footer,
            card_at(840, false).read_from_the_screen().footer,
            card_at(2779, false).built_on_a_doubtful_reading().footer,
            card_at(2779, true)
                .read_from_the_screen()
                .built_on_a_doubtful_reading()
                .footer,
            // Every caveat at once, which is the widest this line ever gets.
            card_at(2779, false)
                .say(Card::PART_REMOVED)
                .read_from_the_screen()
                .built_on_a_doubtful_reading()
                .footer,
        ] {
            assert!(
                footer.chars().count() <= Card::FOOTER_LIMIT,
                "{footer:?} is {} chars",
                footer.chars().count()
            );
        }
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
            match &card.body {
                CardBody::Translation(text) | CardBody::Problem(text) | CardBody::Waiting(text) => {
                    into.push((what, text.clone()))
                }
                // **Every label and every value on its own.** An analysis was
                // one string with newlines and was checked a line at a time
                // for that reason; now it is rows, so each half of each row is
                // its own thing a reader is shown, and each is checked.
                CardBody::Analysis {
                    translation, rows, ..
                } => {
                    if let Some(text) = translation {
                        into.push((what, text.clone()));
                    }
                    for (label, value) in rows {
                        into.push((what, label.clone()));
                        into.push((what, value.clone()));
                    }
                }
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
                    counterpart: Some("직사각형".to_owned()),
                    word: Some("rectangle".to_owned()),
                    translation: Some("제공자는 하나의 직사각형을 반환합니다.".to_owned()),
                    lemma: "rest".to_owned(),
                    part_of_speech: "동사".to_owned(),
                    sense: "포인터를 멈추게 하는 것".to_owned(),
                    structure: "제한 조건을 설정하는 구문".to_owned(),
                    note: String::new(),
                    model: "qwen2.5-coder:7b".to_owned(),
                    latency_ms: 3670,
                    cached: false,
                    links: vec![],
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
                "{what} has a run of spaces in it, from a line continuation rustfmt rejoined with its indentation: {sentence:?}",
            );
            assert_eq!(
                sentence.trim(),
                sentence,
                "{what} has space at one end: {sentence:?}",
            );
            assert!(
                !sentence.contains('\n'),
                "{what} has a newline in it, and a card lays out its own lines: {sentence:?}",
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
        let CardBody::Analysis { rows, .. } = &card.body else {
            panic!("{card:?}");
        };
        // As rows, which is what the card now carries. An empty field is left
        // out entirely rather than shown blank, and that is what the missing
        // sense and note are doing here.
        assert_eq!(
            rows.iter()
                .map(|(label, value)| (label.as_str(), value.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("원형", "rest"),
                ("품사", "동사"),
                ("구조", "제한 조건을 설정하는 구문"),
            ],
        );
        // And the block a reader copies is still the block they always got.
        assert_eq!(
            CardBody::rows_as_text(rows).lines().collect::<Vec<_>>(),
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
                engine: EngineId::Local,
                model: "fixture".to_owned(),
                latency_ms: 900,
                cached: false,
                part_was_removed: false,
                name_changed: false,
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
            engine: EngineId::Local,
            model: "qwen2.5:7b".to_owned(),
            latency_ms: 2644,
            cached: false,
            part_was_removed: false,
            name_changed: false,
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
        assert!(after.footer.starts_with("로컬"), "{}", after.footer);
    }
}

/// Where a card marks a word, and when it refuses to.
#[cfg(test)]
mod mark_tests {
    use super::*;

    const SENTENCE: &str = "The provider returns one rectangle per rendered line.";

    #[test]
    fn a_word_that_appears_once_is_found_where_it_is() {
        let (from, to) = CardBody::mark_of(SENTENCE, "rectangle").expect("it is in there");
        assert_eq!(&SENTENCE[from..to], "rectangle");
    }

    /// **The case the rule exists for.**
    ///
    /// This sentence is the one this repository tests its own translations
    /// against, and `card` is in it twice. A reader who hovered the second
    /// would be shown a mark on the first, with nothing on the card to say it
    /// had chosen.
    #[test]
    fn a_word_that_appears_twice_is_not_marked_at_all() {
        let twice = "A card on the wrong sentence is worse than no card.";
        assert_eq!(CardBody::mark_of(twice, "card"), None);
    }

    #[test]
    fn a_word_that_is_not_there_is_not_marked() {
        assert_eq!(CardBody::mark_of(SENTENCE, "triangle"), None);
        assert_eq!(CardBody::mark_of(SENTENCE, "  "), None);
        assert_eq!(CardBody::mark_of("", "x"), None);
    }

    /// Byte offsets, and Korean is three bytes a character.
    ///
    /// The offsets have to slice the string they came from. A renderer that
    /// counts something else converts, and the one that does says so where it
    /// does it.
    #[test]
    fn korean_offsets_slice_the_string_they_came_from() {
        let translation = "제공자는 각 렌더링된 줄당 하나의 직사각형을 반환합니다.";
        let (from, to) = CardBody::mark_of(translation, "직사각형").expect("it is in there");
        assert_eq!(&translation[from..to], "직사각형");
        assert!(from > 4, "byte offsets, not character ones: {from}");
    }

    /// The card computes both halves, or neither, and never a wrong one.
    #[test]
    fn the_card_marks_the_word_and_its_counterpart() {
        let card = Card::analysed(
            SENTENCE,
            &crate::analyse::DeepAnalysis {
                lemma: "rectangle".to_owned(),
                word: Some("rectangle".to_owned()),
                translation: Some("제공자는 하나의 직사각형을 반환합니다.".to_owned()),
                counterpart: Some("직사각형".to_owned()),
                ..crate::analyse::DeepAnalysis::default()
            },
        );
        let CardBody::Analysis {
            translation,
            word_at,
            counterpart_at,
            ..
        } = &card.body
        else {
            panic!("{card:?}");
        };
        let (from, to) = word_at.expect("the word is in the original once");
        assert_eq!(&card.original[from..to], "rectangle");
        let (from, to) = counterpart_at.expect("the counterpart is in the translation once");
        assert_eq!(&translation.as_deref().unwrap()[from..to], "직사각형");
    }

    /// No counterpart, no mark, and the card is otherwise the same card.
    #[test]
    fn a_card_with_nothing_agreed_marks_nothing() {
        let card = Card::analysed(
            SENTENCE,
            &crate::analyse::DeepAnalysis {
                lemma: "rectangle".to_owned(),
                word: Some("rectangle".to_owned()),
                translation: Some("제공자는 하나의 직사각형을 반환합니다.".to_owned()),
                ..crate::analyse::DeepAnalysis::default()
            },
        );
        let CardBody::Analysis {
            counterpart_at,
            word_at,
            ..
        } = &card.body
        else {
            panic!("{card:?}");
        };
        assert_eq!(*counterpart_at, None);
        // The word half still stands. Half a pair is still a true thing to
        // show: it says which word the pointer was on, which the card could
        // not say at all before.
        assert!(word_at.is_some());
    }
}

/// The caveat a card carries when a name did not survive.
#[cfg(test)]
mod name_changed_tests {
    use super::Card;
    use crate::translate::{EngineId, QuickTranslation};

    /// A name that did not survive is said on the card the reader looks at.
    ///
    /// The translation around it is usually right, so the card keeps it and
    /// carries the caveat. What a reader cannot afford is copying a name that
    /// does not exist: measured 2026-08-18, `windows-rs` came back as
    /// `windows-r스`, which looks almost right and matches nothing.
    #[test]
    fn a_changed_name_is_said_on_the_card() {
        let card = Card::translated(
            "The LedgerDataSheetService returns one row per sheet.",
            &QuickTranslation {
                text: "레저데이터시트서비스는 한 장당 한 행씩 반환합니다.".to_owned(),
                engine: EngineId::Local,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 1500,
                cached: false,
                part_was_removed: false,
                name_changed: true,
            },
        );
        assert!(
            card.footer.contains(Card::NAME_CHANGED),
            "the footer does not say the name changed: {}",
            card.footer,
        );
        assert!(
            card.footer.chars().count() <= Card::FOOTER_LIMIT,
            "the fourth caveat pushed the footer over its limit: {}",
            card.footer,
        );
    }

    /// Two caveats at once, since they accumulate and are never dropped.
    #[test]
    fn a_changed_name_and_a_cut_answer_are_both_said() {
        let card = Card::translated(
            "The LedgerDataSheetService returns one row per sheet.",
            &QuickTranslation {
                text: "레저데이터시트서비스는 한 장당".to_owned(),
                engine: EngineId::Local,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 1500,
                cached: false,
                part_was_removed: true,
                name_changed: true,
            },
        );
        assert!(card.footer.contains(Card::PART_REMOVED), "{}", card.footer);
        assert!(card.footer.contains(Card::NAME_CHANGED), "{}", card.footer);
    }
}
