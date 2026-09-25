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
//! named applications (`CONTRIBUTING.md`, "Claiming something works").

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::time::{Duration, Instant};

use lens_core::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, ExpandedUnit, ProtectedReason, ProviderInfo,
    TextAcquirer, TextTarget,
};
use lens_core::geometry::{line_under, ScreenPoint, ScreenRect};
use lens_core::text::{self, SegmentedText};
use windows::core::{Interface, HRESULT};
use windows::Win32::Foundation::{POINT, RPC_E_TIMEOUT};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, SAFEARRAY};
use windows::Win32::System::Variant::VARIANT;
// The SafeArray helpers live under Ole, not Com, despite operating on a type
// declared in Com.
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
    SafeArrayUnaccessData,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
    IUIAutomationTextPattern, IUIAutomationTextRange, IUIAutomationTreeWalker,
    TextPatternRangeEndpoint_Start, TextUnit_Line, TextUnit_Paragraph, TextUnit_Word,
    TreeScope_Descendants, UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId, UIA_E_TIMEOUT,
};

use crate::com::ComScope;
use crate::window;

/// How long to keep asking a woken renderer, before giving up on this hover.
///
/// This was a **sleep of 140ms followed by one retry**, carried over from Mori
/// where it is paid once per deliberate keypress. A reader described what that
/// produces, exactly:
///
/// > Alt을 눌렀는데 처음에는 반응이 없고, 두 번째에는 못한다고 하고,
/// > 세 번째에는 번역된 걸 보여주고
///
/// That is this constant and the line below it. A renderer that has not
/// finished building its tree at 140ms fails the retry; the window is by then
/// recorded as woken, so the second hover sends no message and makes no retry
/// at all; and the third succeeds only because the tree finished on its own
/// while the reader was trying again.
///
/// So it polls instead. The tree is asked for as soon as it might exist and
/// every [`WAKE_STEP`] after that, and the answer arrives when it is ready
/// rather than at a moment somebody guessed. Longer than 140ms in the worst
/// case and shorter in the common one.
///
/// Affordable now in a way it was not: acquisition runs on its own thread
/// (issue #42), so waiting here costs the card some latency and costs the
/// pointer, the card timers and the pause key nothing.
const WAKE_PATIENCE: Duration = Duration::from_millis(420);

/// How long before a window that did not wake may be asked again.
///
/// The other half of the defect this file records. A window was recorded as
/// woken the moment the message went out, whether or not it ever answered, so a
/// renderer that was still building got one message and then silence for the
/// session. Forgetting it on failure fixed that and broke something worse:
/// every failing point paid the full wait, and a sweep across one window
/// managed 45 points in twenty seconds.
///
/// Two seconds. Long enough that a loop asking about a hundred points sends one
/// message, short enough that a reader who tries again gets a real second
/// attempt rather than the silence they reported.
const WAKE_RETRY_AFTER: Duration = Duration::from_secs(2);

