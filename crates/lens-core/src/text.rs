//! Word, sentence, and paragraph around a point in a block of text.
//!
//! Three ranges are needed at once. The word is what the user pointed at; the
//! sentence is what makes the word translatable. A bare word produces confident
//! nonsense on anything polysemous: `produce` in "the test suite didn't produce
//! anything" is not the verb a dictionary leads with, and `gift` is a present in
//! English and a poison in German.
//!
//! # What this is not
//!
//! This is **not** a correct Unicode sentence segmenter. It handles Latin-script
//! terminators with an abbreviation guard, CJK terminators, and line breaks.
//! ICU's `BreakIterator` is the right long-term answer, and even ICU's own
//! documentation is explicit that sentence boundaries stay approximate around
//! abbreviations.
//!
//! The shortcut is deliberate and scoped: Milestone 1 measures whether text
//! *acquisition* works, and a segmenter good enough to tell a word from its
//! sentence is enough to measure that. Replacing this with ICU is an M3
//! decision, taken when translation quality is what is being judged. Recorded in
//! `docs/research/hover-text-acquisition.md` §4 so the shortcut is never
//! mistaken later for a considered opinion about segmentation quality.

use unicode_segmentation::UnicodeSegmentation;

/// A byte range into the context string. Byte offsets, not character offsets,
/// because that is what slicing a `str` needs and converting at every use is
/// where off-by-one bugs live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
}

impl TextSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    pub const fn contains(self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// The text this span covers. Returns `""` rather than panicking on a span
    /// that does not line up with `text`, because a span can outlive the read
    /// that produced it when a provider answers late.
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        text.get(self.start..self.end).unwrap_or("")
    }
}

/// The text around a point, split three ways.
#[derive(Debug, Clone)]
pub struct SegmentedText {
    context: String,
    focus: usize,
    word: Option<TextSpan>,
    sentence: TextSpan,
    paragraph: TextSpan,
}

impl SegmentedText {
    /// Everything that was read, unmodified. The spans index into this.
    pub fn context(&self) -> &str {
        &self.context
    }

    /// The byte offset the pointer resolved to, clamped to a char boundary.
    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn word(&self) -> Option<TextSpan> {
        self.word
    }

    pub fn sentence(&self) -> TextSpan {
        self.sentence
    }

    pub fn paragraph(&self) -> TextSpan {
        self.paragraph
    }

    /// The word under the pointer, or `None` when the pointer is on whitespace
    /// or punctuation. `None` is a real answer: there is no word there.
    pub fn word_text(&self) -> Option<&str> {
        self.word.map(|span| span.slice(&self.context))
    }

    pub fn sentence_text(&self) -> &str {
        self.sentence.slice(&self.context)
    }

    pub fn paragraph_text(&self) -> &str {
        self.paragraph.slice(&self.context)
    }
}

/// Split `context` around the byte offset `focus`.
///
/// `focus` is clamped into range and onto a character boundary, so a caller that
/// computed it from a character count in another encoding cannot panic this.
pub fn segment(context: &str, focus: usize) -> SegmentedText {
    let focus = clamp_to_char_boundary(context, focus);
    let paragraph = paragraph_span(context, focus);
    let sentence = sentence_span(context, paragraph, focus);
    let word = word_span(context, sentence, focus);

    SegmentedText {
        context: context.to_owned(),
        focus,
        word,
        sentence,
        paragraph,
    }
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        // The last boundary, not `len()`, so a focus past the end still points
        // at the final character rather than past it.
        return text
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }
    let mut offset = offset;
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Trim whitespace off both ends of a span without moving the focus out of it.
fn trim_span(text: &str, span: TextSpan, focus: usize) -> TextSpan {
    let slice = span.slice(text);
    let leading = slice.len() - slice.trim_start().len();
    let trailing = slice.len() - slice.trim_end().len();
    let start = (span.start + leading).min(focus);
    let end = (span.end - trailing).max(focus + 1).min(text.len());
    TextSpan::new(start.min(end), end)
}

/// Paragraphs are separated by a blank line. A single newline stays inside the
/// paragraph, because a wrapped line and a new paragraph are different things
/// and providers report both as `\n`.
fn paragraph_span(text: &str, focus: usize) -> TextSpan {
    let mut start = 0usize;
    let mut end = text.len();

    for (sep_start, sep_end) in blank_line_separators(text) {
        if sep_end <= focus {
            start = sep_end;
        } else if sep_start > focus {
            end = sep_start;
            break;
        } else {
            // The focus landed inside the separator itself. There is no
            // paragraph under the pointer; give it the run of whitespace and let
            // sentence detection collapse to the same thing.
            start = sep_start;
            end = sep_end;
            break;
        }
    }

    trim_span(text, TextSpan::new(start, end), focus)
}

