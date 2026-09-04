#ifndef LENS_MENUBAR_H
#define LENS_MENUBAR_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Create the menu bar item. Returns true on success.
bool lens_menubar_create(void);

/// Destroy the menu bar item.
void lens_menubar_destroy(void);

/// Update the menu bar title (e.g. "Lens" or "Lens (paused)").
void lens_menubar_set_title(const char *title);

/// Returns whether the user has toggled pause via the menu.
/// Clears the flag after reading (edge-triggered).
bool lens_menubar_poll_pause_toggle(void);

/// Returns whether the user has requested quit.
bool lens_menubar_poll_quit(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_MENUBAR_H */
