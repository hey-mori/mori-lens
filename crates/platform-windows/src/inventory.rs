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
    /// The window's title, for telling two windows of one process apart.
    ///
    /// A title is not text under the pointer, so the rule against logging
    /// captured content does not reach it. It is still someone's document name,
    /// so the probe keeps it behind a flag and does not print it by default.
    pub title: Option<String>,
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
            let title = window.title.clone();
            // SAFETY: read-only automation queries on a handle just enumerated;
            // a window that closes mid-walk yields an error, not a crash.
            let inventory = unsafe { inventory_of(&automation, HWND(window.id as *mut _), name) };
            inventory
                .map(|found| WindowTextInventory { title, ..found })
                .filter(|found| found.text_elements > 0)
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

/// Every word a window's provider will hand over, by handle.
///
/// **For comparing one reading of a window against another**, which is the
/// only question this answers and the only reason it returns text rather than
/// a count. `hover-probe` renders a covered window's pixels, recognises them,
/// and asks how many of those words the accessibility tree also holds. That is
/// the difference between *this window exposes no text* and *this window
/// exposes text a pointer cannot reach*, and issue #14 has been open on
/// exactly that distinction.
///
/// A set rather than the text, deliberately: it answers overlap and it cannot
/// reconstruct a document's order. Never printed, and the probe reports counts
/// (`docs/privacy-model.md`).
///
/// Not used by the product, and it must not be. `lens.exe` reads what is under
/// the pointer, and a function that hands over a whole window's text is a
/// different tool.
pub fn text_words_of(window_id: i64) -> std::collections::HashSet<String> {
    let mut words = std::collections::HashSet::new();
    let _com = ComScope::enter();
    // SAFETY: creating the documented automation co-class in-process.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        return words;
    };

    // SAFETY: read-only automation queries on a handle the caller enumerated;
    // a window that closes mid-walk yields an error rather than a crash.
    unsafe {
        let Ok(element) = automation.ElementFromHandle(HWND(window_id as *mut _)) else {
            return words;
        };
        let Ok(condition) = automation
            .CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))
        else {
            return words;
        };
        let Ok(found) = element.FindAll(TreeScope_Descendants, &condition) else {
            return words;
        };
        let count = found.Length().unwrap_or(0);
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
            // A larger cap than the inventory's, because this is comparing
            // against a whole window's pixels rather than sampling.
            let Ok(text) = range.GetText(20_000) else {
                continue;
            };
            words.extend(comparable(&text.to_string()));
        }
    }
    words
}

/// Words as they compare between a recogniser and a provider.
///
/// Lower-cased and stripped to letters, digits and underscores, so that
/// punctuation the two disagree about does not count as a difference. The same
/// normalisation `lens_core::translate` uses on the other comparison of this
/// kind, and for the same reason.
fn comparable(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// Where a window's text elements are, by handle.
///
/// **Layout, never content**, which is the same thing `lens_core::barren`
/// keeps and for the same reason: a rectangle says where a line was and
/// nothing about what it said.
///
/// It exists to answer a question `ElementFromPoint` cannot be asked about a
/// covered window. That call returns the deepest element in the **global**
/// z-order, so over a buried window it answers about whatever is on top. A
/// rectangle does not care what is in front, so a grid can be tested against
/// these without arranging a desktop.
///
/// What that measures is whether a coordinate falls **inside** an element that
/// holds text, which is the step before resolving one. If a large share of a
/// window's points do and `ElementFromPoint` still resolves almost none, the
/// gap is in turning a point into an element rather than in the text being
/// absent (#14).
///
/// Not used by the product. Ordered as the tree gave them, deepest last,
/// because a caller testing containment wants the innermost match.
pub fn text_rectangles_of(window_id: i64) -> Vec<lens_core::geometry::ScreenRect> {
    let mut found = Vec::new();
    let _com = ComScope::enter();
    // SAFETY: creating the documented automation co-class in-process.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        return found;
    };

    // SAFETY: read-only automation queries on a handle the caller enumerated.
    unsafe {
        let Ok(element) = automation.ElementFromHandle(HWND(window_id as *mut _)) else {
            return found;
        };
        let Ok(condition) = automation
            .CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))
        else {
            return found;
        };
        let Ok(children) = element.FindAll(TreeScope_Descendants, &condition) else {
            return found;
        };
        let count = children.Length().unwrap_or(0);
        for index in 0..count.min(MAX_ELEMENTS) {
            let Ok(child) = children.GetElement(index) else {
                continue;
            };
            let Ok(rect) = child.CurrentBoundingRectangle() else {
                continue;
            };
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 0 && height > 0 {
                found.push(lens_core::geometry::ScreenRect::new(
                    rect.left, rect.top, width, height,
                ));
            }
        }
    }
    found
}
