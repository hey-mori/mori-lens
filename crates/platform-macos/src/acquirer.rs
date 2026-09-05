//! macOS Accessibility text acquisition.
//!
//! Implements [`lens_core::acquire::TextAcquirer`] using the macOS AX API.
//! The chain, and what each step can fail with:
//!
//! ```text
//! AX permission check         → Unavailable   not trusted
//! AXUIElementCopyElementAtPosition → ElementGone   UI changed mid-call
//! Role / password check       → Protected       refused before any text is read
//! Walk to text element        → Unsupported     no text interface found
//! AXRangeForPosition          → Empty           no text at the point
//! Extract text + bounds       → the characters and their rectangles
//! ```
//!
//! None of this is covered by `cargo test` against real apps. The AX API needs
//! a live desktop with a real window, so the verification is running the
//! product against named applications and recording results.

use std::time::Instant;

use lens_core::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, AppIdentity, ExpandedUnit, ProtectedReason,
    ProviderInfo, TextAcquirer, TextTarget,
};
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::text::{self, SegmentedText};

use crate::coords::{self, DisplayTopology};
use crate::ffi;
use crate::permission::AxPermission;
use crate::utf;

/// How long the whole acquisition chain may take before giving up.
///
/// Mirrors the Windows adapter's chain budget. An AX call into a wedged
/// application must not stall the pointer loop indefinitely.
const CHAIN_BUDGET_MS: u64 = 400;

/// Maximum characters to extract from a single text element.
const MAX_TEXT_CHARS: usize = 4_000;

/// RAII wrapper for an `AXUIElementRef`.
///
/// Retains on creation, releases on drop. Not `Send` or `Sync` because AX
/// elements are tied to the thread that created them.
struct AxElement(ffi::AXUIElementRef);

impl AxElement {
    /// Wrap an already-retained element. Takes ownership of the caller's
    /// retain count.
    unsafe fn adopt(raw: ffi::AXUIElementRef) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Self(raw))
        }
    }

    /// Create a new reference by retaining.
    unsafe fn retain(raw: ffi::AXUIElementRef) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            // SAFETY: raw is a valid AXUIElementRef; CFRetain returns it.
            unsafe { ffi::CFRetain(raw as ffi::CFTypeRef) };
            Some(Self(raw))
        }
    }

    fn raw(&self) -> ffi::AXUIElementRef {
        self.0
    }
}

impl Drop for AxElement {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: We own this reference (either adopted or retained).
            unsafe { ffi::CFRelease(self.0 as ffi::CFTypeRef) };
        }
    }
}

/// RAII wrapper for a `CFTypeRef` returned by AX copy functions.
struct AxOwnedValue(ffi::CFTypeRef);

impl AxOwnedValue {
    unsafe fn adopt(raw: ffi::CFTypeRef) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Self(raw))
        }
    }

    fn raw(&self) -> ffi::CFTypeRef {
        self.0
    }

    fn as_element(&self) -> ffi::AXUIElementRef {
        self.0 as ffi::AXUIElementRef
    }

    fn as_string(&self) -> ffi::CFStringRef {
        self.0 as ffi::CFStringRef
    }
}

impl Drop for AxOwnedValue {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: We own this reference from a Copy function.
            unsafe { ffi::CFRelease(self.0) };
        }
    }
}

/// The macOS text acquirer.
///
/// Implements [`TextAcquirer`] by reading the Accessibility tree. Each call
/// creates a fresh system-wide element; nothing is cached across calls because
/// an element from a closed window is a crash waiting to happen.
pub struct MacTextAcquirer {
    topology: DisplayTopology,
}

impl std::fmt::Debug for MacTextAcquirer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacTextAcquirer")
            .field("displays", &self.topology.displays.len())
            .finish()
    }
}

impl Default for MacTextAcquirer {
    fn default() -> Self {
        Self::new()
    }
}

impl MacTextAcquirer {
    /// Create a new acquirer, capturing the current display topology.
    pub fn new() -> Self {
        Self {
            topology: DisplayTopology::capture(),
        }
    }

    /// Re-capture the display topology (e.g. after a display change).
    pub fn refresh_displays(&mut self) {
        self.topology = DisplayTopology::capture();
    }

