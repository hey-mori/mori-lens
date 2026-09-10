//! macOS screen capture.
//!
//! Captures a region of the screen into a BGRA pixel buffer using
//! ScreenCaptureKit. No image is written to disk (privacy invariant).
//!
//! The display containing the pointer is selected (not the request origin).
//! The crop is clipped to that display's bounds. The actual clipped origin
//! and scale are returned so callers can reconstruct global coordinates.

use std::os::raw::{c_int, c_void};

extern "C" {
    fn lens_capture_screen(
        req_x: f64,
        req_y: f64,
        req_width: f64,
        req_height: f64,
        ptr_x: f64,
        ptr_y: f64,
        out_pixels: *mut *mut u8,
        out_width: *mut c_int,
        out_height: *mut c_int,
        out_stride: *mut c_int,
        out_origin_x: *mut f64,
        out_origin_y: *mut f64,
        out_scale: *mut f64,
    ) -> bool;
    fn lens_capture_available() -> bool;
    fn lens_capture_request() -> bool;
}

/// Whether screen capture is available (Screen Recording permission granted).
pub fn is_available() -> bool {
    // SAFETY: lens_capture_available is a pure query.
    unsafe { lens_capture_available() }
}

/// Request Screen Recording permission from the user.
///
/// Returns true if permission is already granted. If not, presents the
/// system dialog and returns false; after the user grants permission in
/// System Settings, the application must be restarted.
pub fn request() -> bool {
    // SAFETY: lens_capture_request is a pure side-effect (shows system dialog).
    unsafe { lens_capture_request() }
}

/// A captured screen region with metadata for coordinate reconstruction.
#[derive(Debug)]
pub struct Capture {
    /// BGRA pixel data (4 bytes/pixel, row-major, top-to-bottom).
    pub pixels: Vec<u8>,
    /// Pixel width.
    pub width: i32,
    /// Pixel height.
    pub height: i32,
    /// Bytes per row.
    pub stride: i32,
    /// Global logical X of the actual (clipped) capture origin.
    pub origin_x: f64,
    /// Global logical Y of the actual (clipped) capture origin.
    pub origin_y: f64,
    /// Display scale factor (pixels per logical point).
    pub scale: f64,
}

/// Capture a region of the screen.
///
/// `req_x`, `req_y`, `req_width`, `req_height` define the requested crop
/// in **logical screen points** (top-left origin).
///
/// `ptr_x`, `ptr_y` is the pointer position in logical screen points.
/// The display containing the pointer is selected for capture.
///
/// The returned `Capture` includes the actual (possibly clipped) origin
/// and scale factor for coordinate reconstruction.
///
/// Returns `None` if the capture fails.
pub fn capture_region(
    req_x: f64,
    req_y: f64,
    req_width: f64,
    req_height: f64,
    ptr_x: f64,
    ptr_y: f64,
) -> Option<Capture> {
    let mut pixels: *mut u8 = std::ptr::null_mut();
    let mut w: c_int = 0;
    let mut h: c_int = 0;
    let mut stride: c_int = 0;
    let mut origin_x: f64 = 0.0;
    let mut origin_y: f64 = 0.0;
    let mut scale: f64 = 1.0;

    // SAFETY: lens_capture_screen writes to valid stack pointers. On success,
    // it allocates a heap buffer that we take ownership of.
    let ok = unsafe {
        lens_capture_screen(
            req_x,
            req_y,
            req_width,
            req_height,
            ptr_x,
            ptr_y,
            &mut pixels,
            &mut w,
            &mut h,
            &mut stride,
            &mut origin_x,
            &mut origin_y,
            &mut scale,
        )
    };

    if !ok || pixels.is_null() || w <= 0 || h <= 0 {
        return None;
    }

    // Take ownership of the C-allocated buffer.
    let len = (stride * h) as usize;
    // SAFETY: pixels was allocated by malloc in the C bridge with at least
    // stride * h bytes. We copy into a Vec and free the original.
    let vec = unsafe {
        let src = std::slice::from_raw_parts(pixels, len);
        let mut v = Vec::with_capacity(len);
        v.extend_from_slice(src);
        // Free the C-allocated buffer.
        extern "C" {
            fn free(ptr: *mut c_void);
        }
        free(pixels as *mut c_void);
        v
    };

    Some(Capture {
        pixels: vec,
        width: w,
        height: h,
        stride,
        origin_x,
        origin_y,
        scale,
    })
}