/// Runs of whitespace that contain two or more newlines.
fn blank_line_separators(text: &str) -> Vec<(usize, usize)> {
    let mut separators = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'\n' {
            index += 1;
            continue;
        }
        let start = index;
        let mut cursor = index + 1;
        let mut newlines = 1usize;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            if bytes[cursor] == b'\n' {
                newlines += 1;
            }
            cursor += 1;
        }
        if newlines >= 2 {
            separators.push((start, cursor));
        }
        index = cursor.max(index + 1);
    }

    separators
}

/// Terminators that only end a sentence when whitespace follows.
///
/// The requirement is what keeps `3.5`, `file.txt`, and `example.com` from
/// being read as sentence ends.
const SPACED_TERMINATORS: [char; 4] = ['.', '!', '?', '…'];

/// Terminators that end a sentence whether or not whitespace follows.
///
/// Japanese and Chinese do not put a space after `。`, so requiring one would
/// collapse a whole paragraph into a single sentence. Found by a failing test
/// on `出力ファイルは空でした。だから何も生成されませんでした。`, which is
/// exactly the shape a user reading Japanese hovers over.
const CJK_TERMINATORS: [char; 3] = ['。', '！', '？'];

/// Characters that may follow a terminator and still belong to the sentence.
const TRAILING: [char; 8] = ['"', '\'', ')', ']', '}', '»', '\u{201d}', '\u{2019}'];

fn is_terminator(ch: char) -> bool {
    SPACED_TERMINATORS.contains(&ch) || CJK_TERMINATORS.contains(&ch)
}

/// Tokens that end in a period without ending a sentence. Single letters are
/// handled separately, which is what covers initials and `e.g.` / `i.e.`.
const ABBREVIATIONS: [&str; 22] = [
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "etc", "fig", "no", "vol", "ch",
    "approx", "dept", "est", "inc", "ltd", "co", "al", "cf",
];

/// The sentence containing `focus`, searched within `paragraph`.
fn sentence_span(text: &str, paragraph: TextSpan, focus: usize) -> TextSpan {
    if paragraph.is_empty() {
        return paragraph;
    }

    let mut start = paragraph.start;
    let mut end = paragraph.end;

    for boundary in sentence_boundaries(text, paragraph) {
        if boundary <= focus {
            start = boundary;
        } else {
            end = boundary;
            break;
        }
    }

    trim_span(text, TextSpan::new(start, end), focus)
}

/// Offsets inside `paragraph` where one sentence ends and the next begins.
fn sentence_boundaries(text: &str, paragraph: TextSpan) -> Vec<usize> {
    let slice = paragraph.slice(text);
    let mut boundaries = Vec::new();
    let mut chars = slice.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        // A line break ends a sentence. Headings, list items, and table cells
        // carry no terminator, and treating a whole block as one sentence would
        // send a screenful of text to be translated as a unit.
        if ch == '\n' {
            boundaries.push(paragraph.start + offset + ch.len_utf8());
            continue;
        }
        if !is_terminator(ch) {
            continue;
        }

        // Consume any run of terminators and closing punctuation: `?!"` ends
        // one sentence, not three.
        let mut end = offset + ch.len_utf8();
        let mut saw_cjk = CJK_TERMINATORS.contains(&ch);
        while let Some(&(next_offset, next_ch)) = chars.peek() {
            if is_terminator(next_ch) || TRAILING.contains(&next_ch) {
                saw_cjk |= CJK_TERMINATORS.contains(&next_ch);
                end = next_offset + next_ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        match slice[end..].chars().next() {
            // End of the paragraph: a boundary, but there is nothing after it,
            // so recording it would produce an empty trailing sentence.
            None => break,
            // A Latin terminator glued to the next word is a decimal point, an
            // ellipsis in a filename, or a domain: not a sentence end. A CJK
            // terminator carries no such ambiguity and needs no space.
            Some(next) if !next.is_whitespace() && !saw_cjk => continue,
            Some(_) => {}
        }

        if ch == '.' && !period_ends_a_sentence(slice, offset, end) {
            continue;
        }

        boundaries.push(paragraph.start + end);
    }

    boundaries
}

/// Whether a period at `offset` is a full stop rather than an abbreviation.
fn period_ends_a_sentence(slice: &str, offset: usize, after: usize) -> bool {
    let preceding = trailing_word(&slice[..offset]);
    if !preceding.is_empty() {
        // A single letter before a period is an initial (`A. Turing`) or the
        // tail of a segmented abbreviation (`e.g.`, `i.e.`).
        if preceding.chars().count() == 1 && preceding.chars().all(char::is_alphabetic) {
            return false;
        }
        if ABBREVIATIONS.contains(&preceding.to_lowercase().as_str()) {
            return false;
        }
    }

    // A lower-case word after the gap usually means the period was not a full
    // stop. Only applied to cased scripts: in Hangul, Kana, or Han there is no
    // case to read, and `is_lowercase` is false for all of them, so this test
    // correctly abstains.
    match slice[after..].split_whitespace().next() {
        Some(next) => !next.chars().next().is_some_and(char::is_lowercase),
        None => true,
    }
}

/// The run of alphanumeric characters at the end of `text`.
fn trailing_word(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_alphanumeric())
        .last()
        .map(|(index, _)| index);
    match start {
        Some(index) => &text[index..],
        None => "",
    }
}

