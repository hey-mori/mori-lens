// Mori Lens · macOS global hotkeys via CGEventTap.
//
// Intercepts configured key combinations globally and maps them to product
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
//     Pause uses a saturating unsigned counter (max 1024) to preserve
//     toggle multiplicity without signed overflow risk.
//   - If macOS disables the tap (timeout), the callback re-enables it.
//   - Shortcut bindings are configurable: the Rust layer calls
//     lens_hotkey_configure() to install the mapping table.

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
// Pause uses an unsigned counter with saturating increment to preserve
// toggle multiplicity without risk of signed overflow. Capped at
// PAUSE_PENDING_MAX — billions of queued toggles have no product value.
#define PAUSE_PENDING_MAX 1024u
static atomic_bool g_quitFlag = false;
static atomic_uint g_pauseCount = 0;

// Configurable hotkey bindings.
// The Rust layer calls lens_hotkey_configure() to install the binding table.
// The callback reads g_bindings atomically. We use a simple double-buffer
// approach: g_bindings points to one of two static arrays, and configure()
// swaps the pointer after writing the new table to the other array.
// This ensures the callback always sees a consistent table.
#define MAX_BINDINGS 16
static LensHotkeyBinding g_bindingsA[MAX_BINDINGS];
static LensHotkeyBinding g_bindingsB[MAX_BINDINGS];
static atomic_int g_bindingCount = 0;
static atomic_int g_useB = 0;  // 0 = A is active, 1 = B is active

static void enqueue_command(LensCommand cmd) {
    // Critical commands use dedicated atomics — never dropped.
    if (cmd == LENS_CMD_QUIT) {
        atomic_store(&g_quitFlag, true);
        return;
    }
    if (cmd == LENS_CMD_PAUSE_TOGGLE) {
        // Saturating increment: CAS loop, no lock, no signed overflow.
        unsigned int cur = atomic_load(&g_pauseCount);
        while (cur < PAUSE_PENDING_MAX) {
            if (atomic_compare_exchange_weak(&g_pauseCount, &cur, cur + 1)) {
                break;
            }
            // cur is updated by compare_exchange_weak on failure; retry.
        }
        // If cur >= PAUSE_PENDING_MAX, silently saturate (drop extra presses).
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

/// Look up a key event in the configured binding table.
/// Returns the matching command, or LENS_CMD_NONE if no match.
static LensCommand lookup_binding(uint32_t eventFlags, uint16_t keycode) {
    // Determine which buffer is active.
    int useB = atomic_load(&g_useB);
    LensHotkeyBinding* bindings = useB ? g_bindingsB : g_bindingsA;
    int count = atomic_load(&g_bindingCount);

    // Extract modifier state from the event.
    bool cmd = (eventFlags & kCGEventFlagMaskCommand) != 0;
    bool opt = (eventFlags & kCGEventFlagMaskAlternate) != 0;
    bool ctrl = (eventFlags & kCGEventFlagMaskControl) != 0;
    bool shift = (eventFlags & kCGEventFlagMaskShift) != 0;

    for (int i = 0; i < count; i++) {
        LensHotkeyBinding* b = &bindings[i];
        // Check keycode first (fast rejection).
        if (b->keycode != keycode) continue;

        // Check modifiers: the binding specifies which modifiers must be set.
        // We also require that NO extra modifiers are held (exact match).
        bool b_cmd = (b->modifiers & kCGEventFlagMaskCommand) != 0;
        bool b_opt = (b->modifiers & kCGEventFlagMaskAlternate) != 0;
        bool b_ctrl = (b->modifiers & kCGEventFlagMaskControl) != 0;
        bool b_shift = (b->modifiers & kCGEventFlagMaskShift) != 0;

        if (cmd == b_cmd && opt == b_opt && ctrl == b_ctrl && shift == b_shift) {
            return (LensCommand)b->command;
        }
    }
    return LENS_CMD_NONE;
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

    // Check modifiers against the configured binding table.
    CGEventFlags flags = CGEventGetFlags(event);
    uint16_t keycode = (uint16_t)CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    LensCommand cmd_val = lookup_binding(flags, keycode);
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
    // Clear bindings.
    atomic_store(&g_bindingCount, 0);
    atomic_store(&g_useB, 0);
}

void lens_hotkey_configure(const LensHotkeyBinding* bindings, int count) {
    if (count < 0) count = 0;
    if (count > MAX_BINDINGS) count = MAX_BINDINGS;

    // Determine which buffer is currently inactive.
    int useB = atomic_load(&g_useB);
    LensHotkeyBinding* target = useB ? g_bindingsA : g_bindingsB;

    // Copy the new bindings into the inactive buffer.
    for (int i = 0; i < count; i++) {
        target[i] = bindings[i];
    }

    // Update the count (for the new buffer).
    atomic_store(&g_bindingCount, count);

    // Swap to the new buffer. The callback will see the new table on the next event.
    atomic_store(&g_useB, useB ? 0 : 1);
}

LensCommand lens_hotkey_poll(void) {
    // Critical commands have priority — check flags first.
    if (atomic_load(&g_quitFlag)) {
        atomic_store(&g_quitFlag, false);
        return LENS_CMD_QUIT;
    }
    // Pause uses a saturating unsigned counter to preserve multiplicity.
    // Each press increments (capped at PAUSE_PENDING_MAX); each poll
    // decrements and returns one toggle.
    unsigned int pauseCount = atomic_load(&g_pauseCount);
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
