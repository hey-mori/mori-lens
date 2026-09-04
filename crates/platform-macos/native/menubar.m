// Mori Lens · macOS menu bar integration.
//
// Provides an NSStatusItem in the menu bar with Pause/Resume and Quit.

#import "menubar.h"
#import <AppKit/AppKit.h>
#import <stdatomic.h>

static NSStatusItem *g_statusItem = nil;
static atomic_bool g_pauseToggle = false;
static atomic_bool g_quitRequested = false;
static bool g_paused = false;

@interface LensMenuDelegate : NSObject
@end

@implementation LensMenuDelegate

- (void)togglePause:(id)sender {
    g_paused = !g_paused;
    atomic_store(&g_pauseToggle, true);
    [self updateMenuTitle];
}

- (void)quit:(id)sender {
    atomic_store(&g_quitRequested, true);
}

- (void)updateMenuTitle {
    if (g_statusItem) {
        NSString *title = g_paused ? @"Lens ⏸" : @"Lens";
        g_statusItem.button.title = title;
    }
}

@end

static LensMenuDelegate *g_delegate = nil;

bool lens_menubar_create(void) {
    @autoreleasepool {
        g_statusItem = [[NSStatusBar systemStatusBar] statusItemWithLength:NSVariableStatusItemLength];
        if (!g_statusItem) return false;

        g_statusItem.button.title = @"Lens";
        g_statusItem.behavior = NSStatusItemBehaviorTerminationOnRemoval;

        g_delegate = [[LensMenuDelegate alloc] init];

        NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Mori Lens"];
        [menu addItemWithTitle:@"Pause" action:@selector(togglePause:) keyEquivalent:@""];
        [menu addItem:[NSMenuItem separatorItem]];
        [menu addItemWithTitle:@"Quit Mori Lens" action:@selector(quit:) keyEquivalent:@"q"];

        g_statusItem.menu = menu;
    }
    return true;
}

void lens_menubar_destroy(void) {
    @autoreleasepool {
        if (g_statusItem) {
            [[NSStatusBar systemStatusBar] removeStatusItem:g_statusItem];
            g_statusItem = nil;
        }
        g_delegate = nil;
    }
}

void lens_menubar_set_title(const char *title) {
    @autoreleasepool {
        if (g_statusItem && title) {
            g_statusItem.button.title = [NSString stringWithUTF8String:title];
        }
    }
}

bool lens_menubar_poll_pause_toggle(void) {
    return atomic_exchange(&g_pauseToggle, false);
}

bool lens_menubar_poll_quit(void) {
    return atomic_exchange(&g_quitRequested, false);
}
