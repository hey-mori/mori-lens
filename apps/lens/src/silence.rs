//! Why no card appeared.
//!
//! Every path through a hover that does **not** end in a card used to say
//! nothing at all. Holding the modifier over a window that exposes no text
//! looked exactly like a product that had crashed, or like one that had not
//! noticed the pointer, and there was no way to tell those apart from the
//! outside.
//!
//! The first person to run a release build asked exactly this: it is running,
//! why does hovering do nothing. The answer was that the modifier was not held,
//! and nothing on screen could have told them.
//!
//! # What may be said
//!
//! The reason and the application. **Never the text**, and never its length
//! either: a length is not content, but it is not useful here and
//! `docs/privacy-model.md` §3 is easier to keep than to argue with.
//!
//! Two outcomes are deliberately silent. A repeat of the sentence already on
//! screen and a hover the pointer outran are both normal, constant, and
//! uninteresting; reporting them would bury the two lines that matter.

use lens_core::acquire::AcquireError;
use lens_core::session::Resolution;

/// One line explaining a hover that produced nothing, when there is one.
///
/// `None` means "this is ordinary, say nothing".
pub fn reason_for(resolution: &Resolution) -> Option<&'static str> {
    match resolution {
        // A card is about to appear. Nothing to explain.
        Resolution::Presented(_) => None,
        // Normal and constant. See the note above.
        Resolution::Duplicate(_) | Resolution::Stale => None,
        Resolution::OffTarget(_) => {
            Some("the pointer is not on text; the nearest is too far to be what you meant")
        }
        Resolution::Failed(error) => Some(match error {
            // The three a user actually meets, in words rather than in the
            // taxonomy's vocabulary. `unsupported` is accurate and tells a
            // reader nothing.
            AcquireError::Unsupported => "this application does not expose its text",
            AcquireError::Protected(_) => "a protected field, refused before it was read",
            AcquireError::Empty => "no text at that exact point",
            // Distinguished from `Empty` on purpose: this hover cost no
            // capture, and a log that said otherwise would make the automatic
            // fallback look more expensive than it is.
            AcquireError::KnownEmpty => "no text there, from the previous reading",
            AcquireError::Timeout => "the application did not answer in time",
            AcquireError::ElementGone => "the window changed while it was being read",
            AcquireError::Stale => "the pointer moved first",
            AcquireError::Unavailable => "the accessibility layer is unreachable",
        }),
    }
}

/// Prints a reason, and only when it changes.
///
/// A pointer resting on a toolbar resolves the same way several times a
/// second. A wall of identical lines is as unreadable as silence and rather
/// more annoying.
#[derive(Debug, Default)]
pub struct Narrator {
    last: String,
}

impl Narrator {
    /// The line to print, or nothing when it would repeat.
    pub fn line_for(&mut self, resolution: &Resolution, app: Option<&str>) -> Option<String> {
        let reason = reason_for(resolution)?;
        let line = format!("no card: {reason} [{}]", app.unwrap_or("?"));
        if line == self.last {
            return None;
        }
        self.last = line.clone();
        Some(line)
    }

    /// The same, for a hover the loop declined before it asked anything.
    ///
    /// Text already in the reader's language never reaches a `Resolution`,
    /// because the loop stops before the translator is involved. It was the
    /// one quiet outcome with no explanation attached.
    pub fn line_for_skip(&mut self, reason: &str, app: Option<&str>) -> Option<String> {
        let line = format!("no card: {reason} [{}]", app.unwrap_or("?"));
        if line == self.last {
            return None;
        }
        self.last = line.clone();
        Some(line)
    }

