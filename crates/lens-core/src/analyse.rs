//! Deep analysis: the second path, and the one nobody pays for by looking.
//!
//! [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md) splits the
//! product into two speeds and this is the slow one. Quick translation runs on
//! every accepted hover and answers in about a second. This answers what the
//! sentence is *doing*, and it runs only when a reader asks for it.
//!
//! # Why it is a separate path rather than more fields on the quick one
//!
//! Measured before either existed. Mori asked one local model for everything at
//! once and paid **4,372ms**, against 1,370ms for two sections. Generation cost
//! is proportional to output tokens, nobody reads eight sections for every word
//! they glance at, and a hover fires far more often than a deliberate ask does.
//! Folding this into the quick path would make every glance pay for the
//! explanation almost nobody wanted.
//!
//! # It is grounded, not re-derived
//!
//! The request carries the translation that is already on the screen. The model
//! is not asked to translate the sentence again: it is asked to explain the
//! sentence it can already see translated. That is one focused generation
//! rather than two full ones.
//!
//! # Nothing here is reachable from a hover
//!
//! [`crate::session`] has no reference to an [`Analyser`] and cannot acquire
//! one. The routing that decides accessibility versus OCR does not know this
//! module exists. That is the M5 gate's "the analysis model is never called
//! without a user action", kept by construction rather than by a check that
//! could be forgotten: there is no code path from a settled hover to here.
//!
//! # What comes back is labelled
//!
//! Grammar analysis from a small local model is frequently wrong in ways a
//! learner cannot detect, which is the reader this exists for. So the output is
//! shown as an explanation from a model, beside the source, and never as
//! settled fact. [`DeepAnalysis::LABEL`] is that label, and it lives here so
//! there is one of it.

use std::time::Duration;

use crate::translate::TranslateError;

/// How long an analysis may take before it is worthless.
///
/// Far longer than [`crate::translate::QUICK_TIMEOUT`], because the two are
/// waited on differently. A hover's answer competes with the reader's attention
/// moving on; this one was asked for, so the reader is looking at the screen
/// waiting for it, and the alternative to a slow answer is no answer.
///
/// It is still bounded. Mori measured a 9.6GB model taking 35 seconds to load
/// cold, and that case has to fail rather than hang.
pub const DEEP_TIMEOUT: Duration = Duration::from_secs(60);

/// The longest source this will analyse.
///
/// Past this the honest answer is that there is no *the* sentence to explain.
/// A model handed a paragraph and asked for its clause structure summarises,
/// and a summary of a paragraph presented as its grammar is the confident
/// wrongness this whole path is supposed to avoid.
///
/// Generous against a sentence and firm against a paragraph. The segmenter
/// already hands over a sentence where it can find one; this catches the case
/// where it could not.
pub const TOO_LONG: usize = 400;

/// What to explain, and what the reader can already see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepRequest {
    /// The source, exactly as the card shows it.
    pub sentence: String,
    /// The word the pointer was on, when there was one.
    pub word: Option<String>,
    /// The translation already on the screen.
    ///
    /// Sent so the model explains rather than re-translates. It is also the
    /// thing that makes the two answers agree: an analysis derived from a
    /// second, independent translation would explain a sentence the reader is
    /// not looking at.
    pub translation: String,
    /// The language the explanation is written in. The reader's, not the
    /// source's.
    pub target_language: String,
}

/// Which parts of an analysis this request can actually be answered with.
///
/// [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md): a follow-up is
/// offered only where it can answer. A button whose only possible reply is "not
/// applicable" spends a call and teaches the reader to distrust the buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answerable {
    /// There is a word to give a dictionary form and a sense for.
    pub the_word: bool,
    /// There is more than one word, so there is a structure to describe.
    pub the_structure: bool,
}

impl Answerable {
    /// Whether anything at all can be said, which is whether to ask at all.
    pub const fn anything(self) -> bool {
        self.the_word || self.the_structure
    }
}

impl DeepRequest {
    pub fn new(
        sentence: impl Into<String>,
        translation: impl Into<String>,
        target_language: impl Into<String>,
    ) -> Self {
        Self {
            sentence: sentence.into(),
            word: None,
            translation: translation.into(),
            target_language: target_language.into(),
        }
    }

    pub fn about_the_word(mut self, word: impl Into<String>) -> Self {
        let word = word.into();
        self.word = (!word.trim().is_empty()).then_some(word);
        self
    }

    /// What this request can be answered with.
    pub fn answerable(&self) -> Answerable {
        let sentence = self.sentence.trim();
        // Too long, or nothing there, and neither half can be answered.
        if sentence.is_empty() || sentence.chars().count() > TOO_LONG {
            return Answerable {
                the_word: false,
                the_structure: false,
            };
        }
        Answerable {
            the_word: self.word.is_some(),
            // Whitespace, because a "word" here means what a reader sees as
            // one. A Korean clause with no spaces in it is one token to a
            // splitter and a structure to a reader, so this errs toward
            // offering: the cost of asking about a single word is one wasted
            // line, and the cost of not asking about a sentence is the feature
            // silently not working.
            the_structure: sentence.split_whitespace().count() > 1 || sentence.chars().count() > 12,
        }
    }
}

