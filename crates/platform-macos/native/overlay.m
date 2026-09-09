// Mori Lens · macOS native overlay.
//
// A transparent NSPanel that shows a translation card. The panel:
// - Does not steal focus (nonactivatingPanel)
// - Passes clicks through (ignoreMouseEvents)
// - Sits above normal windows (statusBar level)
// - Has per-pixel alpha transparency
//
// Thread safety: All AppKit UI mutations are dispatched to the main queue.
// The product loop runs on a worker thread; this bridge ensures NSPanel
// operations always execute on the main thread where AppKit requires them.

#import "overlay.h"
#import <AppKit/AppKit.h>
#import <CoreText/CoreText.h>
#import <stdatomic.h>

static NSPanel *g_panel = nil;
static atomic_bool g_visible = false;

// Rounded-rect path for the card background.
static CGPathRef make_rounded_rect(CGRect rect, CGFloat radius) {
    CGMutablePathRef path = CGPathCreateMutable();
    CGPathAddRoundedRect(path, NULL, rect, radius, radius);
    return path;
}

bool lens_overlay_create(void) {
    // Must be called on main thread (called during setup before worker starts).
    if (g_panel) return true;

    @autoreleasepool {
        NSRect frame = NSMakeRect(0, 0, 1, 1);
        NSUInteger style = NSWindowStyleMaskBorderless;

        g_panel = [[NSPanel alloc]
            initWithContentRect:frame
                      styleMask:style
                        backing:NSBackingStoreBuffered
                          defer:YES];

        if (!g_panel) return false;

        [g_panel setOpaque:NO];
        [g_panel setBackgroundColor:[NSColor clearColor]];
        [g_panel setIgnoresMouseEvents:YES];
        [g_panel setLevel:NSStatusWindowLevel];
        [g_panel setCollectionBehavior:
            NSWindowCollectionBehaviorCanJoinAllSpaces |
            NSWindowCollectionBehaviorStationary |
            NSWindowCollectionBehaviorIgnoresCycle];
        [g_panel setHasShadow:NO];
        [g_panel setAlphaValue:1.0];
        [g_panel setHidesOnDeactivate:NO];
        [g_panel setBecomesKeyOnlyIfNeeded:YES];
        [g_panel setFloatingPanel:YES];
    }
    return true;
}

bool lens_overlay_show(const char *source_text,
                       const char *translation_text,
                       double x, double y,
                       double width, double height) {
    if (!g_panel) {
        if (!lens_overlay_create()) return false;
    }

    // Copy strings for the async block (block captures them).
    // Under ARC, the block automatically retains captured objects.
    NSString *src = source_text ? [NSString stringWithUTF8String:source_text] : nil;
    NSString *trans = translation_text ? [NSString stringWithUTF8String:translation_text] : nil;

    // Dispatch all AppKit UI mutations to the main queue.
    dispatch_async(dispatch_get_main_queue(), ^{
        @autoreleasepool {
            NSScreen *screen = [NSScreen mainScreen];
            if (!screen || !g_panel) {
                return;
            }

            CGFloat screenH = [screen frame].size.height;
            NSRect frame = NSMakeRect(x, screenH - y - height, width, height);
            [g_panel setFrame:frame display:NO];

            NSView *contentView = [g_panel contentView];
            CGFloat w = frame.size.width;
            CGFloat h = frame.size.height;

            CALayer *layer = [CALayer layer];
            layer.frame = CGRectMake(0, 0, w, h);
            layer.cornerRadius = 8.0;
            layer.backgroundColor = [[NSColor colorWithCalibratedWhite:0.12 alpha:0.94] CGColor];
            layer.masksToBounds = YES;
            [contentView setWantsLayer:YES];
            [contentView setLayer:layer];

            CGFloat scale = [screen backingScaleFactor];
            int pxW = (int)(w * scale);
            int pxH = (int)(h * scale);

            if (pxW <= 0 || pxH <= 0) {
                return;
            }

            CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
            CGContextRef ctx = CGBitmapContextCreate(
                NULL, pxW, pxH, 8, pxW * 4,
                cs, (CGBitmapInfo)kCGImageAlphaPremultipliedFirst);
            CGColorSpaceRelease(cs);

            if (!ctx) {
                return;
            }

            CGContextScaleCTM(ctx, scale, scale);

            CGRect bgRect = CGRectMake(0, 0, w, h);
            CGPathRef clipPath = make_rounded_rect(bgRect, 8.0);
            CGContextAddPath(ctx, clipPath);
            CGContextClip(ctx);
            CGPathRelease(clipPath);

            CGContextSetFillColorWithColor(ctx,
                [[NSColor colorWithCalibratedWhite:0.12 alpha:0.94] CGColor]);
            CGContextFillRect(ctx, bgRect);

            CGFloat pad = 12.0;
            CGFloat textY = h - pad;

            if (src) {
                CTFontRef srcFont = CTFontCreateWithName(CFSTR("LucidaGrande"), 11.0, NULL);
                NSDictionary *srcAttrs = @{
                    (id)kCTFontAttributeName: (__bridge id)srcFont,
                    (id)kCTForegroundColorAttributeName:
                        (id)[[NSColor colorWithCalibratedWhite:0.7 alpha:1.0] CGColor],
                };
                NSAttributedString *srcStr =
                    [[NSAttributedString alloc] initWithString:src attributes:srcAttrs];
                CTLineRef srcLine = CTLineCreateWithAttributedString(
                    (__bridge CFAttributedStringRef)srcStr);
                CGContextSetTextPosition(ctx, pad, textY - 14);
                CTLineDraw(srcLine, ctx);
                CFRelease(srcLine);
                CFRelease(srcFont);
                textY -= 28;
            }

            if (trans) {
                CTFontRef transFont = CTFontCreateWithName(CFSTR("LucidaGrande"), 13.0, NULL);
                NSDictionary *transAttrs = @{
                    (id)kCTFontAttributeName: (__bridge id)transFont,
                    (id)kCTForegroundColorAttributeName: (id)[[NSColor whiteColor] CGColor],
                };
                NSAttributedString *transStr =
                    [[NSAttributedString alloc] initWithString:trans attributes:transAttrs];
                CTFramesetterRef setter =
                    CTFramesetterCreateWithAttributedString(
                        (__bridge CFAttributedStringRef)transStr);
                CGRect textRect = CGRectMake(pad, pad, w - 2 * pad, textY - pad);
                CGPathRef textPath = CGPathCreateWithRect(textRect, NULL);
                CTFrameRef ctFrame =
                    CTFramesetterCreateFrame(setter, CFRangeMake(0, 0), textPath, NULL);
                CTFrameDraw(ctFrame, ctx);
                CFRelease(ctFrame);
                CFRelease(textPath);
                CFRelease(setter);
                CFRelease(transFont);
            }

            CGImageRef img = CGBitmapContextCreateImage(ctx);
            layer.contents = (__bridge id)img;
            layer.contentsScale = scale;
            CGImageRelease(img);
            CGContextRelease(ctx);

            [g_panel orderFront:nil];
            atomic_store(&g_visible, true);
        }
    });

    // Optimistically set visible — the async block will confirm.
    atomic_store(&g_visible, true);
    return true;
}

