//! Text under the pointer, through UI Automation.
//!
//! The chain, and what each step can fail with:
//!
//! ```text
//! ElementFromPoint      → ElementGone   the UI changed mid-call
//! CurrentIsPassword     → Protected     refused before any text is read
//! GetCurrentPattern     → Unsupported   this provider exposes no text
//! RangeFromPoint        → Empty         no text near the point
//! ExpandToEnclosingUnit → the word, and the enclosing paragraph or line
//! GetText               → the characters
//! GetBoundingRectangles → where they are, in physical screen pixels
//! ```
//!
//! None of this is covered by `cargo test`. UI Automation needs a live desktop
//! with a real window, so the verification for this file is a probe run against
//! named applications (`CLAUDE.md`, "Claiming something works").

use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr;
use std::time::{Duration, Instant};

use lens_core::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, ExpandedUnit, ProtectedReason, ProviderInfo,
    TextAcquirer, TextTarget,
};
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::text::{self, SegmentedText};
use windows::core::{Interface, HRESULT};
use windows::Win32::Foundation::{POINT, RPC_E_TIMEOUT};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, SAFEARRAY};
// The SafeArray helpers live under Ole, not Com, despite operating on a type
// declared in Com.
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
    SafeArrayUnaccessData,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
    IUIAutomationTextPattern, IUIAutomationTextRange, TextPatternRangeEndpoint_Start,
    TextUnit_Line, TextUnit_Paragraph, TextUnit_Word, UIA_TextPatternId, UIA_E_TIMEOUT,
};

use crate::com::ComScope;
use crate::window;

/// How long to let a woken Chromium renderer build its tree before re-reading.
///
/// Short enough to stay imperceptible, long enough for the renderer to answer
/// in practice. Measured by Mori at 140ms; carried over as a starting value and
/// re-measured by the probe, because Lens pays it on a hover budget rather than
/// on a keypress.
const WAKE_SETTLE_MS: u64 = 140;

/// How long UI Automation may wait for one cross-process call to come back.
///
/// **The default is two minutes.** That is the number behind the 45-second
/// `acquire` in issue #28: nothing in this file was slow, the provider simply
/// never answered and UI Automation was content to wait. A hover is not a
/// two-minute operation, so the client says so.
///
/// 800ms against a measured p95 of 20ms, so it is a guard rather than a budget:
/// an application that trips this is wedged, not busy.
const TRANSACTION_TIMEOUT_MS: u32 = 800;

/// How long to wait to reach a provider at all, default two seconds.
///
/// Separate from the transaction because it fails for a different reason: a
/// process that is not answering its message pump has nothing to time out, it
/// has nothing to connect to.
const CONNECTION_TIMEOUT_MS: u32 = 300;

/// How long the whole chain of calls may take before it gives up.
///
/// [`TRANSACTION_TIMEOUT_MS`] bounds one call. It cannot bound eight of them,
/// and a provider answering every call in 700ms is under the per-call limit
/// while still stalling the caller for most of a second. This bounds the sum.
///
/// The two guards catch different failures and neither replaces the other.
const CHAIN_BUDGET: Duration = Duration::from_millis(400);

/// The clock one pass of the chain runs against.
///
/// A deadline rather than a timeout: it cannot interrupt a call already in
/// flight, it only refuses to start the next one. That is the honest bound, and
/// it is why the per-call limit above exists as well.
#[derive(Debug, Clone, Copy)]
struct Deadline {
    started: Instant,
    budget: Duration,
}

impl Deadline {
    fn new(budget: Duration) -> Self {
        Self {
            started: Instant::now(),
            budget,
        }
    }

    fn spent(&self) -> Duration {
        self.started.elapsed()
    }

    /// `Err(Timeout)` once the budget is gone, so a call site can use `?`.
    fn still_worth_it(&self) -> Result<(), AcquireError> {
        if self.spent() >= self.budget {
            return Err(AcquireError::Timeout);
        }
        Ok(())
    }
}

