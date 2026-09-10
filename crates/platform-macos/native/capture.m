// Mori Lens · macOS screen capture via ScreenCaptureKit SCStream.
//
// Captures a crop region around the pointer on the correct display.
// Pixel data is extracted directly from the CVPixelBuffer.
// No image is written to disk (privacy invariant).
//
// Coordinate contract:
//   - Input x/y/width/height are in global logical points (top-left origin).
//   - The display containing (x,y) is selected.
//   - The crop is clipped to that display's bounds.
//   - sourceRect is in display-local logical points.
//   - Output pixels are at the display's native scale.

#import "capture.h"
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <CoreVideo/CoreVideo.h>
#import <stdlib.h>
#import <string.h>

// From overlay.m — the overlay panel's CGWindowID, or 0 if not created.
extern int32_t lens_overlay_window_id(void);

// Delegate that captures a single frame. The caller owns the stop lifecycle.
@interface LensCaptureDelegate : NSObject <SCStreamDelegate, SCStreamOutput>
@property (nonatomic, assign) uint8_t *pixelData;
@property (nonatomic, assign) int32_t pixelWidth;
@property (nonatomic, assign) int32_t pixelHeight;
@property (nonatomic, assign) int32_t pixelStride;
@property (nonatomic, assign) BOOL done;
@end

@implementation LensCaptureDelegate

- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer ofType:(SCStreamOutputType)type {
    if (self.done) return;
    if (type != SCStreamOutputTypeScreen) return;

    CVImageBufferRef pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer);
    if (!pixelBuffer) return;

    CVPixelBufferLockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);

    uint8_t *baseAddr = (uint8_t *)CVPixelBufferGetBaseAddress(pixelBuffer);
    size_t bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer);
    size_t bufWidth = CVPixelBufferGetWidth(pixelBuffer);
    size_t bufHeight = CVPixelBufferGetHeight(pixelBuffer);

    if (!baseAddr || bufWidth == 0 || bufHeight == 0) {
        CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }

    // Overflow guard: bytesPerRow * bufHeight must not overflow size_t.
    if (bufHeight > SIZE_MAX / bytesPerRow) {
        CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }

    size_t totalBytes = bytesPerRow * bufHeight;
    uint8_t *copy = (uint8_t *)malloc(totalBytes);
    if (!copy) {
        CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }
    memcpy(copy, baseAddr, totalBytes);

    CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);

    self.pixelData = copy;
    self.pixelWidth = (int32_t)bufWidth;
    self.pixelHeight = (int32_t)bufHeight;
    self.pixelStride = (int32_t)bytesPerRow;
    self.done = YES;
    // The caller owns the stop lifecycle (see stopCaptureWithCompletionHandler
    // in lens_capture_screen). The delegate only records the frame.
}

@end

bool lens_capture_available(void) {
    if (@available(macOS 12.3, *)) {
        // Check the actual Screen Recording permission, not just the OS
        // version. CGPreflightScreenCaptureAccess returns true only when
        // the app has been granted permission to capture the screen.
        return CGPreflightScreenCaptureAccess();
    }
    return false;
}

/// Find the SCDisplay containing the given global point.
/// Returns nil if no display contains the point.
static SCDisplay *find_display_for_point(SCShareableContent *content, double gx, double gy) {
    for (SCDisplay *d in content.displays) {
        CGRect bounds = d.frame;
        if (gx >= bounds.origin.x
            && gx < bounds.origin.x + bounds.size.width
            && gy >= bounds.origin.y
            && gy < bounds.origin.y + bounds.size.height) {
            return d;
        }
    }
    return nil;
}

/// Compute the scale factor of a CG display by comparing pixel width to
/// point width.
static double display_scale(CGDirectDisplayID displayID) {
    CGRect bounds = CGDisplayBounds(displayID);
    size_t pxWide = CGDisplayPixelsWide(displayID);
    if (bounds.size.width > 0.0) {
        return (double)pxWide / bounds.size.width;
    }
    return 1.0;
}

