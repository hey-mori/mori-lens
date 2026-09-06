// Mori Lens · macOS global hotkeys via CGEventTap.
//
// Intercepts Cmd+Opt+key combinations globally and maps them to product
// commands. Uses CGEventTapCreate with Default (non-ListenOnly) option
// so matched hotkeys are consumed and not forwarded to the foreground app.
//
// The event tap runs on the main thread's run loop. Commands are queued
// in a bounded ring buffer and polled by the Rust product loop.
//
// Contract:
//   - Single producer (CGEventTap callback on main thread run loop).
//   - Single consumer (Rust product loop on worker thread).
//   - Only the producer writes head. Only the consumer writes tail.
//   - True SPSC: no shared mutable state between producer and consumer.
//   - On overflow, the newest command is dropped (not the oldest).
//   - Critical commands (Quit, Pause) use separate atomic flags that
//     bypass the ring buffer, guaranteeing delivery even under storm.
//   - If macOS disables the tap (timeout), the callback re-enables it.

#import "hotkey.h"
#import <CoreGraphics/CoreGraphics.h>
#import <Carbon/Carbon.h>
#import <stdatomic.h>

// Ring buffer for pending commands (power-of-2 size for fast modulo).
// True SPSC: only the producer writes head, only the consumer writes tail.
// On overflow, the NEWEST command is dropped (not the oldest), preserving
// the SPSC invariant. Critical commands (Quit, Pause) use separate atomic
// flags that bypass the ring buffer entirely, guaranteeing delivery.
#define CMD_QUEUE_SIZE 64
static atomic_int g_cmdQueue[CMD_QUEUE_SIZE];
static atomic_int g_cmdHead = 0;  // write position (producer only)
static atomic_int g_cmdTail = 0;  // read position (consumer only)

// Critical command flags — bypass the ring buffer.
// Quit uses a bool (idempotent command).
// Pause uses a counter to preserve toggle multiplicity: two rapid presses
// must produce two toggle events, not collapse into one.
static atomic_bool g_quitFlag = false;
static atomic_int g_pauseCount = 0;

static void enqueue_command(LensCommand cmd) {
    // Critical commands use dedicated atomics — never dropped.
    if (cmd == LENS_CMD_QUIT) {
        atomic_store(&g_quitFlag, true);
        return;
    }
    if (cmd == LENS_CMD_PAUSE_TOGGLE) {
        atomic_fetch_add(&g_pauseCount, 1);
        return;
    }

    int head = atomic_load(&g_cmdHead);
    int tail = atomic_load(&g_cmdTail);
    int next_head = (head + 1) % CMD_QUEUE_SIZE;

    if (next_head == tail) {
        // Queue full — drop the NEWEST command (the one we're trying to
        // enqueue). This preserves the SPSC invariant: only the producer
        // writes head, only the consumer writes tail. The oldest unconsumed
        // command remains in the queue.
        return;
    }

    atomic_store(&g_cmdQueue[head], (int)cmd);
    atomic_store(&g_cmdHead, next_head);
}

static LensCommand map_key_to_command(uint16_t keycode) {
    // macOS virtual keycodes (from HIToolbox/Events.h):
    //   0x25 = L (pause)
    //   0x02 = D (deep)
    //   0x08 = C (copy)
    //   0x01 = S (save)
    //   0x1F = O (ocr)
    //   0x0C = Q (quit)
    switch (keycode) {
        case 0x25: return LENS_CMD_PAUSE_TOGGLE;   // L
        case 0x02: return LENS_CMD_DEEP;            // D
        case 0x08: return LENS_CMD_COPY;            // C
        case 0x01: return LENS_CMD_SAVE;            // S
        case 0x1F: return LENS_CMD_MANUAL_OCR;      // O
        case 0x0C: return LENS_CMD_QUIT;            // Q
        default:   return LENS_CMD_NONE;
    }
}

// Forward declarations for the event tap (used in callback for re-enable).
static CFMachPortRef g_eventTap = NULL;
static CFRunLoopSourceRef g_runLoopSource = NULL;

