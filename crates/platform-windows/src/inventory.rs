//! What a window holds, asked of the window rather than of the screen.
//!
//! `ElementFromHandle` takes a window handle and no coordinate, so a provider's
//! tree can be walked while the window is completely covered. That removes the
//! occlusion limit from measurement, though not from the product: the pointer
//! is still a coordinate and `ElementFromPoint` still answers with whatever is
//! on top.
//!
//! The valuable part is not the convenience. It is that a tree walk **knows**
//! which elements hold text and where each one is, so a resolution can be
//! checked against the element it should have hit. The grid sweep never had
//! that: it samples hopefully and judges a hit by whether the returned
//! rectangle happens to contain the point.

use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    TreeScope_Descendants, UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
};

use crate::com::ComScope;
use crate::window::top_level_windows;

/// What one window holds.
#[derive(Debug, Clone, Default)]
pub struct WindowTextInventory {
    pub process_name: String,
    /// Descendants advertising a text pattern.
    pub text_elements: usize,
    /// How many of those returned any characters.
    pub with_content: usize,
    pub characters: usize,
    /// The shortest text element. A window whose smallest is its own height
    /// holds its text in one piece, and a pointer cannot aim inside it.
    pub smallest_height: Option<i32>,
    /// Elements a resolution was checked against.
    pub probed: usize,
    /// How many of those resolved back onto the text they were taken from.
    pub resolved_into_element: usize,
    /// Probes whose resolution came back with no rectangle at all.
    ///
    /// Not a miss. Text that resolved but is not rendered is scrolled out of
    /// view, and the product treats that as on target for the same reason:
    /// `TextTarget::is_on_target` returns true when there is no rectangle,
    /// because unrendered is not the same as mislocated. Counted separately so
    /// the instrument and the product answer alike and neither hides it.
    pub unlocated: usize,
    /// How far each miss landed from its probe point, in pixels.
    ///
    /// Kept rather than reduced to a count, because the two kinds of miss are
    /// different facts: a few pixels is glyph-box slack, and hundreds is the
    /// provider answering with text somewhere else entirely. Collapsing them
    /// is the mistake ADR-0009 exists to correct.
    pub miss_distances: Vec<f64>,
}

/// How many elements to walk per window.
///
/// A browser tab can advertise thousands, and walking all of them makes the
/// provider work as hard as the grid sweep did. This is a survey, not a census.
const MAX_ELEMENTS: i32 = 200;

/// How many of those to check a resolution against.
const MAX_PROBES: usize = 12;

/// Inventory every top-level window whose process name passes `wanted`.
pub fn text_inventory(wanted: impl Fn(&str) -> bool) -> Vec<WindowTextInventory> {
    let _com = ComScope::enter();
    // SAFETY: creating the documented automation co-class in-process.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        return Vec::new();
    };

    top_level_windows()
        .into_iter()
        .filter_map(|window| {
            let name = window.process_name.clone().unwrap_or_else(|| "?".into());
            if !wanted(&name) {
                return None;
            }
            // SAFETY: read-only automation queries on a handle just enumerated;
            // a window that closes mid-walk yields an error, not a crash.
            let inventory = unsafe { inventory_of(&automation, HWND(window.id as *mut _), name) };
            inventory.filter(|found| found.text_elements > 0)
        })
        .collect()
}

/// # Safety
/// `hwnd` must have been a valid window handle when obtained.
unsafe fn inventory_of(
    automation: &IUIAutomation,
    hwnd: HWND,
    process_name: String,
) -> Option<WindowTextInventory> {
    // SAFETY: every call is a read-only query that returns an error rather
    // than a null this could dereference.
    unsafe {
        let element = automation.ElementFromHandle(hwnd).ok()?;
        let condition = automation
            .CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))
            .ok()?;
        let found = element.FindAll(TreeScope_Descendants, &condition).ok()?;
        let count = found.Length().unwrap_or(0);

        let mut inventory = WindowTextInventory {
            process_name,
            text_elements: count as usize,
            ..WindowTextInventory::default()
        };

        for index in 0..count.min(MAX_ELEMENTS) {
            let Ok(child) = found.GetElement(index) else {
                continue;
            };
            let Some(pattern) = text_pattern(&child) else {
                continue;
            };
            let Ok(range) = pattern.DocumentRange() else {
                continue;
            };
            let text = range
                .GetText(400)
                .map(|t| t.to_string())
                .unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }

            inventory.with_content += 1;
            inventory.characters += text.chars().count();

            let Ok(rect) = child.CurrentBoundingRectangle() else {
                continue;
            };
            let height = rect.bottom - rect.top;
            if height > 0 {
                inventory.smallest_height =
                    Some(inventory.smallest_height.map_or(height, |s| s.min(height)));
            }

            if inventory.probed < MAX_PROBES {
                // Only counted when a probe point could be found at all. An
                // element whose text is scrolled out of view has no rendered
                // rectangle, and scoring that as a miss would blame the
                // provider for the window's scroll position.
                if let Some(located) = resolves_onto_its_own_text(&pattern) {
                    inventory.probed += 1;
                    match located {
                        // The product's rule, not strict containment: a
                        // resolution within about a line of the point is the
                        // text the pointer is on (ADR-0009).
                        Some(distance) if distance <= f64::from(height.max(1)) => {
                            inventory.resolved_into_element += 1;
                        }
                        Some(distance) => inventory.miss_distances.push(distance),
                        None => inventory.unlocated += 1,
                    }
                }
            }
        }

        Some(inventory)
    }
}