/// Is this failure the provider being slow rather than the provider saying no?
///
/// It matters because every call site in the chain maps its error to a
/// site-specific meaning: a failed `GetCurrentPattern` means `Unsupported`, a
/// failed `RangeFromPoint` means `Empty`. Reporting a timed-out call that way
/// would put "this application exposes no text" in the compatibility matrix for
/// an application that exposes plenty and answered too slowly once. Distinguishing
/// failure reasons is the whole output of Milestone 1 (`CLAUDE.md`).
///
/// Three codes because three layers can be the one that gives up: UI Automation
/// itself, the COM runtime underneath it, and Win32 under that.
fn is_timeout(code: HRESULT) -> bool {
    const WIN32_ERROR_TIMEOUT: HRESULT = HRESULT(0x8007_05B4_u32 as i32);
    code == HRESULT(UIA_E_TIMEOUT as i32) || code == RPC_E_TIMEOUT || code == WIN32_ERROR_TIMEOUT
}

/// Map a COM failure to this call site's meaning, unless it was a timeout.
fn because(error: &windows::core::Error, otherwise: AcquireError) -> AcquireError {
    if is_timeout(error.code()) {
        AcquireError::Timeout
    } else {
        otherwise
    }
}

/// Reads text under a screen point through UI Automation.
///
/// Not `Send`: it holds COM interfaces bound to the apartment it was created
/// in. Construct it on the thread that will use it, which is the acquisition
/// worker, never the pointer loop.
pub struct UiaTextAcquirer {
    automation: IUIAutomation,
    /// Windows already sent a wake message, so the cost is paid once per window
    /// rather than once per hover. A closed window's handle staying in the set
    /// is harmless: handles are not reused often enough to matter, and the worst
    /// case is one skipped wake on a cold tree.
    woken: RefCell<HashSet<isize>>,
    /// Whether the per-call limits were actually accepted.
    ///
    /// Recorded rather than assumed. `IUIAutomation2` is Windows 8 and later,
    /// and a client that silently fell back to the two-minute default while
    /// reporting a bounded one would be worse than no guard at all: the
    /// operator would stop looking for the freeze.
    bounded: bool,
    /// Held for the acquirer's lifetime so the apartment outlives the
    /// interfaces created inside it.
    _com: ComScope,
}

impl std::fmt::Debug for UiaTextAcquirer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UiaTextAcquirer")
            .field("woken_windows", &self.woken.borrow().len())
            .finish_non_exhaustive()
    }
}