void lens_overlay_hide(void) {
    atomic_store(&g_visible, false);
    dispatch_async(dispatch_get_main_queue(), ^{
        @autoreleasepool {
            if (g_panel) {
                [g_panel orderOut:nil];
            }
        }
    });
}

void lens_overlay_destroy(void) {
    atomic_store(&g_visible, false);
    dispatch_async(dispatch_get_main_queue(), ^{
        @autoreleasepool {
            if (g_panel) {
                [g_panel close];
                g_panel = nil;
            }
        }
    });
}

bool lens_overlay_is_visible(void) {
    return atomic_load(&g_visible);
}

/// Return the CGWindowID of the overlay panel, or 0 if not created.
/// Used by capture.m to exclude the overlay from ScreenCaptureKit capture.
int32_t lens_overlay_window_id(void) {
    if (!g_panel) return 0;
    return (int32_t)[g_panel windowNumber];
}

bool lens_mouse_position(double *x, double *y) {
    if (!x || !y) return false;
    @autoreleasepool {
        NSPoint loc = [NSEvent mouseLocation];
        NSScreen *screen = [NSScreen mainScreen];
        if (!screen) return false;
        CGFloat screenH = [screen frame].size.height;
        *x = loc.x;
        *y = screenH - loc.y;
    }
    return true;
}

double lens_display_scale_at(double x, double y) {
    @autoreleasepool {
        NSScreen *mainScreen = [NSScreen mainScreen];
        if (!mainScreen) return 1.0;
        CGFloat mainH = [mainScreen frame].size.height;
        NSPoint appkitPoint = NSMakePoint(x, mainH - y);
        for (NSScreen *screen in [NSScreen screens]) {
            NSRect frame = [screen frame];
            if (NSPointInRect(appkitPoint, frame)) {
                return [screen backingScaleFactor];
            }
        }
        return [mainScreen backingScaleFactor];
    }
}

double lens_display_scale_at_physical(double px, double py) {
    @autoreleasepool {
        NSScreen *mainScreen = [NSScreen mainScreen];
        if (!mainScreen) return 1.0;
        for (NSScreen *screen in [NSScreen screens]) {
            CGFloat scale = [screen backingScaleFactor];
            NSRect frame = [screen frame];
            double physLeft = frame.origin.x * scale;
            double physTop = frame.origin.y * scale;
            double physRight = (frame.origin.x + frame.size.width) * scale;
            double physBottom = (frame.origin.y + frame.size.height) * scale;
            if (px >= physLeft && px < physRight && py >= physTop && py < physBottom) {
                return scale;
            }
        }
        return [mainScreen backingScaleFactor];
    }
}

void lens_process_appkit_events(void) {
    @autoreleasepool {
        [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                 beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.001]];
    }
}

bool lens_ctrl_held(void) {
    @autoreleasepool {
        NSEventModifierFlags flags = [NSEvent modifierFlags];
        return (flags & NSEventModifierFlagControl) != 0;
    }
}

bool lens_option_held(void) {
    @autoreleasepool {
        NSEventModifierFlags modifiers = [NSEvent modifierFlags];
        return (modifiers & NSEventModifierFlagOption) != 0;
    }
}
