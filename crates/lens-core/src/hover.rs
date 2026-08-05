//! Hover intent: deciding that the user is reading, before spending anything.
//!
//! This is the module that makes the product cheap. Everything downstream
//! (an accessibility call, a model request, a capture) happens only after this
//! machine says a pointer has *settled*, and stops mattering the moment it says
//! the pointer has moved on.
//!
//! It is deliberately pure: no clock, no threads, no OS types. Time arrives as
//! a monotonic [`Timestamp`] on each sample, so every rule here is testable
//! without waiting for it.

use crate::geometry::ScreenPoint;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

/// Monotonic time since the process started.
///
/// Not `Instant`, because an `Instant` cannot be constructed at an arbitrary
/// value and the tests would then need a real clock to describe a 320ms dwell.
pub type Timestamp = Duration;

/// Identifies one hover attempt.
///
/// Every asynchronous result carries the generation it was requested under and
/// is discarded if that generation is no longer current. This is not an
/// optimisation: without it, a translation that arrives after the pointer moved
/// gets drawn over a sentence it does not belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

/// Identifies *what* was resolved, so the same target is not answered twice.
///
/// Built from the application and the resolved text rather than from the
/// pointer position, because a user re-reading the same sentence moves the
/// pointer by a few pixels and means nothing by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetFingerprint(u64);

impl TargetFingerprint {
    /// Hashes the identifying parts. The text is hashed, never retained: a
    /// fingerprint that could be read back would be a record of what the user
    /// looked at (`docs/privacy-model.md` §3).
    pub fn of(app: Option<&str>, text: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        app.unwrap_or("").hash(&mut hasher);
        text.trim().hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Tuning for the intent rules.
///
/// The defaults are the starting values from
/// `docs/research/hover-text-acquisition.md` §3. They are a place to measure
/// from, not a claim about what feels right; that answer comes from M1.
#[derive(Debug, Clone, Copy)]
pub struct HoverConfig {
    /// How long the pointer must rest before anything is spent on it.
    pub dwell: Duration,
    /// How far it may drift during the dwell and still count as resting.
    pub movement_radius_px: f64,
    /// How long a settled target survives the pointer leaving, so crossing a
    /// gap between words does not tear the card down and rebuild it.
    pub hide_grace: Duration,
    /// How long the same target stays answered before it is answered again.
    pub same_target_cooldown: Duration,
}

impl Default for HoverConfig {
    fn default() -> Self {
        Self {
            dwell: Duration::from_millis(320),
            movement_radius_px: 6.0,
            hide_grace: Duration::from_millis(250),
            same_target_cooldown: Duration::from_millis(1000),
        }
    }
}

/// One observation of the pointer and the input state around it.
#[derive(Debug, Clone, Copy)]
pub struct PointerSample {
    pub point: ScreenPoint,
    pub at: Timestamp,
    /// A mouse button is down: the user is clicking or dragging, not reading.
    pub button_down: bool,
    /// The view is scrolling, so whatever is under the pointer is moving.
    pub scrolling: bool,
    /// The user is typing. Their attention is on the caret, not the pointer.
    pub typing: bool,
    /// The pointer is somewhere Lens must not read: its own overlay, a denied
    /// application, or a surface the policy puts behind a modifier that is not
    /// held. Decided by the caller, because it needs the OS to answer it.
    pub suppressed: bool,
}

impl PointerSample {
    /// A plain move with no cancelling input. Convenience for tests and for
    /// callers that do not track keyboard state yet.
    pub fn moved(point: ScreenPoint, at: Timestamp) -> Self {
        Self {
            point,
            at,
            button_down: false,
            scrolling: false,
            typing: false,
            suppressed: false,
        }
    }

    fn cancels(self) -> bool {
        self.button_down || self.scrolling || self.typing || self.suppressed
    }
}

/// What the machine wants the rest of the system to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverEvent {
    /// The pointer settled. Resolve this point, under this generation.
    Settled {
        generation: Generation,
        point: ScreenPoint,
    },
    /// The target is gone. Hide anything shown for this generation.
    Left { generation: Generation },
}

/// Whether a resolved result should be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    /// Current and new. Show it.
    Present,
    /// The pointer moved while this was being resolved. Drop it silently.
    Stale,
    /// The same target was answered within the cooldown. Keep what is shown.
    Duplicate,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    /// Resting, but not yet long enough.
    Candidate {
        anchor: ScreenPoint,
        since: Timestamp,
    },
    /// Settled and dispatched. The result may not have arrived yet.
    Settled {
        generation: Generation,
        anchor: ScreenPoint,
    },
    /// Settled, but the pointer has stepped outside the radius. Held for the
    /// grace period so a small excursion does not tear the target down.
    Leaving {
        generation: Generation,
        anchor: ScreenPoint,
        since: Timestamp,
    },
}