    /// Get an attribute value from an element.
    ///
    /// Returns `None` if the attribute is not supported or the call fails.
    fn get_attribute(
        &self,
        element: ffi::AXUIElementRef,
        attr: ffi::CFTypeRef,
    ) -> Option<AxOwnedValue> {
        let mut value: ffi::CFTypeRef = std::ptr::null();
        // SAFETY: element is a valid AXUIElementRef. attr is a CFTypeRef from
        // a framework global. The out-pointer is valid.
        let err = unsafe { ffi::AXUIElementCopyAttributeValue(element, attr, &mut value) };
        if err != ffi::K_AX_ERROR_SUCCESS {
            return None;
        }
        // SAFETY: On success, value is a retained CFTypeRef we must release.
        unsafe { AxOwnedValue::adopt(value) }
    }

    /// Get a parameterized attribute value.
    fn get_parameterized_attribute(
        &self,
        element: ffi::AXUIElementRef,
        attr: ffi::CFTypeRef,
        parameter: ffi::CFTypeRef,
    ) -> Option<AxOwnedValue> {
        let mut value: ffi::CFTypeRef = std::ptr::null();
        // SAFETY: Same contract as get_attribute, plus a parameter.
        let err = unsafe {
            ffi::AXUIElementCopyParameterizedAttributeValue(element, attr, parameter, &mut value)
        };
        if err != ffi::K_AX_ERROR_SUCCESS {
            return None;
        }
        // SAFETY: On success, the Copy function returned a retained CFTypeRef.
        unsafe { AxOwnedValue::adopt(value) }
    }

    /// Get the string role of an element.
    fn get_role(&self, element: ffi::AXUIElementRef) -> Option<String> {
        let value = self.get_attribute(element, ffi::ax_role())?;
        // SAFETY: value is a CFTypeRef from get_attribute; if it was a string
        // attribute, as_string reinterprets it as CFStringRef.
        unsafe { utf::cf_string_to_string(value.as_string()) }
    }

    /// Get the position of an element as a CGPoint (logical points).
    #[allow(dead_code)]
    fn get_position(&self, element: ffi::AXUIElementRef) -> Option<ffi::CGPoint> {
        let value = self.get_attribute(element, ffi::ax_position())?;
        let mut point = ffi::CGPoint { x: 0.0, y: 0.0 };
        // SAFETY: value is an AXValueRef wrapping a CGPoint. We pass a pointer
        // to our stack-allocated CGPoint.
        let ok = unsafe {
            ffi::AXValueGetValue(
                value.raw() as ffi::AXValueRef,
                ffi::K_AX_VALUE_TYPE_CG_POINT,
                &mut point as *mut ffi::CGPoint as *mut std::os::raw::c_void,
            )
        };
        if ok == 0 {
            None
        } else {
            Some(point)
        }
    }

    /// Get the size of an element as a CGSize (logical points).
    #[allow(dead_code)]
    fn get_size(&self, element: ffi::AXUIElementRef) -> Option<ffi::CGSize> {
        let value = self.get_attribute(element, ffi::ax_size())?;
        let mut size = ffi::CGSize {
            width: 0.0,
            height: 0.0,
        };
        // SAFETY: Same pattern as get_position.
        let ok = unsafe {
            ffi::AXValueGetValue(
                value.raw() as ffi::AXValueRef,
                ffi::K_AX_VALUE_TYPE_CG_SIZE,
                &mut size as *mut ffi::CGSize as *mut std::os::raw::c_void,
            )
        };
        if ok == 0 {
            None
        } else {
            Some(size)
        }
    }

    /// Get the string value of an element.
    fn get_string_value(&self, element: ffi::AXUIElementRef) -> Option<String> {
        let value = self.get_attribute(element, ffi::ax_value())?;
        // SAFETY: value is a CFTypeRef from the AXValue attribute; if it is a
        // string, as_string reinterprets it as CFStringRef.
        unsafe { utf::cf_string_to_string(value.as_string()) }
    }

