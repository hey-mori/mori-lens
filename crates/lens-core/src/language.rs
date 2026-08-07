//! Whether the text under the pointer needs translating at all.
//!
//! Without this, a Korean reader hovering over Korean pays a second and a half
//! for a card that re-words their own language. Measured against
//! `qwen2.5:3b`: a Korean sentence in comes back as a different Korean
//! sentence, confidently. On a screen that is mostly the reader's own
//! language, that is most hovers.
//!
//! # What this is and is not
//!
//! **Script detection, not language identification.** It answers "is this
//! written in the same writing system as the target" and nothing more. That
//! settles the case worth settling first, because Hangul, Kana, Han, Cyrillic,
//! Arabic, Hebrew and Devanagari separate at a glance and cost nothing to
//! check.
//!
//! It cannot tell English from French, or Spanish from Portuguese: they share
//! a script. A reader whose target language is Latin-script therefore still
//! pays for a translation of text already in their language, and
//! [`Decision::Undecidable`] says so rather than guessing. A statistical
//! classifier is the answer there and is not here
//! (`docs/research/hover-text-acquisition.md` §5).

/// A writing system, at the granularity that matters for this decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,
    Hangul,
    Han,
    Kana,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Devanagari,
    Thai,
    Other,
}

/// How a language is written: the script that identifies it, plus any it
/// habitually borrows.
///
/// The split matters for Japanese and Chinese, which share Han. Counting Han
/// as evidence of Japanese made Chinese look like Japanese, so Lens refused to
/// translate it for a Japanese reader, which is worse than the wasted call it
/// was trying to avoid. Kana is the tell: Japanese prose is full of it and
/// Chinese has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Written {
    /// Must be present for the text to be in this language.
    pub signature: &'static [Script],
    /// Counts towards the share, but proves nothing on its own.
    pub companions: &'static [Script],
}

impl Written {
    const fn only(signature: &'static [Script]) -> Self {
        Self {
            signature,
            companions: &[],
        }
    }
}

/// The script of one character, or `None` for anything that is not a letter.
///
/// Digits, punctuation, symbols and whitespace return `None` rather than
/// counting as Latin. A sentence of mostly numbers with two Korean words in it
/// is Korean, and counting the digits would make it Latin.
pub fn script_of(ch: char) -> Option<Script> {
    if !ch.is_alphabetic() {
        return None;
    }
    Some(match ch as u32 {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0400..=0x052F => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F => Script::Arabic,
        0x0900..=0x097F => Script::Devanagari,
        0x0E00..=0x0E7F => Script::Thai,
        0x3040..=0x30FF | 0x31F0..=0x31FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => Script::Han,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => Script::Hangul,
        _ if ch.is_ascii() => Script::Latin,
        _ => Script::Other,
    })
}

/// How much of a text is written in each script.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptProfile {
    counts: Vec<(Script, usize)>,
    letters: usize,
}

impl ScriptProfile {
    pub fn of(text: &str) -> Self {
        let mut profile = Self::default();
        for script in text.chars().filter_map(script_of) {
            profile.letters += 1;
            match profile.counts.iter_mut().find(|(s, _)| *s == script) {
                Some((_, count)) => *count += 1,
                None => profile.counts.push((script, 1)),
            }
        }
        profile
    }

    pub fn letters(&self) -> usize {
        self.letters
    }

    /// How many letters are written in `script`.
    pub fn count_of(&self, script: Script) -> usize {
        self.counts
            .iter()
            .find(|(s, _)| *s == script)
            .map_or(0, |(_, count)| *count)
    }

    /// The share of letters written in any of `scripts`.
    pub fn share_of(&self, scripts: &[Script]) -> f64 {
        if self.letters == 0 {
            return 0.0;
        }
        let matching: usize = scripts.iter().map(|s| self.count_of(*s)).sum();
        matching as f64 / self.letters as f64
    }
}

/// What to do with a piece of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Send it to the engine.
    Translate,
    /// It is already in the reader's language. Show nothing and spend nothing.
    AlreadyInTarget,
    /// Same script as the target, so it cannot be told apart without a
    /// classifier this crate does not have. Translated anyway.
    ///
    /// Distinct from [`Self::Translate`] so a caller can report the honest
    /// reason, and so the day a classifier arrives, the place it plugs in is
    /// already named.
    Undecidable,
}

