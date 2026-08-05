//! The loop, assembled: policy, hover intent, and acquisition routing.
//!
//! This exists so the ordering rules live in one tested place instead of being
//! re-decided by every binary that drives a pointer. The rules it owns:
//!
//! - a denied application is never acquired from, at all
//! - accessibility is always tried first
//! - OCR is reached only after a failure OCR could plausibly repair, and in v1
//!   only when the user asked for it
//! - a protected field never reaches OCR
//! - a result from a superseded generation is never returned as presentable
//!
//! Every one of those is a line from `docs/decisions/ADR-0002-accessibility-first-acquisition.md`
//! or `docs/privacy-model.md`, and every one has a test in `contract_tests.rs`
//! driven by fake acquirers.

use crate::acquire::{AcquireContext, AcquireError, TextAcquirer, TextTarget};
use crate::geometry::ScreenPoint;
use crate::hover::{
    Generation, HoverConfig, HoverEvent, HoverMachine, PointerSample, Presentation, Timestamp,
};
use crate::policy::AppPolicy;

/// When the OCR fallback may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrMode {
    /// No fallback at all.
    Off,
    /// Only when the user explicitly asks for this hover. The v1 default: it
    /// means a user who never reaches for OCR never enables a capture path
    /// (`docs/privacy-model.md` §4).
    Manual,
    /// Whenever accessibility fails with something OCR could repair. Not a v1
    /// setting; the variant exists so the routing is written once.
    Automatic,
}

/// Input state that is not the pointer position.
#[derive(Debug, Clone, Copy, Default)]
pub struct InputState {
    pub button_down: bool,
    pub scrolling: bool,
    pub typing: bool,
    /// The policy modifier is held.
    pub modifier_held: bool,
    /// The user asked for OCR on this hover.
    pub ocr_requested: bool,
}

/// What came of resolving a settled hover.
#[derive(Debug)]
pub enum Resolution {
    /// Current, new, and ready to show.
    Presented(Box<TextTarget>),
    /// Current, but the same target is already answered. Nothing to do.
    Duplicate(Box<TextTarget>),
    /// Text was read, and it is too far from the pointer to be what the user
    /// meant. The pointer is over empty space and the nearest text is
    /// elsewhere on the page.
    ///
    /// Not a failure. `RangeFromPoint` did exactly what it promises, and this
    /// is the only place the difference is caught, because inside a document
    /// that call never fails
    /// (`docs/decisions/ADR-0009-pointer-distance-gate.md`).
    OffTarget(Box<TextTarget>),
    /// The pointer moved first. Dropped silently, by design.
    Stale,
    /// Nothing could be read. The variant carries why, which is the datum
    /// Milestone 1 exists to collect.
    Failed(AcquireError),
}

/// Policy plus hover intent plus acquisition routing.
pub struct HoverSession {
    machine: HoverMachine,
    policy: AppPolicy,
    accessibility: Box<dyn TextAcquirer>,
    ocr: Option<Box<dyn TextAcquirer>>,
    ocr_mode: OcrMode,
}

impl std::fmt::Debug for HoverSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HoverSession")
            .field("machine", &self.machine)
            .field("policy", &self.policy)
            .field("has_ocr", &self.ocr.is_some())
            .field("ocr_mode", &self.ocr_mode)
            .finish()
    }
}

impl HoverSession {
    /// A generation for work the hover machine did not start.
    ///
    /// Manual OCR is a keypress, not a settled hover, and it still has to be
    /// cancellable in the same way: a translation that comes back after the
    /// reader has moved on is dropped by generation, and a request with no
    /// generation could not be.
    ///
    /// Taking one also makes every hover in flight stale, which is right. The
    /// reader pressed a key; the sentence they were hovering a moment ago is
    /// not what they are asking about.
    pub fn next_generation(&mut self) -> Generation {
        self.machine.take_generation()
    }

    /// One line describing the policy, for a reload message.
    pub fn policy_summary(&self) -> String {
        self.policy.describe()
    }

    /// Replace the policy and the timings without losing the current hover.
    ///
    /// For a settings file edited while Lens is running. The machine keeps its
    /// state on purpose: a reader hovering a sentence when the file is saved
    /// should not have the card yanked away because a rule they did not touch
    /// was re-read.
    pub fn reconfigure(&mut self, config: HoverConfig, policy: AppPolicy) {
        self.machine.reconfigure(config);
        self.policy = policy;
    }

