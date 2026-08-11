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

/// How much of the source an explanation may repeat before it is a
/// restatement of it.
///
/// Four fifths. An explanation of a sentence quotes a word or two of it, and
/// often should: naming the subject is how you describe a clause. Keeping four
/// fifths of the words is not quoting, it is handing the sentence back.
///
/// Measured against the two answers that prompted it, which kept **all** of
/// their source's words, and against an explanation of the same sentence that
/// kept a quarter.
const RESTATEMENT_SHARE: f64 = 0.8;

/// What to explain, and what the reader can already see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepRequest {
    /// The source, exactly as the card shows it.
    pub sentence: String,
    /// The word the pointer was on, when there was one.
    pub word: Option<String>,
    /// The translation already on the screen, when there is one.
    ///
    /// Sent so the model explains rather than re-translates. It is also the
    /// thing that makes the two answers agree: an analysis derived from a
    /// second, independent translation would explain a sentence the reader is
    /// not looking at.
    ///
    /// **`None` is a real case and not a missing value.** Text already in the
    /// reader's language is never translated, correctly: a Korean reader
    /// hovering Korean does not want it re-worded. Until 2026-08-06 that also
    /// meant Lens did nothing for it at all, which left the whole of somebody's
    /// own language outside the product. A word you can read and do not know
    /// is the commonest thing on a technical screen.
    pub translation: Option<String>,
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
    /// A request about a sentence the reader has a translation of.
    pub fn new(
        sentence: impl Into<String>,
        translation: impl Into<String>,
        target_language: impl Into<String>,
    ) -> Self {
        Self {
            sentence: sentence.into(),
            word: None,
            translation: Some(translation.into()).filter(|text: &String| !text.trim().is_empty()),
            target_language: target_language.into(),
        }
    }

    /// A request about text the reader can already read.
    ///
    /// No translation, because there is none and there should not be. What is
    /// asked for is the same thing minus the grounding: what the word means
    /// here, what the sentence is doing.
    pub fn about_what_they_can_read(
        sentence: impl Into<String>,
        target_language: impl Into<String>,
    ) -> Self {
        Self {
            sentence: sentence.into(),
            word: None,
            translation: None,
            target_language: target_language.into(),
        }
    }

    /// Name the word the reader is looking at, when there is one.
    ///
    /// A word that is the whole **sentence** is dropped, for the reason
    /// [`crate::translate::QuickRequest::with_focus`] gives: a recogniser that
    /// merged a line into one word has not found a word, and this prompt asks
    /// that word for a dictionary form and a part of speech, which a sentence
    /// does not have.
    ///
    /// **A one-word source is not that case**, and the difference matters more
    /// here than on the quick path. There the focus is only a hint and the
    /// model has the sentence either way; here [`Answerable::the_word`] decides
    /// whether the lemma, the part of speech and the sense are asked for at
    /// all, and for a single word those are the *only* useful half.
    ///
    /// Dropping it there turned the analysis of one word into an analysis of
    /// nothing, which is what
    /// `a_single_word_has_no_structure_to_describe` caught.
    pub fn about_the_word(mut self, word: impl Into<String>) -> Self {
        let word = word.into();
        let same = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        // A sentence has whitespace in it; a word does not. That is the whole
        // distinction, and it is why this is not simply string equality.
        let the_source_is_a_sentence = self.sentence.split_whitespace().count() > 1;
        let restates_the_sentence = the_source_is_a_sentence && same(&word) == same(&self.sentence);
        self.word = (!word.trim().is_empty() && !restates_the_sentence).then_some(word);
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
    /// The part of the translation that carries the focused word.
    ///
    /// `None` far more often than not, and that is the design rather than a
    /// shortfall. See [`DeepAnalysis::agreed_counterpart`]: this is set only where
    /// two differently framed questions returned the same span, which on the
    /// measured sample was five times in eight and was right five times of
    /// five (`docs/benchmarks/2026-08-11-one-word-and-its-counterpart.md`).
    pub counterpart: Option<String>,
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
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        [
            ("원형", self.lemma.clone()),
            ("품사", self.part_of_speech.clone()),
            ("이 문장에서", self.sense.clone()),
            // Last of the explanations and before the structure, because it is
            // the only row that points at something already on the card rather
            // than telling the reader a new fact.
            ("번역에서", self.counterpart.clone().unwrap_or_default()),
            ("구조", self.structure.clone()),
            ("참고", self.note.clone()),
        ]
        .into_iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .collect()
    }

    /// The largest share of a translation a counterpart may be.
    ///
    /// The trivially correct answer to "which part of this means that" is "all of
    /// it", and a span that is most of the sentence points at nothing. Measured
    /// counterparts were 6 to 19 per cent of their translation, so this is far
    /// above every real answer and below the degenerate one.
    pub const COUNTERPART_SHARE: f64 = 0.6;

    /// Two answers to the same question, kept only if they are the same answer.
    ///
    /// # Why exact equality and not containment
    ///
    /// The obvious relaxation is to accept one span containing the other, since
    /// `비어` and `비어있었습니다` disagree only about extent and both are right.
    /// It was measured and it is wrong: the one error in the sample was `틀린`
    /// against `틀린 문장`, which is the same relationship. The relaxation buys two
    /// correct highlights and the single wrong one, and a wrong highlight is the
    /// thing this must not produce
    /// (`docs/benchmarks/2026-08-11-one-word-and-its-counterpart.md`).
    ///
    /// # Why two questions at all
    ///
    /// One question answered eight of eight by every mechanical check, and one of
    /// those eight was false: `sentence` came back as `틀린`, which means *wrong*.
    /// Membership in the translation is not correctness, and asking from the other
    /// end is the cheapest thing that is.
    pub fn agreed_counterpart(translation: &str, first: &str, second: &str) -> Option<String> {
        let first = first.trim();
        if first.is_empty() || first != second.trim() {
            return None;
        }
        if !translation.contains(first) {
            return None;
        }
        let (span, whole) = (first.chars().count(), translation.chars().count());
        if whole == 0 || span as f64 / whole as f64 > Self::COUNTERPART_SHARE {
            return None;
        }
        Some(first.to_owned())
    }

    /// The same analysis, carrying a counterpart it did not come with.
    ///
    /// A separate step because it is a separate question: the analysis comes
    /// from one call against a schema, and this comes from two more against
    /// another. Keeping them apart is what lets either fail without the other.
    #[must_use]
    pub fn with_counterpart(mut self, counterpart: Option<String>) -> Self {
        self.counterpart = counterpart;
        self
    }

    /// Whether this is worth putting on a card.
    ///
    /// A model that answered the schema with five empty strings satisfied the
    /// schema and said nothing. That is a malformed answer wearing valid
    /// clothes, and it must not become a card with a heading and no content.
    pub fn says_anything(&self) -> bool {
        !self.lines().is_empty()
    }

    /// Drop the explanations that are the sentence again.
    ///
    /// **Measured 2026-08-06, on the path that has no translation to ground
    /// on.** Asked what `이 값은 캐시에 저장되며 TTL이 지나면 만료됩니다.` is
    /// doing, the model answered with that sentence, verbatim, plus an English
    /// gloss of it. Asked about `접근성 트리가 차가우면 첫 읽기가 느립니다.`, the
    /// same sentence with a parenthesis inserted.
    ///
    /// A reader who can already read the sentence has been handed it back
    /// under a heading that promised to explain it. That is worse than an
    /// empty field: an empty field says nothing, and this says something and
    /// is wrong about what it is.
    ///
    /// # Why not [`crate::translate::reads_as_the_source`]
    ///
    /// It exists for the neighbouring failure and will not fire here. It
    /// refuses to judge a source under eight words, because on the translation
    /// path a short answer that resembles a short source is usually a correct
    /// translation carrying names through. Both sentences above are six words.
    ///
    /// The question is also different. There it is *did you fail to
    /// translate*; here it is *did you restate*, and a short sentence restated
    /// is still restated.
    ///
    /// [`Self::lemma`] is exempt for the same reason as in
    /// [`Self::only_what_is_written_in`]: it is a word taken **from** the
    /// source and is supposed to be.
    pub fn without_restatements_of(mut self, source: &str) -> Self {
        let source_words = crate::translate::comparable_words(source);
        // Below this the measure is too coarse to mean anything: a two-word
        // source is most of itself after any paraphrase.
        if source_words.len() < 3 {
            return self;
        }
        let restates = |text: &String| {
            let words = crate::translate::comparable_words(text);
            let kept = source_words
                .iter()
                .filter(|word| words.contains(word))
                .count();
            kept as f64 / source_words.len() as f64 >= RESTATEMENT_SHARE
        };
        for field in [
            &mut self.part_of_speech,
            &mut self.sense,
            &mut self.structure,
            &mut self.note,
        ] {
            if !field.trim().is_empty() && restates(field) {
                field.clear();
            }
        }
        self
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
            analysis
                .lines()
                .iter()
                .map(|(label, value)| (*label, value.as_str()))
                .collect::<Vec<_>>(),
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

#[cfg(test)]
mod own_language_tests {
    use super::*;

    /// Text in the reader's own language is a request with no grounding, and
    /// that is a state rather than a gap.
    #[test]
    fn a_request_about_readable_text_carries_no_translation() {
        let request = DeepRequest::about_what_they_can_read(
            "RowQuery는 멱등성을 보장하지 않습니다.",
            "Korean",
        )
        .about_the_word("멱등성");

        assert_eq!(request.translation, None);
        let answerable = request.answerable();
        assert!(answerable.the_word);
        assert!(answerable.the_structure);
    }

    /// An empty translation is no translation, so a caller that passes one by
    /// accident gets the ungrounded prompt rather than an instruction to
    /// compare against nothing.
    #[test]
    fn an_empty_translation_is_the_same_as_none() {
        assert_eq!(
            DeepRequest::new("a sentence", "", "Korean").translation,
            None
        );
        assert_eq!(
            DeepRequest::new("a sentence", "   ", "Korean").translation,
            None,
        );
        assert_eq!(
            DeepRequest::new("a sentence", "번역", "Korean")
                .translation
                .as_deref(),
            Some("번역"),
        );
    }

    /// The length rule does not care which kind it is: a paragraph is still
    /// not a sentence to explain.
    #[test]
    fn a_paragraph_in_the_readers_own_language_is_still_refused() {
        let long = "단어 ".repeat(TOO_LONG);
        assert!(!DeepRequest::about_what_they_can_read(long, "Korean")
            .answerable()
            .anything());
    }
}

#[cfg(test)]
mod restatement_tests {
    use super::*;

    /// The two answers that prompted this, from `qwen2.5:7b` on 2026-08-06.
    #[test]
    fn an_explanation_that_is_the_sentence_again_is_dropped() {
        let source = "이 값은 캐시에 저장되며 TTL이 지나면 만료됩니다.";
        let dropped = DeepAnalysis {
            structure: "이 값은 캐시에 저장되며, TTL이 지나면 만료됩니다. (This value is stored in the cache.)".to_owned(),
            sense: "시간 제한(Time To Live)".to_owned(),
            ..DeepAnalysis::default()
        }
        .without_restatements_of(source);

        assert_eq!(dropped.structure, "", "{dropped:?}");
        assert_eq!(dropped.sense, "시간 제한(Time To Live)");
    }

    /// The same sentence with a parenthesis inserted is still the sentence.
    #[test]
    fn a_paraphrase_that_keeps_every_word_is_a_restatement() {
        let source = "접근성 트리가 차가우면 첫 읽기가 느립니다.";
        let dropped = DeepAnalysis {
            structure: "접근성 트리가 차가우면 (이 조건에서 접근성이 낮다면) 첫 읽기가 느립니다."
                .to_owned(),
            ..DeepAnalysis::default()
        }
        .without_restatements_of(source);
        assert_eq!(dropped.structure, "");
    }

    /// A real description quotes a word or two and is kept.
    #[test]
    fn an_explanation_that_quotes_the_subject_survives() {
        let source = "RowQuery는 멱등성을 보장하지 않습니다.";
        let kept = DeepAnalysis {
            structure: "RowQuery가 동일한 요청을 반복할 때 일관된 결과를 보장하지 않는다는 의미."
                .to_owned(),
            sense: "동일한 요청을 여러 번 수행해도 같은 결과를 얻는 성질".to_owned(),
            ..DeepAnalysis::default()
        }
        .without_restatements_of(source);
        assert!(!kept.structure.is_empty(), "{kept:?}");
        assert!(!kept.sense.is_empty(), "{kept:?}");
    }

    /// The dictionary form is a word taken from the source and is supposed to
    /// be, which is the same exemption the language filter makes.
    #[test]
    fn the_dictionary_form_is_not_a_restatement() {
        let kept = DeepAnalysis {
            lemma: "멱등성".to_owned(),
            ..DeepAnalysis::default()
        }
        .without_restatements_of("멱등성 멱등성 멱등성");
        assert_eq!(kept.lemma, "멱등성");
    }

    /// Too short to judge. A two-word source is most of itself after any
    /// paraphrase, so the measure would fire on a correct answer.
    #[test]
    fn a_source_of_two_words_is_not_judged() {
        let kept = DeepAnalysis {
            sense: "캐시 만료".to_owned(),
            ..DeepAnalysis::default()
        }
        .without_restatements_of("캐시 만료");
        assert_eq!(kept.sense, "캐시 만료");
    }

    /// `reads_as_the_source` will not do this job, and the reason is a number.
    ///
    /// It refuses to judge a source under eight words, because on the
    /// translation path a short answer resembling a short source is usually a
    /// correct translation carrying names through. Both sentences above are
    /// six words, so it sees neither.
    #[test]
    fn the_echo_check_does_not_cover_this_and_that_is_why_this_exists() {
        let source = "접근성 트리가 차가우면 첫 읽기가 느립니다.";
        let restated = "접근성 트리가 차가우면 (이 조건에서) 첫 읽기가 느립니다.";
        assert!(!crate::translate::reads_as_the_source(source, restated));

        let dropped = DeepAnalysis {
            structure: restated.to_owned(),
            ..DeepAnalysis::default()
        }
        .without_restatements_of(source);
        assert_eq!(dropped.structure, "");
    }
}

#[cfg(test)]
mod focus_tests {
    use super::*;

    /// The same rule on the deep path, where getting it wrong is worse: the
    /// prompt asks for a dictionary form and a part of speech, and a sentence
    /// has neither.
    /// The regression this rule caused when it was first written, and the
    /// reason it is not plain string equality.
    ///
    /// A one-word source's word **is** the word. Dropping it turns off the
    /// lemma, the part of speech and the sense, which for a single word are
    /// the only half worth asking for.
    #[test]
    fn a_one_word_source_keeps_its_word() {
        let request =
            DeepRequest::new("vacuously", "공허하게", "Korean").about_the_word("vacuously");
        assert_eq!(request.word.as_deref(), Some("vacuously"));
        let answerable = request.answerable();
        assert!(answerable.the_word);
        assert!(!answerable.the_structure);
    }

    #[test]
    fn a_word_that_is_the_whole_sentence_is_dropped() {
        let sentence = "RowQuery는 멱등성을 보장하지 않습니다.";
        let request =
            DeepRequest::about_what_they_can_read(sentence, "Korean").about_the_word(sentence);
        assert_eq!(request.word, None);
        // And the request is still worth making, for the structure.
        assert!(request.answerable().the_structure);
        assert!(!request.answerable().the_word);
    }

    #[test]
    fn a_real_word_still_reaches_the_prompt() {
        let request = DeepRequest::about_what_they_can_read(
            "RowQuery는 멱등성을 보장하지 않습니다.",
            "Korean",
        )
        .about_the_word("멱등성");
        assert_eq!(request.word.as_deref(), Some("멱등성"));
        assert!(request.answerable().the_word);
    }
}

/// The rule that decides whether a counterpart is shown at all.
///
/// Every case here is a row from
/// `docs/benchmarks/2026-08-11-one-word-and-its-counterpart.md`, which is why
/// the spans are the ones they are.
#[cfg(test)]
mod counterpart_tests {
    use super::DeepAnalysis;

    const TRANSLATION: &str = "제공자는 각 렌더링된 줄당 하나의 직사각형을 반환합니다.";

    #[test]
    fn two_answers_that_match_are_kept() {
        assert_eq!(
            DeepAnalysis::agreed_counterpart(TRANSLATION, "직사각형", "직사각형"),
            Some("직사각형".to_owned()),
        );
    }

    #[test]
    fn whitespace_is_not_a_disagreement() {
        assert_eq!(
            DeepAnalysis::agreed_counterpart(TRANSLATION, " 직사각형 ", "직사각형"),
            Some("직사각형".to_owned()),
        );
    }

    /// **The case the whole rule exists for.**
    ///
    /// `sentence` came back as `틀린`, which means *wrong*, from a question
    /// that answered every mechanical check. The second question said
    /// `틀린 문장`. One contains the other, and accepting containment would
    /// have kept the error.
    #[test]
    fn one_span_inside_the_other_is_still_a_disagreement() {
        let translation = "틀린 문장에 카드를 넣는 것이 카드를 안 넣는 것보다 더 나쁘다.";
        assert_eq!(
            DeepAnalysis::agreed_counterpart(translation, "틀린", "틀린 문장"),
            None,
        );
    }

    /// And the two right answers the same rule throws away, on purpose.
    #[test]
    fn a_right_answer_of_the_wrong_extent_is_thrown_away_too() {
        let translation = "출력 파일은 비어있었습니다.";
        assert_eq!(
            DeepAnalysis::agreed_counterpart(translation, "비어", "비어있었습니다"),
            None,
        );
    }

    /// A span the model made up colours nothing, whatever the other answer said.
    #[test]
    fn a_span_that_is_not_in_the_translation_is_refused() {
        assert_eq!(
            DeepAnalysis::agreed_counterpart(TRANSLATION, "사각형들", "사각형들"),
            None,
        );
    }

    #[test]
    fn an_empty_span_is_not_an_answer() {
        assert_eq!(DeepAnalysis::agreed_counterpart(TRANSLATION, "", ""), None);
        assert_eq!(DeepAnalysis::agreed_counterpart("", "가", "가"), None);
    }

    /// The trivially correct answer to "which part means that" is "all of it".
    #[test]
    fn most_of_the_translation_is_not_a_counterpart() {
        assert_eq!(
            DeepAnalysis::agreed_counterpart(TRANSLATION, TRANSLATION, TRANSLATION),
            None,
        );
    }

    /// Measured counterparts were 6 to 19 per cent of their translation, so
    /// every real answer is far under the ceiling and this pins the distance.
    #[test]
    fn every_measured_counterpart_is_well_under_the_ceiling() {
        for (translation, span) in [
            ("출력 파일은 비어있었습니다.", "비어"),
            ("여기 읽은 것은 이 기계에서 나가지 않습니다.", "기계"),
            (
                "제공자는 각 렌더링된 줄당 하나의 직사각형을 반환합니다.",
                "직사각형",
            ),
            ("포인터는 유일한 트리거입니다.", "트리거"),
        ] {
            let share = span.chars().count() as f64 / translation.chars().count() as f64;
            assert!(
                share < DeepAnalysis::COUNTERPART_SHARE / 2.0,
                "{span:?} is {share:.2} of its translation",
            );
            assert!(DeepAnalysis::agreed_counterpart(translation, span, span).is_some());
        }
    }

    /// A counterpart reaches the card as its own row, and never as a blank one.
    #[test]
    fn the_row_appears_only_when_there_is_one() {
        let with = DeepAnalysis {
            lemma: "rectangle".to_owned(),
            counterpart: Some("직사각형".to_owned()),
            ..DeepAnalysis::default()
        };
        assert!(with.lines().iter().any(|(_, value)| value == "직사각형"));

        let without = DeepAnalysis {
            lemma: "rectangle".to_owned(),
            ..DeepAnalysis::default()
        };
        assert_eq!(without.lines().len(), 1);
    }
}
