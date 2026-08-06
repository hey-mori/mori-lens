//! Reading pixels through the same door the accessibility path uses.
//!
//! `HoverSession` has routed to an OCR fallback since it was written:
//! `with_ocr`, `OcrMode`, `ocr_fallback`, and five contract tests driving them
//! with fakes. **Nothing implemented the trait.** The only `TextAcquirer` in
//! the product was UI Automation's, so `ocr_mode` was always `Off`, the
//! fallback always returned `None`, and the real OCR lived in a second,
//! parallel path in the app that the session knew nothing about.
//!
//! Two designs for one job, one of them wired. This is the other one, made
//! real.
//!
//! # What it changes about when OCR runs
//!
//! Nothing, by default. The product asks for `OcrMode::Manual`, under which
//! this is not installed at all and `Ctrl+Alt+O` is served by the app's own
//! path as before. The M4 gate's "OCR never runs unasked" holds exactly as it
//! did.
//!
//! What it makes possible is `OcrMode::Automatic`, where a hover the
//! accessibility path could not serve falls through to pixels with no
//! keypress. That is what
//! [ADR-0002](../../../docs/decisions/ADR-0002-accessibility-first-acquisition.md)
//! describes and what VS Code and KakaoTalk need to be readable by hovering
//! rather than by pressing a key. It is a setting, off unless asked for,
//! because a tool that photographs the screen on its own has to be turned on
//! by somebody rather than arrive that way.
//!
//! # The app's manual path is still its own, and that is not an oversight
//!
//! `Ctrl+Alt+O` goes through `apps/lens/src/manual_ocr.rs`, not through here.
//! It carries two things a `TextTarget` has no room for: the count of misread
//! words that reaches the card's footer, and its own refusals in the reader's
//! language, which are more specific than `AcquireError` can be. Collapsing
//! them would cost both to remove a duplication that is only skin deep.
//!
//! # It must not read Lens's own card
//!
//! The accessibility path cannot see the overlay: it is a different window and
//! the acquirer refuses it by process. A camera has no such luck. So this hides
//! the card and waits for the compositor before capturing, and the hook is
//! passed in rather than assumed, because `lens-core` owns the trait and knows
//! nothing about overlays.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use lens_core::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, ProviderInfo, TextAcquirer, TextTarget,
};
use lens_core::barren::Barren;
use lens_core::geometry::{ScreenPoint, ScreenRect};

use crate::{capture, ocr, window};

/// How much screen one reading may look at, in pixels at 100% scaling.
///
/// Wide enough for a line of prose at a readable size, and tall enough for the
/// line above and below so a sentence that wraps is not cut in half.
///
/// Small enough to be a crop rather than a screenshot: **6.6% of a 1080p
/// screen**. Defined here rather than in the app, because two definitions of
/// how much of somebody's screen may be photographed is one too many.
pub const BAND_WIDTH: i32 = 760;
/// See [`BAND_WIDTH`].
pub const BAND_HEIGHT: i32 = 180;

/// How long to let the compositor take the card down before capturing.
///
/// Without it the capture races the hide and gets the card it was trying to
/// avoid, which is Lens translating its own translation.
const SETTLE: Duration = Duration::from_millis(60);

/// The band around a point, at this monitor's scaling.
pub fn band_around(pointer: ScreenPoint, scale: f32) -> ScreenRect {
    let width = (BAND_WIDTH as f32 * scale).round() as i32;
    let height = (BAND_HEIGHT as f32 * scale).round() as i32;
    ScreenRect::new(pointer.x - width / 2, pointer.y - height / 2, width, height)
}

/// Reads the pixels around a point.
pub struct OcrTextAcquirer {
    /// Called before every capture, to take the card off the screen.
    ///
    /// A hook rather than an `Overlay`, so this compiles and can be tested
    /// without one, and so the thing that owns the window stays the thing that
    /// decides how to hide it.
    hide_the_card: Box<dyn Fn(ScreenRect)>,
    /// Where the last reading found lines, so a hover in the same margin is
    /// not another capture. See [`lens_core::barren`].
    ///
    /// `RefCell` because `acquire` takes `&self`: the trait is shared, and a
    /// reading that costs half a second is not something two threads should be
    /// doing at once anyway. This acquirer lives on the hover worker's thread
    /// and nothing else touches it.
    barren: RefCell<Barren>,
    /// A monotonic origin, so the memory can be timed without an `Instant` in
    /// `lens-core`.
    since: Instant,
}

impl std::fmt::Debug for OcrTextAcquirer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("OcrTextAcquirer").finish()
    }
}

impl OcrTextAcquirer {
    pub fn new(hide_the_card: Box<dyn Fn(ScreenRect)>) -> Self {
        Self {
            hide_the_card,
            barren: RefCell::new(Barren::new()),
            since: Instant::now(),
        }
    }

    /// Whether this machine can recognise anything at all.
    ///
    /// Asked before the acquirer is installed rather than on the first hover,
    /// so a machine with no language pack says so in the banner instead of
    /// answering every fallback with the same failure.
    pub fn is_available() -> bool {
        !ocr::available_languages().is_empty()
    }
}