    /// Walk up from an element to find the nearest ancestor (or self) that
    /// has a non-empty string value, indicating it is a text element.
    fn find_text_element(&self, element: ffi::AXUIElementRef) -> Option<(AxElement, String)> {
        // Check the element itself first.
        if let Some(text) = self.get_string_value(element) {
            if !text.is_empty() {
                // SAFETY: element is a valid AXUIElementRef from a live call.
                return unsafe { AxElement::retain(element) }.map(|e| (e, text));
            }
        }

        // Walk up parents looking for text. We retain each parent as we visit
        // it so the element stays alive even after the AxOwnedValue is dropped.
        // SAFETY: element is a valid AXUIElementRef; CFRetain returns it.
        let mut current_ref = unsafe { AxElement::retain(element)? };
        for _ in 0..30 {
            let parent_val = self.get_attribute(current_ref.raw(), ffi::ax_parent())?;
            let parent_raw = parent_val.as_element();
            if let Some(text) = self.get_string_value(parent_raw) {
                if !text.is_empty() {
                    // SAFETY: parent_raw is a valid element from get_attribute.
                    return unsafe { AxElement::retain(parent_raw) }.map(|e| (e, text));
                }
            }
            // Retain the parent before parent_val drops and releases its ref.
            // SAFETY: parent_raw is valid while parent_val is alive.
            let next_ref = unsafe { AxElement::retain(parent_raw) };
            drop(parent_val);
            current_ref = next_ref?;
        }
        None
    }

    /// Check if an element (or its ancestors up to the window) is a password
    /// field.
    fn is_protected(&self, element: ffi::AXUIElementRef) -> bool {
        // Check the subrole for password indicators.
        if let Some(subrole_val) = self.get_attribute(element, ffi::ax_subrole()) {
            // SAFETY: subrole_val is a CFTypeRef from AXSubrole; reinterpreted.
            if let Some(subrole) = unsafe { utf::cf_string_to_string(subrole_val.as_string()) } {
                if subrole == "AXSecureTextField" {
                    return true;
                }
            }
        }

        // Check the role.
        if let Some(role) = self.get_role(element) {
            if role == "AXPasswordField" {
                return true;
            }
        }

        // Walk up a few levels to check for secure text field container.
        // Same retain-per-visit pattern as find_text_element to avoid leaks.
        // SAFETY: element is valid; CFRetain returns it.
        let mut current_ref = match unsafe { AxElement::retain(element) } {
            Some(r) => r,
            None => return false,
        };
        for _ in 0..5 {
            let parent_val = match self.get_attribute(current_ref.raw(), ffi::ax_parent()) {
                Some(v) => v,
                None => return false,
            };
            let parent_raw = parent_val.as_element();

            if let Some(subrole_val) = self.get_attribute(parent_raw, ffi::ax_subrole()) {
                // SAFETY: subrole_val is a CFTypeRef from AXSubrole.
                if let Some(subrole) = unsafe { utf::cf_string_to_string(subrole_val.as_string()) }
                {
                    if subrole == "AXSecureTextField" {
                        return true;
                    }
                }
            }

            // SAFETY: parent_raw is valid while parent_val is alive.
            let next_ref = unsafe { AxElement::retain(parent_raw) };
            drop(parent_val);
            current_ref = match next_ref {
                Some(r) => r,
                None => return false,
            };
        }

        false
    }

    /// Get the bounds for a character range within a text element.
    ///
    /// Returns a `ScreenRect` in physical pixels.
    fn bounds_for_range(
        &self,
        element: ffi::AXUIElementRef,
        range: ffi::CFRange,
        scale: f64,
    ) -> Option<ScreenRect> {
        // Create an AXValue wrapping the CFRange as the parameter.
        // SAFETY: We pass a pointer to a stack-allocated CFRange.
        let range_value = unsafe {
            ffi::AXValueCreate(
                ffi::K_AX_VALUE_TYPE_CF_RANGE,
                &range as *const ffi::CFRange as *const std::os::raw::c_void,
            )
        };
        if range_value.is_null() {
            return None;
        }

        let result = self.get_parameterized_attribute(
            element,
            ffi::ax_bounds_for_range(),
            range_value as ffi::CFTypeRef,
        );

        // SAFETY: We created range_value and must release it.
        unsafe { ffi::CFRelease(range_value as ffi::CFTypeRef) };

        let value = result?;

        // The result is an AXValue wrapping a CGRect.
        let mut rect = ffi::CGRect {
            origin: ffi::CGPoint { x: 0.0, y: 0.0 },
            size: ffi::CGSize {
                width: 0.0,
                height: 0.0,
            },
        };
        // SAFETY: value is an AXValueRef wrapping a CGRect.
        let ok = unsafe {
            ffi::AXValueGetValue(
                value.raw() as ffi::AXValueRef,
                ffi::K_AX_VALUE_TYPE_CG_RECT,
                &mut rect as *mut ffi::CGRect as *mut std::os::raw::c_void,
            )
        };
        if ok == 0 {
            return None;
        }

        Some(coords::cg_rect_to_screen(rect, scale))
    }