/// The hover intent state machine.
///
/// ```text
/// IDLE ──rest──▶ CANDIDATE ──dwell──▶ SETTLED ──drift──▶ LEAVING
///   ▲                │                    ▲                 │
///   └────────────────┴────────────────────┴──── grace ──────┘
/// ```
#[derive(Debug)]
pub struct HoverMachine {
    config: HoverConfig,
    state: State,
    generation: Generation,
    last_presented: Option<(TargetFingerprint, Timestamp)>,
}

impl HoverMachine {
    pub fn new(config: HoverConfig) -> Self {
        Self {
            config,
            state: State::Idle,
            generation: Generation(0),
            last_presented: None,
        }
    }

    pub fn config(&self) -> &HoverConfig {
        &self.config
    }

    /// The generation any in-flight work should be checked against.
    /// Take the next generation without a hover having settled.
    ///
    /// For work a keypress starts rather than a pointer: manual OCR. It has to
    /// be cancellable the same way, which means it needs a generation, and
    /// taking one correctly makes every hover in flight stale.
    pub fn take_generation(&mut self) -> Generation {
        self.generation = Generation(self.generation.0 + 1);
        self.generation
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    /// Whether a result requested under `generation` is still wanted.
    pub fn is_current(&self, generation: Generation) -> bool {
        generation == self.generation
            && matches!(self.state, State::Settled { .. } | State::Leaving { .. })
    }

    /// Feed one pointer observation. At most one event comes back per sample.
    /// Change the dwell and the radii without discarding the current hover.
    ///
    /// The state machine's state is deliberately kept. A dwell changed from
    /// 320ms to 400ms should apply to the next hover, not cancel the one the
    /// reader is looking at.
    pub fn reconfigure(&mut self, config: HoverConfig) {
        self.config = config;
    }

    pub fn observe(&mut self, sample: PointerSample) -> Option<HoverEvent> {
        if sample.cancels() {
            return self.abandon();
        }

        match self.state {
            State::Idle => {
                self.state = State::Candidate {
                    anchor: sample.point,
                    since: sample.at,
                };
                None
            }

            State::Candidate { anchor, since } => {
                if anchor.distance_to(sample.point) > self.config.movement_radius_px {
                    // Moved on. This is a new candidate, and the dwell restarts
                    // from here rather than accumulating across the movement.
                    self.state = State::Candidate {
                        anchor: sample.point,
                        since: sample.at,
                    };
                    return None;
                }
                if sample.at.saturating_sub(since) < self.config.dwell {
                    return None;
                }
                self.generation = Generation(self.generation.0 + 1);
                self.state = State::Settled {
                    generation: self.generation,
                    anchor,
                };
                Some(HoverEvent::Settled {
                    generation: self.generation,
                    // The anchor, not the latest sample: the anchor is where the
                    // user actually stopped, and the sample may be a few pixels
                    // of drift away from the word they meant.
                    point: anchor,
                })
            }

            State::Settled { generation, anchor } => {
                if anchor.distance_to(sample.point) > self.config.movement_radius_px {
                    self.state = State::Leaving {
                        generation,
                        anchor,
                        since: sample.at,
                    };
                }
                None
            }

            State::Leaving {
                generation,
                anchor,
                since,
            } => {
                if anchor.distance_to(sample.point) <= self.config.movement_radius_px {
                    // Came back inside. Nothing was hidden, so nothing is shown
                    // again: the target never stopped being current.
                    self.state = State::Settled { generation, anchor };
                    return None;
                }
                if sample.at.saturating_sub(since) < self.config.hide_grace {
                    return None;
                }
                self.state = State::Candidate {
                    anchor: sample.point,
                    since: sample.at,
                };
                Some(HoverEvent::Left { generation })
            }
        }
    }

    /// Decide whether a resolved target should be shown.
    ///
    /// Call this when a result arrives, never before: the fingerprint is not
    /// knowable until the text has been read.
    pub fn should_present(
        &mut self,
        generation: Generation,
        fingerprint: TargetFingerprint,
        now: Timestamp,
    ) -> Presentation {
        if !self.is_current(generation) {
            return Presentation::Stale;
        }
        if let Some((last, at)) = self.last_presented {
            if last == fingerprint && now.saturating_sub(at) < self.config.same_target_cooldown {
                // Refresh the timer so re-reading one sentence for a while does
                // not re-request it every cooldown period.
                self.last_presented = Some((last, now));
                return Presentation::Duplicate;
            }
        }
        self.last_presented = Some((fingerprint, now));
        Presentation::Present
    }

    /// Drop out of any settled state, emitting a hide if something was shown.
    fn abandon(&mut self) -> Option<HoverEvent> {
        let event = match self.state {
            State::Settled { generation, .. } | State::Leaving { generation, .. } => {
                Some(HoverEvent::Left { generation })
            }
            State::Idle | State::Candidate { .. } => None,
        };
        self.state = State::Idle;
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Timestamp {
        Duration::from_millis(value)
    }

    fn machine() -> HoverMachine {
        HoverMachine::new(HoverConfig::default())
    }

    fn rest_at(machine: &mut HoverMachine, point: ScreenPoint, start: u64) -> Option<HoverEvent> {
        machine.observe(PointerSample::moved(point, ms(start)));
        machine.observe(PointerSample::moved(point, ms(start + 400)))
    }

    #[test]
    fn a_resting_pointer_settles_after_the_dwell() {
        let mut machine = machine();
        let point = ScreenPoint::new(100, 100);

        assert_eq!(machine.observe(PointerSample::moved(point, ms(0))), None);
        assert_eq!(machine.observe(PointerSample::moved(point, ms(319))), None);

        assert_eq!(
            machine.observe(PointerSample::moved(point, ms(320))),
            Some(HoverEvent::Settled {
                generation: Generation(1),
                point,
            })
        );
    }

    #[test]
    fn a_moving_pointer_never_settles() {
        let mut machine = machine();
        // Sweeping across the screen for two seconds, well past the dwell.
        for step in 0..60u64 {
            let sample =
                PointerSample::moved(ScreenPoint::new(100 + step as i32 * 20, 100), ms(step * 33));
            assert_eq!(machine.observe(sample), None, "settled while sweeping");
        }
        assert_eq!(machine.generation(), Generation(0));
    }

    #[test]
    fn drift_inside_the_radius_still_counts_as_resting() {
        let mut machine = machine();
        machine.observe(PointerSample::moved(ScreenPoint::new(100, 100), ms(0)));
        // Four pixels of hand tremor, inside the six pixel radius.
        machine.observe(PointerSample::moved(ScreenPoint::new(103, 102), ms(200)));
        let event = machine.observe(PointerSample::moved(ScreenPoint::new(102, 101), ms(400)));

        assert!(matches!(event, Some(HoverEvent::Settled { .. })));
    }

    #[test]
    fn the_settled_point_is_the_anchor_not_the_drifted_sample() {
        let mut machine = machine();
        let anchor = ScreenPoint::new(100, 100);
        machine.observe(PointerSample::moved(anchor, ms(0)));
        let event = machine.observe(PointerSample::moved(ScreenPoint::new(104, 100), ms(400)));

        assert_eq!(
            event,
            Some(HoverEvent::Settled {
                generation: Generation(1),
                point: anchor,
            })
        );
    }

    #[test]
    fn leaving_is_held_for_the_grace_period() {
        let mut machine = machine();
        rest_at(&mut machine, ScreenPoint::new(100, 100), 0);

        let far = ScreenPoint::new(400, 400);
        // Outside the radius, but inside the grace window.
        assert_eq!(machine.observe(PointerSample::moved(far, ms(500))), None);
        assert_eq!(machine.observe(PointerSample::moved(far, ms(700))), None);

        assert_eq!(
            machine.observe(PointerSample::moved(far, ms(751))),
            Some(HoverEvent::Left {
                generation: Generation(1)
            })
        );
    }

    #[test]
    fn returning_within_the_grace_period_keeps_the_target() {
        let mut machine = machine();
        let anchor = ScreenPoint::new(100, 100);
        rest_at(&mut machine, anchor, 0);

        machine.observe(PointerSample::moved(ScreenPoint::new(200, 200), ms(500)));
        assert_eq!(machine.observe(PointerSample::moved(anchor, ms(600))), None);
        // Still generation 1: it was never hidden, so it is not re-requested.
        assert!(machine.is_current(Generation(1)));
        assert_eq!(machine.generation(), Generation(1));
    }

    #[test]
    fn clicking_scrolling_and_typing_all_abandon_a_settled_target() {
        for (name, mutate) in [
            (
                "button",
                (|s: &mut PointerSample| s.button_down = true) as fn(&mut PointerSample),
            ),
            ("scroll", |s: &mut PointerSample| s.scrolling = true),
            ("typing", |s: &mut PointerSample| s.typing = true),
            ("suppressed", |s: &mut PointerSample| s.suppressed = true),
        ] {
            let mut machine = machine();
            let point = ScreenPoint::new(100, 100);
            rest_at(&mut machine, point, 0);

            let mut sample = PointerSample::moved(point, ms(500));
            mutate(&mut sample);

            assert_eq!(
                machine.observe(sample),
                Some(HoverEvent::Left {
                    generation: Generation(1)
                }),
                "{name} did not abandon the target",
            );
            assert!(!machine.is_current(Generation(1)), "{name} left it current");
        }
    }

    #[test]
    fn a_cancelling_input_does_not_emit_a_hide_when_nothing_was_shown() {
        let mut machine = machine();
        let mut sample = PointerSample::moved(ScreenPoint::new(10, 10), ms(0));
        sample.scrolling = true;
        assert_eq!(machine.observe(sample), None);
    }

    #[test]
    fn a_result_from_a_previous_generation_is_stale() {
        let mut machine = machine();
        rest_at(&mut machine, ScreenPoint::new(100, 100), 0);
        let first = machine.generation();

        // Pointer leaves, settles somewhere else.
        machine.observe(PointerSample::moved(ScreenPoint::new(900, 900), ms(600)));
        machine.observe(PointerSample::moved(ScreenPoint::new(900, 900), ms(900)));
        rest_at(&mut machine, ScreenPoint::new(900, 900), 1000);

        assert_ne!(machine.generation(), first);
        assert_eq!(
            machine.should_present(first, TargetFingerprint::of(None, "late answer"), ms(1500)),
            Presentation::Stale,
        );
    }

    #[test]
    fn the_same_target_within_the_cooldown_is_a_duplicate() {
        let mut machine = machine();
        rest_at(&mut machine, ScreenPoint::new(100, 100), 0);
        let generation = machine.generation();
        let target = TargetFingerprint::of(Some("chrome"), "The output files came back empty.");

        assert_eq!(
            machine.should_present(generation, target, ms(400)),
            Presentation::Present
        );
        assert_eq!(
            machine.should_present(generation, target, ms(900)),
            Presentation::Duplicate
        );
    }

    #[test]
    fn a_different_target_is_never_a_duplicate() {
        let mut machine = machine();
        rest_at(&mut machine, ScreenPoint::new(100, 100), 0);
        let generation = machine.generation();

        machine.should_present(
            generation,
            TargetFingerprint::of(Some("chrome"), "one"),
            ms(400),
        );
        assert_eq!(
            machine.should_present(
                generation,
                TargetFingerprint::of(Some("chrome"), "two"),
                ms(450)
            ),
            Presentation::Present,
        );
    }

    #[test]
    fn the_same_text_in_a_different_app_is_a_different_target() {
        let a = TargetFingerprint::of(Some("chrome"), "Read the docs");
        let b = TargetFingerprint::of(Some("code"), "Read the docs");
        assert_ne!(a, b);
    }

    #[test]
    fn surrounding_whitespace_does_not_change_a_fingerprint() {
        // UI Automation range expansion often includes a trailing space; the
        // same sentence must not be re-requested because of it.
        let a = TargetFingerprint::of(Some("chrome"), "Read the docs");
        let b = TargetFingerprint::of(Some("chrome"), "  Read the docs\n");
        assert_eq!(a, b);
    }

    #[test]
    fn generations_increase_once_per_settle() {
        let mut machine = machine();
        for expected in 1..=3u64 {
            let point = ScreenPoint::new(100 * expected as i32, 100);
            let base = expected * 2000;
            rest_at(&mut machine, point, base);
            assert_eq!(machine.generation(), Generation(expected));
            // Walk away so the next rest is a fresh candidate.
            machine.observe(PointerSample::moved(ScreenPoint::new(0, 0), ms(base + 500)));
            machine.observe(PointerSample::moved(ScreenPoint::new(0, 0), ms(base + 900)));
        }
    }
}
