#ifndef LENS_CAPTURE_H
#define LENS_CAPTURE_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Capture a region of the screen into a BGRA pixel buffer.
///
/// `req_x`, `req_y`, `req_width`, `req_height` define the requested crop
/// region in global screen logical points (top-left origin).
///
/// `ptr_x`, `ptr_y` is the pointer position in global screen logical points.
/// The display containing the POINTER is selected (not the request origin),
/// so that OCR near a display edge selects the correct monitor.
///
/// The crop region is clipped to the selected display's bounds. If the
/// clipped region is empty, returns false.
///
/// Output parameters:
///   `*out_pixels`  — heap-allocated BGRA buffer (4 bytes/pixel, row-major,
///                    top-to-bottom). Caller must `free()` it.
///   `*out_width`   — pixel width of the captured region.
///   `*out_height`  — pixel height of the captured region.
///   `*out_stride`  — bytes per row.
///   `*out_origin_x` — global logical X of the actual (clipped) capture origin.
///   `*out_origin_y` — global logical Y of the actual (clipped) capture origin.
///   `*out_scale`   — display scale factor (pixels per logical point).
///
/// Returns true on success, false on failure.
bool lens_capture_screen(double req_x, double req_y,
                         double req_width, double req_height,
                         double ptr_x, double ptr_y,
                         uint8_t **out_pixels,
                         int32_t *out_width,
                         int32_t *out_height,
                         int32_t *out_stride,
                         double *out_origin_x,
                         double *out_origin_y,
                         double *out_scale);

/// Check whether screen capture is available (Screen Recording permission).
bool lens_capture_available(void);

/// Request Screen Recording permission from the user.
///
/// On macOS 12.3+, this calls CGRequestScreenCaptureAccess which presents
/// a system dialog. After the user grants permission in System Settings,
/// the application must be restarted for the permission to take effect.
/// Returns true if permission was already granted, false otherwise.
bool lens_capture_request(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_CAPTURE_H */
