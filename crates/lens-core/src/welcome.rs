//! The first hover, arranged rather than hoped for.
//!
//! Every tool this reader has used trains the same reflex: **select, then
//! act**. Lens asks them to stop moving, which has no precedent, which is
//! invisible, and which is indistinguishable from the product being broken. A
//! reader who has not learnt the gesture and a reader whose window is
//! unsupported see exactly the same thing, and it is nothing.
//!
//! So the first hover is staged: a window this repository owns, holding a
//! sentence this repository wrote, which the reader hovers and which answers.
//! See [`docs/what-finished-means.md`](../../../docs/what-finished-means.md) §1.
//!
//! # What is real about it
//!
//! Everything except where the characters came from. The dwell, the distance
//! gate, the language decision, the engine, the card, the overlay: all of it is
//! the ordinary path, so the first card is also proof that Ollama answered, a
//! model is installed, and the overlay draws on this machine.
//!
//! Only acquisition is different, and it is different because a Win32 window
//! cannot be read the way a real one is. This repository measured that plain
//! `EDIT` controls expose no text pattern at all
//! (`docs/compatibility/windows-app-matrix.md`), so a welcome built from them
//! would fail to acquire, and the very first thing a new reader ever saw would
//! be the product not working.
//!
//! The lesson therefore **hands over** its sentence instead of exposing it, and
//! says so: [`crate::acquire::AcquisitionSource::Lesson`] exists so that no
//! log, benchmark, or compatibility row can mistake a lesson for an
//! acquisition.
//!
//! # Why this file chooses the sentence
//!
//! A lesson in the reader's own language shows **no card**: `decide` returns
//! [`Decision::AlreadyInTarget`] and the product deliberately spends nothing.
//! That is right everywhere else and fatal here, where the whole point is that
//! something appears.
//!
//! So the sentence is not fixed. It is chosen by asking `decide` the same
//! question the product is about to ask, and keeping the first one that
//! answers with a card. An English reader gets the Korean lesson without
//! anybody having written a rule about English readers.

use crate::geometry::{line_under, ScreenPoint, ScreenRect};
use crate::language::{decide, Decision};

/// The lesson in each language this ships with.
///
/// Two lines, and the count is a decision. One teaches only the gesture; three
/// is a page, and a page is read rather than hovered. The second line is the
/// privacy promise, because it is the claim a reader is least likely to
/// believe and most able to check: they are about to watch it be kept.
///
/// The text of the first line **is** the instruction it teaches, so a reader
/// who never hovers has still been told what to do, and a reader who does
/// hover gets that same instruction back in their own language from the thing
/// they just learnt to use.
const LESSONS: [Lesson; 2] = [
    Lesson {
        language: "English",
        lines: [
            "Rest the pointer on this line and wait. Do not click.",
            "Nothing you read here leaves this machine.",
        ],
    },
    Lesson {
        language: "Korean",
        lines: [
            "이 줄에 포인터를 올려 두고 기다리세요. 누르지 마세요.",
            "여기서 읽은 것은 이 컴퓨터를 떠나지 않습니다.",
        ],
    },
];

/// One language's version of the lesson.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lesson {
    /// What language its lines are written in, in the spelling
    /// [`crate::settings::Settings::language`] uses.
    pub language: &'static str,
    pub lines: [&'static str; 2],
}

impl Lesson {
    /// The lines, in the order they are drawn.
    pub fn lines(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.lines.iter().copied()
    }

    /// Which line the pointer is on, given where they were drawn.
    ///
    /// The rectangles come from the adapter, which is the only thing that knows
    /// its own font and its own scaling. The rule that turns them into an
    /// answer is [`line_under`], which is the same rule the OCR path uses, so a
    /// lesson and a screen reading cannot disagree about what "on this line"
    /// means.
    pub fn line_under(&self, drawn: &[ScreenRect], point: ScreenPoint) -> Option<&'static str> {
        line_under(drawn, point).and_then(|index| self.lines.get(index).copied())
    }
}

/// The lesson to show a reader whose target language is `reader_language`.
///
/// # Why a card is not enough, and `Translate` is
///
/// The obvious rule is "keep any lesson that is not already in their
/// language". It is wrong, and the test that caught it is
/// `an_english_reader_is_taught_in_korean`.
///
/// `decide` answers [`Decision::Undecidable`] for text in the **same script**
/// as the target, because telling English from French needs a classifier this
/// crate does not have. Undecidable text is translated anyway, so an English
/// reader shown the English lesson does get a card. It is a card restating a
/// sentence they could already read, which teaches the gesture and teaches
/// that the gesture is pointless.
///
/// So the first pass demands [`Decision::Translate`]: a different script, which
/// is the only thing this crate can actually prove.
///
/// # And why `Undecidable` is still the fallback
///
/// A reader whose `language` this crate has no script for makes every lesson
/// undecidable, and demanding proof would leave them with no first hover at
/// all. They get one, chosen without proof, which is exactly how the rest of
/// the product treats them.
///
/// Returns `None` only when every shipped lesson is one the reader can already
/// read. Showing a window whose every line produces no card would teach that
/// hovering does nothing, which is the opposite of the exercise.
pub fn lesson_for(reader_language: &str) -> Option<Lesson> {
    let every_line = |lesson: &Lesson, answer: Decision| {
        lesson
            .lines()
            .all(|line| decide(line, reader_language) == answer)
    };
    LESSONS
        .into_iter()
        .find(|lesson| every_line(lesson, Decision::Translate))
        .or_else(|| {
            LESSONS.into_iter().find(|lesson| {
                lesson
                    .lines()
                    .all(|line| decide(line, reader_language) != Decision::AlreadyInTarget)
            })
        })
}

