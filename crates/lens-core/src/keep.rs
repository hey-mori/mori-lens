//! What a reader keeps, and the shape of the file it goes into.
//!
//! [`crate::card::Card::for_the_clipboard`] answered "let me have this", and
//! the clipboard holds one thing until the next thing replaces it. This is the
//! version that survives a paste into the wrong window, a reboot, and a week.
//!
//! # This is the first thing Lens writes down that came off a screen
//!
//! The settings file is what somebody typed. The translation cache is memory
//! and dies with the process, deliberately: `docs/privacy-model.md` says a
//! durable cache would be a record of what was read, so it is a separate
//! opt-in and does not exist.
//!
//! A saved-items file **is** a record of what was read, and the difference
//! that makes it allowed is that a person made each entry, one keypress at a
//! time, knowing what was on the card in front of them. That is the M8 gate's
//! first clause and it is a property of the trigger, not of this module.
//!
//! # Why plain Markdown rather than a database
//!
//! Because the gate's other two clauses are *export round-trips* and *deletion
//! is real*, and a file somebody can open in any editor satisfies both without
//! code. Export is the file. Deletion is selecting a block and pressing
//! delete, which is more real than a button that sets a flag.
//!
//! It also means a reader can keep their own notes in it. Nothing here parses
//! the file back, so there is nothing to break by writing in it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One thing a reader decided to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The text on the screen, exactly as the card showed it.
    pub original: String,
    /// The translation, or the explanation, as the card showed it.
    pub answer: String,
    /// The caveat, when the answer carries one.
    ///
    /// An explanation is a model's account of grammar and a learner cannot
    /// tell a wrong one, which is what
    /// [`crate::analyse::DeepAnalysis::LABEL`] is for. A note outlives the
    /// card it was read from by much further than a clipboard does, so this
    /// matters more here than anywhere.
    pub caveat: Option<String>,
    /// The application it was read in, when it is known. Useful for a reader
    /// looking back, and it is the process name the policy already works with
    /// rather than anything new.
    pub app: Option<String>,
    /// The source language of the original text, when known.
    ///
    /// Stored in the library database for structured queries. The saved.md
    /// file does not use it; it is metadata for the reader looking back.
    pub source_language: Option<String>,
    /// The target language the text was translated into.
    ///
    /// The reader's own language, from the settings file. Carried so a library
    /// entry can be searched by language pair.
    pub target_language: Option<String>,
    /// The model that produced the answer.
    ///
    /// The name the engine reported, e.g. `qwen2.5:7b`. Useful when a reader
    /// is comparing answers across models, and it is what the footer already
    /// shows on the card.
    pub model: Option<String>,
    /// Which inference provider produced the answer.
    ///
    /// The inference engine identity: `"ollama"`, `"lmstudio"`, or `None` for
    /// legacy/unknown provenance. Distinct from `model` (which model ID) and
    /// from provider_scope (where inference ran relative to trust boundary).
    /// Bound at inference time from the resolved runtime lane, not read from
    /// live settings at save time.
    pub provider: Option<String>,
}

/// The heading a file starts with, written once when it is created.
///
/// It says what the file is and what may be done to it, because the answer to
/// "how do I delete this" has to be in the file rather than in a manual.
pub const PREAMBLE: &str = concat!(
    "# Saved from Mori Lens\n",
    "\n",
    "One block per thing you kept. Plain Markdown, so this is also the export:\n",
    "copy it, sync it, or open it in anything.\n",
    "\n",
    "Delete a block to delete it. Nothing reads this file back, so your own\n",
    "notes are safe to write in it.\n",
);

/// The block appended for one entry.
///
/// A heading with the date, the source, the answer, and the caveat when there
/// is one. Sources and answers are quoted as Markdown blockquotes so a
/// sentence with a `#` or a `-` in it cannot turn into a heading or a list in
/// somebody's editor.
pub fn block(entry: &Entry, at: SystemTime) -> String {
    let mut out = String::from("\n---\n\n## ");
    out.push_str(&date_of(at));
    if let Some(app) = entry.app.as_deref().filter(|name| !name.trim().is_empty()) {
        out.push_str(" · ");
        out.push_str(app.trim());
    }
    out.push_str("\n\n");
    quote_into(&entry.original, &mut out);
    out.push('\n');
    quote_into(&entry.answer, &mut out);
    if let Some(caveat) = entry
        .caveat
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        out.push('\n');
        out.push_str(caveat.trim());
        out.push('\n');
    }
    out
}

/// Write `text` as a blockquote, one line at a time.
///
/// Every line gets its own marker. A blockquote whose second line has no `>`
/// is a paragraph in Markdown, and an analysis is five lines.
fn quote_into(text: &str, out: &mut String) {
    for line in text.trim().lines() {
        out.push_str("> ");
        out.push_str(line.trim_end());
        out.push('\n');
    }
}