bool lens_capture_screen(double req_x, double req_y,
                         double req_width, double req_height,
                         double ptr_x, double ptr_y,
                         uint8_t **out_pixels,
                         int32_t *out_width,
                         int32_t *out_height,
                         int32_t *out_stride,
                         double *out_origin_x,
                         double *out_origin_y,
                         double *out_scale) {
    if (!out_pixels || !out_width || !out_height || !out_stride) return false;
    if (!out_origin_x || !out_origin_y || !out_scale) return false;
    if (req_width <= 0 || req_height <= 0) return false;
    // Reject NaN/Inf crossing FFI.
    if (req_x != req_x || req_y != req_y || req_width != req_width || req_height != req_height) return false;
    if (ptr_x != ptr_x || ptr_y != ptr_y) return false;
    // Reject unreasonably large captures (> 10000 points in either dimension).
    if (req_width > 10000 || req_height > 10000) return false;

    if (@available(macOS 12.3, *)) {
        // Step 1: Get shareable content.
        __block SCShareableContent *content = nil;
        __block NSError *contentError = nil;
        dispatch_semaphore_t sema1 = dispatch_semaphore_create(0);

        [SCShareableContent getShareableContentWithCompletionHandler:^(SCShareableContent *c, NSError *e) {
            content = c;
            contentError = e;
            dispatch_semaphore_signal(sema1);
        }];

        if (dispatch_semaphore_wait(sema1, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC)) != 0) {
            return false;
        }
        if (contentError || !content || content.displays.count == 0) return false;

        // Step 2: Find the display containing the POINTER (not the request
        // origin). This ensures OCR near a display edge selects the correct
        // monitor even when the band extends into another display's space.
        SCDisplay *display = find_display_for_point(content, ptr_x, ptr_y);
        if (!display) {
            // Fallback: try the first display if the pointer is between displays.
            display = content.displays[0];
        }

        // Step 3: Compute the crop rect in display-local coordinates.
        CGRect displayRect = display.frame;
        double cropX = req_x - displayRect.origin.x;
        double cropY = req_y - displayRect.origin.y;
        double cropW = req_width;
        double cropH = req_height;

        // Clip to display bounds.
        if (cropX < 0) { cropW += cropX; cropX = 0; }
        if (cropY < 0) { cropH += cropY; cropY = 0; }
        if (cropX + cropW > displayRect.size.width) {
            cropW = displayRect.size.width - cropX;
        }
        if (cropY + cropH > displayRect.size.height) {
            cropH = displayRect.size.height - cropY;
        }
        if (cropW <= 0 || cropH <= 0) return false;

        // Step 4: Configure the stream for the crop region.
        // Exclude the Lens overlay from capture so OCR never reads its own card.
        NSMutableArray *excludeWindows = [NSMutableArray array];
        int32_t overlayWID = lens_overlay_window_id();
        if (overlayWID != 0) {
            [excludeWindows addObject:@(overlayWID)];
        }
        SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:excludeWindows];
        SCStreamConfiguration *config = [[SCStreamConfiguration alloc] init];

        // sourceRect: the crop region in display-local logical points.
        config.sourceRect = CGRectMake(cropX, cropY, cropW, cropH);

        // Output dimensions: native pixel size of the crop region.
        CGDirectDisplayID cgID = display.displayID;
        double scale = display_scale(cgID);
        int outW = (int)(cropW * scale);
        int outH = (int)(cropH * scale);
        if (outW <= 0 || outH <= 0) return false;

        // Return the actual (clipped) global logical origin and scale.
        // The caller uses these for coordinate reconstruction, NOT the
        // original request origin (which may differ after clipping).
        *out_origin_x = displayRect.origin.x + cropX;
        *out_origin_y = displayRect.origin.y + cropY;
        *out_scale = scale;

        config.width = outW;
        config.height = outH;
        if (@available(macOS 14.0, *)) {
            config.captureResolution = SCCaptureResolutionAutomatic;
        }
        config.pixelFormat = 'BGRA';

        LensCaptureDelegate *delegate = [[LensCaptureDelegate alloc] init];

        NSError *streamError = nil;
        SCStream *stream = [[SCStream alloc] initWithFilter:filter configuration:config delegate:delegate];

        [stream addStreamOutput:delegate type:SCStreamOutputTypeScreen sampleHandlerQueue:dispatch_queue_create("lens.capture", DISPATCH_QUEUE_SERIAL) error:&streamError];
        if (streamError) return false;

        // Step 5: Start capture (no completion handler — the frame poll
        // below is the combined start+frame deadline). If start fails
        // asynchronously, no frames arrive and the poll times out.
        // This avoids capturing stack-local state in a completion block
        // that could fire after the function returns on timeout.
        [stream startCaptureWithCompletionHandler:^(NSError * _Nullable error) {
            (void)error;
        }];

        // Wait for the delegate to capture a frame (up to 5 seconds).
        for (int i = 0; i < 100; i++) {
            if (delegate.done) break;
            [NSThread sleepForTimeInterval:0.05];
        }

        // Stop the stream and wait for completion to ensure all callbacks
        // have finished before we access the delegate's pixel data.
        dispatch_semaphore_t sema3 = dispatch_semaphore_create(0);
        [stream stopCaptureWithCompletionHandler:^(NSError * _Nullable error) {
            (void)error;
            dispatch_semaphore_signal(sema3);
        }];
        dispatch_semaphore_wait(sema3, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

        if (!delegate.pixelData) return false;

        *out_pixels = delegate.pixelData;
        *out_width = delegate.pixelWidth;
        *out_height = delegate.pixelHeight;
        *out_stride = delegate.pixelStride;
        return true;
    }

    return false;
}
