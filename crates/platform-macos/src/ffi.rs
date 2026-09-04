//! Raw FFI declarations for the macOS Accessibility and CoreGraphics APIs.
//!
//! Everything in this module is `unsafe` by nature: thin declarations over C
//! functions whose contracts the caller must uphold. Higher-level modules wrap
//! these in safe Rust types.
//!
//! No product logic lives here.

#![allow(non_camel_case_types, dead_code)]

use std::os::raw::{c_char, c_double, c_void};
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Opaque types
// ---------------------------------------------------------------------------

/// An Accessibility UI element reference (a CoreFoundation type).
#[repr(C)]
#[derive(Debug)]
pub struct __AXUIElement {
    _private: [u8; 0],
}

/// An Accessibility value wrapper (a CoreFoundation type).
#[repr(C)]
#[derive(Debug)]
pub struct __AXValue {
    _private: [u8; 0],
}

/// A CoreFoundation string.
#[repr(C)]
#[derive(Debug)]
pub struct __CFString {
    _private: [u8; 0],
}

/// A CoreFoundation array.
#[repr(C)]
#[derive(Debug)]
pub struct __CFArray {
    _private: [u8; 0],
}

/// A CoreFoundation data blob.
#[repr(C)]
#[derive(Debug)]
pub struct __CFData {
    _private: [u8; 0],
}

// ---------------------------------------------------------------------------
// Pointer aliases matching the C typedefs
// ---------------------------------------------------------------------------

pub type AXUIElementRef = *mut __AXUIElement;
pub type AXValueRef = *mut __AXValue;
pub type CFStringRef = *mut __CFString;
pub type CFArrayRef = *const __CFArray;
pub type CFMutableArrayRef = *mut __CFArray;
pub type CFDataRef = *const __CFData;
pub type CFTypeRef = *const c_void;
pub type CFIndex = i64;
pub type CFStringEncoding = u32;
pub type Boolean = u8;
pub type CGError = i32;

/// A CoreGraphics point (two doubles). Matches `CGPoint` in C.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CGPoint {
    pub x: c_double,
    pub y: c_double,
}

/// A CoreGraphics size.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CGSize {
    pub width: c_double,
    pub height: c_double,
}

/// A CoreGraphics rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

/// A CoreFoundation range (location + length), passed by value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CFRange {
    pub location: CFIndex,
    pub length: CFIndex,
}

// ---------------------------------------------------------------------------
// AXError
// ---------------------------------------------------------------------------

pub type AXError = i32;

pub const K_AX_ERROR_SUCCESS: AXError = 0;
pub const K_AX_ERROR_FAILURE: AXError = -25200;
pub const K_AX_ERROR_INVALID_UI_ELEMENT: AXError = -25201;
pub const K_AX_ERROR_CANNOT_COMPLETE: AXError = -25202;
pub const K_AX_ERROR_ATTRIBUTE_UNSUPPORTED: AXError = -25203;
pub const K_AX_ERROR_API_EXCEPTION: AXError = -25204;

// ---------------------------------------------------------------------------
// AXValueType
// ---------------------------------------------------------------------------

pub type AXValueType = u32;

pub const K_AX_VALUE_TYPE_CG_POINT: AXValueType = 1;
pub const K_AX_VALUE_TYPE_CG_SIZE: AXValueType = 2;
pub const K_AX_VALUE_TYPE_CG_RECT: AXValueType = 3;
pub const K_AX_VALUE_TYPE_CF_RANGE: AXValueType = 5;

// ---------------------------------------------------------------------------
// CGError
// ---------------------------------------------------------------------------

pub const K_CG_ERROR_SUCCESS: CGError = 0;

// ---------------------------------------------------------------------------
// CFString encoding
// ---------------------------------------------------------------------------

pub const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x08000100;
pub const K_CF_STRING_ENCODING_ASCII: CFStringEncoding = 0x0100;

