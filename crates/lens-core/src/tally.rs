//! What a session actually did, counted, and what its own numbers suggest.
//!
//! # Why counting is not the same as logging
//!
//! `silence::Narrator` prints a refusal once and suppresses the repeat, because
//! a line every time the pointer crosses a margin is the product making itself
//! the noise. That is right for a console and it means **the console is a lower
//! bound**: a reader's session log on 2026-08-18 showed 42 refusals of one kind
//! and the true figure is higher by however many arrived back to back.
//!
//! A count is not a log. It is the thing that can answer *is this working for
//! me*, which is [#165](https://github.com/swarea/mori-lens/issues/165)'s user
//! outcome and which a release binary cannot answer at all today: it is a
//! Windows-subsystem application, so everything the banner prints goes nowhere.
//!
//! # Why the advice is here and not at the call site
//!
//! The numbers that matter are a ratio between two things the loop sees at
//! different moments, and a rule about them written where they are printed is a
//! rule nobody can test. This is `lens-core`, so it is arithmetic with unit
//! tests rather than a sentence inside a `println!`.

use crate::session::OcrMode;

/// What one session has done so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    cards: u32,
    /// Refusals by [`crate::acquire::AcquireError::reason`], in first-seen
    /// order so a report reads the way the session went.
    refused: Vec<(&'static str, u32)>,
}

/// How much of a session has to be one refusal before it is worth mentioning.
///
/// A third. Below that a reader is hovering things that are not text, which is
/// ordinary and is not a setting they should change; above it the product is
/// failing at the thing they installed it for.
const WORTH_MENTIONING: f64 = 1.0 / 3.0;

/// Fewest settled hovers before any of this is said.
///
/// Three refusals out of four hovers is not a pattern, it is a Tuesday. This is
/// the same reasoning `reach::RUN_BEFORE_OFFERING` uses and a larger number,
/// because that one is about a single application and this is about a session.
const ENOUGH_TO_JUDGE: u32 = 12;

impl Tally {
    /// A settled hover that produced a card.
    pub fn card(&mut self) {
        self.cards += 1;
    }

    /// A settled hover that produced nothing, and why.
    pub fn refusal(&mut self, reason: &'static str) {
        match self.refused.iter_mut().find(|(kind, _)| *kind == reason) {
            Some((_, count)) => *count += 1,
            None => self.refused.push((reason, 1)),
        }
    }

    /// Settled hovers, answered or not.
    pub fn settled(&self) -> u32 {
        self.cards + self.refused.iter().map(|(_, count)| count).sum::<u32>()
    }

    /// How many produced a card.
    pub fn cards(&self) -> u32 {
        self.cards
    }

    /// Refusals, commonest first.
    pub fn refusals(&self) -> Vec<(&'static str, u32)> {
        let mut sorted = self.refused.clone();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted
    }

    /// How many of a kind.
    pub fn refusals_of(&self, reason: &str) -> u32 {
        self.refused
            .iter()
            .find(|(kind, _)| *kind == reason)
            .map(|(_, count)| *count)
            .unwrap_or_default()
    }

    /// What this session's own numbers suggest to the reader.
    ///
    /// **At most one sentence, and often none.** A panel that always has advice
    /// on it is a panel whose advice nobody reads, which is the same reasoning
    /// that keeps `reach` to one offer per application.
    ///
    /// The one rule here comes from a reader's own session: 42 of 65 refusals
    /// were `unsupported`, which is precisely the failure OCR exists to repair,
    /// and `ocr` was `manual`, so it never ran once. Nothing told them.
    pub fn advice(&self, ocr: OcrMode) -> Option<String> {
        if self.settled() < ENOUGH_TO_JUDGE {
            return None;
        }
        match ocr {
            // Already reading pixels, so there is nothing to suggest.
            OcrMode::Automatic => return None,
            // **A refusal, not a default.** A reader who wrote `ocr = off`
            // wants the capture path dead rather than polite, and a product
            // that suggests turning it back on is arguing with a decision they
            // made. `ocr_that_is_off_does_not_run_even_when_asked_for` is the
            // same rule one layer down.
            OcrMode::Off => return None,
            OcrMode::Manual => {}
        }
        let unsupported = self.refusals_of("unsupported");
        let share = f64::from(unsupported) / f64::from(self.settled());
        if share < WORTH_MENTIONING {
            return None;
        }
        Some(format!(
            "{unsupported}번의 호버에서 창이 글자를 내주지 않았습니다. 설정에서 ocr = automatic 으로 두면 그런 자리를 화면에서 읽습니다",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::Tally;
    use crate::session::OcrMode;

    fn session(cards: u32, unsupported: u32, other: u32) -> Tally {
        let mut tally = Tally::default();
        for _ in 0..cards {
            tally.card();
        }
        for _ in 0..unsupported {
            tally.refusal("unsupported");
        }
        for _ in 0..other {
            tally.refusal("timeout");
        }
        tally
    }

    #[test]
    fn it_counts_what_happened() {
        let tally = session(20, 42, 12);
        assert_eq!(tally.cards(), 20);
        assert_eq!(tally.settled(), 74);
        assert_eq!(tally.refusals_of("unsupported"), 42);
        assert_eq!(tally.refusals(), vec![("unsupported", 42), ("timeout", 12)]);
    }

    /// The reader's own session, and the sentence nobody said to them.
    ///
    /// 42 refusals of the one kind OCR repairs, with `ocr` on its shipped
    /// default, so the capture path never ran once.
    #[test]
    fn a_session_full_of_windows_that_expose_nothing_says_so() {
        let said = session(20, 42, 12).advice(OcrMode::Manual).expect("advice");
        assert!(said.contains("42"), "{said}");
        assert!(said.contains("automatic"), "{said}");
    }

    /// Nothing to say to somebody it is already working for.
    #[test]
    fn a_session_that_is_working_is_left_alone() {
        assert_eq!(session(40, 3, 2).advice(OcrMode::Manual), None);
    }

    /// Nothing to say to somebody who already turned it on.
    #[test]
    fn a_reader_who_already_asked_is_not_asked_again() {
        assert_eq!(session(20, 42, 12).advice(OcrMode::Automatic), None);
    }

    /// Three refusals out of four hovers is a Tuesday, not a pattern.
    #[test]
    fn too_little_of_a_session_to_judge_says_nothing() {
        assert_eq!(session(1, 3, 0).advice(OcrMode::Manual), None);
        // And the same shape, once there is enough of it.
        assert!(session(4, 12, 0).advice(OcrMode::Manual).is_some());
    }

    /// `off` is a refusal, not a default, so it is not talked out of.
    ///
    /// A reader who wrote `ocr = off` wants the capture path dead. Suggesting
    /// they turn it on is the product arguing with a decision they made, and
    /// `ocr_that_is_off_does_not_run_even_when_asked_for` is the same rule one
    /// layer down.
    #[test]
    fn a_reader_who_turned_it_off_is_not_argued_with() {
        assert_eq!(session(20, 42, 12).advice(OcrMode::Off), None);
    }
}
