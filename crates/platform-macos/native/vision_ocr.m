// Mori Lens · macOS Vision framework OCR.
//
// Uses VNRecognizeTextRequest to recognize text from a pixel buffer.
// The buffer is passed directly from Rust (BGRA, top-to-bottom).
// No image is written to disk (privacy invariant).

#import "vision_ocr.h"
#import <Vision/Vision.h>
#import <CoreGraphics/CoreGraphics.h>
#import <stdlib.h>
#import <string.h>

bool lens_vision_available(void) {
    // Vision is available on macOS 10.15+. We just check that the class exists.
    return NSClassFromString(@"VNRecognizeTextRequest") != nil;
}

LensOcrResult lens_vision_recognize(const uint8_t *pixels,
                                    int32_t width,
                                    int32_t height,
                                    int32_t stride) {
    LensOcrResult result = { .text = NULL, .word_count = 0, .word_boxes = NULL };

    if (!pixels || width <= 0 || height <= 0) return result;

    @autoreleasepool {
        // Create a CGImage from the raw pixel buffer.
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        // The input is BGRA, top-to-bottom.
        // kCGImageAlphaPremultipliedFirst + kCGBitmapByteOrder32Little = BGRA.
        CGBitmapInfo bitmapInfo = kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little;
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL, pixels, (size_t)(stride * height), NULL);

        CGImageRef image = CGImageCreate(
            (size_t)width, (size_t)height,
            8, 32,                      // bits per component, bits per pixel
            (size_t)stride,
            cs,
            bitmapInfo,
            provider,
            NULL, NO,
            kCGRenderingIntentDefault);

        CGDataProviderRelease(provider);
        CGColorSpaceRelease(cs);

        if (!image) return result;

        // Create and configure the Vision request.
        VNRecognizeTextRequest *request =
            [[VNRecognizeTextRequest alloc] init];
        request.recognitionLevel = 1; // accurate
        request.usesLanguageCorrection = YES;

        // Perform the request.
        VNImageRequestHandler *handler =
            [[VNImageRequestHandler alloc] initWithCGImage:image
                                                  options:@{}];
        NSError *error = nil;
        [handler performRequests:@[request] error:&error];

        if (error) {
            CGImageRelease(image);
            return result;
        }

        // Collect results.
        NSArray<VNRecognizedTextObservation *> *observations = request.results;
        if (!observations.count) {
            CGImageRelease(image);
            return result;
        }

        // Build the full text and word boxes.
        NSMutableString *fullText = [NSMutableString string];
        NSMutableArray *boxes = [NSMutableArray array];
        int wordCount = 0;

        for (VNRecognizedTextObservation *obs in observations) {
            VNRecognizedText *topCandidate =
                [obs topCandidates:1].firstObject;
            if (!topCandidate) continue;

            NSString *text = topCandidate.string;
            if (fullText.length > 0) {
                [fullText appendString:@"\n"];
            }
            [fullText appendString:text];

            // Get the bounding box (normalized, bottom-left origin).
            CGRect box = obs.boundingBox;
            // Convert to pixel coordinates, top-left origin.
            float px = (float)(box.origin.x * width);
            float py = (float)((1.0 - box.origin.y - box.size.height) * height);
            float pw = (float)(box.size.width * width);
            float ph = (float)(box.size.height * height);

            [boxes addObject:@(px)];
            [boxes addObject:@(py)];
            [boxes addObject:@(pw)];
            [boxes addObject:@(ph)];
            wordCount++;
        }

        // Copy text to a C string.
        const char *utf8 = [fullText UTF8String];
        if (utf8) {
            result.text = strdup(utf8);
        }

        // Copy boxes to a C array.
        result.word_count = wordCount;
        if (wordCount > 0) {
            result.word_boxes = (float *)malloc(sizeof(float) * wordCount * 4);
            for (int i = 0; i < wordCount * 4; i++) {
                result.word_boxes[i] = [boxes[i] floatValue];
            }
        }

        CGImageRelease(image);
    }

    return result;
}

void lens_ocr_result_free(LensOcrResult *result) {
    if (result) {
        free(result->text);
        free(result->word_boxes);
        result->text = NULL;
        result->word_boxes = NULL;
        result->word_count = 0;
    }
}
