//! macOS screen capture.
//!
//! Captures a region of the screen into a BGRA pixel buffer using
//! ScreenCaptureKit. No image is written to disk (privacy invariant).
//!
//! The display containing the pointer is selected (not the request origin).
//! The crop is clipped to that display's bounds. The actual clipped origin
//! and scale are returned so callers can reconstruct global coordinates.

use std::os::raw::{c_int, c_void};

/// Validate frame dimensions and compute stride * height.
///
/// Returns None if:
/// - width, height, or stride are not positive
/// - width * 4 overflows (minimum bytes per row for BGRA)
/// - stride < width * 4 (stride must cover at least one full row of pixels)
/// - stride * height overflows
fn checked_frame_layout(width: i32, height: i32, stride: i32) -> Option<usize> {
    if width <= 0 || height <= 0 || stride <= 0 {
        return None;
    }
    let min_stride = (width as usize).checked_mul(4)?;
    if (stride as usize) < min_stride {
        return None;
    }
    (stride as usize).checked_mul(height as usize)
}

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

    if !ok {
        return None;
    }

    // Defensive checks: the C bridge only sets pixels on success, but guard
    // against future changes. If we reject after a successful return, free
    // the buffer to avoid leaking it.
    if pixels.is_null() || w <= 0 || h <= 0 {
        if !pixels.is_null() {
            // SAFETY: pixels was allocated by malloc in the C bridge.
            unsafe {
                extern "C" {
                    fn free(ptr: *mut c_void);
                }
                free(pixels as *mut c_void);
            }
        }
        return None;
    }

    // Checked arithmetic: stride * h could overflow on very large displays.
    // Also validates that stride covers at least width * 4 bytes (BGRA).
    let len = match checked_frame_layout(w, h, stride) {
        Some(n) => n,
        None => {
            // SAFETY: pixels was allocated by malloc in the C bridge.
            unsafe {
                extern "C" {
                    fn free(ptr: *mut c_void);
                }
                free(pixels as *mut c_void);
            }
            return None;
        }
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_frame_layout_normal_case() {
        // 1920x1080 at 4 bytes/pixel = 7680 stride
        assert_eq!(checked_frame_layout(1920, 1080, 7680), Some(8294400));
    }

    #[test]
    fn checked_frame_layout_zero_stride() {
        assert_eq!(checked_frame_layout(100, 100, 0), None);
    }

    #[test]
    fn checked_frame_layout_zero_height() {
        assert_eq!(checked_frame_layout(100, 0, 400), None);
    }

    #[test]
    fn checked_frame_layout_zero_width() {
        assert_eq!(checked_frame_layout(0, 100, 400), None);
    }

    #[test]
    fn checked_frame_layout_negative_stride() {
        assert_eq!(checked_frame_layout(100, 100, -1), None);
    }

    #[test]
    fn checked_frame_layout_negative_height() {
        assert_eq!(checked_frame_layout(100, -1, 400), None);
    }

    #[test]
    fn checked_frame_layout_negative_width() {
        assert_eq!(checked_frame_layout(-1, 100, 400), None);
    }

    #[test]
    fn checked_frame_layout_stride_too_small_for_width() {
        // width=100 needs at least 400 bytes/row (4 bytes/pixel)
        // stride=399 is less than width*4
        assert_eq!(checked_frame_layout(100, 100, 399), None);
    }

    #[test]
    fn checked_frame_layout_stride_exactly_width_times_four() {
        // Minimum valid: stride = width * 4 (no padding)
        assert_eq!(checked_frame_layout(100, 50, 400), Some(20000));
    }

    #[test]
    fn checked_frame_layout_stride_with_padding() {
        // Stride can be larger than width*4 (row padding)
        assert_eq!(checked_frame_layout(100, 50, 512), Some(25600));
    }

    #[test]
    fn checked_frame_layout_overflow() {
        let large_stride = i32::MAX / 2;
        let large_height = i32::MAX;
        #[cfg(target_pointer_width = "64")]
        assert!(checked_frame_layout(1, large_height, large_stride).is_some());
        #[cfg(target_pointer_width = "32")]
        assert_eq!(checked_frame_layout(1, large_height, large_stride), None);
    }

    #[test]
    fn checked_frame_layout_small_capture() {
        // Minimum sensible capture: 1x1 pixel at 4 bytes/pixel
        assert_eq!(checked_frame_layout(1, 1, 4), Some(4));
    }
}