/// How often to ask, while waiting for a tree to appear.
///
/// A read costs a few milliseconds against a p50 of 3 to 7, so this is mostly
/// sleeping. Fine enough to return early on a fast renderer, coarse enough not
/// to spend the wait making cross-process calls.
const WAKE_STEP: Duration = Duration::from_millis(25);

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
/// failure reasons is the whole output of Milestone 1 (`CONTRIBUTING.md`).
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
    /// When each window was last sent a wake message.
    ///
    /// A time rather than a set, and that is the whole of the throttle. A set
    /// answers "has this window ever been woken", which is the wrong question
    /// twice over: yes forever means a renderer that was still building its
    /// tree never gets another message, and forgetting it on failure means the
    /// next point pays the full wait again. A sweep across one window found 45
    /// points in twenty seconds that way.
    woken: RefCell<HashMap<isize, Instant>>,
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
        // **A precondition, not a courtesy.** This type's whole contract is
        // rectangles in physical screen pixels, and UI Automation hands a
        // process that has not declared awareness the same rectangles divided
        // by the display scaling. It cannot honour its contract in a
        // virtualised process, so it stops being possible to construct one
        // there. Idempotent, and a no-op when a manifest has already set the
        // context.
        //
        // Leaving this to the caller cost two days on issue #40: every tool
        // that read through this and forgot was measuring plausible numbers in
        // the wrong coordinate space. See `window::declare_dpi_awareness`.
        window::declare_dpi_awareness();
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
            woken: RefCell::new(HashMap::new()),
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

    /// Send the wake message, unless this window had one recently.
    ///
    /// Returns whether one went out, so the caller only waits when something
    /// actually changed.
    ///
    /// **Throttled rather than once.** Once meant a renderer still building its
    /// tree got a single message and then silence for the session. Never meant
    /// every failing point paid the full wait, and a sweep managed 45 points in
    /// twenty seconds. A reader gets another chance [`WAKE_RETRY_AFTER`] later,
    /// and a loop asking about a hundred points does not.
    ///
    /// No sleep here. Waiting is [`Self::read_until_awake`]'s job, because how
    /// long to wait is a question about whether an answer has arrived and not
    /// one a constant can settle.
    fn wake_once(&self, hwnd: windows::Win32::Foundation::HWND) -> bool {
        let key = window::window_key(hwnd);
        let now = Instant::now();
        let mut woken = self.woken.borrow_mut();
        if let Some(last) = woken.get(&key) {
            if now.duration_since(*last) < WAKE_RETRY_AFTER {
                return false;
            }
        }
        woken.insert(key, now);
        drop(woken);
        window::wake_accessibility(hwnd);
        true
    }

    /// Read until the tree exists, or until this hover has waited long enough.
    ///
    /// Returns the last failure rather than a synthesised one, so the
    /// compatibility matrix records what the provider actually said.
    fn read_until_awake(
        &self,
        point: POINT,
        context: &AcquireContext,
    ) -> Result<Reading, AcquireError> {
        let deadline = Instant::now() + WAKE_PATIENCE;
        let mut last = self.read_once(point, context);
        while last.is_err() && Instant::now() < deadline {
            std::thread::sleep(WAKE_STEP);
            last = self.read_once(point, context);
        }
        last
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
            // `wake_once` is last, and deliberately: it has the side effect of
            // sending the message, so it must not run for a failure a wake
            // could not repair or for a hover that is not allowed to wake.
            if context.allow_accessibility_wake
                && Self::wake_could_help(error)
                && hwnd.is_some_and(|hwnd| self.wake_once(hwnd))
            {
                woke = true;
                // Re-run the whole chain, not just the read: the provider that
                // answers after activation is a different object from the stub
                // that answered before it
                // (`docs/research/inherited-from-mori.md` §3).
                reading = self.read_until_awake(native, context);
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
pub(crate) fn bounding_rectangles(range: &IUIAutomationTextRange) -> Vec<ScreenRect> {
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

/// What the element under a point actually is.
///
/// A diagnostic, and it exists because of a measurement that has not been
/// explained. The same plain `EDIT` control answers `Empty` from inside its own
/// process and, between runs, `Protected(Declared)` from another one, which
/// means `CurrentIsPassword` returned true for a control that does not carry
/// `ES_PASSWORD` (issue #40).
///
/// Both answers are fail-safe, so this is not a privacy defect: the refusal
/// direction is the harmless one and no run has ever shown the reverse. It is a
/// **correctness** problem for the only artifact Milestone 1 produces. An
/// application recorded as exposing protected fields, when it exposes ordinary
/// ones, is a wrong row in the compatibility matrix, and every real target Lens
/// reads is another process.
///
/// The hypothesis this is built to test is the last one in that issue: that
/// `CurrentIsPassword` is answering about a **different element** from the one
/// under the point. If the element's own rectangle does not contain the point
/// that produced it, the refusal is about something else.
///
/// **This changes no refusal.** `may_be_read` is untouched, and it must be: a
/// declared password field is refused whatever this reports, because unreadable
/// means unprovable and unprovable is treated as secure
/// (`docs/privacy-model.md` §1). This only makes the record honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementUnderPoint {
    pub control_id: Option<i32>,
    /// `None` when the property could not be read at all.
    pub is_password: Option<bool>,
    pub bounds: Option<ScreenRect>,
    /// Whether the element's own rectangle contains the point it came from.
    ///
    /// `false` with a rectangle present is the finding this was built for.
    pub contains_point: bool,
}

impl UiaTextAcquirer {
    /// Describe the element under a point, without reading any text from it.
    ///
    /// No text pattern is requested and no range is expanded, so nothing a
    /// protected field contains can reach a Rust value through here.
    pub fn describe_element_at(&self, point: ScreenPoint) -> Option<ElementUnderPoint> {
        let native = POINT {
            x: point.x,
            y: point.y,
        };
        // SAFETY: `ElementFromPoint` returns an owned interface or an error.
        let element = unsafe { self.automation.ElementFromPoint(native) }.ok()?;

        // SAFETY: reading documented properties off a live element.
        let is_password = unsafe { element.CurrentIsPassword() }
            .ok()
            .map(|value| value.as_bool());
        // SAFETY: as above; the rectangle is returned by value.
        let bounds = unsafe { element.CurrentBoundingRectangle() }
            .ok()
            .map(|rect| {
                ScreenRect::new(
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                )
            });

        Some(ElementUnderPoint {
            control_id: control_type(&element),
            is_password,
            contains_point: bounds.is_some_and(|rect| rect.contains(point)),
            bounds,
        })
    }
}

/// One large text container's GetVisibleRanges measurement.
///
/// Returned by [`measure_visible_ranges`] for each of the top containers in a
/// window. The probe uses this to decide whether GetVisibleRanges exposes
/// line-level geometry that the normal path (RangeFromPoint) misses.
#[derive(Debug, Clone)]
pub struct VisibleRangeSample {
    /// Height of the container element in physical pixels.
    pub container_height: i32,
    /// Number of ranges GetVisibleRanges returned.
    pub range_count: i32,
    /// Bounding rectangles extracted from those ranges.
    pub rects: Vec<ScreenRect>,
    /// How long GetVisibleRanges took.
    pub get_visible_ranges_time: Duration,
    /// Of line-shaped rectangles (height <= 40), how many answered correctly
    /// when RangeFromPoint was called at the line's own centre.
    pub point_at_centre_hits: usize,
    pub point_at_centre_total: usize,
    /// Grid coverage: how many grid points land inside a line rectangle.
    pub grid_points: usize,
    pub grid_points_on_a_line: usize,
}

/// Measure whether the foreground window's large text containers expose
/// line-level geometry through GetVisibleRanges.
///
/// Returns one sample per container, up to the five largest. This is a
/// read-only measurement; no text is read.
pub fn measure_visible_ranges(window_id: i64) -> Vec<VisibleRangeSample> {
    let _com = ComScope::enter();
    // SAFETY: read-only COM creation on a freshly initialised apartment.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        return Vec::new();
    };

    // SAFETY: the handle is a live window the OS just returned.
    let Ok(element) = (unsafe {
        automation.ElementFromHandle(windows::Win32::Foundation::HWND(window_id as *mut _))
    }) else {
        return Vec::new();
    };

    // SAFETY: property condition construction, read-only.
    let Ok(condition) = (unsafe {
        automation
            .CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))
    }) else {
        return Vec::new();
    };
    // SAFETY: read-only tree walk.
    let Ok(found) = (unsafe { element.FindAll(TreeScope_Descendants, &condition) }) else {
        return Vec::new();
    };
    // SAFETY: read-only length query on a live element array.
    let count = unsafe { found.Length().unwrap_or(0) };

    let mut candidates = Vec::new();
    for index in 0..count.min(200) {
        // SAFETY: read-only element access on a live array.
        let Ok(child) = (unsafe { found.GetElement(index) }) else {
            continue;
        };
        // SAFETY: read-only property on a live element.
        let Ok(rect) = (unsafe { child.CurrentBoundingRectangle() }) else {
            continue;
        };
        let area = (rect.right - rect.left) as i64 * (rect.bottom - rect.top) as i64;
        candidates.push((area, rect.bottom - rect.top, child));
    }
    candidates.sort_by_key(|(area, _, _)| std::cmp::Reverse(*area));

    let mut samples = Vec::new();
    for (_, height, child) in candidates.into_iter().take(5) {
        // SAFETY: GetCurrentPattern returns an owned interface or an error.
        let Ok(pattern) = (unsafe { child.GetCurrentPattern(UIA_TextPatternId) }) else {
            continue;
        };
        let Ok(pattern) = pattern.cast::<IUIAutomationTextPattern>() else {
            continue;
        };

        let start = Instant::now();
        // SAFETY: GetVisibleRanges is a read-only query on a live pattern.
        let ranges = unsafe { pattern.GetVisibleRanges() }.ok();
        let range_count = ranges
            .as_ref()
            // SAFETY: read-only length query on a live COM array.
            .and_then(|array| unsafe { array.Length() }.ok())
            .unwrap_or(0);
        let rects: Vec<ScreenRect> = match &ranges {
            Some(array) => {
                // SAFETY: read-only length on a live COM array.
                let len = unsafe { array.Length() }.unwrap_or(0);
                (0..len)
                    // SAFETY: read-only element access on a live COM array.
                    .filter_map(|at| unsafe { array.GetElement(at) }.ok())
                    .flat_map(|range| bounding_rectangles(&range))
                    .collect()
            }
            None => Vec::new(),
        };
        let get_visible_ranges_time = start.elapsed();

        // Test RangeFromPoint at each line's own centre.
        let mut asked = 0usize;
        let mut answered_with_itself = 0usize;
        for rect in rects.iter().filter(|rect| rect.height <= 40) {
            let probe = POINT {
                x: rect.x + 2,
                y: rect.y + rect.height / 2,
            };
            asked += 1;
            // SAFETY: RangeFromPoint is a read-only query.
            let answered = unsafe { pattern.RangeFromPoint(probe) }
                .ok()
                .and_then(|range| {
                    // SAFETY: ExpandToEnclosingUnit is a read-only mutation
                    // of the range, not the provider.
                    unsafe { range.ExpandToEnclosingUnit(TextUnit_Line) }
                        .ok()
                        .map(|()| range)
                })
                .map(|range| {
                    bounding_rectangles(&range)
                        .iter()
                        .any(|got| got.y == rect.y && got.height == rect.height)
                })
                .unwrap_or(false);
            if answered {
                answered_with_itself += 1;
            }
        }

        // Grid coverage.
        let lines_only: Vec<ScreenRect> = rects
            .iter()
            .copied()
            .filter(|rect| rect.height <= 40)
            .collect();
        // SAFETY: read-only property.
        let area = unsafe { child.CurrentBoundingRectangle() }
            .map(|rect| {
                ScreenRect::new(
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                )
            })
            .unwrap_or(ScreenRect::new(0, 0, 0, 0));
        let (mut points, mut on_a_line) = (0usize, 0usize);
        let mut y = area.y + 4;
        while y < area.bottom() - 4 {
            let mut x = area.x + 4;
            while x < area.right() - 4 {
                points += 1;
                if line_under(&lines_only, ScreenPoint::new(x, y)).is_some() {
                    on_a_line += 1;
                }
                x += 24;
            }
            y += 24;
        }

        samples.push(VisibleRangeSample {
            container_height: height,
            range_count,
            rects,
            get_visible_ranges_time,
            point_at_centre_hits: answered_with_itself,
            point_at_centre_total: asked,
            grid_points: points,
            grid_points_on_a_line: on_a_line,
        });
    }

    samples
}