    /// Get the character range at a screen point within a text element.
    ///
    /// Uses `AXRangeForPosition` parameterized attribute.
    fn range_for_position(
        &self,
        element: ffi::AXUIElementRef,
        point: ffi::CGPoint,
    ) -> Option<ffi::CFRange> {
        // Create an AXValue wrapping the CGPoint.
        // SAFETY: Stack-allocated CGPoint, passed by pointer.
        let point_value = unsafe {
            ffi::AXValueCreate(
                ffi::K_AX_VALUE_TYPE_CG_POINT,
                &point as *const ffi::CGPoint as *const std::os::raw::c_void,
            )
        };
        if point_value.is_null() {
            return None;
        }

        let result = self.get_parameterized_attribute(
            element,
            ffi::ax_range_for_position(),
            point_value as ffi::CFTypeRef,
        );

        // SAFETY: We created point_value and must release it.
        unsafe { ffi::CFRelease(point_value as ffi::CFTypeRef) };

        let value = result?;

        // The result is an AXValue wrapping a CFRange.
        let mut range = ffi::CFRange {
            location: 0,
            length: 0,
        };
        // SAFETY: value is an AXValueRef wrapping a CFRange.
        let ok = unsafe {
            ffi::AXValueGetValue(
                value.raw() as ffi::AXValueRef,
                ffi::K_AX_VALUE_TYPE_CF_RANGE,
                &mut range as *mut ffi::CFRange as *mut std::os::raw::c_void,
            )
        };
        if ok == 0 {
            None
        } else {
            Some(range)
        }
    }

    /// Get the process name for a PID using `proc_name`.
    fn process_name_for_pid(pid: i32) -> Option<String> {
        let mut buf = [0u8; 256];
        // SAFETY: proc_name writes up to buf.len() bytes into our stack buffer.
        let len = unsafe {
            ffi::proc_name(
                pid,
                buf.as_mut_ptr() as *mut std::os::raw::c_char,
                buf.len() as i32,
            )
        };
        if len <= 0 {
            return None;
        }
        String::from_utf8(buf[..len as usize].to_vec()).ok()
    }

    /// Get the process name at a screen point without reading any text.
    ///
    /// This is a lightweight metadata lookup: it finds the AX element at the
    /// point, walks up to the application element, and extracts the PID.
    /// No text content is read. This is used for policy checking before
    /// acquisition.
    pub fn process_name_at(&self, point: ScreenPoint) -> Option<String> {
        // Check permission.
        if AxPermission::check() != AxPermission::Trusted {
            return None;
        }

        let scale = self.topology.scale_at_screen_point(point);
        let cg_point = coords::screen_point_to_cg(point, scale);

        // SAFETY: AXUIElementCreateSystemWide always returns a valid element.
        let system_wide = unsafe { AxElement::adopt(ffi::AXUIElementCreateSystemWide())? };

        let mut element_at_point: ffi::AXUIElementRef = std::ptr::null_mut();
        // SAFETY: system_wide is valid. Out-pointer is valid.
        let err = unsafe {
            ffi::AXUIElementCopyElementAtPosition(
                system_wide.raw(),
                cg_point.x as f32,
                cg_point.y as f32,
                &mut element_at_point,
            )
        };
        if err != ffi::K_AX_ERROR_SUCCESS || element_at_point.is_null() {
            return None;
        }

        // SAFETY: On success, element_at_point is a retained AXUIElementRef.
        let element = unsafe { AxElement::adopt(element_at_point)? };

        // Walk up to find the application element.
        // SAFETY: element is valid; CFRetain returns it.
        let mut current_ref = unsafe { AxElement::retain(element.raw())? };
        for _ in 0..50 {
            let role = self.get_role(current_ref.raw());
            if role.as_deref() == Some("AXApplication") {
                if let Some(pid_val) =
                    self.get_attribute(current_ref.raw(), ffi::ax_process_identifier())
                {
                    let mut pid: i32 = 0;
                    // SAFETY: pid_val.raw() is a CFNumberRef.
                    let ok = unsafe {
                        ffi::CFNumberGetValue(
                            pid_val.raw(),
                            ffi::K_CF_NUMBER_INT_TYPE,
                            &mut pid as *mut i32 as *mut std::os::raw::c_void,
                        )
                    };
                    if ok != 0 && pid > 0 {
                        return Self::process_name_for_pid(pid);
                    }
                }
                break;
            }
            let parent_val = self.get_attribute(current_ref.raw(), ffi::ax_parent())?;
            // SAFETY: parent_val.as_element() is valid while parent_val is alive.
            let next_ref = unsafe { AxElement::retain(parent_val.as_element()) };
            drop(parent_val);
            current_ref = next_ref?;
        }
        None
    }

