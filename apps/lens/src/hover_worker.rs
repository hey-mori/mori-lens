//! Acquisition on its own thread, so a slow read never stalls the pointer.
//!
//! # The bug this exists for
//!
//! `HoverSession::resolve` used to be called from the pointer loop. While it
//! ran, nothing else in the product did: no pointer sampling, no card
//! retirement, and **the Ctrl+Alt+L pause key stopped responding**. That last
//! one is why this was worth fixing. The pause key is the one control somebody
//! reaches for when a tool that reads their screen starts behaving oddly, and
//! it was unavailable for exactly as long as the tool was behaving oddly
//! (issue #42).
//!
//! A per-call timeout bounded the freeze at roughly 800ms, and about double
//! that with an accessibility wake in the path. That is a guard, not a fix:
//! most of a second of a dead pointer is visible, and the key is still gone
//! for it.
//!
//! # Why the whole session moves, rather than just the acquirer
//!
//! The acquirer holds COM interfaces bound to the apartment it was created in,
//! so it is not `Send` and cannot be handed to a thread; it has to be *built*
//! there. Splitting `HoverSession` around it would leave the hover machine on
//! one thread and its acquirers on another, with the state that decides
//! whether a generation is still current sitting on the far side of a channel
//! from the code that asks.
//!
//! Moving the whole session is smaller and truer: the worker owns the machine,
//! the policy and the acquirers together, exactly as they were, and the pointer
//! loop keeps only what it needs to draw with. What crosses the channel is a
//! pointer sample going one way and a resolution coming back, both plain data.
//!
//! # Samples queue rather than being dropped
//!
//! While the worker is inside a slow `acquire`, the main loop keeps sampling
//! and those samples wait. When the worker comes back it processes them in
//! order, and each carries the timestamp it was taken at, so the hover machine
//! sees the true history rather than a compressed one. Dropping to the newest
//! sample would be cheaper and wrong: dwell and movement radius are judgements
//! about a sequence, and a sequence with the middle removed is a different one.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use lens_core::geometry::ScreenPoint;
use lens_core::hover::{Generation, HoverConfig, HoverEvent, Timestamp};
use lens_core::policy::AppPolicy;
use lens_core::session::{HoverSession, InputState, Resolution};

/// What the pointer loop asks the worker to do.
enum ToHover {
    Sample {
        position: ScreenPoint,
        now: Timestamp,
        input: InputState,
        process_name: Option<String>,
    },
    Reconfigure {
        config: Box<HoverConfig>,
        policy: Box<AppPolicy>,
    },
    /// Manual OCR is a keypress rather than a settled hover, and it still needs
    /// a generation from the machine so its answer can go stale the same way.
    TakeGeneration,
}

/// What the worker sends back.
pub enum FromHover {
    /// A hover settled and was resolved. Carries everything the drawing side
    /// needs, because it can no longer ask the session anything.
    Settled {
        generation: Generation,
        point: ScreenPoint,
        now: Timestamp,
        process_name: Option<String>,
        resolution: Resolution,
    },
    Generation(Generation),
}