// ---------------------------------------------------------------------------
// AX attribute name accessors
//
// The kAX*Attribute constants are CFStringRef globals from the
// ApplicationServices framework. On some macOS versions they are not
// exported as linkable symbols, so we create them at runtime from their
// known ASCII names and cache them in LazyLock statics.
// ---------------------------------------------------------------------------

/// Create a permanent CFStringRef from an ASCII name.
///
/// The string is retained once and never released (it lives for the process
/// lifetime). This is the standard pattern for creating CFString constants
/// that mirror framework globals.
fn make_cfstring(name: &str) -> CFStringRef {
    // SAFETY: CFStringCreateWithCString with a valid C string and allocator
    // returns a retained CFStringRef. We pass null allocator for default.
    unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr() as *const c_char,
            K_CF_STRING_ENCODING_ASCII,
        )
    }
}

/// A CFStringRef that is safe to share across threads.
///
/// CFStrings are immutable once created, so sharing a reference across
/// threads is sound. The raw pointer is not Send/Sync by default because
/// Rust cannot know the pointee is immutable.
struct SyncCFString(CFStringRef);

// SAFETY: CFStrings are immutable reference-counted objects. Sharing a
// CFStringRef across threads is safe because nobody mutates the string
// contents through a CFStringRef.
unsafe impl Send for SyncCFString {}
// SAFETY: Same reasoning as Send: the CFStringRef points to an immutable
// string that is safe to read from multiple threads concurrently.
unsafe impl Sync for SyncCFString {}

macro_rules! cf_attr {
    ($name:ident, $c_literal:expr) => {
        pub fn $name() -> CFTypeRef {
            static S: LazyLock<SyncCFString> =
                LazyLock::new(|| SyncCFString(make_cfstring($c_literal)));
            S.0 as CFTypeRef
        }
    };
}

cf_attr!(ax_role, "AXRole\0");
cf_attr!(ax_subrole, "AXSubrole\0");
cf_attr!(ax_value, "AXValue\0");
cf_attr!(ax_title, "AXTitle\0");
cf_attr!(ax_description, "AXDescription\0");
cf_attr!(ax_position, "AXPosition\0");
cf_attr!(ax_size, "AXSize\0");
cf_attr!(ax_parent, "AXParent\0");
cf_attr!(ax_window, "AXWindow\0");
cf_attr!(ax_top_level_ui_element, "AXTopLevelUIElement\0");
cf_attr!(ax_children, "AXChildren\0");
cf_attr!(ax_selected_text, "AXSelectedText\0");
cf_attr!(ax_selected_text_range, "AXSelectedTextRange\0");
cf_attr!(ax_visible_character_range, "AXVisibleCharacterRange\0");
cf_attr!(ax_number_of_characters, "AXNumberOfCharacters\0");
cf_attr!(ax_enabled, "AXEnabled\0");
cf_attr!(ax_process_identifier, "AXProcessIdentifier\0");
cf_attr!(ax_string_for_range, "AXStringForRange\0");
cf_attr!(ax_range_for_position, "AXRangeForPosition\0");
cf_attr!(ax_bounds_for_range, "AXBoundsForRange\0");
cf_attr!(
    ax_trusted_check_option_prompt,
    "AXTrustedCheckOptionPrompt\0"
);

// ---------------------------------------------------------------------------
// Accessibility framework functions
// ---------------------------------------------------------------------------