/// Whether the welcome should be staged at all.
///
/// True on the run that created the settings file and never again. It is
/// deliberately **not** a setting a reader can leave on: a window that returns
/// every launch is a nag, and the tray menu is where somebody who wants it back
/// asks for it.
pub fn show_on(first_run: bool) -> bool {
    first_run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_korean_reader_is_taught_in_english() {
        let lesson = lesson_for("Korean").expect("Korean readers have a lesson");
        assert_eq!(lesson.language, "English");
    }

    /// The case the fixed sentence would have failed silently.
    ///
    /// An English reader hovering an English lesson gets `AlreadyInTarget`, no
    /// card, and the conclusion that the product does not work. Nothing about
    /// English is written down anywhere; this falls out of asking `decide`.
    #[test]
    fn an_english_reader_is_taught_in_korean() {
        let lesson = lesson_for("English").expect("English readers have a lesson");
        assert_eq!(lesson.language, "Korean");
    }

    /// Every shipped lesson must produce a card for the reader it is shown to.
    ///
    /// This is the property the whole module exists for, asserted against the
    /// real decision function rather than against a belief about it.
    #[test]
    fn whatever_is_chosen_produces_a_card() {
        for reader in ["Korean", "English", "Japanese", "French"] {
            let Some(lesson) = lesson_for(reader) else {
                continue;
            };
            for line in lesson.lines() {
                assert_ne!(
                    decide(line, reader),
                    Decision::AlreadyInTarget,
                    "the {reader} reader's lesson line shows no card: {line:?}",
                );
            }
        }
    }

    /// And for a reader whose language can be judged, it must be a real
    /// translation rather than a restatement.
    ///
    /// `French` is in this list on purpose. It shares its script with the
    /// English lesson, so the weaker rule would hand a French reader English
    /// sentences and call the restatement a lesson.
    #[test]
    fn a_judgeable_reader_is_taught_in_another_script() {
        for reader in ["Korean", "English", "Japanese", "French"] {
            let lesson = lesson_for(reader).expect("a judgeable reader has a lesson");
            for line in lesson.lines() {
                assert_eq!(
                    decide(line, reader),
                    Decision::Translate,
                    "the {reader} reader's lesson is not provably foreign: {line:?}",
                );
            }
        }
    }

    /// A reader whose language this cannot judge still gets taught.
    ///
    /// `decide` answers `Undecidable` for a language it has no script for, and
    /// `Undecidable` is not `AlreadyInTarget`, so the first lesson is kept.
    /// Worth pinning: the alternative reading of "cannot judge" is "show
    /// nothing", which would leave those readers with no first hover at all.
    #[test]
    fn an_unknown_language_still_gets_the_first_lesson() {
        let lesson = lesson_for("Klingon").expect("an unjudgeable reader still gets one");
        assert_eq!(lesson.language, "English");
    }

    #[test]
    fn the_pointer_lands_on_the_line_it_is_over() {
        let lesson = lesson_for("Korean").unwrap();
        let drawn = [
            ScreenRect::new(100, 100, 400, 20),
            ScreenRect::new(100, 130, 400, 20),
        ];
        assert_eq!(
            lesson.line_under(&drawn, ScreenPoint::new(200, 135)),
            Some(lesson.lines[1]),
        );
        assert_eq!(lesson.line_under(&drawn, ScreenPoint::new(200, 400)), None);
    }

    /// More rectangles than lines cannot index past the end.
    ///
    /// The adapter draws chrome as well as lesson lines, and an off-by-one in
    /// what it passes here would panic in a release binary on the first hover
    /// a new reader ever performs.
    #[test]
    fn a_rectangle_with_no_line_behind_it_is_not_a_line() {
        let lesson = lesson_for("Korean").unwrap();
        let drawn = [
            ScreenRect::new(100, 100, 400, 20),
            ScreenRect::new(100, 130, 400, 20),
            ScreenRect::new(100, 160, 400, 20),
        ];
        assert_eq!(lesson.line_under(&drawn, ScreenPoint::new(200, 165)), None);
    }

    #[test]
    fn the_welcome_is_the_first_run_and_nothing_else() {
        assert!(show_on(true));
        assert!(!show_on(false));
    }
}