/// `YYYY-MM-DD HH:MM`, in UTC.
///
/// UTC, and said to be UTC nowhere, because the alternative is worse. Local
/// time needs the platform's timezone database, which `lens-core` cannot reach
/// without the OS crates [ADR-0003](../../../docs/decisions/ADR-0003-rust-core-platform-adapters.md)
/// keeps out of it. A reader looking back at their own notes wants to know
/// roughly when, and roughly when is what this gives.
///
/// Hand-rolled rather than a date crate, for the same reason the cache is a
/// map and a counter: the whole calculation is fifteen lines and it keeps this
/// crate dependency-free.
fn date_of(at: SystemTime) -> String {
    let seconds = at
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let rest = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        (rest % 3600) / 60,
    )
}

/// Days since 1970-01-01 to a calendar date.
///
/// Howard Hinnant's `civil_from_days`, which is the standard way to do this
/// without a library and is correct across the leap rules rather than
/// approximately right.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn a_translation() -> Entry {
        Entry {
            original: "The pointer must rest before anything is read.".to_owned(),
            answer: "포인터가 멈춘 뒤에야 무엇이든 읽습니다.".to_owned(),
            caveat: None,
            app: Some("Code".to_owned()),
            source_language: Some("English".to_owned()),
            target_language: Some("Korean".to_owned()),
            model: Some("qwen2.5:7b".to_owned()),
            provider: None,
        }
    }

    #[test]
    fn a_kept_translation_reads_as_a_block_somebody_can_edit() {
        // 2026-08-07 03:59 UTC.
        let block = block(&a_translation(), at(1_786_075_140));
        assert_eq!(
            block,
            concat!(
                "\n---\n\n",
                "## 2026-08-07 03:59 · Code\n\n",
                "> The pointer must rest before anything is read.\n",
                "\n",
                "> 포인터가 멈춘 뒤에야 무엇이든 읽습니다.\n",
            ),
        );
    }

    /// The caveat is the reason this matters more than the clipboard does.
    #[test]
    fn an_explanation_keeps_the_warning_that_a_model_wrote_it() {
        let entry = Entry {
            original: "RowQuery는 멱등성을 보장하지 않습니다.".to_owned(),
            answer: "원형 · 멱등성\n품사 · 명사".to_owned(),
            caveat: Some(crate::analyse::DeepAnalysis::LABEL.to_owned()),
            app: None,
            source_language: None,
            target_language: None,
            model: None,
            provider: None,
        };
        let block = block(&entry, at(0));
        assert!(
            block.contains("> 원형 · 멱등성\n> 품사 · 명사\n"),
            "{block}"
        );
        assert!(block.ends_with("AI 설명 · 사실 확인 필요\n"), "{block}");
    }

    /// Every line of a multi-line answer is quoted.
    ///
    /// A blockquote whose second line has no marker is a paragraph, and an
    /// analysis is five lines, so a half-quoted block would render as a quote
    /// followed by loose text.
    #[test]
    fn every_line_of_an_answer_carries_its_own_marker() {
        let entry = Entry {
            original: "one".to_owned(),
            answer: "a\nb\nc".to_owned(),
            caveat: None,
            app: None,
            source_language: None,
            target_language: None,
            model: None,
            provider: None,
        };
        let block = block(&entry, at(0));
        for line in ["> a", "> b", "> c"] {
            assert!(block.contains(&format!("{line}\n")), "{block}");
        }
    }

    /// A sentence with Markdown in it stays a sentence.
    #[test]
    fn a_hash_in_the_source_does_not_become_a_heading() {
        let entry = Entry {
            original: "# not a heading, and - not a list".to_owned(),
            answer: "제목이 아닙니다".to_owned(),
            caveat: None,
            app: None,
            source_language: None,
            target_language: None,
            model: None,
            provider: None,
        };
        let block = block(&entry, at(0));
        assert!(block.contains("> # not a heading"), "{block}");
        // The only line starting with `#` is the one this module wrote.
        let headings: Vec<&str> = block.lines().filter(|line| line.starts_with('#')).collect();
        assert_eq!(headings.len(), 1, "{headings:?}");
    }

    #[test]
    fn an_unknown_application_leaves_the_heading_alone() {
        let entry = Entry {
            app: None,
            ..a_translation()
        };
        assert!(block(&entry, at(0)).starts_with("\n---\n\n## 1970-01-01 00:00\n"));
    }

    /// The date arithmetic, on the days it is easy to get wrong.
    #[test]
    fn the_calendar_survives_leap_years_and_centuries() {
        for (seconds, expected) in [
            (0u64, "1970-01-01 00:00"),
            (951_782_400, "2000-02-29 00:00"),
            (1_078_012_800, "2004-02-29 00:00"),
            (1_709_164_800, "2024-02-29 00:00"),
            (4_107_542_400, "2100-03-01 00:00"),
            (1_786_075_140, "2026-08-07 03:59"),
        ] {
            assert_eq!(date_of(at(seconds)), expected, "at {seconds}");
        }
    }

    /// The heading says how to delete, because the answer to that has to be in
    /// the file rather than in a manual.
    #[test]
    fn the_preamble_says_what_may_be_done_to_the_file() {
        assert!(PREAMBLE.contains("Delete a block"));
        assert!(PREAMBLE.contains("export"));
        assert!(!PREAMBLE.contains('\u{2014}'), "no em dash");
    }
}
