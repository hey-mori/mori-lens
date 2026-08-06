//! When to tell a reader why nothing happened, and what to press.
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
//!
//! # The other silence, and it is the commoner one
//!
//! Everything above is about an application with no text to give. The far more
//! likely first experience is an application with plenty: **the reader is not
//! holding the modifier.**
//!
//! The default tier is `Modifier`, deliberately, because a tool that reads
//! every window from first launch has to earn that. So a new user starts Lens,
//! rests the pointer on a sentence, and nothing happens. There is a console
//! line for it, and `crate::acquire` records that the first person to run a
//! release build asked exactly this question. A release binary is a Windows
//! subsystem application: somebody who double-clicked it from Explorer has
//! nowhere to read that line.
//!
//! The evidence for saying something is the same shape as above, and the
//! conditions are what keep it from being a nag:
//!
//! - the pointer is **resting**, not passing through
//! - where a card would have appeared if the modifier were held, so it is
//!   never shown over a denied application or an empty desktop
//! - **twice**, because one pause is a person reading
//! - and **never once a card has been shown**, because then they know

use std::time::Duration;

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
    /// Whether any card at all has been shown this session.
    seen_a_card: bool,
    /// Where the pointer has been resting, and since when.
    rest: Option<(crate::geometry::ScreenPoint, Duration)>,
    /// How many separate rests have happened with the modifier not held.
    rests: u32,
    told_about_the_modifier: bool,
}

/// How long the pointer must be still before it counts as a rest.
///
/// Longer than the dwell that triggers a card, on purpose. The dwell asks
/// "does this person want this sentence"; this asks "is this person waiting
/// for something", and waiting takes longer than pointing.
const REST: Duration = Duration::from_millis(900);

/// How far the pointer may drift and still be the same rest.
///
/// A little more than the hover machine's own radius. A hand that is holding
/// still is not holding perfectly still, and a rest that restarts on every
/// tremor never completes.
const REST_RADIUS: f64 = 12.0;

/// How many rests before the modifier is mentioned.
///
/// Two. One pause is somebody reading the sentence under their pointer, which
/// is the normal way to use a screen and not a request for help.
pub const RESTS_BEFORE_TELLING: u32 = 2;

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

    /// A card was shown, of any kind.
    ///
    /// Ends the first-run hint for the session: a reader who has seen one card
    /// knows the product works, and telling them how to make it work is then
    /// just noise about something they already did.
    pub fn card_shown(&mut self) {
        self.seen_a_card = true;
    }

    /// The pointer moved, and the modifier is the only thing stopping a read.
    ///
    /// `blocked_by_the_modifier` is the caller's answer to a question only the
    /// policy and the keyboard can settle together: would this point be read if
    /// the modifier were held, and is it not being read because it is not.
    /// False everywhere else, including over a denied application, over Lens's
    /// own card, and when the modifier **is** held.
    ///
    /// Returns `true` exactly once per session, on the second rest.
    pub fn pointer(
        &mut self,
        point: crate::geometry::ScreenPoint,
        now: Duration,
        blocked_by_the_modifier: bool,
    ) -> bool {
        if !blocked_by_the_modifier || self.seen_a_card || self.told_about_the_modifier {
            self.rest = None;
            return false;
        }

        match self.rest {
            // Moved far enough to be somewhere else. A new rest starts here
            // rather than the old one continuing, or a pointer wandering across
            // a page would accumulate a rest it never actually took.
            Some((anchor, _)) if anchor.distance_to(point) > REST_RADIUS => {
                self.rest = Some((point, now));
            }
            Some((_, since)) if now.saturating_sub(since) >= REST => {
                self.rest = Some((point, now));
                self.rests += 1;
                if self.rests >= RESTS_BEFORE_TELLING {
                    self.told_about_the_modifier = true;
                    return true;
                }
            }
            Some(_) => {}
            None => self.rest = Some((point, now)),
        }
        false
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

#[cfg(test)]
mod modifier_tests {
    use super::*;
    use crate::geometry::ScreenPoint;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    /// Rest twice in one place, and be told once.
    #[test]
    fn two_rests_without_the_modifier_says_something_once() {
        let mut reach = Reach::new();
        let here = ScreenPoint::new(400, 300);
        assert!(!reach.pointer(here, ms(0), true));
        assert!(
            !reach.pointer(here, ms(950), true),
            "the first rest only counts"
        );
        assert!(reach.pointer(here, ms(1900), true), "the second says it");
        for step in 0..10 {
            assert!(
                !reach.pointer(here, ms(2900 + step * 950), true),
                "a hint is shown once a session, not once a rest",
            );
        }
    }

    /// A pointer crossing the screen is not resting.
    #[test]
    fn a_moving_pointer_never_rests() {
        let mut reach = Reach::new();
        for step in 0..40i32 {
            let point = ScreenPoint::new(100 + step * 40, 300);
            assert!(
                !reach.pointer(point, ms(step as u64 * 950), true),
                "step {step} counted a rest for a pointer that moved 40px",
            );
        }
    }

    /// A hand holding still is not holding perfectly still.
    #[test]
    fn a_small_tremor_does_not_restart_the_rest() {
        let mut reach = Reach::new();
        assert!(!reach.pointer(ScreenPoint::new(400, 300), ms(0), true));
        assert!(!reach.pointer(ScreenPoint::new(404, 302), ms(950), true));
        assert!(reach.pointer(ScreenPoint::new(398, 297), ms(1900), true));
    }

    /// Never over an application the modifier would not have unlocked.
    #[test]
    fn nothing_is_said_where_the_modifier_is_not_the_reason() {
        let mut reach = Reach::new();
        let here = ScreenPoint::new(400, 300);
        for step in 0..10 {
            assert!(!reach.pointer(here, ms(step * 950), false));
        }
    }

    /// A reader who has seen a card knows it works.
    #[test]
    fn a_card_ends_the_hint_for_the_session() {
        let mut reach = Reach::new();
        let here = ScreenPoint::new(400, 300);
        assert!(!reach.pointer(here, ms(0), true));
        reach.card_shown();
        for step in 1..10 {
            assert!(!reach.pointer(here, ms(step * 950), true));
        }
    }

    /// Holding the modifier clears the run, so putting it down later starts
    /// again rather than finishing a rest taken before it was held.
    #[test]
    fn holding_the_modifier_ends_a_rest_in_progress() {
        let mut reach = Reach::new();
        let here = ScreenPoint::new(400, 300);
        assert!(!reach.pointer(here, ms(0), true));
        assert!(
            !reach.pointer(here, ms(500), false),
            "the modifier went down"
        );
        assert!(
            !reach.pointer(here, ms(1000), true),
            "so this is a new rest"
        );
        assert!(!reach.pointer(here, ms(1950), true));
        assert!(reach.pointer(here, ms(2900), true));
    }
}
