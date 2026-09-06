#ifndef LENS_CAPTURE_H
#define LENS_CAPTURE_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Capture a region of the screen into an RGBA pixel buffer.
///
/// `x`, `y`, `width`, `height` are in global screen points (top-left origin).
/// The display containing (x, y) is selected. The crop region is clipped to
/// that display's bounds. Output pixels are at the display's native scale.
///
/// On return, `*out_pixels` points to a heap-allocated buffer (BGRA format,
/// row-major, top-to-bottom). The caller must free it with `free()`.
/// `*out_stride` is the number of bytes per row.
///
/// Returns true on success. Returns false if the capture failed (e.g. missing
/// Screen Recording permission, invalid region, or no display found).
bool lens_capture_screen(double x, double y,
                         double width, double height,
                         uint8_t **out_pixels,
                         int32_t *out_width,
                         int32_t *out_height,
                         int32_t *out_stride);

/// Check whether screen capture is available (Screen Recording permission).
bool lens_capture_available(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_CAPTURE_H */
