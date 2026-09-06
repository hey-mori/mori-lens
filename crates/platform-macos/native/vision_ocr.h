#ifndef LENS_VISION_OCR_H
#define LENS_VISION_OCR_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Result of an OCR recognition.
typedef struct {
    /// The recognized text (UTF-8, heap-allocated). Caller must free with
    /// lens_ocr_result_free. NULL if recognition failed.
    char *text;
    /// Number of recognized words.
    int32_t word_count;
    /// Per-word data: 6 floats per word [x, y, w, h, utf8_offset, utf8_length].
    ///   x, y, w, h — bounding box in pixel coordinates (top-left origin).
    ///   utf8_offset — byte offset of this word in `text`.
    ///   utf8_length — byte length of this word in `text`.
    /// Length is word_count * 6. Heap-allocated; freed by lens_ocr_result_free.
    float *word_data;
} LensOcrResult;

/// Recognize text from a raw pixel buffer.
///
/// `pixels` points to BGRA pixel data (4 bytes per pixel), row-major,
/// top-to-bottom. `width` and `height` are in pixels. `stride` is the number
/// of bytes per row (typically width * 4).
///
/// Returns per-word bounding boxes via `boundingBoxForRange:` for accurate
/// word-level geometry. Each word includes its UTF-8 offset in the full text.
///
/// Returns a result with text=NULL on failure.
LensOcrResult lens_vision_recognize(const uint8_t *pixels,
                                    int32_t width,
                                    int32_t height,
                                    int32_t stride);

/// Free an OCR result.
void lens_ocr_result_free(LensOcrResult *result);

/// Check whether the Vision framework OCR is available on this system.
bool lens_vision_available(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_VISION_OCR_H */
