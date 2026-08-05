//! When a card appears, stays, and goes.
//!
//! Extracted from the main loop because it was the one part of this product
//! with no coverage at all, and because the only bug a user has reported here
//! lived in it and survived a green suite: the card was retired 25ms after
//! appearing, which combined with streaming produced a visible flicker.
//!
//! Every part the loop wires together, hover intent, acquisition, translation,
//! the overlay, has its own tests. The wiring had none. This is the wiring,
//! with the operating system, the model, and the window taken out so a
//! sequence of events can be played through it in microseconds.
//!
//! It decides; it does not act. The caller performs the returned [`Action`],
//! which is what lets the decisions be tested without a screen.

use std::time::Duration;

use lens_core::card::pointer_left_the_card;
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::Generation;

/// How long the pointer may sit outside a card's anchor before it is retired.
///
/// Paired with the distance rule rather than replacing it: crossing the gap
/// between two words takes the pointer outside the anchor for a few frames,
/// and a card that vanishes there is a card nobody finishes reading.
pub const DISMISS_GRACE: Duration = Duration::from_millis(250);

/// What the loop should do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Draw this answer at this anchor.
    Show,
    /// Retire the card.
    Hide,
}

/// The lifetime of the visible card.
#[derive(Debug)]
pub struct CardLifecycle {
    /// The rectangle the visible card explains, when one is visible.
    anchor: Option<ScreenRect>,
    /// When the pointer left that rectangle, if it has.
    left_at: Option<Duration>,
    /// The hover the loop is currently interested in.
    current: Generation,
}

impl Default for CardLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl CardLifecycle {
    pub fn new() -> Self {
        Self {
            anchor: None,
            left_at: None,
            // Generation 0 is the value the hover machine starts from, so an
            // answer can never be current before a hover has settled.
            current: Generation(0),
        }
    }

    /// Test-only, and marked so rather than left to look like API.
    ///
    /// The loop never asks what the lifecycle is holding: it performs the
    /// returned [`Action`] and nothing else. Exposing these unconditionally
    /// would invite a caller to reason about the state in parallel, which is
    /// how the two get out of step.
    #[cfg(test)]
    fn anchor(&self) -> Option<ScreenRect> {
        self.anchor
    }

    #[cfg(test)]
    fn showing(&self) -> bool {
        self.anchor.is_some()
    }

    /// A new hover settled, so answers for older ones stop being wanted.
    pub fn hover_settled(&mut self, generation: Generation) {
        self.current = generation;
    }

    /// An answer arrived. Should it be drawn?
    ///
    /// An answer whose generation is not current belongs to a hover the reader
    /// has already left, and drawing it would put a translation over a
    /// sentence they are not looking at.
    pub fn answer_arrived(&mut self, generation: Generation, anchor: ScreenRect) -> Action {
        if generation != self.current {
            return Action::Nothing;
        }
        self.anchor = Some(anchor);
        self.left_at = None;
        Action::Show
    }

    /// The pointer moved. Should the card go?
    pub fn pointer_moved(&mut self, point: ScreenPoint, now: Duration) -> Action {
        let Some(anchor) = self.anchor else {
            return Action::Nothing;
        };

        if !pointer_left_the_card(anchor, point) {
            // Back inside, or never left. Any grace already spent is
            // forgiven, so a hand wobbling on the boundary does not
            // accumulate its way to a dismissal.
            self.left_at = None;
            return Action::Nothing;
        }

        match self.left_at {
            None => {
                self.left_at = Some(now);
                Action::Nothing
            }
            Some(since) if now.saturating_sub(since) >= DISMISS_GRACE => {
                self.anchor = None;
                self.left_at = None;
                Action::Hide
            }
            Some(_) => Action::Nothing,
        }
    }

