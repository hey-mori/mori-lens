#ifndef LENS_MENUBAR_H
#define LENS_MENUBAR_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

bool lens_menubar_create(void);
void lens_menubar_destroy(void);
void lens_menubar_set_title(const char *title);

/// Edge-triggered polls. Each returns true once per user action, then resets.
bool lens_menubar_poll_pause_toggle(void);
bool lens_menubar_poll_quit(void);
bool lens_menubar_poll_deep(void);
bool lens_menubar_poll_copy(void);
bool lens_menubar_poll_save(void);
bool lens_menubar_poll_ocr(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_MENUBAR_H */