/// The Unicode word containing `focus`, searched within `sentence`.
fn word_span(text: &str, sentence: TextSpan, focus: usize) -> Option<TextSpan> {
    let slice = sentence.slice(text);
    if slice.is_empty() {
        return None;
    }
    let local = focus.checked_sub(sentence.start)?;

    let words = || {
        slice
            .unicode_word_indices()
            .map(|(offset, word)| TextSpan::new(offset, offset + word.len()))
    };

    let word = words()
        .find(|span| span.contains(local))
        // Only when nothing contains the offset: a pointer resting on the
        // trailing edge of the last word still means that word. Checked second,
        // because checking it first lets a word ending at the offset steal the
        // hit from the word starting there, which in `出力ファイル` resolved
        // `力` when the pointer was on `フ`.
        .or_else(|| words().find(|span| span.end == local))?;

    let narrowed = narrow_to_script_run(slice, word, local);
    Some(TextSpan::new(
        sentence.start + narrowed.start,
        sentence.start + narrowed.end,
    ))
}

/// Writing systems that are worth splitting a word on.
///
/// Digits are folded into `Latin` so `qwen2` stays one token, while `2025년`
/// splits into the number and the counter, which is the useful reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Hangul,
    Han,
    Kana,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Other,
}

/// `None` for a character that belongs to whichever run it is next to:
/// apostrophes, hyphens, underscores, and combining marks. Without this,
/// `didn't` would split into three.
fn script_of(ch: char) -> Option<Script> {
    if ch.is_ascii_digit() {
        return Some(Script::Latin);
    }
    if !ch.is_alphanumeric() {
        return None;
    }
    Some(match ch as u32 {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        // Cyrillic, plus the Cyrillic Supplement block.
        0x0400..=0x052F => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F => Script::Arabic,
        0x3040..=0x30FF | 0x31F0..=0x31FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => Script::Han,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => Script::Hangul,
        _ if ch.is_ascii() => Script::Latin,
        _ => Script::Other,
    })
}