    /// A card appeared, so the next quiet hover is worth reporting again.
    pub fn card_shown(&mut self) {
        self.last.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::acquire::{AcquisitionSource, ProtectedReason, ProviderInfo, TextTarget};
    use lens_core::geometry::ScreenRect;
    use lens_core::text;
    use lens_core::AppIdentity;

    fn target() -> Box<TextTarget> {
        Box::new(TextTarget {
            app: AppIdentity {
                process_name: Some("chrome".to_owned()),
                window_title: None,
            },
            source: AcquisitionSource::Accessibility,
            provider: ProviderInfo::default(),
            segmented: text::segment("A sentence.", 0),
            word_bounds: vec![ScreenRect::new(10, 10, 40, 18)],
            context_bounds: vec![ScreenRect::new(10, 10, 200, 18)],
        })
    }

    #[test]
    fn the_two_ordinary_outcomes_say_nothing() {
        assert_eq!(reason_for(&Resolution::Stale), None);
        assert_eq!(reason_for(&Resolution::Duplicate(target())), None);
        assert_eq!(reason_for(&Resolution::Presented(target())), None);
    }

    /// The one the first user hit: an application with nothing to read.
    #[test]
    fn an_application_that_exposes_nothing_says_so_in_words() {
        let reason = reason_for(&Resolution::Failed(AcquireError::Unsupported)).expect("a reason");
        assert!(reason.contains("does not expose"), "{reason}");
        // Not the taxonomy's own word, which is accurate and tells a reader
        // nothing.
        assert!(!reason.contains("unsupported"), "{reason}");
    }

    #[test]
    fn every_failure_a_user_can_meet_has_a_sentence() {
        for error in [
            AcquireError::Unsupported,
            AcquireError::Empty,
            AcquireError::Timeout,
            AcquireError::ElementGone,
            AcquireError::Unavailable,
            AcquireError::Protected(ProtectedReason::Declared),
        ] {
            assert!(
                reason_for(&Resolution::Failed(error)).is_some(),
                "{error:?} has no sentence",
            );
        }
    }

    #[test]
    fn the_application_is_named_and_the_text_is_not() {
        let mut narrator = Narrator::default();
        let line = narrator
            .line_for(&Resolution::Failed(AcquireError::Unsupported), Some("code"))
            .expect("a line");
        assert!(line.contains("[code]"), "{line}");
        assert!(!line.contains("A sentence"), "{line}");
    }

    /// A pointer parked on a toolbar resolves the same way forty times a
    /// second. It must produce one line, not forty.
    #[test]
    fn the_same_reason_is_said_once() {
        let mut narrator = Narrator::default();
        let resolution = Resolution::Failed(AcquireError::Unsupported);
        assert!(narrator.line_for(&resolution, Some("code")).is_some());
        for _ in 0..40 {
            assert_eq!(narrator.line_for(&resolution, Some("code")), None);
        }
    }

    #[test]
    fn moving_to_a_different_application_is_worth_saying_again() {
        let mut narrator = Narrator::default();
        let resolution = Resolution::Failed(AcquireError::Unsupported);
        assert!(narrator.line_for(&resolution, Some("code")).is_some());
        assert!(narrator.line_for(&resolution, Some("notepad")).is_some());
    }

    #[test]
    fn a_card_in_between_makes_the_next_silence_worth_reporting() {
        let mut narrator = Narrator::default();
        let resolution = Resolution::Failed(AcquireError::Unsupported);
        assert!(narrator.line_for(&resolution, Some("code")).is_some());
        assert_eq!(narrator.line_for(&resolution, Some("code")), None);
        narrator.card_shown();
        assert!(
            narrator.line_for(&resolution, Some("code")).is_some(),
            "after a card, the same silence is news again",
        );
    }

    /// Text already in the reader's language is the one quiet outcome that
    /// never reaches a `Resolution`, and it was the one with nothing to say.
    #[test]
    fn a_sentence_already_in_the_readers_language_is_explained_once() {
        let mut narrator = Narrator::default();
        let line = narrator
            .line_for_skip("already in Korean", Some("chrome"))
            .expect("a line");
        assert!(line.contains("already in Korean"), "{line}");
        assert!(line.contains("[chrome]"), "{line}");
        assert_eq!(
            narrator.line_for_skip("already in Korean", Some("chrome")),
            None
        );
    }
}
