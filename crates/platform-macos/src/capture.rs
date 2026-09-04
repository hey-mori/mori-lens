//! macOS screen capture.
//!
//! Captures a region of the screen into an RGBA pixel buffer using
//! `CGWindowListCreateImage`. No image is written to disk (privacy invariant).
//!
//! Requires Screen Recording permission on macOS 10.15+. Use
//! [`is_available`] to check before capturing.

use std::os::raw::{c_int, c_void};

extern "C" {
    fn lens_capture_screen(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        out_pixels: *mut *mut u8,
        out_width: *mut c_int,
        out_height: *mut c_int,
        out_stride: *mut c_int,
    ) -> bool;
    fn lens_capture_available() -> bool;
}

/// Whether screen capture is available (Screen Recording permission granted).
pub fn is_available() -> bool {
    // SAFETY: lens_capture_available is a pure query.
    unsafe { lens_capture_available() }
}

/// A captured screen region as RGBA pixels.
#[derive(Debug)]
pub struct Capture {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
}

/// Capture a region of the screen.
///
/// `x`, `y`, `width`, `height` are in **logical screen points** (top-left
/// origin). The returned pixel buffer is in physical pixels (RGBA, 4 bytes
/// per pixel, row-major, top-to-bottom).
///
/// Returns `None` if the capture fails (e.g. missing Screen Recording
/// permission, or invalid region).
pub fn capture_region(x: f64, y: f64, width: f64, height: f64) -> Option<Capture> {
    let mut pixels: *mut u8 = std::ptr::null_mut();
    let mut w: c_int = 0;
    let mut h: c_int = 0;
    let mut stride: c_int = 0;

    // SAFETY: lens_capture_screen writes to valid stack pointers. On success,
    // it allocates a heap buffer that we take ownership of.
    let ok = unsafe {
        lens_capture_screen(
            x,
            y,
            width,
            height,
            &mut pixels,
            &mut w,
            &mut h,
            &mut stride,
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
    })
}
