//! When to tell a reader that this application cannot be read, and what to do.
//!
//! Lens has two ways to get text and only one of them is automatic. The
//! accessibility path runs on every settled hover; OCR runs when the user asks
//! for it (`docs/decisions/ADR-0002-accessibility-first-acquisition.md`). That
//! is the right default and it has one cost: in an application that exposes
//! nothing, **hovering does nothing, and nothing is what a broken product also
//! does**.
//!
//! Two applications make this concrete rather than theoretical. A sweep of VS
//! Code resolved 7 points out of 247, all of them label controls, and KakaoTalk
//! 14 out of 104 with one separable target. A developer and a messenger: the
//! two surfaces the first user spends the day in
//! (issues #14 and #34). In both, the product is silent and the reader has no
//! way to learn that a hotkey would have worked.
//!
//! There is a console line for this already, and it is not an answer: the
//! release binary is a Windows subsystem application, so somebody who launched
//! it from Explorer has no console to read it in.
//!
//! # Why this is a decision and not an `if`
//!
//! A card on every unreadable hover would be constant. A uniform pointer path
//! crosses margins, icons, images and chrome, and `Unsupported` is the correct
//! answer at every one of them; a product that complains each time has made
//! itself the noise. So the offer is made when the evidence is about the
//! **application** rather than about the pixel:
//!
//! - only after [`RUN_BEFORE_OFFERING`] settled hovers in a row failed the same
//!   way, in the same application
//! - once per application for the life of the session
//! - never again once that application has been read successfully even once,
//!   because then it was the pointer, not the surface
//!
//! # Only one failure is offered a fallback
//!
//! `Unsupported` means the provider has no text to give. Every other failure
//! is either transient or must not be worked around:
//!
//! - **`Protected` is never offered OCR, and this is the important one.**
//!   Reading a password field with a camera is precisely the thing the refusal
//!   exists to prevent, and an offer to do it is worse than useless: it is the
//!   product suggesting it. See `docs/privacy-model.md`.
//! - `Timeout`, `ElementGone`, `Stale` and `Unavailable` are conditions that
//!   pass. Offering a fallback for a slow frame teaches the reader a hotkey
//!   they did not need.
//! - `Empty` is what a margin returns, and a margin is not an unreadable
//!   application.

use crate::acquire::AcquireError;

/// How many settled hovers in a row must fail before the offer is made.
///
/// Three. One is a pixel, two is a coincidence, and three in the same
/// application without a single success between them is the surface. It is
/// counted in **settled** hovers rather than in samples, so this is three
/// deliberate rests of the pointer and not three ticks of a loop.
pub const RUN_BEFORE_OFFERING: u32 = 3;

/// Whether an application has proved it cannot be read, and has not been told.
#[derive(Debug, Default)]
pub struct Reach {
    /// The application the current run belongs to. A run does not survive
    /// moving to another window: three failures spread over three
    /// applications say nothing about any of them.
    current: Option<String>,
    run: u32,
    /// Applications already told, and applications that have been read
    /// successfully. Both mean "say nothing more about this one", for
    /// different reasons, and keeping them in one set is what stops a
    /// successful application being offered a fallback after a bad patch.
    settled: Vec<String>,
}

impl Reach {
    pub fn new() -> Self {
        Self::default()
    }

    /// A settled hover in `app` produced text.
    ///
    /// Ends any run, and takes the application out of the running for good: a
    /// surface that has been read once is readable, and a later failure is
    /// about where the pointer was.
    pub fn readable(&mut self, app: Option<&str>) {
        self.run = 0;
        self.current = None;
        if let Some(app) = app {
            self.remember(app);
        }
    }

    /// A settled hover in `app` produced nothing, for `reason`.
    ///
    /// Returns `true` exactly once per application, on the hover that completes
    /// the run. The caller shows the card; this decides.
    pub fn unreadable(&mut self, app: Option<&str>, reason: &AcquireError) -> bool {
        if !matches!(reason, AcquireError::Unsupported) {
            // Not evidence about the application. Deliberately does not reset
            // the run either: a timeout in the middle of three refusals should
            // not buy an unreadable window another three hovers of silence.
            return false;
        }
        let Some(app) = app else {
            // Nothing to attribute the run to, and nothing to remember it
            // against, so it could be offered forever.
            return false;
        };
        if self.settled.iter().any(|known| known == app) {
            return false;
        }
        if self.current.as_deref() != Some(app) {
            self.current = Some(app.to_owned());
            self.run = 0;
        }
        self.run += 1;
        if self.run < RUN_BEFORE_OFFERING {
            return false;
        }
        self.remember(app);
        self.run = 0;
        self.current = None;
        true
    }