impl Decision {
    pub const fn should_translate(self) -> bool {
        matches!(self, Self::Translate | Self::Undecidable)
    }
}

/// How a target language is written.
///
/// Matched loosely on the name, because the target arrives as a human-readable
/// string the user typed. An unrecognised language yields `None`, and
/// everything is then translated, which is the safe direction: paying for a
/// translation nobody needed costs a second, and skipping one somebody wanted
/// is a product that does not work.
pub fn written_as(name: &str) -> Option<Written> {
    let name = name.trim().to_ascii_lowercase();
    Some(match name.as_str() {
        "korean" | "ko" | "한국어" => Written::only(&[Script::Hangul]),
        // Kana is the tell. Han alone is Chinese as often as Japanese.
        "japanese" | "ja" | "日本語" => Written {
            signature: &[Script::Kana],
            companions: &[Script::Han],
        },
        "chinese" | "zh" | "中文" => Written::only(&[Script::Han]),
        "russian" | "ru" | "ukrainian" | "uk" => Written::only(&[Script::Cyrillic]),
        "arabic" | "ar" => Written::only(&[Script::Arabic]),
        "hebrew" | "he" => Written::only(&[Script::Hebrew]),
        "greek" | "el" => Written::only(&[Script::Greek]),
        "hindi" | "hi" => Written::only(&[Script::Devanagari]),
        "thai" | "th" => Written::only(&[Script::Thai]),
        "english" | "en" | "french" | "fr" | "german" | "de" | "spanish" | "es" | "portuguese"
        | "pt" | "italian" | "it" | "dutch" | "nl" | "polish" | "pl" | "vietnamese" | "vi"
        | "turkish" | "tr" | "indonesian" | "id" => Written::only(&[Script::Latin]),
        _ => return None,
    })
}

/// How much of a text must be in the target's script before Lens decides the
/// reader can already read it.
///
/// Well under a majority, and measured rather than picked. Korean technical
/// prose is mostly Latin by letter count: the sentence used in the tests below
/// is 27% Hangul and is Korean a Korean reader does not need translated.
/// English with one Korean word in it sits near 4%, so the two separate with
/// room to spare.
pub const ALREADY_READABLE_SHARE: f64 = 0.20;

/// Text shorter than this is not judged on script at all.
///
/// A single word gives too little to go on, and the cost of being wrong is
/// asymmetric: refusing to translate a word somebody pointed at is a broken
/// product, and translating one they did not need is a second.
pub const MIN_LETTERS_TO_JUDGE: usize = 8;

/// How many letters of an unexpected script it takes to call it a third
/// language.
///
/// Four. A Korean sentence can carry one or two Han characters as a gloss and
/// that is ordinary Korean; four is not, and the case this exists for produced
/// dozens.
///
/// Deliberately not a share. The failure is a **paragraph inserted into an
/// otherwise correct answer**, so the answer stays mostly in the right script,
/// and a share threshold set low enough to catch it would fire on a legitimate
/// gloss.
pub const ENOUGH_TO_BE_A_THIRD_LANGUAGE: usize = 4;

/// How much shorter than its source a salvaged answer may be, as a divisor.
///
/// A quarter. Korean renders English at roughly half to three quarters of the
/// letter count, so a quarter is well under anything a real translation
/// reaches, and the case this exists for was **six letters against
/// seventy-four**.
///
/// Compared on letters rather than characters, so the spaces and punctuation
/// that differ between two writing systems are not part of the comparison.
const SHORTEST_A_TRANSLATION_GETS: usize = 4;

