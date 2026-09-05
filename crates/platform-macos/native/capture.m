// Mori Lens · macOS screen capture via ScreenCaptureKit SCStream.
//
// Uses SCStream to capture a single frame. Pixel data is extracted
// directly from the CVPixelBuffer (no CoreImage dependency).
// No image is written to disk (privacy invariant).

#import "capture.h"
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <CoreVideo/CoreVideo.h>
#import <stdlib.h>
#import <string.h>

// Delegate that captures a single frame and stops the stream.
@interface LensCaptureDelegate : NSObject <SCStreamDelegate, SCStreamOutput>
@property (nonatomic, assign) uint8_t *pixelData;
@property (nonatomic, assign) int32_t pixelWidth;
@property (nonatomic, assign) int32_t pixelHeight;
@property (nonatomic, assign) int32_t pixelStride;
@property (nonatomic, assign) BOOL done;
@property (nonatomic, strong) SCStream *stream;
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

    // SCStream typically delivers BGRA or RGBA premultiplied pixels.
    // We copy the raw data and note the stride. The Rust side handles
    // any format conversion if needed (Vision OCR accepts RGBA).
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

    [self.stream stopCaptureWithCompletionHandler:^(NSError * _Nullable error) {
        (void)error;
        // Stream stopped.
    }];
}

@end

bool lens_capture_available(void) {
    if (@available(macOS 12.3, *)) {
        return true;
    }
    return false;
}

bool lens_capture_screen(double x, double y,
                         double width, double height,
                         uint8_t **out_pixels,
                         int32_t *out_width,
                         int32_t *out_height,
                         int32_t *out_stride) {
    (void)x; (void)y;  // Region used by SCStream config width/height.
    if (!out_pixels || !out_width || !out_height || !out_stride) return false;
    if (width <= 0 || height <= 0) return false;

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

        // Step 2: Create content filter for the main display.
        SCDisplay *display = content.displays[0];
        SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];

        // Step 3: Configure the stream for the requested region.
        SCStreamConfiguration *config = [[SCStreamConfiguration alloc] init];
        config.width = (int)width;
        config.height = (int)height;
        config.captureResolution = SCCaptureResolutionAutomatic;
        config.pixelFormat = 'BGRA';

        LensCaptureDelegate *delegate = [[LensCaptureDelegate alloc] init];

        NSError *streamError = nil;
        SCStream *stream = [[SCStream alloc] initWithFilter:filter configuration:config delegate:delegate];
        delegate.stream = stream;

        [stream addStreamOutput:delegate type:SCStreamOutputTypeScreen sampleHandlerQueue:dispatch_queue_create("lens.capture", DISPATCH_QUEUE_SERIAL) error:&streamError];
        if (streamError) return false;

        // Step 4: Start capture and wait for one frame.
        __block NSError *startError = nil;
        dispatch_semaphore_t sema2 = dispatch_semaphore_create(0);

        [stream startCaptureWithCompletionHandler:^(NSError *error) {
            startError = error;
            dispatch_semaphore_signal(sema2);
        }];

        if (dispatch_semaphore_wait(sema2, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC)) != 0) {
            return false;
        }
        if (startError) return false;

        // Wait for the delegate to capture a frame (up to 5 seconds).
        for (int i = 0; i < 100; i++) {
            if (delegate.done) break;
            [NSThread sleepForTimeInterval:0.05];
        }

        // Stop the stream and wait for completion to ensure the delegate
        // is not deallocated while callbacks are still in flight.
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