/// One ancestor in the chain from `ElementFromPoint` to the window root.
#[derive(Debug, Clone)]
pub struct AncestorStep {
    /// Depth from the starting element (0 = the element itself).
    pub depth: usize,
    /// UIA control type id.
    pub control_type: Option<i32>,
    /// Bounding rectangle in physical pixels.
    pub bounds: Option<ScreenRect>,
    /// Whether this ancestor has a TextPattern.
    pub has_text_pattern: bool,
    /// If it has a TextPattern, how many visible ranges it returns.
    pub visible_range_count: Option<i32>,
    /// If it has a TextPattern, how many line-shaped rects.
    pub line_rect_count: Option<usize>,
    /// Time for GetVisibleRanges on this ancestor.
    pub get_visible_ranges_time: Option<Duration>,
    /// If it has a TextPattern, grid coverage from its visible ranges.
    pub grid_coverage: Option<(usize, usize)>,
}

/// Result of walking the ancestor chain from one probe point.
#[derive(Debug, Clone)]
pub struct AncestorChainSample {
    /// The probe point used.
    pub point: ScreenPoint,
    /// Control type of the element at the point.
    pub start_control_type: Option<i32>,
    /// The full ancestor chain, from start to root.
    pub ancestors: Vec<AncestorStep>,
    /// Total time to walk the chain.
    pub chain_time: Duration,
}