/// What a model said about a sentence.
///
/// Every field may be empty. A model that had nothing useful to say about the
/// register of a sentence should say nothing, and an empty field is that: the
/// card leaves the line out rather than printing a heading over a blank.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeepAnalysis {
    /// The dictionary form of the word the pointer was on.
    pub lemma: String,
    /// Its part of speech, in the reader's language.
    pub part_of_speech: String,
    /// What it means **in this sentence**, which is the part a dictionary
    /// cannot give.
    pub sense: String,
    /// What the sentence is doing: its clauses and how they hang together.
    pub structure: String,
    /// An idiom, a register, or a false friend worth knowing. Often empty, and
    /// that is the correct answer most of the time.
    pub note: String,
    /// Which model said it, for the same reason a translation carries one.
    pub model: String,
    pub latency_ms: u64,
    /// Whether this came back from memory rather than from the model.
    ///
    /// The footer says what just happened, and 4.4초 on a press that cost
    /// microseconds is a lie about the only number on the card a reader can
    /// check against their own experience of having waited.
    pub cached: bool,
}

impl DeepAnalysis {
    /// What the card puts over the analysis, so it is never read as fact.
    ///
    /// [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md) requires
    /// this and the M5 gate checks for it. The reader this is for is a learner,
    /// who by definition cannot tell a wrong grammar explanation from a right
    /// one.
    pub const LABEL: &str = "AI 설명 · 사실 확인 필요";

    /// The lines a card would show, in order, with their headings.
    ///
    /// Empty fields are left out entirely rather than shown blank. Built here
    /// rather than in the overlay so the ordering and the headings are one
    /// decision, and so they can be tested without a window.
    pub fn lines(&self) -> Vec<(&'static str, &str)> {
        [
            ("원형", self.lemma.as_str()),
            ("품사", self.part_of_speech.as_str()),
            ("이 문장에서", self.sense.as_str()),
            ("구조", self.structure.as_str()),
            ("참고", self.note.as_str()),
        ]
        .into_iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .collect()
    }

    /// Whether this is worth putting on a card.
    ///
    /// A model that answered the schema with five empty strings satisfied the
    /// schema and said nothing. That is a malformed answer wearing valid
    /// clothes, and it must not become a card with a heading and no content.
    pub fn says_anything(&self) -> bool {
        !self.lines().is_empty()
    }

    /// Drop the explanations that came back in the wrong language.
    ///
    /// **Measured, and the reason this exists.** The prompt says every value
    /// must be written in the reader's language. Against `qwen2.5-coder:7b` on
    /// 2026-08-06 it was ignored twice in four sentences: a part of speech came
    /// back as `adverb`, and a clause structure came back as an English
    /// sentence. The reader this path is built for is somebody who could not
    /// read the source, so an explanation in the source's language explains
    /// nothing, confidently, under a heading in their own.
    ///
    /// Dropped rather than retried. A retry doubles the wait for an answer
    /// that was wrong in a way the model has just shown it does not notice,
    /// and a shorter honest analysis is better than a slower one.
    ///
    /// **[`Self::lemma`] is exempt, and that is the whole subtlety.** It is the
    /// dictionary form of the word the reader pointed at, so it belongs in the
    /// *source's* language. Filtering it by the target's script would delete
    /// the one field that is correct precisely when it looks wrong.
    pub fn only_what_is_written_in(mut self, target_language: &str) -> Self {
        let Some(written) = crate::language::written_as(target_language) else {
            // An unrecognised target cannot be checked against, and the safe
            // direction here is the same as everywhere else in `language`:
            // keep what there is rather than delete on a guess.
            return self;
        };
        let wrong = |text: &String| {
            !text.trim().is_empty()
                && crate::language::ScriptProfile::of(text).share_of(written.signature)
                    < crate::language::ALREADY_READABLE_SHARE
        };
        for field in [
            &mut self.part_of_speech,
            &mut self.sense,
            &mut self.structure,
            &mut self.note,
        ] {
            if wrong(field) {
                field.clear();
            }
        }
        self
    }
}

/// Explains a sentence that has already been translated.
///
/// Separate from [`crate::translate::Translator`] on purpose. The two have
/// different models, prompts, budgets and timeouts, and a single trait would
/// invite one implementation to share a prompt between them, which is how the
/// slow path's cost lands on the fast one.
pub trait Analyser {
    fn deep(&self, request: &DeepRequest) -> Result<DeepAnalysis, TranslateError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_sentence() -> DeepRequest {
        DeepRequest::new(
            "The pointer must rest before anything is read.",
            "포인터가 멈춘 뒤에야 무엇이든 읽습니다.",
            "Korean",
        )
    }

    #[test]
    fn a_sentence_with_a_word_can_be_answered_both_ways() {
        let answerable = a_sentence().about_the_word("rest").answerable();
        assert!(answerable.the_word);
        assert!(answerable.the_structure);
    }

