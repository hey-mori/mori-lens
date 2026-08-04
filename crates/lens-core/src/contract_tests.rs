//! Contract tests over fake acquirers.
//!
//! These prove the ordering rules without an operating system. They are the
//! only place the "accessibility first, OCR fallback" promise is checked as
//! behaviour rather than as prose, and they are cheap enough to run on every
//! commit, which the real adapters never will be.
//!
//! What they deliberately do **not** prove: that UI Automation works. Nothing
//! here touches it. That is the probe's job
//! (`docs/decisions/ADR-0008-probe-before-product.md`).

use crate::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, AppIdentity, ProtectedReason, ProviderInfo,
    TextAcquirer, TextTarget,
};
use crate::geometry::{ScreenPoint, ScreenRect};
use crate::hover::{HoverConfig, HoverEvent, PointerSample, Timestamp};
use crate::policy::{AppPolicy, AppTier};
use crate::session::{HoverSession, InputState, OcrMode, Resolution};
use crate::text;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// An acquirer that answers however the test tells it to, and counts calls.
struct FakeAcquirer {
    source: AcquisitionSource,
    answer: Result<&'static str, AcquireError>,
    app: Option<&'static str>,
    calls: Rc<Cell<usize>>,
}

impl FakeAcquirer {
    fn reading(source: AcquisitionSource, text: &'static str) -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                source,
                answer: Ok(text),
                app: Some("chrome"),
                calls: Rc::clone(&calls),
            },
            calls,
        )
    }

    fn failing(source: AcquisitionSource, error: AcquireError) -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                source,
                answer: Err(error),
                app: Some("chrome"),
                calls: Rc::clone(&calls),
            },
            calls,
        )
    }
}

impl TextAcquirer for FakeAcquirer {
    fn acquire(
        &self,
        _point: ScreenPoint,
        _context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        self.calls.set(self.calls.get() + 1);
        let content = self.answer?;
        Ok(TextTarget {
            app: AppIdentity {
                process_name: self.app.map(str::to_owned),
                window_title: None,
            },
            source: self.source,
            provider: ProviderInfo::default(),
            segmented: text::segment(content, 0),
            word_bounds: vec![ScreenRect::new(100, 100, 60, 18)],
            context_bounds: vec![ScreenRect::new(10, 100, 400, 18)],
        })
    }
}

fn ms(value: u64) -> Timestamp {
    Duration::from_millis(value)
}

/// A session that hovers automatically in `chrome`, so the tests are about
/// routing rather than about the policy default.
fn session(accessibility: FakeAcquirer) -> HoverSession {
    let mut policy = AppPolicy::default();
    policy.set("chrome", AppTier::Allow);
    HoverSession::new(HoverConfig::default(), policy, Box::new(accessibility))
}

/// Drive the pointer until it settles, and return the generation.
fn settle(session: &mut HoverSession, point: ScreenPoint) -> crate::hover::Generation {
    session.observe(PointerSample::moved(point, ms(0)));
    let event = session.observe(PointerSample::moved(point, ms(400)));
    match event {
        Some(HoverEvent::Settled { generation, .. }) => generation,
        other => panic!("expected the pointer to settle, got {other:?}"),
    }
}

const POINT: ScreenPoint = ScreenPoint::new(120, 108);

#[test]
fn a_successful_accessibility_read_never_reaches_ocr() {
    let (accessibility, uia_calls) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "Hello there.");
    let (ocr, ocr_calls) = FakeAcquirer::reading(AcquisitionSource::Ocr, "should not run");

    let mut session = session(accessibility).with_ocr(Box::new(ocr), OcrMode::Automatic);
    let generation = settle(&mut session, POINT);

    let resolution = session.resolve(generation, POINT, ms(400), InputState::default());

    assert!(matches!(resolution, Resolution::Presented(_)));
    assert_eq!(uia_calls.get(), 1);
    assert_eq!(ocr_calls.get(), 0, "OCR ran despite a successful read");
}

#[test]
fn a_repairable_failure_reaches_ocr_when_it_is_automatic() {
    let (accessibility, _) =
        FakeAcquirer::failing(AcquisitionSource::Accessibility, AcquireError::Unsupported);
    let (ocr, ocr_calls) = FakeAcquirer::reading(AcquisitionSource::Ocr, "Read from pixels.");

    let mut session = session(accessibility).with_ocr(Box::new(ocr), OcrMode::Automatic);
    let generation = settle(&mut session, POINT);

    let resolution = session.resolve(generation, POINT, ms(400), InputState::default());

    assert!(matches!(resolution, Resolution::Presented(_)));
    assert_eq!(ocr_calls.get(), 1);
}

