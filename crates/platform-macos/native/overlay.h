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

#ifdef __cplusplus
}
#endif

#endif /* LENS_OVERLAY_H */