    /// No clause structure for a single word, which ADR-0005 names.
    #[test]
    fn a_single_word_has_no_structure_to_describe() {
        let answerable = DeepRequest::new("rest", "휴식", "Korean")
            .about_the_word("rest")
            .answerable();
        assert!(answerable.the_word);
        assert!(!answerable.the_structure, "one word is not a structure");
        assert!(answerable.anything());
    }

    /// A paragraph is refused before a model is asked, not explained badly.
    #[test]
    fn a_paragraph_is_not_a_sentence_and_is_not_analysed() {
        let long = "word ".repeat(TOO_LONG);
        let answerable = DeepRequest::new(long, "…", "Korean")
            .about_the_word("word")
            .answerable();
        assert!(!answerable.anything());
    }

    /// Korean runs clauses together without spaces, and a splitter counting
    /// them would call a whole clause one word.
    #[test]
    fn a_korean_clause_with_no_spaces_still_has_a_structure() {
        let answerable = DeepRequest::new("포인터가멈춘뒤에야읽습니다", "…", "Korean").answerable();
        assert!(answerable.the_structure);
        assert!(!answerable.the_word, "no word was pointed at");
    }

    #[test]
    fn an_empty_word_is_no_word_rather_than_an_empty_one() {
        assert_eq!(a_sentence().about_the_word("   ").word, None);
    }

    #[test]
    fn empty_fields_are_left_out_rather_than_shown_blank() {
        let analysis = DeepAnalysis {
            lemma: "rest".to_owned(),
            part_of_speech: String::new(),
            sense: "  ".to_owned(),
            structure: "주어 + 조동사 + 동사".to_owned(),
            ..DeepAnalysis::default()
        };
        assert_eq!(
            analysis.lines(),
            vec![("원형", "rest"), ("구조", "주어 + 조동사 + 동사")],
        );
    }

    /// A schema satisfied with nothing in it is not an answer.
    #[test]
    fn an_analysis_of_five_empty_strings_says_nothing() {
        assert!(!DeepAnalysis::default().says_anything());
        assert!(DeepAnalysis {
            note: "격식체".to_owned(),
            ..DeepAnalysis::default()
        }
        .says_anything());
    }

    /// The label is copy a reader sees, so it obeys the same rules the card's
    /// copy tests enforce.
    #[test]
    fn the_label_reads_as_a_caveat_and_not_as_a_heading() {
        assert!(!DeepAnalysis::LABEL.contains("  "));
        assert!(!DeepAnalysis::LABEL.contains('\n'));
        assert!(!DeepAnalysis::LABEL.contains('\u{2014}'), "no em dash");
        assert!(
            DeepAnalysis::LABEL.contains("AI"),
            "the reader has to be able to tell a model said it",
        );
    }

    /// The two the live run actually caught.
    #[test]
    fn an_explanation_in_the_wrong_language_is_dropped() {
        let answered = DeepAnalysis {
            lemma: "vacuously".to_owned(),
            part_of_speech: "adverb".to_owned(),
            sense: "공허하게, 비어있게".to_owned(),
            structure: "The sentence compares placing a card to having none.".to_owned(),
            ..DeepAnalysis::default()
        }
        .only_what_is_written_in("Korean");

        assert_eq!(answered.part_of_speech, "");
        assert_eq!(answered.structure, "");
        assert_eq!(answered.sense, "공허하게, 비어있게");
    }

    /// The field that is right when it looks wrong.
    #[test]
    fn the_dictionary_form_stays_in_the_language_it_belongs_to() {
        let kept = DeepAnalysis {
            lemma: "rest".to_owned(),
            part_of_speech: "동사".to_owned(),
            ..DeepAnalysis::default()
        }
        .only_what_is_written_in("Korean");

        assert_eq!(kept.lemma, "rest", "a lemma is not an explanation");
        assert_eq!(kept.part_of_speech, "동사");
    }

    /// Korean carrying an identifier is Korean.
    #[test]
    fn an_explanation_naming_code_is_not_the_wrong_language() {
        let kept = DeepAnalysis {
            structure: "RowQuery가 주어이고 뒤의 절이 목적어입니다".to_owned(),
            ..DeepAnalysis::default()
        }
        .only_what_is_written_in("Korean");
        assert!(!kept.structure.is_empty(), "{kept:?}");
    }

    /// A target nothing is known about deletes nothing.
    #[test]
    fn an_unrecognised_target_language_keeps_everything() {
        let kept = DeepAnalysis {
            part_of_speech: "adverb".to_owned(),
            ..DeepAnalysis::default()
        }
        .only_what_is_written_in("Klingon");
        assert_eq!(kept.part_of_speech, "adverb");
    }

    /// The slow path is allowed to be slow, and the fast one is not.
    #[test]
    fn the_deep_budget_is_far_longer_than_a_hover_may_wait() {
        assert!(DEEP_TIMEOUT > crate::translate::QUICK_TIMEOUT * 2);
    }
}