    /// Resolve the application identity (process name, window title) from
    /// the element at the pointer. Walks up to the application element for
    /// the PID, then uses `proc_name` for the process name.
    fn resolve_app_identity(&self, element: ffi::AXUIElementRef) -> AppIdentity {
        let mut process_name: Option<String> = None;
        let mut window_title: Option<String> = None;

        // Walk up to find the application element (role = "AXApplication").
        // SAFETY: element is valid; CFRetain returns it.
        let mut current_ref = match unsafe { AxElement::retain(element) } {
            Some(r) => r,
            None => return AppIdentity::default(),
        };

        for _ in 0..50 {
            let role = self.get_role(current_ref.raw());

            // Check for application role.
            if role.as_deref() == Some("AXApplication") {
                // Get the PID via the process identifier attribute.
                if let Some(pid_val) =
                    self.get_attribute(current_ref.raw(), ffi::ax_process_identifier())
                {
                    let mut pid: i32 = 0;
                    // SAFETY: pid_val.raw() is a CFNumberRef from the AX API.
                    // CFNumberGetValue writes an i32 to our stack variable.
                    let ok = unsafe {
                        ffi::CFNumberGetValue(
                            pid_val.raw(),
                            ffi::K_CF_NUMBER_INT_TYPE,
                            &mut pid as *mut i32 as *mut std::os::raw::c_void,
                        )
                    };
                    if ok != 0 && pid > 0 {
                        process_name = Self::process_name_for_pid(pid);
                    }
                }
                break;
            }

            // Walk up.
            let parent_val = match self.get_attribute(current_ref.raw(), ffi::ax_parent()) {
                Some(v) => v,
                None => break,
            };
            // SAFETY: parent_raw is valid while parent_val is alive.
            let next_ref = unsafe { AxElement::retain(parent_val.as_element()) };
            drop(parent_val);
            current_ref = match next_ref {
                Some(r) => r,
                None => break,
            };
        }

        // Try to get the window title by walking up to the window element.
        // SAFETY: element is valid; CFRetain returns it.
        if let Some(mut win_ref) = unsafe { AxElement::retain(element) } {
            for _ in 0..30 {
                let role = self.get_role(win_ref.raw());
                if role.as_deref() == Some("AXWindow") {
                    if let Some(title_val) = self.get_attribute(win_ref.raw(), ffi::ax_title()) {
                        // SAFETY: title_val is a CFTypeRef from AXTitle attribute.
                        window_title = unsafe { utf::cf_string_to_string(title_val.as_string()) };
                    }
                    break;
                }
                let parent_val = match self.get_attribute(win_ref.raw(), ffi::ax_parent()) {
                    Some(v) => v,
                    None => break,
                };
                // SAFETY: parent_val.as_element() is a valid AXUIElementRef.
                let next_ref = unsafe { AxElement::retain(parent_val.as_element()) };
                drop(parent_val);
                win_ref = match next_ref {
                    Some(r) => r,
                    None => break,
                };
            }
        }

        AppIdentity {
            process_name,
            window_title,
        }
    }
}

