#ifndef LENS_OVERLAY_H
#define LENS_OVERLAY_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Create the overlay panel. Returns true on success.
/// The panel is created hidden; call lens_overlay_show to display it.
bool lens_overlay_create(void);

/// Show the overlay with text at the given position.
/// Coordinates are in global screen points (top-left origin).
/// width and height are in points.
bool lens_overlay_show(const char *source_text,
                       const char *translation_text,
                       double x, double y,
                       double width, double height);

/// Hide the overlay.
void lens_overlay_hide(void);

/// Destroy the overlay panel and free resources.
void lens_overlay_destroy(void);

/// Returns whether the overlay is currently visible.
bool lens_overlay_is_visible(void);

/// Get the current mouse position in global screen points (top-left origin).
/// Returns true on success, writing to *x and *y.
bool lens_mouse_position(double *x, double *y);

/// Get the backing scale factor for the display containing the given point.
/// The point is in logical screen points (top-left origin).
double lens_display_scale_at(double x, double y);

/// Get the backing scale factor for the display containing the given point.
/// The point is in PHYSICAL screen pixels (top-left origin). This correctly
/// handles mixed-scale multi-monitor setups where using logical points would
/// select the wrong display.
double lens_display_scale_at_physical(double px, double py);

/// Process pending AppKit events on the current thread.
///
/// Must be called regularly from the main thread (e.g. every 25ms) when a
/// polling loop owns the main thread. Drains both the NSRunLoop (AppKit UI
/// events: menu actions, panel rendering) and the main dispatch queue
/// (notification blocks from NSWorkspace lifecycle observers).
void lens_process_appkit_events(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_OVERLAY_H */