    fn remember(&mut self, app: &str) {
        if !self.settled.iter().any(|known| known == app) {
            self.settled.push(app.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acquire::ProtectedReason;

    fn unsupported(reach: &mut Reach, app: &str) -> bool {
        reach.unreadable(Some(app), &AcquireError::Unsupported)
    }

    #[test]
    fn one_unreadable_hover_says_nothing() {
        let mut reach = Reach::new();
        assert!(!unsupported(&mut reach, "Code"));
    }

    #[test]
    fn three_in_a_row_offers_once_and_then_never_again() {
        let mut reach = Reach::new();
        assert!(!unsupported(&mut reach, "Code"));
        assert!(!unsupported(&mut reach, "Code"));
        assert!(
            unsupported(&mut reach, "Code"),
            "the third completes the run"
        );
        for _ in 0..10 {
            assert!(
                !unsupported(&mut reach, "Code"),
                "an application is told once a session, not once a hover",
            );
        }
    }

    #[test]
    fn a_run_does_not_carry_across_applications() {
        let mut reach = Reach::new();
        assert!(!unsupported(&mut reach, "Code"));
        assert!(!unsupported(&mut reach, "chrome"));
        assert!(!unsupported(&mut reach, "Code"), "the run restarted");
        assert!(!unsupported(&mut reach, "Code"));
        assert!(unsupported(&mut reach, "Code"));
    }

    #[test]
    fn an_application_that_has_been_read_is_never_offered_a_fallback() {
        // The pointer wandering onto a toolbar three times is not a reason to
        // tell somebody their working editor cannot be read.
        let mut reach = Reach::new();
        reach.readable(Some("Code"));
        for _ in 0..5 {
            assert!(!unsupported(&mut reach, "Code"));
        }
    }

    #[test]
    fn a_success_in_the_middle_of_a_run_ends_it() {
        let mut reach = Reach::new();
        assert!(!unsupported(&mut reach, "Code"));
        assert!(!unsupported(&mut reach, "Code"));
        reach.readable(Some("Code"));
        assert!(!unsupported(&mut reach, "Code"));
    }

    /// The one that matters most, and the one worth failing loudly.
    #[test]
    fn a_protected_field_is_never_offered_a_camera() {
        let mut reach = Reach::new();
        for _ in 0..20 {
            assert!(
                !reach.unreadable(
                    Some("KeePass"),
                    &AcquireError::Protected(ProtectedReason::Declared),
                ),
                "Lens must never suggest reading a protected field with OCR",
            );
        }
    }

    #[test]
    fn a_transient_failure_is_not_evidence_about_an_application() {
        let mut reach = Reach::new();
        for _ in 0..10 {
            assert!(!reach.unreadable(Some("Word"), &AcquireError::Timeout));
            assert!(!reach.unreadable(Some("Word"), &AcquireError::ElementGone));
            assert!(!reach.unreadable(Some("Word"), &AcquireError::Empty));
        }
    }

    #[test]
    fn a_transient_failure_does_not_reset_a_run_of_refusals() {
        // A window that exposes nothing and is also slow must not buy itself
        // another three hovers of silence by being slow at the right moment.
        let mut reach = Reach::new();
        assert!(!unsupported(&mut reach, "Code"));
        assert!(!reach.unreadable(Some("Code"), &AcquireError::Timeout));
        assert!(!unsupported(&mut reach, "Code"));
        assert!(unsupported(&mut reach, "Code"));
    }

    #[test]
    fn an_unnamed_application_is_never_offered_anything() {
        // There would be nothing to remember the offer against, so it would be
        // made on every third hover for the rest of the session.
        let mut reach = Reach::new();
        for _ in 0..10 {
            assert!(!reach.unreadable(None, &AcquireError::Unsupported));
        }
    }
}