/// A live acquisition thread.
pub struct HoverWorker {
    to: Sender<ToHover>,
    from: Receiver<FromHover>,
    /// Messages taken off the channel while waiting for a specific reply.
    ///
    /// Without this, asking for a generation would consume whatever settled
    /// hover happened to be in the queue and drop it, so pressing the OCR key
    /// at the wrong moment would silently discard a card.
    pending: VecDeque<FromHover>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl HoverWorker {
    /// Start the thread, building the session on it.
    ///
    /// `build` runs on the worker thread and must construct the acquirers
    /// there. That is the whole reason it is a closure rather than a value:
    /// a `HoverSession` cannot be moved across a thread boundary, and the
    /// compiler will say so if this is ever changed to take one.
    pub fn start<F>(build: F) -> Option<Self>
    where
        F: FnOnce() -> Option<HoverSession> + Send + 'static,
    {
        let (to, orders) = mpsc::channel::<ToHover>();
        let (answers, from) = mpsc::channel::<FromHover>();
        let (ready, started) = mpsc::channel::<bool>();

        let join = std::thread::Builder::new()
            .name("lens-acquire".to_owned())
            .spawn(move || {
                let Some(mut session) = build() else {
                    let _ = ready.send(false);
                    return;
                };
                if ready.send(true).is_err() {
                    return;
                }
                // Ends when the sender is dropped, which is what stops this
                // thread on exit without a flag anybody has to remember to set.
                while let Ok(order) = orders.recv() {
                    match order {
                        ToHover::Sample {
                            position,
                            now,
                            input,
                            process_name,
                        } => {
                            let sample =
                                session.sample(position, now, input, process_name.as_deref());
                            if let Some(HoverEvent::Settled { generation, point }) =
                                session.observe(sample)
                            {
                                let resolution = session.resolve(generation, point, now, input);
                                if answers
                                    .send(FromHover::Settled {
                                        generation,
                                        point,
                                        now,
                                        process_name,
                                        resolution,
                                    })
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        ToHover::Reconfigure { config, policy } => {
                            session.reconfigure(*config, *policy);
                        }
                        ToHover::TakeGeneration => {
                            let generation = session.next_generation();
                            if answers.send(FromHover::Generation(generation)).is_err() {
                                return;
                            }
                        }
                    }
                }
            })
            .ok()?;

        // The session is built on the far thread, so failing to build it is
        // reported back rather than panicking there and leaving the product
        // running with a worker that answers nothing.
        if started.recv().ok()? {
            Some(Self {
                to,
                from,
                pending: VecDeque::new(),
                join: Some(join),
            })
        } else {
            let _ = join.join();
            None
        }
    }

    pub fn sample(
        &self,
        position: ScreenPoint,
        now: Timestamp,
        input: InputState,
        process_name: Option<String>,
    ) {
        let _ = self.to.send(ToHover::Sample {
            position,
            now,
            input,
            process_name,
        });
    }

    pub fn reconfigure(&self, config: HoverConfig, policy: AppPolicy) {
        let _ = self.to.send(ToHover::Reconfigure {
            config: Box::new(config),
            policy: Box::new(policy),
        });
    }

    /// A generation for work the hover machine did not start.
    ///
    /// A round trip, because the counter lives with the machine and there is
    /// exactly one of it. Bounded, because a keypress that hangs the product
    /// waiting for a thread is a worse bug than the one this file fixes.
    pub fn take_generation(&mut self) -> Option<Generation> {
        self.to.send(ToHover::TakeGeneration).ok()?;
        let deadline = std::time::Instant::now() + Duration::from_millis(1500);
        while std::time::Instant::now() < deadline {
            match self.from.recv_timeout(Duration::from_millis(50)) {
                Ok(FromHover::Generation(generation)) => return Some(generation),
                // Not this reply. Kept, not dropped: it is a card somebody is
                // waiting for.
                Ok(other) => self.pending.push_back(other),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
        None
    }

    /// The next thing the worker has said, if it has said anything.
    pub fn try_recv(&mut self) -> Option<FromHover> {
        if let Some(held) = self.pending.pop_front() {
            return Some(held);
        }
        // Disconnected is treated as empty on purpose: the worker having gone
        // away is not something this loop can do anything about mid-frame, and
        // the product keeps drawing whatever card it already has.
        self.from.try_recv().ok()
    }
}

impl Drop for HoverWorker {
    fn drop(&mut self) {
        // Dropping the sender is what ends the loop. Joining after it means
        // exit waits for at most one acquisition rather than abandoning a
        // thread inside a cross-process COM call.
        let (dead, _) = mpsc::channel();
        let live = std::mem::replace(&mut self.to, dead);
        drop(live);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer, TextTarget};

    /// An acquirer that takes as long as it is told to and always fails.
    ///
    /// Failing is right for these: what is under test is the threading, and a
    /// synthesised `TextTarget` would prove nothing about it while inviting
    /// somebody to assert on its contents later.
    struct Slow {
        cost: Duration,
        calls: Arc<AtomicUsize>,
    }

    impl TextAcquirer for Slow {
        fn acquire(
            &self,
            _point: ScreenPoint,
            _context: &AcquireContext,
        ) -> Result<TextTarget, AcquireError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.cost);
            Err(AcquireError::Unsupported)
        }
    }

    fn worker(cost: Duration) -> (HoverWorker, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let theirs = Arc::clone(&calls);
        let worker = HoverWorker::start(move || {
            Some(HoverSession::new(
                HoverConfig {
                    dwell: Duration::from_millis(40),
                    ..HoverConfig::default()
                },
                AppPolicy::default(),
                Box::new(Slow {
                    cost,
                    calls: theirs,
                }),
            ))
        })
        .expect("worker");
        (worker, calls)
    }

    fn resting() -> InputState {
        InputState {
            button_down: false,
            scrolling: false,
            typing: false,
            modifier_held: true,
            ocr_requested: false,
        }
    }

    /// Rest the pointer until the worker answers, and say how long the sending
    /// side was ever blocked for.
    fn rest_until_answered(worker: &mut HoverWorker, cost: Duration) -> (Duration, bool) {
        let mut worst = Duration::ZERO;
        let mut answered = false;
        let mut at = Duration::ZERO;
        for _ in 0..200 {
            let began = std::time::Instant::now();
            worker.sample(
                ScreenPoint::new(500, 500),
                at,
                resting(),
                Some("app".into()),
            );
            worst = worst.max(began.elapsed());
            while let Some(message) = worker.try_recv() {
                if matches!(message, FromHover::Settled { .. }) {
                    answered = true;
                }
            }
            if answered {
                break;
            }
            at += Duration::from_millis(10);
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = cost;
        (worst, answered)
    }

    /// The bug, as a test. A read that takes half a second must not stop the
    /// loop that samples the pointer, because that loop is also the one the
    /// pause key is read on.
    #[test]
    fn a_slow_read_never_blocks_the_sending_side() {
        let cost = Duration::from_millis(500);
        let (mut worker, calls) = worker(cost);
        let (worst, answered) = rest_until_answered(&mut worker, cost);

        assert!(answered, "the worker never answered a settled hover");
        assert!(calls.load(Ordering::SeqCst) >= 1, "nothing was acquired");
        assert!(
            worst < Duration::from_millis(100),
            "sending a sample blocked for {worst:?}, and the read costs {cost:?}. \
             That is the loop stalling on acquisition, which is the whole defect.",
        );
    }

    /// Asking for a generation must not swallow a card.
    ///
    /// The reply and a settled hover come back on one channel. Taking messages
    /// off it until the right one appears is correct only if the others are
    /// kept, and dropping them would lose a card the reader was waiting for at
    /// exactly the moment they pressed a key.
    #[test]
    fn waiting_for_a_generation_keeps_whatever_else_arrived() {
        let (mut worker, _) = worker(Duration::from_millis(5));

        // Settle a hover and leave its answer sitting in the channel.
        let mut at = Duration::ZERO;
        for _ in 0..40 {
            worker.sample(
                ScreenPoint::new(400, 400),
                at,
                resting(),
                Some("app".into()),
            );
            at += Duration::from_millis(10);
            std::thread::sleep(Duration::from_millis(3));
        }
        std::thread::sleep(Duration::from_millis(60));

        let generation = worker.take_generation().expect("a generation");
        assert!(generation.0 > 0, "a generation was never taken");

        let mut settled = 0;
        while let Some(message) = worker.try_recv() {
            if matches!(message, FromHover::Settled { .. }) {
                settled += 1;
            }
        }
        assert!(
            settled >= 1,
            "the settled hover in the channel was consumed by the generation request",
        );
    }

    /// A worker whose session cannot be built reports it rather than starting
    /// a thread that answers nothing for the life of the process.
    #[test]
    fn a_session_that_cannot_be_built_is_not_a_running_worker() {
        assert!(HoverWorker::start(|| None).is_none());
    }
}