/// Narrow a word to the run of one writing system around `focus`.
///
/// UAX#29 does not break between two letters of different scripts, so
/// `RangeFromPoint를` comes back as one word: the identifier plus the Korean
/// object particle. For a tool whose primary case is Korean prose containing
/// English terms, that is the wrong unit to look up. Splitting on the script
/// boundary gives back the identifier, and it does the same useful thing for
/// `出力ファイル`, which separates into the Han compound and the katakana word.
fn narrow_to_script_run(slice: &str, word: TextSpan, focus: usize) -> TextSpan {
    let word_text = word.slice(slice);
    let mut run_start = word.start;
    let mut run_script: Option<Script> = None;

    for (offset, ch) in word_text.char_indices() {
        let absolute = word.start + offset;
        let Some(script) = script_of(ch) else {
            // Neutral: stays with the run in progress.
            continue;
        };
        match run_script {
            None => run_script = Some(script),
            Some(current) if current == script => {}
            Some(_) => {
                if absolute > focus {
                    return TextSpan::new(run_start, absolute);
                }
                run_start = absolute;
                run_script = Some(script);
            }
        }
    }

    TextSpan::new(run_start, word.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Segment at the first occurrence of `needle`, which is how a human
    /// describes "the pointer is on this word".
    fn at(text: &str, needle: &str) -> SegmentedText {
        let offset = text.find(needle).expect("needle is in the text");
        segment(text, offset)
    }

    const SAMPLE: &str =
        "The output files came back empty, so the full test suite didn't produce anything.";

    #[test]
    fn the_word_under_the_pointer_is_the_word_under_the_pointer() {
        assert_eq!(at(SAMPLE, "produce").word_text(), Some("produce"));
        assert_eq!(at(SAMPLE, "output").word_text(), Some("output"));
    }

    #[test]
    fn pointing_mid_word_still_resolves_the_whole_word() {
        let offset = SAMPLE.find("produce").expect("present") + 3;
        assert_eq!(segment(SAMPLE, offset).word_text(), Some("produce"));
    }

    #[test]
    fn a_contraction_is_one_word() {
        assert_eq!(at(SAMPLE, "didn't").word_text(), Some("didn't"));
    }

    #[test]
    fn pointing_at_a_space_yields_no_word_but_still_a_sentence() {
        let offset = SAMPLE.find(" so ").expect("present");
        let segmented = segment(SAMPLE, offset);
        assert_eq!(segmented.word_text(), None);
        assert_eq!(segmented.sentence_text(), SAMPLE);
    }

    #[test]
    fn the_sentence_is_the_whole_sentence() {
        assert_eq!(at(SAMPLE, "produce").sentence_text(), SAMPLE);
    }

    #[test]
    fn sentences_split_on_terminators() {
        let text = "First one here. Second one follows! Third one? Done.";
        assert_eq!(at(text, "First").sentence_text(), "First one here.");
        assert_eq!(at(text, "Second").sentence_text(), "Second one follows!");
        assert_eq!(at(text, "Third").sentence_text(), "Third one?");
        assert_eq!(at(text, "Done").sentence_text(), "Done.");
    }

    #[test]
    fn an_abbreviation_does_not_end_a_sentence() {
        let text = "Ask Dr. Kim about it. Then leave.";
        assert_eq!(at(text, "Kim").sentence_text(), "Ask Dr. Kim about it.");
    }

    #[test]
    fn an_initial_does_not_end_a_sentence() {
        let text = "A. Turing wrote it. Nobody argued.";
        assert_eq!(at(text, "Turing").sentence_text(), "A. Turing wrote it.");
    }

    #[test]
    fn a_lower_case_continuation_does_not_end_a_sentence() {
        let text = "Version 1. beta shipped anyway. Then it stopped.";
        assert_eq!(
            at(text, "beta").sentence_text(),
            "Version 1. beta shipped anyway."
        );
    }

    #[test]
    fn a_decimal_point_is_not_a_sentence_end() {
        let text = "It grew by 3.5 percent. Nobody noticed.";
        assert_eq!(
            at(text, "percent").sentence_text(),
            "It grew by 3.5 percent."
        );
    }

    #[test]
    fn a_terminator_run_ends_one_sentence_not_three() {
        let text = "Really?! Yes.";
        assert_eq!(at(text, "Really").sentence_text(), "Really?!");
        assert_eq!(at(text, "Yes").sentence_text(), "Yes.");
    }

    #[test]
    fn a_quoted_terminator_keeps_its_closing_quote() {
        let text = "She said \"stop.\" Then she left.";
        assert_eq!(at(text, "said").sentence_text(), "She said \"stop.\"");
    }

    #[test]
    fn a_line_break_ends_a_sentence_even_without_a_terminator() {
        // A heading followed by prose. Without this rule both are translated as
        // one run-on unit.
        let text = "Getting started\nInstall the runtime first.";
        assert_eq!(at(text, "Getting").sentence_text(), "Getting started");
        assert_eq!(
            at(text, "Install").sentence_text(),
            "Install the runtime first."
        );
    }

    #[test]
    fn paragraphs_split_on_a_blank_line_and_sentences_stay_inside_them() {
        let text = "First para, first sentence. Still first para.\n\nSecond para here.";
        let segmented = at(text, "Still");
        assert_eq!(
            segmented.paragraph_text(),
            "First para, first sentence. Still first para."
        );
        assert_eq!(segmented.sentence_text(), "Still first para.");

        assert_eq!(at(text, "Second").paragraph_text(), "Second para here.");
    }

    #[test]
    fn a_wrapped_line_stays_in_one_paragraph() {
        let text = "A long line that the viewer\nwrapped for us.\n\nA new paragraph.";
        assert_eq!(
            at(text, "wrapped").paragraph_text(),
            "A long line that the viewer\nwrapped for us."
        );
    }

    #[test]
    fn korean_text_segments_without_a_case_signal() {
        let text = "출력 파일이 비어 있었다. 그래서 아무것도 생성되지 않았다.";
        assert_eq!(at(text, "출력").sentence_text(), "출력 파일이 비어 있었다.");
        assert_eq!(at(text, "출력").word_text(), Some("출력"));
    }

    #[test]
    fn japanese_terminators_end_a_sentence() {
        let text = "出力ファイルは空でした。だから何も生成されませんでした。";
        assert_eq!(at(text, "出力").sentence_text(), "出力ファイルは空でした。");
    }

    #[test]
    fn mixed_script_text_keeps_the_pointed_at_word() {
        let text = "이 API는 RangeFromPoint를 이용해 nearest text range를 가져온다.";
        assert_eq!(
            at(text, "RangeFromPoint").word_text(),
            Some("RangeFromPoint"),
        );
        assert_eq!(at(text, "nearest").word_text(), Some("nearest"));
    }

    #[test]
    fn a_korean_particle_is_not_part_of_an_english_identifier() {
        // UAX#29 returns `API는` as one word. Looking that up finds nothing;
        // the term the reader wants explained is `API`.
        let text = "이 API는 동작한다.";
        assert_eq!(at(text, "API").word_text(), Some("API"));

        // Pointing at the particle itself yields the particle, not the term.
        let offset = text.find('는').expect("present");
        assert_eq!(segment(text, offset).word_text(), Some("는"));
    }

    #[test]
    fn a_digit_run_splits_from_an_adjacent_counter_but_not_from_latin() {
        assert_eq!(at("2025년에 시작했다.", "2025").word_text(), Some("2025"));
        // A version-like identifier stays whole: the digits are the same run.
        assert_eq!(at("Run qwen2 locally.", "qwen2").word_text(), Some("qwen2"));
    }

    #[test]
    fn kana_words_hold_together_but_han_comes_back_one_character_at_a_time() {
        // The known limitation, asserted rather than hidden. UAX#29 gives each
        // ideograph its own word because splitting Han needs dictionary data,
        // which is what ICU brings and this segmenter does not have. Katakana
        // is a word-break class and does hold together.
        //
        // This is a real gap for a Japanese reader: hovering `出` in `出力`
        // asks about one character of a two-character compound. It is
        // acceptable for Milestone 1, which measures acquisition rather than
        // segmentation quality, and it is the concrete reason M3 replaces this
        // module (`docs/research/hover-text-acquisition.md` §4).
        let text = "出力ファイルは空でした。";
        assert_eq!(at(text, "出力").word_text(), Some("出"));

        let offset = text.find('フ').expect("present");
        assert_eq!(segment(text, offset).word_text(), Some("ファイル"));
    }

    #[test]
    fn a_focus_past_the_end_does_not_panic() {
        let segmented = segment(SAMPLE, SAMPLE.len() + 500);
        assert!(!segmented.sentence_text().is_empty());
    }

    #[test]
    fn a_focus_inside_a_multibyte_character_does_not_panic() {
        let text = "출력 파일이 비어 있었다.";
        // Halfway into the first three-byte character.
        let segmented = segment(text, 1);
        assert_eq!(segmented.word_text(), Some("출력"));
    }

    #[test]
    fn empty_text_yields_empty_spans_and_no_word() {
        let segmented = segment("", 0);
        assert_eq!(segmented.word_text(), None);
        assert_eq!(segmented.sentence_text(), "");
    }

    #[test]
    fn whitespace_only_text_yields_no_word() {
        let segmented = segment("   \n\n   ", 3);
        assert_eq!(segmented.word_text(), None);
    }

    #[test]
    fn spans_index_into_the_retained_context() {
        let segmented = at(SAMPLE, "produce");
        let word = segmented.word().expect("a word was found");
        assert_eq!(word.slice(segmented.context()), "produce");
        assert!(segmented.sentence().contains(segmented.focus()));
    }
}
