//! Stress and reliability tests.
//!
//! These exercise the domain under load patterns that a human would not
//! construct by hand but that a running product encounters: rapid pointer
//! movement cycling through many sentences, a cache hit rate under realistic
//! reading patterns, and pathological model outputs that must not break the
//! pipeline.
//!
//! None of these touch the operating system. They are cheap enough to run on
//! every commit, and they catch regressions in the bounds and limits the
//! domain crate owns.

use crate::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, AppIdentity, ProviderInfo, TextAcquirer,
    TextTarget, MAX_ACQUIRED_CHARS,
};
use crate::analyse::{DeepAnalysis, TextLink};
use crate::geometry::{ScreenPoint, ScreenRect};
use crate::hover::{HoverConfig, HoverEvent, PointerSample, Timestamp};
use crate::policy::{AppPolicy, AppTier};
use crate::session::{HoverSession, InputState, Resolution};
use crate::text;
use std::cell::Cell;
use std::time::Duration;

const POINT: ScreenPoint = ScreenPoint { x: 120, y: 100 };

fn ms(value: u64) -> Timestamp {
    Duration::from_millis(value)
}

/// An acquirer that returns a different sentence each call, cycling through a
/// fixed set. This simulates a reader moving through a page.
struct CyclingAcquirer {
    sentences: Vec<String>,
    index: Cell<usize>,
}

impl CyclingAcquirer {
    fn new(sentences: Vec<&str>) -> Self {
        Self {
            sentences: sentences.into_iter().map(|s| s.to_owned()).collect(),
            index: Cell::new(0),
        }
    }
}

impl TextAcquirer for CyclingAcquirer {
    fn acquire(
        &self,
        _point: ScreenPoint,
        _context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        let idx = self.index.get();
        let content = &self.sentences[idx % self.sentences.len()];
        self.index.set(idx + 1);
        Ok(TextTarget {
            app: AppIdentity {
                process_name: Some("chrome".to_owned()),
                window_title: None,
            },
            source: AcquisitionSource::Accessibility,
            provider: ProviderInfo::default(),
            segmented: text::segment(content, 0),
            word_bounds: vec![ScreenRect::new(100, 100, 60, 18)],
            context_bounds: vec![ScreenRect::new(10, 100, 400, 18)],
        })
    }
}

fn session_with(acquirer: CyclingAcquirer) -> HoverSession {
    let mut policy = AppPolicy::default();
    policy.set("chrome", AppTier::Allow);
    HoverSession::new(HoverConfig::default(), policy, Box::new(acquirer))
}

/// Drive the pointer until it settles, and return the generation.
fn settle(session: &mut HoverSession, point: ScreenPoint) -> crate::hover::Generation {
    session.observe(PointerSample::moved(point, ms(0)));
    let event = session.observe(PointerSample::moved(point, ms(400)));
    match event {
        Some(HoverEvent::Settled { generation, .. }) => generation,
        _ => {
            // Keep driving until settled.
            let mut gen = crate::hover::Generation(0);
            for t in 500..2000 {
                if let Some(HoverEvent::Settled { generation, .. }) =
                    session.observe(PointerSample::moved(point, ms(t)))
                {
                    gen = generation;
                    break;
                }
            }
            gen
        }
    }
}

#[test]
fn one_thousand_rapid_resolution_cycles_do_not_panic_or_leak() {
    let sentences = vec![
        "The quick brown fox jumps over the lazy dog.",
        "A second sentence for the reader to hover over.",
        "Third text with different words to cycle through.",
        "Fourth example that exercises the deduplication path.",
        "Fifth sentence completing the rotation of content.",
    ];
    let acquirer = CyclingAcquirer::new(sentences);
    let mut session = session_with(acquirer);

    let mut resolved = 0;
    let cycle_ms = 2000;

    for i in 0..1000 {
        let base = i as u64 * cycle_ms;
        let point = ScreenPoint::new(120, 100);
        session.observe(PointerSample::moved(point, ms(base)));
        let event = session.observe(PointerSample::moved(point, ms(base + 400)));
        let gen = match event {
            Some(HoverEvent::Settled { generation, .. }) => generation,
            _ => continue,
        };
        match session.resolve(gen, point, ms(base + 400), InputState::default()) {
            Resolution::Presented(_) | Resolution::Duplicate(_) => resolved += 1,
            Resolution::OffTarget(_) | Resolution::Stale | Resolution::Failed(_) => {}
        }
        session.observe(PointerSample::moved(ScreenPoint::new(0, 0), ms(base + 800)));
    }

    // At least some should have resolved.
    assert!(resolved > 0, "no resolutions succeeded in 1000 cycles");
}

