#ifndef LENS_CLIPBOARD_H
#define LENS_CLIPBOARD_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Put text on the system clipboard. Returns true on success.
bool lens_clipboard_put_text(const char *text);

#ifdef __cplusplus
}
#endif

#endif /* LENS_CLIPBOARD_H */