    pub fn new(
        config: HoverConfig,
        policy: AppPolicy,
        accessibility: Box<dyn TextAcquirer>,
    ) -> Self {
        Self {
            machine: HoverMachine::new(config),
            policy,
            accessibility,
            ocr: None,
            ocr_mode: OcrMode::Off,
        }
    }

    pub fn with_ocr(mut self, ocr: Box<dyn TextAcquirer>, mode: OcrMode) -> Self {
        self.ocr = Some(ocr);
        self.ocr_mode = mode;
        self
    }

    pub fn policy(&self) -> &AppPolicy {
        &self.policy
    }

    pub fn policy_mut(&mut self) -> &mut AppPolicy {
        &mut self.policy
    }

    pub fn generation(&self) -> Generation {
        self.machine.generation()
    }

    /// Build a pointer sample with the policy already applied.
    ///
    /// The caller supplies the process name under the pointer, because only the
    /// operating system can answer that. Folding the policy in here is what
    /// makes "a denied application is never acquired from" a property of the
    /// core rather than of each binary's main loop.
    pub fn sample(
        &self,
        point: ScreenPoint,
        at: Timestamp,
        input: InputState,
        process_name: Option<&str>,
    ) -> PointerSample {
        PointerSample {
            point,
            at,
            button_down: input.button_down,
            scrolling: input.scrolling,
            typing: input.typing,
            suppressed: !self.policy.allows(process_name, input.modifier_held),
        }
    }

    pub fn observe(&mut self, sample: PointerSample) -> Option<HoverEvent> {
        self.machine.observe(sample)
    }

    /// Resolve a settled hover into a target, or into the reason there is none.
    pub fn resolve(
        &mut self,
        generation: Generation,
        point: ScreenPoint,
        now: Timestamp,
        input: InputState,
    ) -> Resolution {
        // Checked before any work, so a pointer that has already moved on costs
        // one comparison instead of a call into a foreign process.
        if !self.machine.is_current(generation) {
            return Resolution::Stale;
        }

        let context = AcquireContext::new(generation);
        let target = match self.accessibility.acquire(point, &context) {
            Ok(target) => target,
            Err(error) => match self.ocr_fallback(error, point, &context, input) {
                Some(Ok(target)) => target,
                Some(Err(ocr_error)) => return Resolution::Failed(ocr_error),
                // The original accessibility failure is reported, not a
                // synthesised one: the compatibility matrix records what the
                // provider did, and "OCR was not offered" is not a provider fact.
                None => return Resolution::Failed(error),
            },
        };

        // Before deduplication, because an off-target resolution must not
        // occupy the "last presented" slot: the user has not been shown
        // anything, so the next real hover on that sentence would otherwise be
        // suppressed as a duplicate of something they never saw.
        if !target.is_on_target(point) {
            return Resolution::OffTarget(Box::new(target));
        }

        match self
            .machine
            .should_present(generation, target.fingerprint(), now)
        {
            Presentation::Present => Resolution::Presented(Box::new(target)),
            Presentation::Duplicate => Resolution::Duplicate(Box::new(target)),
            Presentation::Stale => Resolution::Stale,
        }
    }

    /// `None` means OCR was not offered this failure, which is different from
    /// OCR being offered it and failing.
    fn ocr_fallback(
        &self,
        error: AcquireError,
        point: ScreenPoint,
        context: &AcquireContext,
        input: InputState,
    ) -> Option<Result<TextTarget, AcquireError>> {
        // Protected is filtered here as well as by `ocr_can_help`, because this
        // is the line that would leak a password field's rendered characters and
        // one check for it is not enough to read as deliberate.
        if matches!(error, AcquireError::Protected(_)) || !error.ocr_can_help() {
            return None;
        }
        let allowed = match self.ocr_mode {
            OcrMode::Off => false,
            OcrMode::Manual => input.ocr_requested,
            OcrMode::Automatic => true,
        };
        if !allowed {
            return None;
        }
        Some(self.ocr.as_ref()?.acquire(point, context))
    }
}