/// Whether the answer is written partly in a language that is neither the
/// source's nor the reader's.
///
/// **Measured, 2026-08-06.** Asked to translate an English sentence into
/// Korean, `qwen2.5:7b` answered the Korean translation with a paragraph of
/// Chinese commentary about the translation task welded into the middle of it.
/// It is not an echo, so [`crate::translate::reads_as_the_source`] does not see
/// it, and it is mostly in the right script, so nothing about the target
/// language sees it either. It goes on a card.
///
/// The rule is narrow on purpose: a script present in the **answer**, absent
/// from the **source**, and not one the target is written in. A Korean answer
/// carrying the English identifiers from an English source is untouched,
/// because Latin was in the source. A Japanese answer full of Han is untouched,
/// because Han is one of Japanese's own scripts.
///
/// An unrecognised target language yields `false`. Nothing is known about what
/// it is written in, and deleting an answer on that basis would be a guess.
pub fn wrote_in_a_third_language(source: &str, answer: &str, target_language: &str) -> bool {
    let in_the_answer = ScriptProfile::of(answer);
    intruding_scripts(source, target_language)
        .into_iter()
        .any(|script| in_the_answer.count_of(script) >= ENOUGH_TO_BE_A_THIRD_LANGUAGE)
}

/// Which scripts would be a third language for this target and this source.
///
/// Shared by [`wrote_in_a_third_language`] and
/// [`without_the_third_language`], because a rule that decided one thing was
/// wrong and a rule that cut it out would be two chances to disagree, and the
/// disagreement would show as an answer cut for a reason it did not have.
fn intruding_scripts(source: &str, target_language: &str) -> Vec<Script> {
    let Some(written) = written_as(target_language) else {
        return Vec::new();
    };
    let in_the_source = ScriptProfile::of(source);
    [
        Script::Hangul,
        Script::Han,
        Script::Kana,
        Script::Cyrillic,
        Script::Greek,
        Script::Arabic,
        Script::Hebrew,
        Script::Devanagari,
        Script::Thai,
        Script::Latin,
    ]
    .into_iter()
    .filter(|script| !written.signature.contains(script) && !written.companions.contains(script))
    .filter(|script| in_the_source.count_of(*script) == 0)
    .collect()
}

/// Punctuation that belongs to the intruding text rather than to the answer.
///
/// CJK symbols and fullwidth forms. A model writing a Chinese paragraph
/// punctuates it with these, and leaving them behind puts a stray `，` in the
/// middle of a Korean sentence that never had one. Korean prose uses the ASCII
/// marks.
fn is_foreign_punctuation(character: char) -> bool {
    matches!(character, '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}')
}

/// The translation, with a third language's text cut out of it.
///
/// `None` when there is nothing left worth showing.
///
/// # Why this rather than a retry
///
/// A retry was the first answer and it is wrong here. Requests go out at
/// `temperature: 0`, so asking the same question again returns the same reply:
/// the echo path can retry because it changes the budget and an empty answer is
/// usually a budget, and nothing about a budget produces a paragraph of
/// Chinese. The retry would cost two and a half seconds to arrive at the same
/// text.
///
/// # Why the longest segment rather than a strip
///
/// The intrusion **splits** the answer. The case this was written for is
///
/// ```text
/// [Korean fragment][Chinese paragraph about the translation task][Korean translation]
/// ```
///
/// so deleting the Chinese leaves a dangling fragment glued to a good
/// sentence. Taking the segment with the most of the reader's own script keeps
/// the sentence and drops the fragment, and in that case the segment kept is
/// the correct translation.
///
/// # When it refuses
///
/// Two ways, and the second was found by running it.
///
/// The best segment holds less than half of the answer's target-script
/// letters: the model interleaved rather than inserted, and no segment of that
/// is a sentence.
///
/// Or the best segment is **too short to be a translation of the source**.
/// Measured 2026-08-06: asked for Korean, the model wrote three words of it and
/// then translated the whole sentence into Chinese instead.
///
/// ```text
/// 빌드가 실패한原因是cargo fmt --check在运行测试之前发现在src/main.rs中存在差异。
/// ```
///
/// There is no Korean translation in that, only the three words before the
/// switch. The first rule passes it, because those three words are all the
/// Korean there is, and a six-letter fragment would have gone on a card as the
/// translation of a seventeen-word sentence.
pub fn without_the_third_language(
    source: &str,
    answer: &str,
    target_language: &str,
) -> Option<String> {
    let intruding = intruding_scripts(source, target_language);
    if intruding.is_empty() {
        return None;
    }
    let written = written_as(target_language)?;

    let is_intruding = |character: char| {
        script_of(character).is_some_and(|script| intruding.contains(&script))
            || is_foreign_punctuation(character)
    };

    let target_letters = |text: &str| {
        text.chars()
            .filter_map(script_of)
            .filter(|script| written.signature.contains(script))
            .count()
    };

    let best = answer
        .split(is_intruding)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .max_by_key(|segment| target_letters(segment))?;

    let kept = target_letters(best);
    if kept == 0 || kept * 2 < target_letters(answer) {
        return None;
    }
    if kept * SHORTEST_A_TRANSLATION_GETS < ScriptProfile::of(source).letters() {
        return None;
    }
    Some(best.to_owned())
}

