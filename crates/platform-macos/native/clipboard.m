// Mori Lens · macOS clipboard via NSPasteboard.

#import "clipboard.h"
#import <AppKit/AppKit.h>

bool lens_clipboard_put_text(const char *text) {
    if (!text) return false;
    @autoreleasepool {
        NSPasteboard *pb = [NSPasteboard generalPasteboard];
        [pb clearContents];
        NSString *nsText = [NSString stringWithUTF8String:text];
        return [pb setString:nsText forType:NSPasteboardTypeString] != 0;
    }
}