impl UiaTextAcquirer {
    /// Create the automation client on this thread.
    ///
    /// Fails with [`AcquireError::Unavailable`] when the accessibility layer
    /// itself cannot be reached, which is a permission or policy problem rather
    /// than a property of any application.
    pub fn new() -> Result<Self, AcquireError> {
        let com = ComScope::enter();
        // `CUIAutomation8` rather than `CUIAutomation`, and the difference is
        // the entire point of this constructor. The original co-class predates
        // `IUIAutomation2`, so a client built from it cannot be given a
        // per-call limit: the cast fails and every call inherits the
        // two-minute default. That is not a documented deprecation, it is a
        // silent one, and the first build of this guard reported UNBOUNDED on
        // Windows 11 because of it.
        //
        // The old class is still the fallback, because a client with no limits
        // reads text and a missing client reads nothing.
        //
        // SAFETY: creating a documented UI Automation co-class in-process.
        let automation: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .or_else(|_| CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER))
        }
        .map_err(|_| AcquireError::Unavailable)?;

        let bounded = bound_calls(&automation);

        Ok(Self {
            automation,
            woken: RefCell::new(HashSet::new()),
            bounded,
            _com: com,
        })
    }

    /// Whether a single wedged provider is bounded, or can still hold this
    /// thread for the UI Automation default of two minutes.
    ///
    /// Public so the product can say which it is at startup instead of leaving
    /// a user to discover it during a freeze.
    pub fn calls_are_bounded(&self) -> bool {
        self.bounded
    }

    /// One pass of the chain, with no waking and no retry.
    fn read_once(&self, point: POINT, context: &AcquireContext) -> Result<Reading, AcquireError> {
        let deadline = Deadline::new(CHAIN_BUDGET);

        // SAFETY: `ElementFromPoint` returns an owned interface or an error, and
        // the target is allowed to disappear mid-call.
        let element = unsafe { self.automation.ElementFromPoint(point) }
            .map_err(|error| because(&error, AcquireError::ElementGone))?;

        // Before anything is read. A password field's contents must never exist
        // as a Rust value, not even briefly (`docs/privacy-model.md` §1).
        may_be_read(&element)?;

        let control_id = control_type(&element);

        // Checked here rather than after every call. These four are the ones
        // that go to another process and can each cost real time; the property
        // reads above are cheap, and a check between them would be noise.
        deadline.still_worth_it()?;

        // SAFETY: a provider that does not implement the pattern returns an
        // error rather than a null this could dereference.
        let pattern = unsafe { element.GetCurrentPattern(UIA_TextPatternId) }
            .map_err(|error| because(&error, AcquireError::Unsupported))?
            .cast::<IUIAutomationTextPattern>()
            .map_err(|_| AcquireError::Unsupported)?;

        deadline.still_worth_it()?;
        // SAFETY: returns the range nearest the point, or an error.
        let hit = unsafe { pattern.RangeFromPoint(point) }
            .map_err(|error| because(&error, AcquireError::Empty))?;

        deadline.still_worth_it()?;
        let word_range = expanded(&hit, ExpandUnit::Word)?;
        // Paragraph is the useful context; a provider that does not support it
        // normalises to something else, so line is tried as a narrower answer
        // rather than assumed to be equivalent.
        let context_range =
            expanded(&hit, ExpandUnit::Paragraph).or_else(|_| expanded(&hit, ExpandUnit::Line))?;

        deadline.still_worth_it()?;
        let context_text = range_text(&context_range, context.max_context_chars)?;
        if context_text.trim().is_empty() {
            return Err(AcquireError::Empty);
        }

        // The pointer's offset inside the context, measured to the degenerate
        // hit range rather than to the expanded word: expansion is allowed to
        // normalise up to a line, and measuring to that start would resolve
        // the line's first word instead of the pointer's.
        let focus = focus_offset(&context_range, &hit, &context_text);

        // What the expansion actually produced, so an over-expanded anchor is
        // a recorded fact rather than an invisible one.
        let expanded_unit =
            ExpandedUnit::classify(&range_text(&word_range, 256).unwrap_or_default());

        Ok(Reading {
            segmented: text::segment(&context_text, focus),
            word_bounds: bounding_rectangles(&word_range),
            context_bounds: bounding_rectangles(&context_range),
            control_id,
            expanded_unit,
        })
    }

    /// Whether this failure is the one a wake message repairs.
    ///
    /// A cold Chromium tree presents as "no text pattern", occasionally as an
    /// empty read. Nothing else is worth a 340ms round trip.
    fn wake_could_help(error: AcquireError) -> bool {
        matches!(error, AcquireError::Unsupported | AcquireError::Empty)
    }

    /// Wake a window's accessibility tree by its opaque id, ahead of any read.
    ///
    /// The sweep uses this to separate two questions that the per-hover retry
    /// mixes together: how much a cold window returns, and how much the same
    /// window returns once its tree exists. Returns whether a message was
    /// actually sent, which is `false` for a window already woken.
    pub fn wake_window(&self, window_id: i64) -> bool {
        self.wake_once(windows::Win32::Foundation::HWND(window_id as *mut _))
    }

    /// Forget which windows have been woken.
    ///
    /// Only for measurement: it lets a sweep re-wake the same window in a
    /// later pass. Nothing in the hover path should call it, because paying
    /// the wake again per hover is the cost the memo exists to avoid.
    pub fn forget_woken(&self) {
        self.woken.borrow_mut().clear();
    }

    /// Send the wake message if this window has not had one. Returns whether it
    /// was sent, so a retry only happens when something actually changed.
    fn wake_once(&self, hwnd: windows::Win32::Foundation::HWND) -> bool {
        let key = window::window_key(hwnd);
        if !self.woken.borrow_mut().insert(key) {
            return false;
        }
        window::wake_accessibility(hwnd);
        std::thread::sleep(std::time::Duration::from_millis(WAKE_SETTLE_MS));
        true
    }
}

