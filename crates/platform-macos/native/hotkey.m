// Mori Lens · macOS global hotkeys via CGEventTap.
//
// Intercepts Cmd+Opt+key combinations globally and maps them to product
// commands. Uses CGEventTapCreate which requires Accessibility permission
// (already requested by Lens for AX text acquisition).
//
// The event tap runs on the main thread's run loop. Commands are queued
// in a lock-free ring buffer and polled by the Rust product loop.

#import "hotkey.h"
#import <CoreGraphics/CoreGraphics.h>
#import <Carbon/Carbon.h>
#import <stdatomic.h>

// Ring buffer for pending commands (power-of-2 size for fast modulo).
#define CMD_QUEUE_SIZE 64
static atomic_int g_cmdQueue[CMD_QUEUE_SIZE];
static atomic_int g_cmdHead = 0;  // write position
static atomic_int g_cmdTail = 0;  // read position

static void enqueue_command(LensCommand cmd) {
    int next_head = (atomic_load(&g_cmdHead) + 1) % CMD_QUEUE_SIZE;
    if (next_head == atomic_load(&g_cmdTail)) {
        // Queue full — drop the oldest command.
        atomic_store(&g_cmdTail, (atomic_load(&g_cmdTail) + 1) % CMD_QUEUE_SIZE);
    }
    atomic_store(&g_cmdQueue[atomic_load(&g_cmdHead)], (int)cmd);
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

static CGEventRef hotkey_callback(CGEventTapProxy proxy, CGEventType type,
                                   CGEventRef event, void *userInfo) {
    (void)proxy;
    (void)userInfo;

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
        // Swallow the event so it doesn't reach the foreground app.
        return NULL;
    }

    return event;
}

static CFMachPortRef g_eventTap = NULL;
static CFRunLoopSourceRef g_runLoopSource = NULL;

bool lens_hotkey_install(void) {
    if (g_eventTap) return true;  // Already installed.

    // Tap all keyDown events globally.
    CGEventMask mask = CGEventMaskBit(kCGEventKeyDown);

    g_eventTap = CGEventTapCreate(
        kCGSessionEventTap,          // session-level (not system-wide)
        kCGHeadInsertEventTap,       // insert before other taps
        kCGEventTapOptionListenOnly, // we swallow events by returning NULL
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
    int tail = atomic_load(&g_cmdTail);
    int head = atomic_load(&g_cmdHead);
    if (tail == head) return LENS_CMD_NONE;

    LensCommand cmd = (LensCommand)atomic_load(&g_cmdQueue[tail]);
    atomic_store(&g_cmdTail, (tail + 1) % CMD_QUEUE_SIZE);
    return cmd;
}
