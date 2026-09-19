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

/// A configurable hotkey binding: (modifiers, keycode) → command.
/// The Rust layer translates semantic shortcuts (e.g., "Cmd+Opt+L") into
/// native modifier flags and keycodes, then calls lens_hotkey_configure()
/// to install the mapping.
typedef struct {
    /// CGEventFlags mask (e.g., kCGEventFlagMaskCommand | kCGEventFlagMaskAlternate).
    uint32_t modifiers;
    /// macOS virtual keycode (e.g., 0x25 for L).
    uint16_t keycode;
    /// The LensCommand to enqueue when this binding matches.
    int command;
} LensHotkeyBinding;

/// Install a global event tap for hotkeys.
///
/// Requires Accessibility permission (which Lens already requests for AX).
/// Returns true on success. After installing, call lens_hotkey_configure()
/// to set the actual shortcut bindings.
bool lens_hotkey_install(void);

/// Remove the global event tap.
void lens_hotkey_remove(void);

/// Configure the hotkey bindings.
///
/// Replaces the current binding table with the provided array. The event tap
/// callback uses this table to match incoming key events. Pass NULL and 0
/// to clear all bindings (no hotkeys will fire).
///
/// This function is safe to call while the event tap is active. The binding
/// table is swapped atomically from the callback's perspective.
void lens_hotkey_configure(const LensHotkeyBinding* bindings, int count);

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
