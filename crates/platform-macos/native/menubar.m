// Mori Lens · macOS menu bar integration.
//
// Provides an NSStatusItem with: Pause, Deep Analysis, Copy, Save,
// Manual OCR, Quit.

#import "menubar.h"
#import <AppKit/AppKit.h>
#import <stdatomic.h>

static NSStatusItem *g_statusItem = nil;
static atomic_bool g_pauseToggle = false;
static atomic_bool g_quitRequested = false;
static atomic_bool g_deepRequested = false;
static atomic_bool g_copyRequested = false;
static atomic_bool g_saveRequested = false;
static atomic_bool g_ocrRequested = false;
static bool g_paused = false;

@interface LensMenuDelegate : NSObject
@end

@implementation LensMenuDelegate

- (void)togglePause:(id)sender {
    g_paused = !g_paused;
    atomic_store(&g_pauseToggle, true);
    [self updateMenuTitle];
}

- (void)requestDeep:(id)sender {
    atomic_store(&g_deepRequested, true);
}

- (void)requestCopy:(id)sender {
    atomic_store(&g_copyRequested, true);
}

- (void)requestSave:(id)sender {
    atomic_store(&g_saveRequested, true);
}

- (void)requestOcr:(id)sender {
    atomic_store(&g_ocrRequested, true);
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

        g_delegate = [[LensMenuDelegate alloc] init];

        NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Mori Lens"];

        NSMenuItem *pauseItem = [[NSMenuItem alloc] initWithTitle:@"Pause" action:@selector(togglePause:) keyEquivalent:@""];
        [pauseItem setTarget:g_delegate];
        [menu addItem:pauseItem];

        [menu addItem:[NSMenuItem separatorItem]];

        NSMenuItem *deepItem = [[NSMenuItem alloc] initWithTitle:@"Deep Analysis" action:@selector(requestDeep:) keyEquivalent:@"d"];
        [deepItem setTarget:g_delegate];
        [deepItem setKeyEquivalentModifierMask:NSEventModifierFlagCommand | NSEventModifierFlagOption];
        [menu addItem:deepItem];

        NSMenuItem *copyItem = [[NSMenuItem alloc] initWithTitle:@"Copy" action:@selector(requestCopy:) keyEquivalent:@"c"];
        [copyItem setTarget:g_delegate];
        [copyItem setKeyEquivalentModifierMask:NSEventModifierFlagCommand | NSEventModifierFlagOption];
        [menu addItem:copyItem];

        NSMenuItem *saveItem = [[NSMenuItem alloc] initWithTitle:@"Save" action:@selector(requestSave:) keyEquivalent:@"s"];
        [saveItem setTarget:g_delegate];
        [saveItem setKeyEquivalentModifierMask:NSEventModifierFlagCommand | NSEventModifierFlagOption];
        [menu addItem:saveItem];

        NSMenuItem *ocrItem = [[NSMenuItem alloc] initWithTitle:@"Manual OCR" action:@selector(requestOcr:) keyEquivalent:@"o"];
        [ocrItem setTarget:g_delegate];
        [ocrItem setKeyEquivalentModifierMask:NSEventModifierFlagCommand | NSEventModifierFlagOption];
        [menu addItem:ocrItem];

        [menu addItem:[NSMenuItem separatorItem]];

        NSMenuItem *quitItem = [[NSMenuItem alloc] initWithTitle:@"Quit Mori Lens" action:@selector(quit:) keyEquivalent:@"q"];
        [quitItem setTarget:g_delegate];
        [menu addItem:quitItem];

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

bool lens_menubar_poll_deep(void) {
    return atomic_exchange(&g_deepRequested, false);
}

bool lens_menubar_poll_copy(void) {
    return atomic_exchange(&g_copyRequested, false);
}

bool lens_menubar_poll_save(void) {
    return atomic_exchange(&g_saveRequested, false);
}

bool lens_menubar_poll_ocr(void) {
    return atomic_exchange(&g_ocrRequested, false);
}