/// What one pass of the chain produced.
struct Reading {
    segmented: SegmentedText,
    word_bounds: Vec<ScreenRect>,
    context_bounds: Vec<ScreenRect>,
    control_id: Option<i32>,
    expanded_unit: ExpandedUnit,
}

impl TextAcquirer for UiaTextAcquirer {
    fn acquire(
        &self,
        point: ScreenPoint,
        context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        let native = POINT {
            x: point.x,
            y: point.y,
        };
        let hwnd = window::window_at(point);
        let app = window::identity(hwnd);

        let mut woke = false;
        let mut reading = self.read_once(native, context);

        if let Err(error) = reading {
            if context.allow_accessibility_wake && Self::wake_could_help(error) {
                if let Some(hwnd) = hwnd.filter(|hwnd| self.wake_once(*hwnd)) {
                    let _ = hwnd;
                    woke = true;
                    // Re-run the whole chain, not just the read: the provider
                    // that answers after activation is a different object from
                    // the stub that answered before it
                    // (`docs/research/inherited-from-mori.md` §3).
                    reading = self.read_once(native, context);
                }
            }
        }

        let reading = reading?;

        Ok(TextTarget {
            app,
            source: AcquisitionSource::Accessibility,
            provider: ProviderInfo {
                control_id: reading.control_id,
                woke_accessibility: woke,
                expanded_unit: reading.expanded_unit,
            },
            segmented: reading.segmented,
            word_bounds: reading.word_bounds,
            context_bounds: reading.context_bounds,
        })
    }
}

/// Refuse before reading anything, when protection is declared or unprovable.
///
/// A failed check yields `Unprovable` rather than "not protected". Unreadable
/// means unprovable, and unprovable is treated as secure
/// (`docs/privacy-model.md` §1). `ProtectedReason` exists so the probe can count
/// the two separately: if `Unprovable` turns out to be common, the strict rule
/// is costing real coverage and that is a decision to revisit with evidence.
fn may_be_read(element: &IUIAutomationElement) -> Result<(), AcquireError> {
    // SAFETY: reading a documented property off a live automation element.
    match unsafe { element.CurrentIsPassword() } {
        Ok(value) if value.as_bool() => Err(AcquireError::Protected(ProtectedReason::Declared)),
        Ok(_) => Ok(()),
        // A timeout here says nothing about protection, and calling it
        // `Unprovable` would put a privacy signal in the matrix for an
        // application that was merely wedged. Both answers refuse, so the
        // guarantee is identical and only the recorded reason differs; the
        // accurate one is the one that keeps the taxonomy worth reading.
        Err(error) => Err(because(
            &error,
            AcquireError::Protected(ProtectedReason::Unprovable),
        )),
    }
}

fn control_type(element: &IUIAutomationElement) -> Option<i32> {
    // SAFETY: reading a documented property off a live automation element.
    unsafe { element.CurrentControlType() }.ok().map(|id| id.0)
}

#[derive(Debug, Clone, Copy)]
enum ExpandUnit {
    Word,
    Line,
    Paragraph,
}

/// Clone a range and expand the copy, so the original stays usable.
///
/// The result is **not** assumed to match the requested unit:
/// `ExpandToEnclosingUnit` may normalise to a larger unit when the provider
/// does not support the one asked for. Whether that happens, and where, is one
/// of the questions Milestone 1 answers.
fn expanded(
    range: &IUIAutomationTextRange,
    unit: ExpandUnit,
) -> Result<IUIAutomationTextRange, AcquireError> {
    // SAFETY: `Clone` returns an owned range or an error.
    let cloned = unsafe { range.Clone() }.map_err(|error| because(&error, AcquireError::Empty))?;
    let native = match unit {
        ExpandUnit::Word => TextUnit_Word,
        ExpandUnit::Line => TextUnit_Line,
        ExpandUnit::Paragraph => TextUnit_Paragraph,
    };
    // SAFETY: expanding an owned range in place.
    unsafe { cloned.ExpandToEnclosingUnit(native) }
        .map_err(|error| because(&error, AcquireError::Unsupported))?;
    Ok(cloned)
}