static CGEventRef hotkey_callback(CGEventTapProxy proxy, CGEventType type,
                                   CGEventRef event, void *userInfo) {
    (void)proxy;
    (void)userInfo;

    // Handle tap-disabled events: re-enable the tap so hotkeys keep working.
    if (type == kCGEventTapDisabledByTimeout
        || type == kCGEventTapDisabledByUserInput) {
        if (g_eventTap) {
            CGEventTapEnable(g_eventTap, true);
        }
        return event;
    }

    if (type != kCGEventKeyDown) return event;

    // Check modifiers: Cmd+Opt (no Ctrl, no Shift).
    CGEventFlags flags = CGEventGetFlags(event);
    bool cmd = (flags & kCGEventFlagMaskCommand) != 0;
    bool opt = (flags & kCGEventFlagMaskAlternate) != 0;
    bool ctrl = (flags & kCGEventFlagMaskControl) != 0;
    bool shift = (flags & kCGEventFlagMaskShift) != 0;

    if (!cmd || !opt || ctrl || shift) return event;

    uint16_t keycode = (uint16_t)CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    LensCommand cmd_val = map_key_to_command(keycode);
    if (cmd_val != LENS_CMD_NONE) {
        enqueue_command(cmd_val);
        // Consume the event so it doesn't reach the foreground app.
        // This requires kCGEventTapOptionDefault (not ListenOnly).
        return NULL;
    }

    return event;
}

bool lens_hotkey_install(void) {
    if (g_eventTap) return true;  // Already installed.

    // Tap all keyDown events globally.
    CGEventMask mask = CGEventMaskBit(kCGEventKeyDown);

    g_eventTap = CGEventTapCreate(
        kCGSessionEventTap,          // session-level (not system-wide)
        kCGHeadInsertEventTap,       // insert before other taps
        kCGEventTapOptionDefault,    // we consume matched events (return NULL)
        mask,
        hotkey_callback,
        NULL);

    if (!g_eventTap) return false;

    g_runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, g_eventTap, 0);
    if (!g_runLoopSource) {
        CFRelease(g_eventTap);
        g_eventTap = NULL;
        return false;
    }

    CFRunLoopAddSource(CFRunLoopGetMain(), g_runLoopSource, kCFRunLoopCommonModes);
    CGEventTapEnable(g_eventTap, true);

    return true;
}

void lens_hotkey_remove(void) {
    if (g_eventTap) {
        CGEventTapEnable(g_eventTap, false);
        if (g_runLoopSource) {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), g_runLoopSource, kCFRunLoopCommonModes);
            CFRelease(g_runLoopSource);
            g_runLoopSource = NULL;
        }
        CFRelease(g_eventTap);
        g_eventTap = NULL;
    }
}

LensCommand lens_hotkey_poll(void) {
    // Critical commands have priority — check flags first.
    if (atomic_load(&g_quitFlag)) {
        atomic_store(&g_quitFlag, false);
        return LENS_CMD_QUIT;
    }
    // Pause uses a counter to preserve multiplicity.
    // Each press increments; each poll decrements and returns one toggle.
    int pauseCount = atomic_load(&g_pauseCount);
    if (pauseCount > 0) {
        atomic_fetch_sub(&g_pauseCount, 1);
        return LENS_CMD_PAUSE_TOGGLE;
    }

    int tail = atomic_load(&g_cmdTail);
    int head = atomic_load(&g_cmdHead);
    if (tail == head) return LENS_CMD_NONE;

    LensCommand cmd = (LensCommand)atomic_load(&g_cmdQueue[tail]);
    atomic_store(&g_cmdTail, (tail + 1) % CMD_QUEUE_SIZE);
    return cmd;
}

/// Check whether the event tap is currently enabled.
bool lens_hotkey_is_enabled(void) {
    if (!g_eventTap) return false;
    return CGEventTapIsEnabled(g_eventTap);
}
