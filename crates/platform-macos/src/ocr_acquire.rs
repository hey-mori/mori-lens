//! macOS OCR text acquisition.
//!
//! Implements [`TextAcquirer`] by capturing a screen region around the pointer
//! and running Vision OCR on it. This is the fallback path when the
//! Accessibility API cannot provide text (e.g. VS Code editor, Electron apps).
//!
//! The capture band is a rectangle centered on the pointer. The OCR result
//! is matched to the pointer position to find the word the user is looking at.

use lens_core::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, AppIdentity, ExpandedUnit, ProviderInfo,
    TextAcquirer, TextTarget,
};
use lens_core::geometry::{line_under, ScreenPoint, ScreenRect};
use lens_core::text::{self, SegmentedText};

use crate::capture;
use crate::coords::DisplayTopology;
use crate::vision_ocr;

/// The size of the capture band around the pointer, in logical points.
///
/// Wide enough to catch the line the user is reading, tall enough for
/// ascenders/descenders plus leading. Measured against the Windows adapter's
/// band which is 760x180 physical pixels.
const BAND_WIDTH_POINTS: f64 = 600.0;
const BAND_HEIGHT_POINTS: f64 = 120.0;

/// OCR-based text acquirer for macOS.
///
/// Captures a screen region around the pointer, runs Vision OCR, and finds
/// the word under the pointer from the recognized text and bounding boxes.
pub struct OcrTextAcquirer {
    topology: DisplayTopology,
}

impl OcrTextAcquirer {
    pub fn new() -> Self {
        Self {
            topology: DisplayTopology::capture(),
        }
    }

    /// Whether OCR acquisition is available (Vision framework + Screen
    /// Recording permission).
    pub fn is_available() -> bool {
        vision_ocr::is_available() && capture::is_available()
    }
}

impl Default for OcrTextAcquirer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for OcrTextAcquirer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OcrTextAcquirer").finish()
    }
}

impl TextAcquirer for OcrTextAcquirer {
    fn acquire(
        &self,
        point: ScreenPoint,
        _context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        if !Self::is_available() {
            return Err(AcquireError::Unavailable);
        }

        // Get the scale factor at the pointer position.
        let scale = self.topology.scale_at_screen_point(point);

        // Convert pointer to logical points for the capture region.
        let px = point.x as f64 / scale;
        let py = point.y as f64 / scale;

        // Capture a band centered on the pointer.
        let band_x = px - BAND_WIDTH_POINTS / 2.0;
        let band_y = py - BAND_HEIGHT_POINTS / 2.0;

        let cap = capture::capture_region(
            band_x,
            band_y,
            BAND_WIDTH_POINTS,
            BAND_HEIGHT_POINTS,
            px,
            py, // pointer position for display selection
        )
        .ok_or(AcquireError::Empty)?;

        // Run Vision OCR on the captured pixels.
        let ocr_result = vision_ocr::recognize(&cap.pixels, cap.width, cap.height, cap.stride)
            .ok_or(AcquireError::Empty)?;

        if ocr_result.text.is_empty() {
            return Err(AcquireError::Empty);
        }

        // Build word bounds in screen coordinates (physical pixels).
        // Use the ACTUAL captured origin (cap.origin_x/y), not the requested
        // band origin. After display-boundary clipping, these may differ.
        let cap_scale = cap.scale;
        let cap_origin_x = cap.origin_x;
        let cap_origin_y = cap.origin_y;
        let word_rects: Vec<ScreenRect> = ocr_result
            .words
            .iter()
            .map(|w| {
                // Word boxes are in capture-local pixel coordinates.
                // Convert to global screen physical pixels using the
                // actual captured origin (not the requested band origin).
                let global_x = (cap_origin_x * cap_scale) + w.x as f64;
                let global_y = (cap_origin_y * cap_scale) + w.y as f64;
                ScreenRect::new(
                    global_x.round() as i32,
                    global_y.round() as i32,
                    w.width,
                    w.height,
                )
            })
            .collect();

        // Find which word the pointer is on.
        let line_idx = line_under(&word_rects, point);
        let focus_word = line_idx.and_then(|idx| ocr_result.words.get(idx));

        // Build the segmented text from the OCR result.
        let full_text = ocr_result.text;
        // Use the word's actual UTF-8 offset (from boundingBoxForRange),
        // not String::find which is ambiguous for repeated words.
        let focus_offset = focus_word.map_or(0, |w| w.utf8_offset);

        let segmented: SegmentedText = text::segment(&full_text, focus_offset);

        // Word bounds: the focused word's rect by index, not by text match.
        let word_bounds = line_idx
            .and_then(|idx| word_rects.get(idx))
            .map(|rect| vec![*rect])
            .unwrap_or_default();

        // Context bounds: all word rects (the full capture band).
        let context_bounds = word_rects;

        Ok(TextTarget {
            app: AppIdentity::default(),
            source: AcquisitionSource::Ocr,
            provider: ProviderInfo {
                control_id: None,
                woke_accessibility: false,
                expanded_unit: ExpandedUnit::Unknown,
            },
            segmented,
            word_bounds,
            context_bounds,
        })
    }
}