/// The text of a range, capped at the provider.
///
/// The cap is applied by the provider so an accidental whole-document range
/// never allocates the whole document.
fn range_text(range: &IUIAutomationTextRange, max_chars: usize) -> Result<String, AcquireError> {
    let cap = i32::try_from(max_chars).unwrap_or(i32::MAX);
    // SAFETY: `GetText` returns an owned BSTR or an error.
    let text =
        unsafe { range.GetText(cap) }.map_err(|error| because(&error, AcquireError::Empty))?;
    Ok(text.to_string())
}

/// Tell the client not to wait two minutes for one call.
///
/// Returns whether the limits were accepted. `IUIAutomation2` is Windows 8 and
/// later, so a `false` here on a supported system means something refused, and
/// that is worth surfacing rather than swallowing.
fn bound_calls(automation: &IUIAutomation) -> bool {
    let Ok(client) = automation.cast::<IUIAutomation2>() else {
        return false;
    };
    // SAFETY: setting two documented properties on an interface just obtained
    // by a successful cast. Both return an error rather than failing silently.
    unsafe {
        client.SetTransactionTimeout(TRANSACTION_TIMEOUT_MS).is_ok()
            && client.SetConnectionTimeout(CONNECTION_TIMEOUT_MS).is_ok()
    }
}

/// The byte offset of the **pointer** within the context text.
///
/// Computed by asking the provider for the text between the context's start and
/// the given range's start, then measuring it. Searching for the word in the
/// context string would resolve to the wrong occurrence whenever a word
/// repeats, which in ordinary prose is most sentences.
///
/// The range passed in must be the **degenerate range from `RangeFromPoint`**,
/// not the expanded word. `ExpandToEnclosingUnit` is allowed to normalise up to
/// a line or a paragraph when a provider does not implement the word unit, and
/// measuring to the expanded range's start then lands the focus at the start of
/// that line. Segmentation would go on to return the line's *first* word rather
/// than the word under the pointer, silently and plausibly. The degenerate
/// range's start is the pointer's own character position, so it is immune to
/// however the expansion behaved.
fn focus_offset(
    context_range: &IUIAutomationTextRange,
    hit_range: &IUIAutomationTextRange,
    context_text: &str,
) -> usize {
    let prefix_chars = (|| {
        // SAFETY: `Clone` returns an owned range or an error.
        let prefix = unsafe { context_range.Clone() }.ok()?;
        // SAFETY: moves the clone's end endpoint to the word's start endpoint,
        // leaving a range that spans context-start to word-start.
        unsafe {
            prefix
                .MoveEndpointByRange(
                    windows::Win32::UI::Accessibility::TextPatternRangeEndpoint_End,
                    hit_range,
                    TextPatternRangeEndpoint_Start,
                )
                .ok()?;
        }
        // SAFETY: `GetText(-1)` means no limit.
        let text = unsafe { prefix.GetText(-1) }.ok()?;
        Some(text.to_string().chars().count())
    })();

    let Some(prefix_chars) = prefix_chars else {
        // The provider would not answer. Point at the start of the context,
        // which yields the first word rather than a panic or a wrong offset in
        // the middle of a character.
        return 0;
    };

    context_text
        .char_indices()
        .nth(prefix_chars)
        .map(|(offset, _)| offset)
        .unwrap_or_else(|| context_text.len().saturating_sub(1))
}

/// Screen rectangles for a range, one per rendered line.
///
/// UI Automation returns a `SAFEARRAY` of doubles in groups of four: left, top,
/// width, height, in physical screen pixels. An empty array means the range is
/// scrolled out of view, which is a real state and not an error.
fn bounding_rectangles(range: &IUIAutomationTextRange) -> Vec<ScreenRect> {
    // SAFETY: returns an owned SAFEARRAY pointer or an error; ownership passes
    // to `read_rect_array`, which destroys it on every path.
    let Ok(array) = (unsafe { range.GetBoundingRectangles() }) else {
        return Vec::new();
    };
    // SAFETY: `array` is a live, owned, one-dimensional SAFEARRAY of doubles.
    unsafe { read_rect_array(array) }
}