#[test]
fn manual_ocr_does_not_run_unless_the_user_asked() {
    let (accessibility, _) =
        FakeAcquirer::failing(AcquisitionSource::Accessibility, AcquireError::Unsupported);
    let (ocr, ocr_calls) = FakeAcquirer::reading(AcquisitionSource::Ocr, "Read from pixels.");

    let mut session = session(accessibility).with_ocr(Box::new(ocr), OcrMode::Manual);
    let generation = settle(&mut session, POINT);

    let unasked = session.resolve(generation, POINT, ms(400), InputState::default());
    assert!(matches!(
        unasked,
        Resolution::Failed(AcquireError::Unsupported)
    ));
    assert_eq!(ocr_calls.get(), 0, "manual OCR ran without being asked");

    let asked = session.resolve(
        generation,
        POINT,
        ms(500),
        InputState {
            ocr_requested: true,
            ..Default::default()
        },
    );
    assert!(matches!(asked, Resolution::Presented(_)));
    assert_eq!(ocr_calls.get(), 1);
}

#[test]
fn a_protected_field_never_reaches_ocr() {
    // The rule with teeth: OCR would read a password field's rendered
    // characters, so a refusal must not be routed to it even with OCR fully on.
    for reason in [ProtectedReason::Declared, ProtectedReason::Unprovable] {
        let (accessibility, _) = FakeAcquirer::failing(
            AcquisitionSource::Accessibility,
            AcquireError::Protected(reason),
        );
        let (ocr, ocr_calls) = FakeAcquirer::reading(AcquisitionSource::Ocr, "the password");

        let mut session = session(accessibility).with_ocr(Box::new(ocr), OcrMode::Automatic);
        let generation = settle(&mut session, POINT);

        let resolution = session.resolve(
            generation,
            POINT,
            ms(400),
            InputState {
                ocr_requested: true,
                ..Default::default()
            },
        );

        assert!(matches!(
            resolution,
            Resolution::Failed(AcquireError::Protected(_))
        ));
        assert_eq!(ocr_calls.get(), 0, "{reason:?} was routed to OCR");
    }
}

#[test]
fn an_unrepairable_failure_is_reported_as_itself() {
    // Not rewritten into a generic error: the matrix records what the provider
    // did, and "timeout" and "unsupported" call for different responses.
    let (accessibility, _) =
        FakeAcquirer::failing(AcquisitionSource::Accessibility, AcquireError::Timeout);
    let (ocr, ocr_calls) = FakeAcquirer::reading(AcquisitionSource::Ocr, "irrelevant");

    let mut session = session(accessibility).with_ocr(Box::new(ocr), OcrMode::Automatic);
    let generation = settle(&mut session, POINT);

    let resolution = session.resolve(generation, POINT, ms(400), InputState::default());

    assert!(matches!(
        resolution,
        Resolution::Failed(AcquireError::Timeout)
    ));
    assert_eq!(ocr_calls.get(), 0);
}

#[test]
fn a_denied_application_is_never_acquired_from() {
    let (accessibility, calls) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "master password");
    let mut session = session(accessibility);

    // Two seconds of a perfectly still pointer over a password manager.
    for step in 0..60u64 {
        let sample = session.sample(
            POINT,
            ms(step * 33),
            InputState {
                modifier_held: true,
                ..Default::default()
            },
            Some("1Password"),
        );
        assert_eq!(session.observe(sample), None, "settled over a denied app");
    }

    assert_eq!(calls.get(), 0);
}

#[test]
fn the_default_policy_needs_a_modifier_in_an_unlisted_application() {
    let (accessibility, calls) = FakeAcquirer::reading(AcquisitionSource::Accessibility, "text");
    let mut session = HoverSession::new(
        HoverConfig::default(),
        AppPolicy::default(),
        Box::new(accessibility),
    );

    for step in 0..40u64 {
        let sample = session.sample(POINT, ms(step * 33), InputState::default(), Some("notepad"));
        assert_eq!(session.observe(sample), None);
    }
    assert_eq!(calls.get(), 0, "hovered automatically in an unlisted app");

    // Same pointer, modifier held.
    let held = InputState {
        modifier_held: true,
        ..Default::default()
    };
    session.observe(session.sample(POINT, ms(2000), held, Some("notepad")));
    let event = session.observe(session.sample(POINT, ms(2400), held, Some("notepad")));
    assert!(matches!(event, Some(HoverEvent::Settled { .. })));
}

