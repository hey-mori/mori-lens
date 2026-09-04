//! macOS Vision framework OCR.
//!
//! Recognizes text from a raw pixel buffer using `VNRecognizeTextRequest`.
//! No image is written to disk: the pixel buffer is passed directly from
//! Rust to Vision in memory (privacy invariant).

use std::os::raw::c_char;

/// Raw OCR result from the C bridge.
#[repr(C)]
struct LensOcrResult {
    text: *mut c_char,
    word_count: i32,
    word_boxes: *mut f32,
}

extern "C" {
    fn lens_vision_recognize(
        pixels: *const u8,
        width: i32,
        height: i32,
        stride: i32,
    ) -> LensOcrResult;
    fn lens_ocr_result_free(result: *mut LensOcrResult);
    fn lens_vision_available() -> bool;
}

/// Whether the Vision framework OCR is available on this system.
pub fn is_available() -> bool {
    // SAFETY: lens_vision_available is a pure query.
    unsafe { lens_vision_available() }
}

/// A recognized word with its bounding box in pixel coordinates.
#[derive(Debug, Clone)]
pub struct RecognizedWord {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Result of a Vision OCR recognition.
#[derive(Debug, Clone)]
pub struct VisionOcrResult {
    pub text: String,
    pub words: Vec<RecognizedWord>,
}

/// Recognize text from a raw RGBA pixel buffer.
///
/// `pixels` is RGBA, 4 bytes per pixel, row-major, top-to-bottom.
/// `width` and `height` are in pixels. `stride` is bytes per row.
///
/// Returns `None` if recognition fails or no text is found.
pub fn recognize(pixels: &[u8], width: i32, height: i32, stride: i32) -> Option<VisionOcrResult> {
    if pixels.is_empty() || width <= 0 || height <= 0 {
        return None;
    }

    // SAFETY: pixels points to a valid buffer of at least stride * height
    // bytes. The C function reads it and returns a result. We free the
    // result afterwards.
    let raw = unsafe { lens_vision_recognize(pixels.as_ptr(), width, height, stride) };

    if raw.text.is_null() {
        return None;
    }

    // Convert the C string to a Rust String.
    // SAFETY: raw.text is a valid null-terminated UTF-8 string from strdup.
    let text = unsafe {
        let c_str = std::ffi::CStr::from_ptr(raw.text);
        c_str.to_string_lossy().into_owned()
    };

    // Extract word boxes.
    let mut words = Vec::new();
    if raw.word_count > 0 && !raw.word_boxes.is_null() {
        // SAFETY: word_boxes has word_count * 4 floats.
        let boxes =
            unsafe { std::slice::from_raw_parts(raw.word_boxes, (raw.word_count as usize) * 4) };
        // Split the text into lines/words to pair with boxes.
        let text_words: Vec<&str> = text.split_whitespace().collect();
        for i in 0..raw.word_count as usize {
            let x = boxes[i * 4] as i32;
            let y = boxes[i * 4 + 1] as i32;
            let w = boxes[i * 4 + 2] as i32;
            let h = boxes[i * 4 + 3] as i32;
            let word_text = text_words.get(i).unwrap_or(&"").to_string();
            words.push(RecognizedWord {
                text: word_text,
                x,
                y,
                width: w,
                height: h,
            });
        }
    }

    // Free the C result. raw was returned by lens_vision_recognize and has
    // not been freed yet.
    let mut raw_mut = raw;
    // SAFETY: raw_mut owns the C result from lens_vision_recognize.
    unsafe { lens_ocr_result_free(&mut raw_mut) };

    Some(VisionOcrResult { text, words })
}