    /// Paused, or stopping. The card goes at once, with no grace.
    pub fn dismiss_now(&mut self) -> Action {
        if self.anchor.take().is_some() {
            self.left_at = None;
            return Action::Hide;
        }
        self.left_at = None;
        Action::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORD: ScreenRect = ScreenRect {
        x: 400,
        y: 300,
        width: 120,
        height: 22,
    };

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn showing() -> CardLifecycle {
        let mut life = CardLifecycle::new();
        life.hover_settled(Generation(1));
        assert_eq!(life.answer_arrived(Generation(1), WORD), Action::Show);
        life
    }

    /// The bug a user reported, as a test.
    #[test]
    fn a_pointer_resting_on_the_word_never_retires_the_card() {
        // The first version compared the pointer to the anchor's centre with a
        // 6px radius and hid the card on the next 25ms tick. With streaming,
        // each partial re-showed a card that was hidden again immediately.
        let mut life = showing();
        for tick in 0..40 {
            let action = life.pointer_moved(ScreenPoint::new(410, 310), ms(tick * 25));
            assert_eq!(action, Action::Nothing, "retired at tick {tick}");
        }
        assert!(life.showing());
    }

    #[test]
    fn a_streamed_answer_redrawing_does_not_restart_anything() {
        // Five partials for one hover, which is what streaming produces. Each
        // is a Show at the same anchor and none of them may be treated as a
        // new card or reset the reader's grace.
        let mut life = showing();
        for _ in 0..5 {
            assert_eq!(life.answer_arrived(Generation(1), WORD), Action::Show);
        }
        assert_eq!(life.anchor(), Some(WORD));
    }

    #[test]
    fn leaving_the_word_retires_the_card_only_after_the_grace() {
        let mut life = showing();
        let away = ScreenPoint::new(400, 900);

        assert_eq!(life.pointer_moved(away, ms(0)), Action::Nothing);
        assert_eq!(life.pointer_moved(away, ms(100)), Action::Nothing);
        assert_eq!(life.pointer_moved(away, ms(249)), Action::Nothing);
        assert_eq!(life.pointer_moved(away, ms(250)), Action::Hide);
        assert!(!life.showing());
    }

    #[test]
    fn coming_back_before_the_grace_expires_keeps_the_card() {
        // Crossing the gap between two words takes the pointer outside the
        // anchor for a few frames. That must not dismiss anything.
        let mut life = showing();
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(400, 900), ms(0)),
            Action::Nothing
        );
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(410, 310), ms(100)),
            Action::Nothing
        );
        // The grace restarts, so the old departure cannot expire later.
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(400, 900), ms(200)),
            Action::Nothing
        );
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(400, 900), ms(300)),
            Action::Nothing
        );
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(400, 900), ms(450)),
            Action::Hide
        );
    }

    #[test]
    fn wobbling_on_the_boundary_does_not_accumulate_into_a_dismissal() {
        // Out, in, out, in. Without resetting the grace on re-entry, enough
        // brief departures would eventually add up and retire a card the
        // reader never left.
        let mut life = showing();
        for tick in 0..20 {
            let point = if tick % 2 == 0 {
                ScreenPoint::new(400, 900)
            } else {
                ScreenPoint::new(410, 310)
            };
            assert_eq!(life.pointer_moved(point, ms(tick * 60)), Action::Nothing);
        }
        assert!(life.showing());
    }

    #[test]
    fn an_answer_for_a_hover_the_reader_left_is_never_drawn() {
        let mut life = CardLifecycle::new();
        life.hover_settled(Generation(7));
        // The pointer moved on before the model answered.
        life.hover_settled(Generation(8));
        assert_eq!(life.answer_arrived(Generation(7), WORD), Action::Nothing);
        assert!(!life.showing());
    }

    #[test]
    fn a_late_answer_does_not_replace_a_current_card() {
        // Generation 8 is showing. Generation 7's slow translation finally
        // arrives, anchored somewhere else entirely. It must not move the card.
        let mut life = CardLifecycle::new();
        life.hover_settled(Generation(8));
        assert_eq!(life.answer_arrived(Generation(8), WORD), Action::Show);

        let elsewhere = ScreenRect::new(1000, 900, 200, 22);
        assert_eq!(
            life.answer_arrived(Generation(7), elsewhere),
            Action::Nothing
        );
        assert_eq!(life.anchor(), Some(WORD), "a stale answer moved the card");
    }

    #[test]
    fn a_new_hover_moves_the_card_to_the_new_text() {
        let mut life = showing();
        let other = ScreenRect::new(700, 500, 90, 22);
        life.hover_settled(Generation(2));
        assert_eq!(life.answer_arrived(Generation(2), other), Action::Show);
        assert_eq!(life.anchor(), Some(other));
    }

    #[test]
    fn pausing_retires_the_card_without_waiting() {
        // Pause means the pointer stops being sampled, so no later movement
        // would ever arrive to expire a grace period.
        let mut life = showing();
        assert_eq!(life.dismiss_now(), Action::Hide);
        assert!(!life.showing());
    }

    #[test]
    fn dismissing_when_nothing_is_shown_asks_for_nothing() {
        let mut life = CardLifecycle::new();
        assert_eq!(life.dismiss_now(), Action::Nothing);
    }

    #[test]
    fn the_pointer_moving_with_no_card_asks_for_nothing() {
        let mut life = CardLifecycle::new();
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(0, 0), ms(9_000)),
            Action::Nothing,
        );
    }

    /// A whole hover, start to finish, as the loop would play it.
    #[test]
    fn one_reading_session_end_to_end() {
        let mut life = CardLifecycle::new();

        // The pointer settles on a word.
        life.hover_settled(Generation(1));
        // Streaming partials, then the finished answer.
        for _ in 0..4 {
            assert_eq!(life.answer_arrived(Generation(1), WORD), Action::Show);
        }
        // The reader reads it, moving a little.
        for tick in 0..10 {
            assert_eq!(
                life.pointer_moved(ScreenPoint::new(430 + tick, 305), ms(tick as u64 * 25)),
                Action::Nothing,
            );
        }
        // They look down at the card itself, which is below the text.
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(440, 360), ms(300)),
            Action::Nothing
        );
        // Then move away to another paragraph.
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(440, 800), ms(400)),
            Action::Nothing
        );
        assert_eq!(
            life.pointer_moved(ScreenPoint::new(440, 800), ms(700)),
            Action::Hide
        );
        assert!(!life.showing());
    }
}