#[test]
fn a_result_for_a_superseded_generation_is_never_presented() {
    let (accessibility, _) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "The first sentence.");
    let mut session = session(accessibility);
    let first = settle(&mut session, POINT);

    // The pointer leaves and settles elsewhere before the answer arrives.
    let elsewhere = ScreenPoint::new(900, 900);
    session.observe(PointerSample::moved(elsewhere, ms(500)));
    session.observe(PointerSample::moved(elsewhere, ms(800)));
    session.observe(PointerSample::moved(elsewhere, ms(1200)));

    assert!(matches!(
        session.resolve(first, POINT, ms(1300), InputState::default()),
        Resolution::Stale
    ));
}

#[test]
fn the_same_sentence_twice_is_a_duplicate_not_a_second_answer() {
    let (accessibility, calls) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "The same sentence.");
    let mut session = session(accessibility);
    let generation = settle(&mut session, POINT);

    assert!(matches!(
        session.resolve(generation, POINT, ms(400), InputState::default()),
        Resolution::Presented(_)
    ));
    assert!(matches!(
        session.resolve(generation, POINT, ms(600), InputState::default()),
        Resolution::Duplicate(_)
    ));
    // Acquisition still ran: dedup happens after the read, because the
    // fingerprint is not knowable before it.
    assert_eq!(calls.get(), 2);
}

#[test]
fn text_a_screen_away_is_suppressed_rather_than_shown() {
    // The fake resolves a word rectangle at (100, 100, 60, 18). A pointer far
    // from it is over empty space, and inside a document RangeFromPoint would
    // still have answered successfully, so this gate is the only thing that
    // catches it (ADR-0009).
    let (accessibility, _) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "A distant sentence.");
    let mut session = session(accessibility);

    let far = ScreenPoint::new(900, 700);
    session.observe(PointerSample::moved(far, ms(0)));
    let Some(HoverEvent::Settled { generation, point }) =
        session.observe(PointerSample::moved(far, ms(400)))
    else {
        panic!("expected the pointer to settle");
    };

    assert!(matches!(
        session.resolve(generation, point, ms(400), InputState::default()),
        Resolution::OffTarget(_)
    ));
}

#[test]
fn a_suppressed_target_does_not_block_the_next_real_hover() {
    // An off-target resolution must not take the "last presented" slot. The
    // user was shown nothing, so the same sentence resolved properly a moment
    // later has to be presented, not dismissed as a duplicate of something
    // they never saw.
    let (accessibility, _) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "The same sentence.");
    let mut session = session(accessibility);

    let far = ScreenPoint::new(900, 700);
    session.observe(PointerSample::moved(far, ms(0)));
    let Some(HoverEvent::Settled { generation, point }) =
        session.observe(PointerSample::moved(far, ms(400)))
    else {
        panic!("expected the pointer to settle");
    };
    assert!(matches!(
        session.resolve(generation, point, ms(400), InputState::default()),
        Resolution::OffTarget(_)
    ));

    // Now a hover that lands on the word.
    assert!(matches!(
        session.resolve(generation, POINT, ms(500), InputState::default()),
        Resolution::Presented(_)
    ));
}

#[test]
fn a_target_carries_its_anchor_and_its_distance_from_the_pointer() {
    let (accessibility, _) =
        FakeAcquirer::reading(AcquisitionSource::Accessibility, "Anchored text.");
    let mut session = session(accessibility);
    let generation = settle(&mut session, POINT);

    let Resolution::Presented(target) =
        session.resolve(generation, POINT, ms(400), InputState::default())
    else {
        panic!("expected a presented target");
    };

    assert_eq!(target.anchor(), Some(ScreenRect::new(100, 100, 60, 18)));
    // The pointer is inside the word rectangle, so the distance is zero and the
    // caller has no reason to distrust the resolution.
    assert_eq!(target.pointer_distance(POINT), Some(0.0));
    // A pointer far out in the margin resolves to the same nearest text, which
    // is exactly the case a threshold has to catch. Measured against the word
    // rectangle (right edge 160), not the wider context rectangle: the word is
    // what would be explained.
    assert_eq!(
        target.pointer_distance(ScreenPoint::new(600, 108)),
        Some(441.0)
    );
}