/// Whether a reader of `target_language` can already read a good part of this.
///
/// Weaker than [`decide`], and asking a different question. `decide` answers
/// "should this be translated at all", with a high bar, because refusing to
/// translate something somebody pointed at is a broken product. This answers
/// "is a model that returned this unchanged **right**", which needs only that
/// the target's own writing system is substantially present.
///
/// It exists for one card. A line of Korean prose carrying English identifiers
/// passed `decide`, was sent, came back nearly unchanged because there was
/// little to change, and the reader was told the model had failed and to try
/// another one. The model had not failed.
pub fn partly_readable(text: &str, target_language: &str) -> bool {
    /// How much of the target's own script has to be there.
    ///
    /// Well under [`ALREADY_READABLE_SHARE`], which is the bar for not
    /// translating at all. This is not a decision to stay silent, it is a
    /// decision about which of two sentences to put on a card that is
    /// appearing either way, so it can afford to be generous.
    ///
    /// **Calibrated against two lines, and it is worth saying which**, because
    /// a threshold set by examples is only as good as its examples:
    ///
    /// - the line that produced the wrong card, a Korean sentence carrying
    ///   thirteen English identifiers, measures **0.158**
    /// - an English sentence with one Korean word dropped in it measures
    ///   **0.05**, and must not qualify
    ///
    /// A fifth was tried first and rejected the first of those, which is the
    /// case this exists for.
    const ENOUGH: f64 = 0.12;

    let Some(written) = written_as(target_language) else {
        return false;
    };
    // Latin is nobody's signature: English, French and Spanish share it, so a
    // high Latin share says nothing about whether a Korean reader can read it.
    if written.signature.contains(&Script::Latin) {
        return false;
    }
    let profile = ScriptProfile::of(text);
    profile.letters() >= MIN_LETTERS_TO_JUDGE && profile.share_of(written.signature) >= ENOUGH
}

