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

bool lens_vision_available(void) {
    return NSClassFromString(@"VNRecognizeTextRequest") != nil;
}

LensOcrResult lens_vision_recognize(const uint8_t *pixels,
                                    int32_t width,
                                    int32_t height,
                                    int32_t stride) {
    LensOcrResult result = { .text = NULL, .word_count = 0, .word_data = NULL };

    if (!pixels || width <= 0 || height <= 0) return result;

    @autoreleasepool {
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        CGBitmapInfo bitmapInfo = kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little;
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL, pixels, (size_t)(stride * height), NULL);

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
        NSMutableArray *wordEntries = [NSMutableArray array];

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
                if (!word || !word.length) return;

                // Get per-word bounding box via Vision API.
                NSError *boxError = nil;
                CGRect box = [topCandidate boundingBoxForRange:wordRange error:&boxError];
                if (boxError || CGRectIsEmpty(box)) return;

                // Convert normalized Vision box (bottom-left origin) to
                // pixel coordinates (top-left origin).
                float px = (float)(box.origin.x * width);
                float py = (float)((1.0 - box.origin.y - box.size.height) * height);
                float pw = (float)(box.size.width * width);
                float ph = (float)(box.size.height * height);

                // Compute UTF-8 offset of this word in the full text.
                NSString *prefix = [obsText substringToIndex:wordRange.location];
                NSData *prefixUtf8 = [prefix dataUsingEncoding:NSUTF8StringEncoding];
                NSUInteger wordUtf8Offset = obsUtf8Start + prefixUtf8.length;

                NSData *wordUtf8 = [word dataUsingEncoding:NSUTF8StringEncoding];
                NSUInteger wordLen = wordUtf8.length;

                [wordEntries addObject:@(px)];
                [wordEntries addObject:@(py)];
                [wordEntries addObject:@(pw)];
                [wordEntries addObject:@(ph)];
                [wordEntries addObject:@((float)wordUtf8Offset)];
                [wordEntries addObject:@((float)wordLen)];
            }];
        }

        const char *utf8 = [fullText UTF8String];
        if (utf8) {
            result.text = strdup(utf8);
        }

        int wordCount = (int)wordEntries.count / 6;
        result.word_count = wordCount;
        if (wordCount > 0) {
            result.word_data = (float *)malloc(sizeof(float) * wordCount * 6);
            for (int i = 0; i < wordCount * 6; i++) {
                result.word_data[i] = [wordEntries[i] floatValue];
            }
        }

        CGImageRelease(image);
    }

    return result;
}

void lens_ocr_result_free(LensOcrResult *result) {
    if (result) {
        free(result->text);
        free(result->word_data);
        result->text = NULL;
        result->word_data = NULL;
        result->word_count = 0;
    }
}