/// # Safety
/// `array` must be an owned, one-dimensional `SAFEARRAY` of `f64`, or null.
/// This function takes ownership and destroys it before returning.
unsafe fn read_rect_array(array: *mut SAFEARRAY) -> Vec<ScreenRect> {
    if array.is_null() {
        return Vec::new();
    }

    // SAFETY: the caller guarantees a live one-dimensional array; every early
    // return below destroys it.
    let rects = unsafe {
        let (Ok(lower), Ok(upper)) = (SafeArrayGetLBound(array, 1), SafeArrayGetUBound(array, 1))
        else {
            let _ = SafeArrayDestroy(array);
            return Vec::new();
        };

        let count = usize::try_from(upper - lower + 1).unwrap_or(0);
        // Four doubles per rectangle. A count that is not a multiple of four
        // means the provider answered with something this code does not
        // understand, and the trailing remainder is dropped rather than read.
        if count < 4 {
            let _ = SafeArrayDestroy(array);
            return Vec::new();
        }

        let mut data: *mut c_void = ptr::null_mut();
        if SafeArrayAccessData(array, &mut data).is_err() || data.is_null() {
            let _ = SafeArrayDestroy(array);
            return Vec::new();
        }

        let values = std::slice::from_raw_parts(data.cast::<f64>(), count);
        let rects: Vec<ScreenRect> = values
            .chunks_exact(4)
            .map(|quad| {
                ScreenRect::new(
                    quad[0].round() as i32,
                    quad[1].round() as i32,
                    quad[2].round() as i32,
                    quad[3].round() as i32,
                )
            })
            .collect();

        let _ = SafeArrayUnaccessData(array);
        let _ = SafeArrayDestroy(array);
        rects
    };

    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping is pure, so it is the one part of this file `cargo test` can
    /// hold to account. Everything around it needs a live desktop.
    #[test]
    fn the_three_layers_that_can_give_up_are_all_recognised() {
        assert!(is_timeout(HRESULT(UIA_E_TIMEOUT as i32)), "UI Automation");
        assert!(is_timeout(RPC_E_TIMEOUT), "the COM runtime");
        assert!(is_timeout(HRESULT(0x8007_05B4_u32 as i32)), "Win32");
    }

    #[test]
    fn a_provider_that_says_no_is_not_reported_as_slow() {
        // UIA_E_ELEMENTNOTAVAILABLE, the ordinary "it went away" answer.
        let gone = windows::core::Error::from_hresult(HRESULT(0x8004_0201_u32 as i32));
        assert_eq!(
            because(&gone, AcquireError::Unsupported),
            AcquireError::Unsupported,
        );
    }

    #[test]
    fn a_timed_out_call_keeps_its_own_name_rather_than_the_call_sites() {
        let slow = windows::core::Error::from_hresult(RPC_E_TIMEOUT);
        // The point of the whole mapping: without it this application would be
        // recorded as exposing no text.
        assert_eq!(
            because(&slow, AcquireError::Unsupported),
            AcquireError::Timeout,
        );
    }

    #[test]
    fn a_spent_deadline_refuses_the_next_call() {
        let deadline = Deadline::new(Duration::ZERO);
        assert_eq!(deadline.still_worth_it(), Err(AcquireError::Timeout));
    }

    #[test]
    fn a_fresh_deadline_allows_the_chain_to_start() {
        assert_eq!(Deadline::new(CHAIN_BUDGET).still_worth_it(), Ok(()));
    }

    /// A guard rather than a budget: it must sit far enough above measured
    /// behaviour that tripping it means wedged, not busy. p95 was 20ms.
    #[test]
    fn the_budget_is_a_guard_not_a_deadline_ordinary_hovers_race() {
        assert!(CHAIN_BUDGET >= Duration::from_millis(200));
        assert!(
            u128::from(TRANSACTION_TIMEOUT_MS) < CHAIN_BUDGET.as_millis() * 4,
            "one wedged call must not be able to outlast several budgets",
        );
    }
}
