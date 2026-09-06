#ifndef LENS_HOTKEY_H
#define LENS_HOTKEY_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Product command identifiers.
/// These map to actions the product loop can execute.
typedef enum {
    LENS_CMD_NONE = 0,
    LENS_CMD_PAUSE_TOGGLE = 1,
    LENS_CMD_DEEP = 2,
    LENS_CMD_COPY = 3,
    LENS_CMD_SAVE = 4,
    LENS_CMD_MANUAL_OCR = 5,
    LENS_CMD_QUIT = 6,
} LensCommand;

/// Install a global event tap that intercepts Cmd+Opt+key combinations
/// and maps them to product commands.
///
/// Requires Accessibility permission (which Lens already requests for AX).
/// Returns true on success.
///
/// Mapped shortcuts:
///   Cmd+Opt+L → Pause/Resume
///   Cmd+Opt+D → Deep Analysis
///   Cmd+Opt+C → Copy
///   Cmd+Opt+S → Save
///   Cmd+Opt+O → Manual OCR
///   Cmd+Opt+Q → Quit
bool lens_hotkey_install(void);

/// Remove the global event tap.
void lens_hotkey_remove(void);

/// Poll for the next pending command. Returns LENS_CMD_NONE if no command
/// is queued. Edge-triggered: each command is returned once.
LensCommand lens_hotkey_poll(void);

/// Check whether the event tap is currently enabled.
/// Returns false if the tap is not installed or has been disabled by macOS.
bool lens_hotkey_is_enabled(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_HOTKEY_H */