/// Milestone 2's gate condition, composed rather than argued.
///
/// "Fast pointer movement produces no card spray." Two separate mechanisms are
/// supposed to prevent it and each is tested on its own elsewhere: the hover
/// machine does not settle while the pointer is moving, and this lifecycle
/// drops an answer whose generation is no longer current. Neither test says
/// anything about the two together, and the gate is about the two together.
///
/// So this runs the real `HoverSession` against a real `CardLifecycle`, with
/// only the operating system and the model replaced, and counts the cards a
/// user would have seen.
#[cfg(test)]
mod gate {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use lens_core::acquire::{
        AcquireContext, AcquireError, AcquisitionSource, ProviderInfo, TextAcquirer, TextTarget,
    };
    use lens_core::geometry::{ScreenPoint, ScreenRect};
    use lens_core::hover::{HoverConfig, HoverEvent};
    use lens_core::policy::{AppPolicy, AppTier};
    use lens_core::session::{HoverSession, InputState, Resolution};
    use lens_core::text;
    use lens_core::AppIdentity;

    use super::{Action, CardLifecycle};

    /// Always answers, and counts how often it was asked.
    struct AlwaysReads(Rc<Cell<usize>>);

    impl TextAcquirer for AlwaysReads {
        fn acquire(
            &self,
            point: ScreenPoint,
            _context: &AcquireContext,
        ) -> Result<TextTarget, AcquireError> {
            self.0.set(self.0.get() + 1);
            Ok(TextTarget {
                app: AppIdentity {
                    process_name: Some("chrome".to_owned()),
                    window_title: None,
                },
                source: AcquisitionSource::Accessibility,
                provider: ProviderInfo::default(),
                // A different sentence per position, so the deduplication that
                // suppresses a repeat hover cannot be what passes this test.
                segmented: text::segment(
                    &format!("Sentence at {} by {} on the page.", point.x, point.y),
                    0,
                ),
                word_bounds: vec![ScreenRect::new(point.x, point.y, 60, 18)],
                context_bounds: vec![ScreenRect::new(point.x, point.y, 400, 18)],
            })
        }
    }

    fn awake() -> InputState {
        InputState {
            button_down: false,
            scrolling: false,
            typing: false,
            modifier_held: false,
            ocr_requested: false,
        }
    }