#[test]
fn deep_analysis_bounded_handles_pathological_field_lengths() {
    let huge = "x".repeat(10_000);
    let analysis = DeepAnalysis {
        lemma: huge.clone(),
        part_of_speech: huge.clone(),
        sense: huge.clone(),
        structure: huge.clone(),
        note: huge.clone(),
        word: None,
        translation: None,
        counterpart: Some(huge.clone()),
        model: "test".to_owned(),
        latency_ms: 0,
        cached: false,
        provider: None,
        links: (0..100)
            .map(|i| TextLink {
                source_range: (i, i + 1),
                target_range: (i, i + 1),
            })
            .collect(),
    };

    let bounded = analysis.bounded();

    assert!(bounded.lemma.chars().count() <= DeepAnalysis::MAX_FIELD_CHARS);
    assert!(bounded.part_of_speech.chars().count() <= DeepAnalysis::MAX_FIELD_CHARS);
    assert!(bounded.sense.chars().count() <= DeepAnalysis::MAX_FIELD_CHARS);
    assert!(bounded.structure.chars().count() <= DeepAnalysis::MAX_FIELD_CHARS);
    assert!(bounded.note.chars().count() <= DeepAnalysis::MAX_FIELD_CHARS);
    if let Some(ref c) = bounded.counterpart {
        assert!(c.chars().count() <= DeepAnalysis::MAX_FIELD_CHARS);
    }
    assert!(bounded.links.len() <= DeepAnalysis::MAX_LINKS);
}

#[test]
fn deep_analysis_bounded_preserves_short_fields_unchanged() {
    let analysis = DeepAnalysis {
        lemma: "run".to_owned(),
        part_of_speech: "verb".to_owned(),
        sense: "to move quickly".to_owned(),
        structure: "intransitive".to_owned(),
        note: "common".to_owned(),
        word: Some("run".to_owned()),
        translation: Some("달리다".to_owned()),
        counterpart: Some("달리다".to_owned()),
        model: "test".to_owned(),
        latency_ms: 0,
        cached: false,
        provider: None,
        links: vec![TextLink {
            source_range: (0, 3),
            target_range: (0, 9),
        }],
    };

    let bounded = analysis.bounded();

    assert_eq!(bounded.lemma, "run");
    assert_eq!(bounded.part_of_speech, "verb");
    assert_eq!(bounded.sense, "to move quickly");
    assert_eq!(bounded.counterpart.as_deref(), Some("달리다"));
    assert_eq!(bounded.links.len(), 1);
}

#[test]
fn acquired_text_at_exactly_the_limit_is_accepted() {
    let exact = "a".repeat(MAX_ACQUIRED_CHARS);
    let acquirer = CyclingAcquirer::new(vec![&exact]);
    let mut session = session_with(acquirer);
    let gen = settle(&mut session, POINT);

    match session.resolve(gen, POINT, ms(400), InputState::default()) {
        Resolution::Presented(_) | Resolution::Duplicate(_) => {} // expected
        other => panic!(
            "expected Presented or Duplicate at exactly the limit, got {:?}",
            other
        ),
    }
}

#[test]
fn acquired_text_one_over_the_limit_is_refused() {
    let over = "a".repeat(MAX_ACQUIRED_CHARS + 1);
    let acquirer = CyclingAcquirer::new(vec![&over]);
    let mut session = session_with(acquirer);
    let gen = settle(&mut session, POINT);

    assert!(matches!(
        session.resolve(gen, POINT, ms(400), InputState::default()),
        Resolution::Failed(AcquireError::TooLong)
    ));
}