impl TextAcquirer for OcrTextAcquirer {
    fn acquire(
        &self,
        point: ScreenPoint,
        context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        let now = self.since.elapsed();
        if context.view_may_have_changed {
            self.barren.borrow_mut().forget();
        } else if self.barren.borrow().nothing_is_at(point, now) {
            // The previous capture already read this pixel and found no line
            // on it. Photographing the screen again to be told the same thing
            // is the cost issue #87 is about.
            return Err(AcquireError::KnownEmpty);
        }

        let scale = window::monitor_scale_at(point);
        let region = band_around(point, scale);
        // The band is computed first so the hook can decide whether the card is
        // even in it. With `Automatic` this runs on every hover accessibility
        // could not serve, and hiding the card each time would take the answer
        // a reader is reading off the screen when they nudge the pointer onto a
        // margin.
        (self.hide_the_card)(region);
        std::thread::sleep(SETTLE);
        let captured = capture::rectangle(region).ok_or(AcquireError::Empty)?;
        // With the focus, so the enlarged pass looks at a crop around the
        // pointer rather than the whole band. This path runs on every hover
        // accessibility could not serve, and most of those land on a margin
        // beside text, where enlarging everything cost 887ms to find nothing.
        let read =
            ocr::read_best_around(&captured, ScreenPoint::new(region.x, region.y), Some(point))
                // Not `Empty`: a machine with no recogniser is a machine where this
                // path cannot work at all, and saying so is different from saying
                // there was nothing to read.
                .ok_or(AcquireError::Unavailable)?;

        // Remembered whether or not this point resolved. A reading that found a
        // word still says where the rest of its region is empty, and that is
        // the same saving on the next hover.
        self.barren
            .borrow_mut()
            .remember(read.covered, read.line_bounds(), now);

        let (line, word) = read.word_at(point).ok_or(AcquireError::Empty)?;
        let text = line.text.trim();
        if text.is_empty() {
            return Err(AcquireError::Empty);
        }

        // Where the word sits in the line, so the segmenter focuses on the same
        // word the pointer is on. `find` rather than an index carried through:
        // the recogniser gives words and a line separately and does not promise
        // the line is their concatenation.
        let focus = text.find(word.text.as_str()).unwrap_or(0);

        Ok(TextTarget {
            app: window::identity(window::window_at(point)),
            source: AcquisitionSource::Ocr,
            provider: ProviderInfo {
                // No control type: there is no control, only pixels. Recorded
                // as absent rather than invented, because the matrix counts
                // these and a fabricated 50020 would be a fabricated row.
                control_id: None,
                woke_accessibility: false,
                expanded_unit: lens_core::acquire::ExpandedUnit::Word,
            },
            segmented: lens_core::text::segment(text, focus),
            word_bounds: vec![word.bounds],
            context_bounds: line.bounds().map(|rect| vec![rect]).unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_is_a_crop_rather_than_a_screenshot() {
        // 6.6% of a 1080p screen, and the figure is asserted because the first
        // version of the comment beside it said "about 4%", which was a number
        // nobody had divided.
        let share = f64::from(BAND_WIDTH * BAND_HEIGHT) / f64::from(1920 * 1080);
        assert!(
            (0.06..0.07).contains(&share),
            "the band is {:.1}% of a 1080p screen",
            share * 100.0,
        );
    }

    #[test]
    fn the_band_is_centred_on_the_pointer_and_scales_with_the_monitor() {
        let at_100 = band_around(ScreenPoint::new(1000, 500), 1.0);
        assert_eq!(at_100.center(), ScreenPoint::new(1000, 500));
        assert_eq!(at_100.width, BAND_WIDTH);

        let at_150 = band_around(ScreenPoint::new(1000, 500), 1.5);
        assert_eq!(at_150.center(), ScreenPoint::new(1000, 500));
        assert_eq!(at_150.width, (BAND_WIDTH as f32 * 1.5).round() as i32);
    }

    /// The card is hidden before anything is captured, every time.
    ///
    /// Not an implementation detail: the accessibility path cannot see the
    /// overlay because it is a different window, and a camera has no such luck.
    /// A capture that raced the hide would have Lens translating its own
    /// translation.
    #[test]
    fn the_card_is_hidden_before_the_capture() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let hides = Arc::new(AtomicUsize::new(0));
        let theirs = Arc::clone(&hides);
        let acquirer = OcrTextAcquirer::new(Box::new(move |_band| {
            theirs.fetch_add(1, Ordering::SeqCst);
        }));

        // The read itself will fail on a test runner with nothing under the
        // point, and that is fine: what is under test is that the hook ran
        // before anything looked at the screen.
        let _ = acquirer.acquire(
            ScreenPoint::new(-30000, -30000),
            &AcquireContext::new(lens_core::hover::Generation(1)),
        );
        assert_eq!(hides.load(Ordering::SeqCst), 1);
    }
}