/// Walk the ancestor chain from `ElementFromPoint` at each probe point,
/// measuring which ancestors expose a useful TextPattern with line geometry.
///
/// This is the decisive measurement for whether a production fallback can
/// discover the right container from the pointer position.
pub fn measure_ancestor_chain(window_id: i64, points: &[ScreenPoint]) -> Vec<AncestorChainSample> {
    let _com = ComScope::enter();
    // SAFETY: read-only COM creation on a freshly initialised apartment.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        return Vec::new();
    };

    // Verify the window handle is valid.
    // SAFETY: the handle is a live window the OS just returned.
    let Ok(_element) = (unsafe {
        automation.ElementFromHandle(windows::Win32::Foundation::HWND(window_id as *mut _))
    }) else {
        return Vec::new();
    };

    // SAFETY: read-only request for the raw-view walker.
    let Ok(walker) = (unsafe { automation.RawViewWalker() }) else {
        return Vec::new();
    };

    let mut samples = Vec::new();

    for &point in points {
        let native = POINT {
            x: point.x,
            y: point.y,
        };
        let point_start = Instant::now();
        // SAFETY: read-only query.
        let Ok(start_elem) = (unsafe { automation.ElementFromPoint(native) }) else {
            continue;
        };
        let start_ct = control_type(&start_elem);

        let mut ancestors = Vec::new();
        // SAFETY: read-only property on a live element.
        let start_bounds = unsafe { start_elem.CurrentBoundingRectangle() }
            .ok()
            .map(|rect| {
                ScreenRect::new(
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                )
            });

        // Step 0: the element itself.
        // SAFETY: GetCurrentPattern returns an owned interface or an error.
        let start_has_tp = unsafe { start_elem.GetCurrentPattern(UIA_TextPatternId) }
            .ok()
            .and_then(|p| p.cast::<IUIAutomationTextPattern>().ok())
            .is_some();
        let (start_vr_count, start_lr_count, start_gvr_time, start_coverage) = if start_has_tp {
            measure_ancestor_text_pattern(&automation, &walker, &start_elem)
        } else {
            (None, None, None, None)
        };

        ancestors.push(AncestorStep {
            depth: 0,
            control_type: start_ct,
            bounds: start_bounds,
            has_text_pattern: start_has_tp,
            visible_range_count: start_vr_count,
            line_rect_count: start_lr_count,
            get_visible_ranges_time: start_gvr_time,
            grid_coverage: start_coverage,
        });

        // Walk up.
        let mut current = start_elem.clone();
        let mut found_useful = false;
        for depth in 1..=20 {
            // SAFETY: read-only tree walk.
            let Ok(parent) = (unsafe { walker.GetParentElement(&current) }) else {
                break;
            };
            // Check if we got a valid element by reading its control type.
            let ct = control_type(&parent);
            if ct.is_none() {
                break;
            }

            // SAFETY: read-only property on a live element.
            let bounds = unsafe { parent.CurrentBoundingRectangle() }
                .ok()
                .map(|rect| {
                    ScreenRect::new(
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                    )
                });

            // SAFETY: GetCurrentPattern returns an owned interface or an error.
            let has_tp = unsafe { parent.GetCurrentPattern(UIA_TextPatternId) }
                .ok()
                .and_then(|p| p.cast::<IUIAutomationTextPattern>().ok())
                .is_some();
            // Only measure GetVisibleRanges on the first ancestor with a TextPattern.
            // Subsequent ones are recorded but not measured (to bound the cost).
            let (vr_count, lr_count, gvr_time, coverage) = if has_tp && !found_useful {
                let result = measure_ancestor_text_pattern(&automation, &walker, &parent);
                if result.1.is_some_and(|n| n > 0) {
                    found_useful = true;
                }
                result
            } else if has_tp {
                // Just record that it has a TextPattern, without measuring.
                (Some(-1), None, None, None)
            } else {
                (None, None, None, None)
            };

            ancestors.push(AncestorStep {
                depth,
                control_type: ct,
                bounds,
                has_text_pattern: has_tp,
                visible_range_count: vr_count,
                line_rect_count: lr_count,
                get_visible_ranges_time: gvr_time,
                grid_coverage: coverage,
            });

            current = parent;
        }

        samples.push(AncestorChainSample {
            point,
            start_control_type: start_ct,
            ancestors,
            chain_time: point_start.elapsed(),
        });
    }

    samples
}