extern "C" {
    /// Returns whether the calling process has been trusted by the
    /// accessibility subsystem.
    pub fn AXIsProcessTrusted() -> Boolean;

    /// Returns whether the calling process is trusted, with options.
    /// `options` is a `CFDictionaryRef` (pass null for defaults).
    pub fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> Boolean;

    /// Creates a system-wide UI element representing the entire desktop.
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;

    /// Creates a UI element for the application with the given PID.
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;

    /// Copies the value of an attribute of a UI element.
    ///
    /// On success, `*value` is set to a `CFTypeRef` the caller must release.
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> AXError;

    /// Copies the value of a parameterized attribute.
    ///
    /// `parameter` is a `CFTypeRef` whose type depends on the attribute (e.g.
    /// a `CFRangeRef` for `AXStringForRange`). On success, `*value` is set to
    /// a `CFTypeRef` the caller must release.
    pub fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> AXError;

    /// Copies multiple attribute values at once.
    pub fn AXUIElementCopyAttributeValues(
        element: AXUIElementRef,
        attributes: CFArrayRef,
        index: CFIndex,
        max_values: CFIndex,
        values: *mut CFArrayRef,
    ) -> AXError;

    /// Returns the UI element at the specified screen position.
    ///
    /// The position is in the global display coordinate space with the origin
    /// at the top-left of the primary display, in **points** (not pixels).
    pub fn AXUIElementCopyElementAtPosition(
        application: AXUIElementRef,
        x: f32,
        y: f32,
        element: *mut AXUIElementRef,
    ) -> AXError;

    /// Posts a notification to a UI element.
    pub fn AXUIElementPostNotification(
        element: AXUIElementRef,
        notification: CFTypeRef,
        extra: CFTypeRef,
    ) -> AXError;

    /// Sets the value of an attribute.
    pub fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: CFTypeRef,
    ) -> AXError;

    /// Copies multiple attribute names supported by the element.
    pub fn AXUIElementCopyAttributeNames(
        element: AXUIElementRef,
        names: *mut CFArrayRef,
    ) -> AXError;

    /// Creates an AXValue wrapping a CGPoint, CGSize, CGRect, or CFRange.
    pub fn AXValueCreate(value_type: AXValueType, value: *const c_void) -> AXValueRef;

    /// Extracts the value from an AXValue into caller-provided storage.
    pub fn AXValueGetValue(
        value: AXValueRef,
        value_type: AXValueType,
        value_out: *mut c_void,
    ) -> Boolean;
}

// ---------------------------------------------------------------------------
// CoreFoundation functions
// ---------------------------------------------------------------------------

extern "C" {
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFRelease(cf: CFTypeRef);

    pub fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(array: CFArrayRef, index: CFIndex) -> *const c_void;

    pub fn CFStringCreateWithCString(
        alloc: *const c_void,
        source: *const c_char,
        encoding: CFStringEncoding,
    ) -> CFStringRef;

    pub fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;
    pub fn CFStringGetCharacters(the_string: CFStringRef, range: CFRange, buffer: *mut u16);
    pub fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: CFStringEncoding,
    ) -> Boolean;

    pub fn CFDataGetLength(the_data: CFDataRef) -> CFIndex;
    pub fn CFDataGetBytePtr(the_data: CFDataRef) -> *const u8;

    pub fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *const c_void;

    #[link_name = "kCFBooleanTrue"]
    pub static K_CF_BOOLEAN_TRUE: __CFData;
}

// ---------------------------------------------------------------------------
// CoreGraphics functions
// ---------------------------------------------------------------------------

extern "C" {
    /// Returns the number of online displays.
    pub fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut u32,
        display_count: *mut u32,
    ) -> CGError;

    /// Returns the bounds of a display in global display coordinates (points,
    /// top-left origin).
    pub fn CGDisplayBounds(display: u32) -> CGRect;

    /// Returns the width of a display in pixels.
    pub fn CGDisplayPixelsWide(display: u32) -> usize;

    /// Returns the height of a display in pixels.
    pub fn CGDisplayPixelsHigh(display: u32) -> usize;

    /// Returns the scale factor of the display containing the given point.
    pub fn CGDisplayBackingScale(display: u32) -> c_double;

    /// Returns the display ID containing the given point.
    pub fn CGDisplayGetDisplayIDFromPoint(point: CGPoint) -> u32;

    /// Returns whether a display is the main display.
    pub fn CGDisplayIsMain(display: u32) -> Boolean;
}

// Get the process name for a PID using libc `proc_name`.
//
// Declared here rather than importing a crate because the function is part of
// the macOS system library and the call is trivial.
extern "C" {
    pub fn proc_name(pid: i32, buffer: *mut c_char, buffer_size: i32) -> i32;
}
