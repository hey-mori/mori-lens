// Mori Lens · macOS native overlay.
//
// A transparent NSPanel that shows a translation card. The panel:
// - Does not steal focus (nonactivatingPanel)
// - Passes clicks through (ignoreMouseEvents)
// - Sits above normal windows (statusBar level)
// - Has per-pixel alpha transparency
//
// This is compiled by the cc crate in build.rs and linked into the
// platform-macos crate.

#import "overlay.h"
#import <AppKit/AppKit.h>
#import <CoreText/CoreText.h>

static NSPanel *g_panel = nil;
static bool g_visible = false;

// Rounded-rect path for the card background.
static CGPathRef make_rounded_rect(CGRect rect, CGFloat radius) {
    CGMutablePathRef path = CGPathCreateMutable();
    CGPathAddRoundedRect(path, NULL, rect, radius, radius);
    return path;
}

bool lens_overlay_create(void) {
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

    @autoreleasepool {
        // Position: macOS screen coordinates, bottom-left origin.
        // The caller passes top-left origin points; we must flip Y.
        NSScreen *screen = [NSScreen mainScreen];
        CGFloat screenH = [screen frame].size.height;
        NSRect frame = NSMakeRect(x, screenH - y - height, width, height);
        [g_panel setFrame:frame display:NO];

        // Build the content view with custom drawing.
        NSView *contentView = [g_panel contentView];
        CGFloat w = frame.size.width;
        CGFloat h = frame.size.height;

        // Use a layer-backed view for drawing.
        CALayer *layer = [CALayer layer];
        layer.frame = CGRectMake(0, 0, w, h);
        layer.cornerRadius = 8.0;
        layer.backgroundColor = [[NSColor colorWithCalibratedWhite:0.12 alpha:0.94] CGColor];
        layer.masksToBounds = YES;
        [contentView setWantsLayer:YES];
        [contentView setLayer:layer];

        // Draw text using a CGContext.
        // We create an image-backed context, draw into it, then set it as
        // the layer contents.
        CGFloat scale = [screen backingScaleFactor];
        int pxW = (int)(w * scale);
        int pxH = (int)(h * scale);

        if (pxW <= 0 || pxH <= 0) return false;

        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        CGContextRef ctx = CGBitmapContextCreate(
            NULL, pxW, pxH, 8, pxW * 4,
            cs, (CGBitmapInfo)kCGImageAlphaPremultipliedFirst);
        CGColorSpaceRelease(cs);

        if (!ctx) return false;

        CGContextScaleCTM(ctx, scale, scale);

        // Background with rounded corners.
        CGRect bgRect = CGRectMake(0, 0, w, h);
        CGPathRef clipPath = make_rounded_rect(bgRect, 8.0);
        CGContextAddPath(ctx, clipPath);
        CGContextClip(ctx);
        CGPathRelease(clipPath);

        // Fill background.
        CGContextSetFillColorWithColor(ctx,
            [[NSColor colorWithCalibratedWhite:0.12 alpha:0.94] CGColor]);
        CGContextFillRect(ctx, bgRect);

        // Draw text using CoreText.
        CGFloat pad = 12.0;
        CGFloat textY = h - pad;

        // Source text (lighter, smaller).
        if (source_text) {
            NSString *src = [NSString stringWithUTF8String:source_text];
            CTFontRef srcFont = CTFontCreateWithName(
                CFSTR("LucidaGrande"), 11.0, NULL);
            NSDictionary *srcAttrs = @{
                (id)kCTFontAttributeName: (__bridge id)srcFont,
                (id)kCTForegroundColorAttributeName:
                    (id)[[NSColor colorWithCalibratedWhite:0.7 alpha:1.0] CGColor],
            };
            NSAttributedString *srcStr =
                [[NSAttributedString alloc] initWithString:src
                                               attributes:srcAttrs];
            CTLineRef srcLine = CTLineCreateWithAttributedString(
                (__bridge CFAttributedStringRef)srcStr);

            CGContextSetTextPosition(ctx, pad, textY - 14);
            CTLineDraw(srcLine, ctx);

            CFRelease(srcLine);
            CFRelease(srcFont);
            textY -= 28;
        }

        // Translation text (brighter, larger).
        if (translation_text) {
            NSString *trans = [NSString stringWithUTF8String:translation_text];
            CTFontRef transFont = CTFontCreateWithName(
                CFSTR("LucidaGrande"), 13.0, NULL);
            NSDictionary *transAttrs = @{
                (id)kCTFontAttributeName: (__bridge id)transFont,
                (id)kCTForegroundColorAttributeName:
                    (id)[[NSColor whiteColor] CGColor],
            };
            NSAttributedString *transStr =
                [[NSAttributedString alloc] initWithString:trans
                                               attributes:transAttrs];

            // Word-wrap: use a CTFramesetter for the translation.
            CTFramesetterRef setter =
                CTFramesetterCreateWithAttributedString(
                    (__bridge CFAttributedStringRef)transStr);
            CGRect textRect = CGRectMake(pad, pad, w - 2 * pad, textY - pad);
            CGPathRef textPath = CGPathCreateWithRect(textRect, NULL);
            CTFrameRef frame =
                CTFramesetterCreateFrame(setter, CFRangeMake(0, 0),
                                         textPath, NULL);

            CTFrameDraw(frame, ctx);

            CFRelease(frame);
            CFRelease(textPath);
            CFRelease(setter);
            CFRelease(transFont);
        }

        // Create image from context and set as layer contents.
        CGImageRef img = CGBitmapContextCreateImage(ctx);
        layer.contents = (__bridge id)img;
        layer.contentsScale = scale;

        CGImageRelease(img);
        CGContextRelease(ctx);

        [g_panel orderFront:nil];
        g_visible = true;
    }

    return true;
}

void lens_overlay_hide(void) {
    if (g_panel) {
        @autoreleasepool {
            [g_panel orderOut:nil];
        }
        g_visible = false;
    }
}

void lens_overlay_destroy(void) {
    if (g_panel) {
        @autoreleasepool {
            [g_panel close];
            g_panel = nil;
        }
        g_visible = false;
    }
}

bool lens_overlay_is_visible(void) {
    return g_visible;
}
