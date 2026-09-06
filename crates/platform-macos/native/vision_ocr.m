// Mori Lens · macOS Vision framework OCR.
//
// Uses VNRecognizeTextRequest to recognize text from a pixel buffer.
// The buffer is passed directly from Rust (BGRA, top-to-bottom).
// No image is written to disk (privacy invariant).
//
// Per-word geometry: uses boundingBoxForRange: for each word token
// enumerated via NSStringEnumerationByWords. This gives accurate
// word-level bounding boxes rather than line-level observation boxes.

#import "vision_ocr.h"
#import <Vision/Vision.h>
#import <CoreGraphics/CoreGraphics.h>
#import <stdlib.h>
#import <string.h>
#import <math.h>

bool lens_vision_available(void) {
    return NSClassFromString(@"VNRecognizeTextRequest") != nil;
}

LensOcrResult lens_vision_recognize(const uint8_t *pixels,
                                    int32_t width,
                                    int32_t height,
                                    int32_t stride) {
    LensOcrResult result = { .text = NULL, .word_count = 0, .words = NULL };

    // Input validation.
    if (!pixels || width <= 0 || height <= 0 || stride <= 0) return result;
    // BGRA requires at least 4 bytes per pixel.
    if (stride < width * 4) return result;
    // Overflow check: stride * height must not overflow size_t.
    if ((uint64_t)stride * (uint64_t)height > SIZE_MAX) return result;
    // Reasonable upper bound: 100 megapixels.
    if ((uint64_t)width * (uint64_t)height > 100ULL * 1000000) return result;

    @autoreleasepool {
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        if (!cs) return result;

        CGBitmapInfo bitmapInfo = kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little;
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL, pixels, (size_t)(stride * height), NULL);
        if (!provider) {
            CGColorSpaceRelease(cs);
            return result;
        }

        CGImageRef image = CGImageCreate(
            (size_t)width, (size_t)height,
            8, 32,
            (size_t)stride,
            cs,
            bitmapInfo,
            provider,
            NULL, NO,
            kCGRenderingIntentDefault);

        CGDataProviderRelease(provider);
        CGColorSpaceRelease(cs);

        if (!image) return result;

        VNRecognizeTextRequest *request =
            [[VNRecognizeTextRequest alloc] init];
        request.recognitionLevel = 1;
        request.usesLanguageCorrection = YES;

        VNImageRequestHandler *handler =
            [[VNImageRequestHandler alloc] initWithCGImage:image
                                                  options:@{}];
        NSError *error = nil;
        [handler performRequests:@[request] error:&error];

        if (error) {
            CGImageRelease(image);
            return result;
        }

        NSArray<VNRecognizedTextObservation *> *observations = request.results;
        if (!observations.count) {
            CGImageRelease(image);
            return result;
        }

        NSMutableString *fullText = [NSMutableString string];
        // Temporary storage for words before copying to the C struct.
        NSMutableArray *wordItems = [NSMutableArray array];

        for (VNRecognizedTextObservation *obs in observations) {
            VNRecognizedText *topCandidate =
                [obs topCandidates:1].firstObject;
            if (!topCandidate) continue;

            NSString *obsText = topCandidate.string;
            if (!obsText.length) continue;

            // Track the UTF-8 byte offset of this observation in fullText.
            NSUInteger obsUtf8Start = [fullText dataUsingEncoding:NSUTF8StringEncoding].length;
            if (fullText.length > 0) {
                [fullText appendString:@"\n"];
                obsUtf8Start = [fullText dataUsingEncoding:NSUTF8StringEncoding].length;
            }
            [fullText appendString:obsText];

            // Enumerate word substrings within this observation.
            NSRange obsRange = NSMakeRange(0, obsText.length);
            [obsText enumerateSubstringsInRange:obsRange
                                       options:NSStringEnumerationByWords
                                    usingBlock:^(NSString *word, NSRange wordRange,
                                                 NSRange enclosingRange, BOOL *stop) {
                (void)enclosingRange;
                (void)stop;
                if (!word || !word.length) return;

                // Get per-word bounding box via Vision API.
                // boundingBoxForRange: returns VNRectangleObservation *, not CGRect.
                NSError *boxError = nil;
                VNRectangleObservation *boxObs =
                    [topCandidate boundingBoxForRange:wordRange error:&boxError];
                if (boxError || !boxObs) return;
                CGRect box = boxObs.boundingBox;
                if (CGRectIsEmpty(box)) return;
                // Validate finite coordinates.
                if (!isfinite(box.origin.x) || !isfinite(box.origin.y) ||
                    !isfinite(box.size.width) || !isfinite(box.size.height)) return;

                // Convert normalized Vision box (bottom-left origin) to
                // pixel coordinates (top-left origin).
                float px = (float)(box.origin.x * width);
                float py = (float)((1.0 - box.origin.y - box.size.height) * height);
                float pw = (float)(box.size.width * width);
                float ph = (float)(box.size.height * height);

                // Compute UTF-8 offset of this word in the full text.
                NSString *prefix = [obsText substringToIndex:wordRange.location];
                NSData *prefixUtf8 = [prefix dataUsingEncoding:NSUTF8StringEncoding];
                uint32_t wordUtf8Offset = (uint32_t)(obsUtf8Start + prefixUtf8.length);

                NSData *wordUtf8 = [word dataUsingEncoding:NSUTF8StringEncoding];
                uint32_t wordLen = (uint32_t)wordUtf8.length;

                // Pack into an NSValue wrapping a LensOcrWord struct.
                LensOcrWord w = {
                    .x = px, .y = py, .width = pw, .height = ph,
                    .utf8_offset = wordUtf8Offset, .utf8_length = wordLen
                };
                [wordItems addObject:[NSValue value:&w withObjCType:@encode(LensOcrWord)]];
            }];
        }

        // Copy text to a C string.
        const char *utf8 = [fullText UTF8String];
        if (utf8) {
            result.text = strdup(utf8);
            if (!result.text) {
                CGImageRelease(image);
                return result;
            }
        }

        // Copy word data to a typed C array.
        int32_t wordCount = (int32_t)wordItems.count;
        result.word_count = wordCount;
        if (wordCount > 0) {
            // Overflow check for allocation.
            if ((uint64_t)wordCount > SIZE_MAX / sizeof(LensOcrWord)) {
                CGImageRelease(image);
                free(result.text);
                result.text = NULL;
                result.word_count = 0;
                return result;
            }
            result.words = (LensOcrWord *)malloc(sizeof(LensOcrWord) * wordCount);
            if (!result.words) {
                CGImageRelease(image);
                free(result.text);
                result.text = NULL;
                result.word_count = 0;
                return result;
            }
            for (int32_t i = 0; i < wordCount; i++) {
                [wordItems[i] getValue:&result.words[i]];
            }
        }

        CGImageRelease(image);
    }

    return result;
}

void lens_ocr_result_free(LensOcrResult *result) {
    if (!result) return;
    free(result->text);
    free(result->words);
    result->text = NULL;
    result->words = NULL;
    result->word_count = 0;
}