    /// A hand sweeping across a page of prose, sampled the way the loop does.
    ///
    /// Every answer is delivered, late, exactly as a model returning a second
    /// after the fact would deliver it. The count that matters is how many
    /// cards were drawn.
    #[test]
    fn a_pointer_sweeping_across_a_page_draws_no_cards() {
        let calls = Rc::new(Cell::new(0));
        let mut policy = AppPolicy::default();
        policy.set("chrome", AppTier::Allow);
        let mut session = HoverSession::new(
            HoverConfig::default(),
            policy,
            Box::new(AlwaysReads(Rc::clone(&calls))),
        );
        let mut life = CardLifecycle::new();

        let mut shown = 0usize;
        let mut pending: Vec<(lens_core::hover::Generation, ScreenRect)> = Vec::new();

        // 60 samples at 25ms, 40px apart: a hand crossing a monitor in a second
        // and a half, which is an ordinary reach for a scrollbar.
        for step in 0..60u32 {
            let now = Duration::from_millis(u64::from(step) * 25);
            let point = ScreenPoint::new(100 + (step as i32) * 40, 300);
            let sample = session.sample(point, now, awake(), Some("chrome"));

            if let Some(HoverEvent::Settled { generation, point }) = session.observe(sample) {
                life.hover_settled(generation);
                if let Resolution::Presented(target) =
                    session.resolve(generation, point, now, awake())
                {
                    pending.push((
                        generation,
                        target
                            .anchor()
                            .unwrap_or(ScreenRect::new(point.x, point.y, 1, 1)),
                    ));
                }
            }

            // Answers arrive a beat later, which is when a real one would.
            if step > 2 {
                for (generation, anchor) in pending.drain(..) {
                    if life.answer_arrived(generation, anchor) == Action::Show {
                        shown += 1;
                    }
                }
            }

            if life.pointer_moved(point, now) == Action::Hide {
                // Retiring a card is not spray; it is the opposite.
            }
        }

        assert_eq!(
            calls.get(),
            0,
            "the pointer never rested, so nothing should have been read",
        );
        assert_eq!(
            shown, 0,
            "{shown} cards were drawn while the pointer was crossing the screen",
        );
    }

    /// The other half, and the one that is not obvious: the pointer stops.
    ///
    /// A reader who sweeps across five sentences and then settles on the sixth
    /// must get **one** card, for the sixth. Without the generation check they
    /// would get the answers to all six, arriving in whatever order the model
    /// finished them, each drawn over the last.
    #[test]
    fn stopping_after_a_sweep_yields_one_card_for_where_the_pointer_stopped() {
        let calls = Rc::new(Cell::new(0));
        let mut policy = AppPolicy::default();
        policy.set("chrome", AppTier::Allow);
        let mut session = HoverSession::new(
            HoverConfig::default(),
            policy,
            Box::new(AlwaysReads(Rc::clone(&calls))),
        );
        let mut life = CardLifecycle::new();

        let mut pending: Vec<(lens_core::hover::Generation, ScreenRect)> = Vec::new();
        let mut now = Duration::ZERO;
        let step = |session: &mut HoverSession,
                    life: &mut CardLifecycle,
                    point: ScreenPoint,
                    now: Duration,
                    pending: &mut Vec<_>| {
            let sample = session.sample(point, now, awake(), Some("chrome"));
            if let Some(HoverEvent::Settled { generation, point }) = session.observe(sample) {
                life.hover_settled(generation);
                if let Resolution::Presented(target) =
                    session.resolve(generation, point, now, awake())
                {
                    pending.push((
                        generation,
                        target
                            .anchor()
                            .unwrap_or(ScreenRect::new(point.x, point.y, 1, 1)),
                    ));
                }
            }
        };

        // Pause on six lines in turn, long enough for each to settle, so six
        // jobs really are outstanding. The pathological case: not a sweep, but
        // a reader skimming down a paragraph.
        //
        // 30 samples, because moving to a new target costs the 250ms leave
        // grace plus the 320ms dwell before it can settle again. At 20 samples
        // this test silently measured three hovers and called them six.
        for sentence in 0..6i32 {
            for _ in 0..30 {
                let point = ScreenPoint::new(200, 200 + sentence * 30);
                step(&mut session, &mut life, point, now, &mut pending);
                now += Duration::from_millis(25);
            }
        }

        assert_eq!(pending.len(), 6, "six hovers should have produced six jobs");
        let last = pending.last().expect("six jobs").0;

        // Now every answer comes back at once, oldest first, the way a queue
        // drains when a model finally catches up.
        let mut shown = Vec::new();
        for (generation, anchor) in pending {
            if life.answer_arrived(generation, anchor) == Action::Show {
                shown.push(generation);
            }
        }

        assert_eq!(
            shown,
            vec![last],
            "every answer but the newest must be dropped; drawing the rest is \
             a translation shown over a sentence the reader has left",
        );
    }
}