impl TextAcquirer for MacTextAcquirer {
    fn acquire(
        &self,
        point: ScreenPoint,
        _context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        let deadline = Instant::now();
        let budget = std::time::Duration::from_millis(CHAIN_BUDGET_MS);

        // 1. Check permission.
        match AxPermission::check() {
            AxPermission::Trusted => {}
            AxPermission::NotTrusted => return Err(AcquireError::Unavailable),
        }

        if deadline.elapsed() >= budget {
            return Err(AcquireError::Timeout);
        }

        // 2. Convert Lens physical pixels to CG logical points.
        let scale = self.topology.scale_at_screen_point(point);
        let cg_point = coords::screen_point_to_cg(point, scale);

        // 3. Get the system-wide element and find the element at position.
        // SAFETY: AXUIElementCreateSystemWide always returns a valid element.
        let system_wide = unsafe {
            AxElement::adopt(ffi::AXUIElementCreateSystemWide()).ok_or(AcquireError::Unavailable)?
        };

        let mut element_at_point: ffi::AXUIElementRef = std::ptr::null_mut();
        // SAFETY: system_wide is a valid system-wide element. The out-pointer
        // is valid. x/y are in logical points.
        let err = unsafe {
            ffi::AXUIElementCopyElementAtPosition(
                system_wide.raw(),
                cg_point.x as f32,
                cg_point.y as f32,
                &mut element_at_point,
            )
        };

        if err != ffi::K_AX_ERROR_SUCCESS || element_at_point.is_null() {
            return Err(AcquireError::ElementGone);
        }

        // SAFETY: On success, element_at_point is a retained AXUIElementRef.
        let element =
            unsafe { AxElement::adopt(element_at_point).ok_or(AcquireError::ElementGone)? };

        if deadline.elapsed() >= budget {
            return Err(AcquireError::Timeout);
        }

        // 4. Check for protected fields before reading any text.
        if self.is_protected(element.raw()) {
            return Err(AcquireError::Protected(ProtectedReason::Declared));
        }

        // 5. Find the enclosing text element.
        let (text_element, full_text) = self
            .find_text_element(element.raw())
            .ok_or(AcquireError::Unsupported)?;

        if full_text.is_empty() {
            return Err(AcquireError::Empty);
        }

        if deadline.elapsed() >= budget {
            return Err(AcquireError::Timeout);
        }

        // 6. Resolve the character offset at the pointer.
        let char_range = self
            .range_for_position(text_element.raw(), cg_point)
            .ok_or(AcquireError::Empty)?;

        // Convert the CFRange (UTF-16) to a byte offset in our Rust string.
        let (byte_start, _byte_end) = utf::cf_range_to_byte_range(&full_text, char_range);
        let focus_byte = byte_start.min(full_text.len());

        if deadline.elapsed() >= budget {
            return Err(AcquireError::Timeout);
        }

        // 7. Segment the text around the focus point.
        // Truncate to the maximum allowed length, stopping at a char boundary
        // to avoid splitting a multi-byte character (CJK, emoji, etc.).
        let truncated = if full_text.len() > MAX_TEXT_CHARS {
            let mut end = MAX_TEXT_CHARS;
            while end > 0 && !full_text.is_char_boundary(end) {
                end -= 1;
            }
            &full_text[..end]
        } else {
            &full_text
        };
        let focus_clamped = focus_byte.min(truncated.len());
        let segmented: SegmentedText = text::segment(truncated, focus_clamped);

        // 8. Get bounds for the word and context ranges.
        let word_bounds = segmented
            .word()
            .and_then(|span| {
                let cf_range = utf::byte_range_to_cf_range(truncated, span.start, span.end);
                self.bounds_for_range(text_element.raw(), cf_range, scale)
            })
            .map(|r| vec![r])
            .unwrap_or_default();

        let sentence_bounds = {
            let span = segmented.sentence();
            let cf_range = utf::byte_range_to_cf_range(truncated, span.start, span.end);
            self.bounds_for_range(text_element.raw(), cf_range, scale)
                .map(|r| vec![r])
                .unwrap_or_default()
        };

        // 9. Build the app identity.
        let app = self.resolve_app_identity(element.raw());

        // 10. Classify the expanded unit.
        let expanded_unit = segmented
            .word()
            .map(|span| ExpandedUnit::classify(span.slice(truncated)))
            .unwrap_or(ExpandedUnit::Unknown);

        Ok(TextTarget {
            app,
            source: AcquisitionSource::Accessibility,
            provider: ProviderInfo {
                control_id: None,
                woke_accessibility: false,
                expanded_unit,
            },
            segmented,
            word_bounds,
            context_bounds: sentence_bounds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquirer_can_be_constructed() {
        // This captures the display topology, which works without AX
        // permission.
        let acquirer = MacTextAcquirer::new();
        // On a machine with no displays (headless CI), the topology may be
        // empty, which is fine. The point is that construction succeeded.
        let _ = &acquirer;
    }

    #[test]
    fn acquire_without_permission_returns_unavailable() {
        // If the process is not trusted (common in CI), acquire should return
        // Unavailable rather than panicking.
        let acquirer = MacTextAcquirer::new();
        let ctx = AcquireContext::new(lens_core::hover::Generation(0));
        let result = acquirer.acquire(ScreenPoint::new(100, 100), &ctx);
        // The result depends on whether AX permission is granted. If not
        // trusted, it must be Unavailable. If trusted, it may be anything.
        if AxPermission::check() == AxPermission::NotTrusted {
            assert!(
                matches!(result, Err(AcquireError::Unavailable)),
                "expected Unavailable when not trusted, got {result:?}"
            );
        }
    }
}
