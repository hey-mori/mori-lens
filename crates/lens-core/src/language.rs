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
