// Mori Lens · macOS lifecycle notifications.
//
// Observes NSWorkspace notifications for sleep/wake and display changes.
// Uses atomic flags for thread-safe polling from the Rust product loop.

#import "lifecycle.h"
#import <AppKit/AppKit.h>
#import <stdatomic.h>

static atomic_bool g_wakeFlag = false;
static atomic_bool g_displayChangeFlag = false;
static id g_wakeObserver = nil;
static id g_displayObserver = nil;

bool lens_lifecycle_start(void) {
    @autoreleasepool {
        NSNotificationCenter *nc = [NSWorkspace sharedWorkspace].notificationCenter;

        g_wakeObserver = [nc addObserverForName:NSWorkspaceDidWakeNotification
                                         object:nil
                                          queue:[NSOperationQueue mainQueue]
                                     usingBlock:^(NSNotification * _Nonnull note) {
            (void)note;
            atomic_store(&g_wakeFlag, true);
        }];

        g_displayObserver = [nc addObserverForName:NSApplicationDidChangeScreenParametersNotification
                                            object:nil
                                             queue:[NSOperationQueue mainQueue]
                                        usingBlock:^(NSNotification * _Nonnull note) {
            (void)note;
            atomic_store(&g_displayChangeFlag, true);
        }];
    }
    return (g_wakeObserver != nil);
}

void lens_lifecycle_stop(void) {
    @autoreleasepool {
        if (g_wakeObserver) {
            [[NSWorkspace sharedWorkspace].notificationCenter removeObserver:g_wakeObserver];
            g_wakeObserver = nil;
        }
        if (g_displayObserver) {
            [[NSWorkspace sharedWorkspace].notificationCenter removeObserver:g_displayObserver];
            g_displayObserver = nil;
        }
    }
}

bool lens_lifecycle_poll_wake(void) {
    return atomic_exchange(&g_wakeFlag, false);
}

bool lens_lifecycle_poll_display_change(void) {
    return atomic_exchange(&g_displayChangeFlag, false);
}