/// Does a point that is provably on rendered text resolve back to that text?
///
/// The question the grid sweep could not ask, because it never knew whether a
/// sampled point had text under it at all.
///
/// **The probe point comes from the text's own rectangles, not from the
/// element's.** An element's bounding rectangle includes padding and, for a
/// container, the whole empty scroll area; its centre is often whitespace, and
/// a resolution from there measures the same "nearest text to an empty region"
/// behaviour already known rather than anything new. `GetBoundingRectangles`
/// on the element's `DocumentRange` returns one rectangle per **rendered line**
/// of actual text, so a point inside the first of those is on a glyph.
///
/// Asked of the element's own text pattern rather than through
/// `ElementFromPoint`, which is why a covered window can still answer.
///
/// # Safety
/// `pattern` must belong to a live element.
unsafe fn resolves_onto_its_own_text(pattern: &IUIAutomationTextPattern) -> Option<Option<f64>> {
    // SAFETY: read-only queries against a pattern this element owns.
    unsafe {
        let document = pattern.DocumentRange().ok()?;
        let lines = document.GetBoundingRectangles().ok()?;
        let first = first_rect(&lines)?;

        // Inside the first line of text, a little in from its left edge rather
        // than at its centre: a long line's centre can fall in a gap between
        // words, and this is asking whether the provider locates text at all,
        // not how it treats spaces.
        let point = windows::Win32::Foundation::POINT {
            x: (first.0 + first.2 * 0.25).round() as i32,
            y: (first.1 + first.3 * 0.5).round() as i32,
        };

        let hit = pattern.RangeFromPoint(point).ok()?;
        let line = hit.Clone().ok()?;
        line.ExpandToEnclosingUnit(windows::Win32::UI::Accessibility::TextUnit_Line)
            .ok()?;
        let bounds = line.GetBoundingRectangles().ok()?;
        Some(distance_to_nearest(&bounds, point))
    }
}

/// The first rectangle in a `GetBoundingRectangles` array, as left, top, width,
/// height.
///
/// # Safety
/// `array` must be a `SAFEARRAY` of `f64` from `GetBoundingRectangles`.
unsafe fn first_rect(
    array: &*mut windows::Win32::System::Com::SAFEARRAY,
) -> Option<(f64, f64, f64, f64)> {
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData,
    };
    // SAFETY: bounds read before the data; the lock is always released.
    unsafe {
        let lower = SafeArrayGetLBound(*array, 1).unwrap_or(0);
        let upper = SafeArrayGetUBound(*array, 1).unwrap_or(-1);
        if upper - lower + 1 < 4 {
            return None;
        }
        let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
        if SafeArrayAccessData(*array, &mut data).is_err() {
            return None;
        }
        let values = std::slice::from_raw_parts(data.cast::<f64>(), 4);
        let rect = (values[0], values[1], values[2], values[3]);
        let _ = SafeArrayUnaccessData(*array);
        // A zero-sized rectangle is text that is not rendered: scrolled out of
        // view, or collapsed. There is nothing to point at.
        (rect.2 > 0.0 && rect.3 > 0.0).then_some(rect)
    }
}

/// How far a point is from the nearest rectangle, zero when inside one.
///
/// A distance rather than a yes or no. Strict containment was already wrong
/// once: it discarded a fifth of good hits over a few pixels of glyph-box
/// slack, which is why the product gates on a line height rather than on
/// containment (ADR-0009). Measuring the same way here keeps the instrument
/// and the product asking the same question.
///
/// `GetBoundingRectangles` returns a flat array of left, top, width, height in
/// fours. Reading it as anything else silently produces plausible rectangles,
/// which is the kind of mistake that looks like a finding.
///
/// # Safety
/// `array` must be a `SAFEARRAY` of `f64` from `GetBoundingRectangles`.
unsafe fn distance_to_nearest(
    array: &*mut windows::Win32::System::Com::SAFEARRAY,
    point: windows::Win32::Foundation::POINT,
) -> Option<f64> {
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData,
    };

    // SAFETY: bounds are read before the data, and the lock is always released.
    unsafe {
        let lower = SafeArrayGetLBound(*array, 1).unwrap_or(0);
        let upper = SafeArrayGetUBound(*array, 1).unwrap_or(-1);
        if upper < lower {
            return None;
        }
        let len = (upper - lower + 1) as usize;
        let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
        if SafeArrayAccessData(*array, &mut data).is_err() {
            return None;
        }
        let values = std::slice::from_raw_parts(data.cast::<f64>(), len);

        let (x, y) = (f64::from(point.x), f64::from(point.y));
        let mut nearest = f64::INFINITY;
        for chunk in values.chunks_exact(4) {
            let (left, top, width, height) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            let dx = (left - x).max(0.0).max(x - (left + width));
            let dy = (top - y).max(0.0).max(y - (top + height));
            nearest = nearest.min(dx.hypot(dy));
        }
        let _ = SafeArrayUnaccessData(*array);
        nearest.is_finite().then_some(nearest)
    }
}

/// # Safety
/// `element` must be live.
unsafe fn text_pattern(element: &IUIAutomationElement) -> Option<IUIAutomationTextPattern> {
    // SAFETY: a provider without the pattern returns an error.
    unsafe {
        element
            .GetCurrentPattern(UIA_TextPatternId)
            .ok()?
            .cast::<IUIAutomationTextPattern>()
            .ok()
    }
}