/// Whether this text needs translating into `target_language`.
pub fn decide(text: &str, target_language: &str) -> Decision {
    let Some(written) = written_as(target_language) else {
        return Decision::Undecidable;
    };

    let profile = ScriptProfile::of(text);
    if profile.letters() < MIN_LETTERS_TO_JUDGE {
        return Decision::Undecidable;
    }

    // The signature must actually appear. Without this Chinese scores 100% on
    // Han, passes as Japanese, and Lens refuses to translate it.
    if profile.share_of(written.signature) == 0.0 {
        return Decision::Translate;
    }

    let share = profile.share_of(written.signature) + profile.share_of(written.companions);
    if share < ALREADY_READABLE_SHARE {
        return Decision::Translate;
    }

    // The target's writing system dominates. For a script only one language
    // family uses, that settles it. For Latin it settles nothing: English,
    // French and Spanish share it and only a classifier tells them apart.
    if written.signature.contains(&Script::Latin) {
        Decision::Undecidable
    } else {
        Decision::AlreadyInTarget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case this file exists for.
    #[test]
    fn korean_text_is_not_translated_for_a_korean_reader() {
        // Measured before this existed: a Korean sentence went to the model
        // and came back as a different Korean sentence, after a second and a
        // half. On a mostly-Korean screen that is most hovers.
        assert_eq!(
            decide(
                "출력 파일이 비어 있었기 때문에 아무것도 생성하지 않았습니다.",
                "Korean"
            ),
            Decision::AlreadyInTarget,
        );
    }

    #[test]
    fn english_text_is_translated_for_a_korean_reader() {
        assert_eq!(
            decide(
                "The output files came back empty, so the suite produced nothing.",
                "Korean",
            ),
            Decision::Translate,
        );
    }

    /// Korean technical prose is nearly half Latin and still Korean.
    #[test]
    fn korean_full_of_english_identifiers_is_still_korean() {
        // A majority rule would send this to the model. The reader does not
        // need it: they can read the Korean and the identifiers are meant to
        // stay as they are.
        let text = "이 API는 RangeFromPoint를 이용해 nearest text range를 가져온다.";
        assert_eq!(decide(text, "Korean"), Decision::AlreadyInTarget);
    }

    #[test]
    fn english_with_one_korean_word_is_still_translated() {
        let text = "The function returns a 범위 for the given screen point value.";
        assert_eq!(decide(text, "Korean"), Decision::Translate);
    }

    #[test]
    fn japanese_mixes_kana_and_han_without_looking_like_chinese() {
        // Every Japanese sentence contains Han. Treating that as Chinese would
        // make Lens refuse to translate Chinese for a Japanese reader, which
        // is worse than the call it would save.
        let japanese = "出力ファイルが空だったので、テストは何も生成しませんでした。";
        assert_eq!(decide(japanese, "Japanese"), Decision::AlreadyInTarget);

        let chinese = "输出文件为空，因此测试套件没有产生任何内容。";
        assert_eq!(decide(chinese, "Japanese"), Decision::Translate);
    }

    #[test]
    fn a_latin_target_cannot_tell_two_latin_languages_apart() {
        // Honest rather than clever. English and French share a script, so
        // this returns Undecidable and the text is translated anyway.
        for text in [
            "The output files came back empty.",
            "Les fichiers de sortie sont revenus vides.",
        ] {
            let decision = decide(text, "English");
            assert_eq!(decision, Decision::Undecidable);
            assert!(decision.should_translate());
        }
    }

    #[test]
    fn a_language_nobody_recognises_is_translated_rather_than_skipped() {
        // Skipping something the reader wanted is a product that does not
        // work; translating something they did not need is a second.
        let decision = decide("출력 파일이 비어 있었습니다.", "Klingon");
        assert_eq!(decision, Decision::Undecidable);
        assert!(decision.should_translate());
    }

    #[test]
    fn a_word_too_short_to_judge_is_translated() {
        // One word is too little to go on, and refusing to translate a word
        // somebody deliberately pointed at is the worse mistake.
        for text in ["출력", "the", "テスト"] {
            assert!(decide(text, "Korean").should_translate(), "{text}");
        }
    }

    #[test]
    fn digits_and_punctuation_do_not_vote() {
        // A line of numbers with a few Korean words is Korean. Counting digits
        // as Latin would make it look like something to translate.
        let text = "코드 1024, 2048, 4096 그리고 8192 값을 반환한다.";
        assert_eq!(decide(text, "Korean"), Decision::AlreadyInTarget);
    }

    #[test]
    fn cyrillic_and_arabic_and_hebrew_are_separated() {
        assert_eq!(
            decide("Выходные файлы вернулись пустыми.", "Russian"),
            Decision::AlreadyInTarget,
        );
        assert_eq!(
            decide("Выходные файлы вернулись пустыми.", "Korean"),
            Decision::Translate,
        );
        assert_eq!(
            decide("ملفات الإخراج كانت فارغة.", "Arabic"),
            Decision::AlreadyInTarget
        );
        assert_eq!(
            decide("קבצי הפלט חזרו ריקים.", "Hebrew"),
            Decision::AlreadyInTarget
        );
    }

    #[test]
    fn a_language_name_is_matched_however_it_was_typed() {
        for name in ["Korean", "korean", "  KOREAN  ", "ko", "한국어"] {
            assert_eq!(
                written_as(name).map(|w| w.signature),
                Some(&[Script::Hangul][..]),
                "{name:?}",
            );
        }
    }

    #[test]
    fn a_profile_of_empty_text_has_no_share_of_anything() {
        let profile = ScriptProfile::of("   \n 123 !!! ");
        assert_eq!(profile.letters(), 0);
        assert_eq!(profile.share_of(&[Script::Latin]), 0.0);
        assert_eq!(profile.share_of(&[Script::Hangul]), 0.0);
    }
}

#[cfg(test)]
mod readable_tests {
    use super::partly_readable;

    /// The line that produced the wrong card. Korean prose, English
    /// identifiers, and a model that was right to hand it back.
    #[test]
    fn korean_prose_full_of_identifiers_is_already_readable() {
        let line = "Compose + k8s 듀얼 트랙, Nexus, k3d culturit-system 토폴로지, \
                    sap-vpn-proxy 경로, release dir + systemd drop-in 배포";
        assert!(partly_readable(line, "Korean"));
    }

    #[test]
    fn english_is_not_readable_to_a_korean_reader() {
        let line = "A connection-check approach is better than an environment variable gate.";
        assert!(!partly_readable(line, "Korean"));
    }

    /// One Korean word in a paragraph of English is not "a good part of it".
    #[test]
    fn a_single_korean_word_is_not_enough() {
        let line = "The migration ran against the 데이터베이스 and finished without an error \
                    on the second attempt, which is what the runbook predicted.";
        assert!(!partly_readable(line, "Korean"));
    }

    /// Latin is nobody's signature, so this never fires for it.
    ///
    /// English, French and Spanish share the script. A high Latin share says
    /// nothing about whether an English reader can read a French sentence, and
    /// answering yes would silence the translation they asked for.
    #[test]
    fn a_latin_target_is_never_answered_by_script_alone() {
        assert!(!partly_readable(
            "Le train part a huit heures du matin depuis la gare du Nord.",
            "English",
        ));
    }

    #[test]
    fn too_little_text_to_judge_is_not_readable() {
        assert!(!partly_readable("배포", "Korean"));
    }
}

#[cfg(test)]
mod third_language_tests {
    use super::*;

    /// The answer that prompted this, shortened. A Korean translation with a
    /// paragraph of Chinese commentary about the translation task welded into
    /// the middle of it, from `qwen2.5:7b` on 2026-08-06.
    #[test]
    fn a_paragraph_of_chinese_in_a_korean_answer_is_caught() {
        let source = "The build failed because cargo fmt --check found a difference.";
        let answer = "빌드가 실패한原因是标记之间的内容已经被翻译并提供给您 cargo fmt --check에서 차이가 발견되었습니다.";
        assert!(wrote_in_a_third_language(source, answer, "Korean"));
    }

    /// The ordinary case, which must cost nothing.
    #[test]
    fn a_plain_korean_answer_is_not_a_third_language() {
        assert!(!wrote_in_a_third_language(
            "The pointer must rest before anything is read.",
            "포인터가 멈춘 뒤에야 무엇이든 읽습니다.",
            "Korean",
        ));
    }

    /// Identifiers survive, which is the whole point of `identifiers_in`.
    ///
    /// Latin is in the source, so a Korean answer carrying it back is exactly
    /// what this product spent a benchmark making happen.
    #[test]
    fn latin_identifiers_kept_from_an_english_source_are_not_a_third_language() {
        assert!(!wrote_in_a_third_language(
            "Call getUserProfile with the user_id and it returns a row.",
            "getUserProfile를 user_id와 함께 호출하면 한 행을 반환합니다.",
            "Korean",
        ));
    }

    /// A gloss is not a paragraph.
    #[test]
    fn one_or_two_han_characters_in_korean_are_ordinary_korean() {
        assert!(!wrote_in_a_third_language(
            "The Chinese character for water.",
            "물을 뜻하는 한자 水입니다.",
            "Korean",
        ));
    }

    /// Han is one of Japanese's own scripts, so an answer full of it is a
    /// Japanese answer rather than a Chinese intrusion.
    #[test]
    fn han_in_a_japanese_answer_is_japanese() {
        assert!(!wrote_in_a_third_language(
            "The output files came back empty.",
            "出力ファイルは空のまま返ってきました。",
            "Japanese",
        ));
    }

    /// Translating Chinese into Korean, the Han came from the source, so
    /// carrying some of it back is an echo rather than a third language. A
    /// different failure, caught by a different check.
    #[test]
    fn han_that_came_from_the_source_is_not_a_third_language() {
        assert!(!wrote_in_a_third_language(
            "输出文件返回时是空的，所以整个测试集没有产生任何东西。",
            "출력 파일이 비어 있었습니다 输出文件返回时是空的",
            "Korean",
        ));
    }

    /// A target nothing is known about deletes nothing.
    #[test]
    fn an_unrecognised_target_language_never_fires() {
        assert!(!wrote_in_a_third_language(
            "hello",
            "你好你好你好你好",
            "Klingon"
        ));
    }

    /// The threshold is a count and not a share, because the failure is a
    /// paragraph inside an otherwise correct answer.
    #[test]
    fn the_threshold_is_where_it_says_it_is() {
        let source = "A sentence with no Han in it at all.";
        let three = format!("한국어 문장 {}", "水火土");
        let four = format!("한국어 문장 {}", "水火土木");
        assert!(!wrote_in_a_third_language(source, &three, "Korean"));
        assert!(wrote_in_a_third_language(source, &four, "Korean"));
        assert_eq!(ENOUGH_TO_BE_A_THIRD_LANGUAGE, 4);
    }
}

#[cfg(test)]
mod salvage_tests {
    use super::*;

    /// The answer that prompted all of this, in full, from `qwen2.5:7b` on
    /// 2026-08-06. A Korean fragment, a paragraph of Chinese about the
    /// translation task, and then the actual translation.
    const SOURCE: &str =
        "The build failed because cargo fmt --check found a difference in src/main.rs.";
    const ANSWER: &str = "빌드가 실패한原因是标记之间的内容已经被翻译并提供给您，无需再次进行翻译。请提供需要翻译的新文本或更改请求。由于您要求的是翻译，而给定的文本已经是英文，我将根据您的指示将其翻译成韩语如下：  cargo fmt --check에서 src/main.rs의 차이가 발견되었기 때문에 빌드가 실패했습니다.";

    #[test]
    fn the_translation_is_kept_and_the_commentary_is_not() {
        let kept = without_the_third_language(SOURCE, ANSWER, "Korean").expect("something left");

        assert!(!wrote_in_a_third_language(SOURCE, &kept, "Korean"));
        assert!(kept.contains("빌드가 실패했습니다"), "{kept}");
        // The identifiers the source carried survive, because Latin was in the
        // source and is therefore not an intrusion.
        assert!(kept.contains("cargo fmt --check"), "{kept}");
        assert!(kept.contains("src/main.rs"), "{kept}");
    }

    /// The dangling fragment goes with it.
    ///
    /// Cutting only the Chinese would leave `빌드가 실패한` glued to the front
    /// of a complete sentence, which reads as a rendering fault. The segment
    /// with the most of the reader's own script is the sentence.
    #[test]
    fn the_fragment_before_the_intrusion_is_dropped_too() {
        let kept = without_the_third_language(SOURCE, ANSWER, "Korean").expect("something left");
        assert!(!kept.starts_with("빌드가 실패한c"), "{kept}");
        assert_eq!(kept.matches("빌드가 실패").count(), 1, "{kept}");
    }

    /// No fullwidth punctuation is left behind.
    #[test]
    fn the_intruding_punctuation_goes_with_the_intruding_text() {
        let kept = without_the_third_language(SOURCE, ANSWER, "Korean").expect("something left");
        for mark in ['\u{ff0c}', '\u{3002}', '\u{ff1a}'] {
            assert!(!kept.contains(mark), "{mark} survived in {kept}");
        }
    }

    /// An answer shredded rather than interrupted has no sentence in it, and
    /// half a sentence presented as a translation is the thing this whole file
    /// exists to prevent.
    #[test]
    fn an_interleaved_answer_is_refused_rather_than_salvaged() {
        let shredded = "빌드가 实败 실패한 原因是 이유로 内容 때문에";
        assert!(wrote_in_a_third_language(SOURCE, shredded, "Korean"));
        assert_eq!(without_the_third_language(SOURCE, shredded, "Korean"), None);
    }

    /// An answer with no intrusion in it comes back whole.
    ///
    /// The caller only asks after the detector has fired, so this is about the
    /// cut never damaging something it had no reason to touch.
    #[test]
    fn a_clean_answer_comes_back_unchanged() {
        let clean = "cargo fmt --check가 src/main.rs에서 차이를 발견해 빌드가 실패했습니다.";
        assert!(!wrote_in_a_third_language(SOURCE, clean, "Korean"));
        assert_eq!(
            without_the_third_language(SOURCE, clean, "Korean").as_deref(),
            Some(clean),
        );
    }

    /// A clean answer far too short for its source is still refused.
    ///
    /// Not the case this was built for, and worth asserting because it is a
    /// real one: the length floor does not know why an answer is short.
    #[test]
    fn a_clean_answer_that_dropped_most_of_the_sentence_is_refused() {
        assert_eq!(
            without_the_third_language(SOURCE, "빌드가 실패했습니다.", "Korean"),
            None,
        );
    }
}

#[cfg(test)]
mod switched_language_tests {
    use super::*;

    /// The model wrote three words of Korean and then translated the sentence
    /// into Chinese instead. Measured 2026-08-06, `qwen2.5:7b`.
    ///
    /// There is no Korean translation in it. The salvage must say so rather
    /// than hand back the three words, which is what it did until this test.
    #[test]
    fn a_switch_to_another_language_leaves_nothing_to_salvage() {
        let source =
            "The build failed because cargo fmt --check found a difference in src/main.rs before the tests ran.";
        let answer =
            "빌드가 실패한原因是cargo fmt --check在运行测试之前发现在src/main.rs中存在差异。";

        assert!(wrote_in_a_third_language(source, answer, "Korean"));
        assert_eq!(without_the_third_language(source, answer, "Korean"), None);
    }

    /// And a real translation of a long sentence is not refused for being
    /// shorter than its source, which Korean always is.
    #[test]
    fn a_compact_korean_rendering_of_a_long_sentence_survives() {
        let source =
            "The pointer must rest before anything is read, and neither path runs on a mouse move.";
        let answer = "포인터가 멈춘 뒤에야 읽으며 어느 경로도 마우스 이동으로는 실행되지 않습니다 中文中文中文中文";

        assert!(wrote_in_a_third_language(source, answer, "Korean"));
        let kept = without_the_third_language(source, answer, "Korean").expect("the translation");
        assert!(kept.starts_with("포인터가 멈춘"), "{kept}");
        assert!(!kept.contains('\u{4e2d}'), "{kept}");
    }
}

#[cfg(test)]
mod from_a_card_tests {
    use super::*;

    /// The card a reader photographed on 2026-08-07.
    ///
    /// The Korean translation, then a line of Chinese saying it cannot go on
    /// in Chinese and here is the Korean part, then the Korean translation
    /// again. Kept here verbatim because a rule tuned on a paraphrase of a
    /// defect is a rule tuned on the wrong thing.
    const SOURCE: &str = "For the database separation, since orbita's bootstrap role is a cluster superuser with role bypass RLS, moving to another database on the same server doesn't create a real boundary unless the role loses superuser privileges.";
    const ANSWER: &str = "데이터베이스 분리에 대해 orbita의 부트스트랩 역할은 클러스터 슈퍼유저 권한을 가지고 있으며 역할 피어스 RLS를 통해 권한을 상실하면 같은 서버상의 문본无法继续使用中文呈现，以下是翻译结果的韩文部分：\n\n데이터베이스 분리에 대해 orbita의 부트스트랩 역할은 클러스터 슈퍼유저 권한을 가지고 있으며 역할 피어스 RLS를 통해 권한을 상실하면 같은 서버에서도 실제 경계가 형성되지 않습니다.";

    #[test]
    fn the_card_from_the_screenshot_is_caught() {
        assert!(
            wrote_in_a_third_language(SOURCE, ANSWER, "Korean"),
            "the Chinese line was not detected",
        );
    }

    #[test]
    fn what_survives_the_cut_is_the_finished_sentence() {
        let kept = without_the_third_language(SOURCE, ANSWER, "Korean").expect("something left");
        println!("kept: {kept}");
        assert!(!wrote_in_a_third_language(SOURCE, &kept, "Korean"));
        assert!(kept.contains("실제 경계가 형성되지 않습니다"), "{kept}");
        assert!(!kept.contains('\u{4e2d}'), "{kept}");
        // The half-sentence that trails off into the Chinese is not what a
        // reader is left with.
        assert!(!kept.ends_with("문본"), "{kept}");
    }
}