/// Helper: measure GetVisibleRanges on an element that has a TextPattern.
///
/// Returns (range_count, line_rect_count, gvr_time).
/// All calls are read-only UIA queries on a live element.
/// Grid coverage is omitted here to isolate the cross-process call cost.
type TextPatternMeasurement = (
    Option<i32>,
    Option<usize>,
    Option<Duration>,
    Option<(usize, usize)>,
);

fn measure_ancestor_text_pattern(
    _automation: &IUIAutomation,
    _walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
) -> TextPatternMeasurement {
    // SAFETY: GetCurrentPattern returns an owned interface or an error.
    let Ok(pattern) = (unsafe { element.GetCurrentPattern(UIA_TextPatternId) }) else {
        return (None, None, None, None);
    };
    let Ok(pattern) = pattern.cast::<IUIAutomationTextPattern>() else {
        return (None, None, None, None);
    };

    let start = Instant::now();
    // SAFETY: GetVisibleRanges is a read-only query on a live pattern.
    let ranges = unsafe { pattern.GetVisibleRanges() }.ok();
    let range_count = ranges
        .as_ref()
        // SAFETY: read-only length query on a live COM array.
        .and_then(|array| unsafe { array.Length() }.ok());
    let rects: Vec<ScreenRect> = match &ranges {
        Some(array) => {
            // SAFETY: read-only length on a live COM array.
            let len = unsafe { array.Length() }.unwrap_or(0);
            (0..len)
                // SAFETY: read-only element access on a live COM array.
                .filter_map(|at| unsafe { array.GetElement(at) }.ok())
                .flat_map(|range| bounding_rectangles(&range))
                .collect()
        }
        None => Vec::new(),
    };
    let gvr_time = start.elapsed();

    let lr_count = rects.iter().filter(|rect| rect.height <= 40).count();

    (range_count, Some(lr_count), Some(gvr_time), None)
}
