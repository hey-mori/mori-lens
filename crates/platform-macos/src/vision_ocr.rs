//! macOS Vision framework OCR.
//!
//! Recognizes text from a raw BGRA pixel buffer using `VNRecognizeTextRequest`.
//! No image is written to disk: the pixel buffer is passed directly from
//! Rust to Vision in memory (privacy invariant).
//!
//! Per-word geometry: each word has a bounding box and UTF-8 offset/length
//! in the full text, obtained via `boundingBoxForRange:`.

use std::os::raw::c_char;

/// Raw OCR result from the C bridge.
#[repr(C)]
struct LensOcrResult {
    text: *mut c_char,
    word_count: i32,
    word_data: *mut f32,
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

/// A recognized word with its bounding box and position in the full text.
#[derive(Debug, Clone)]
pub struct RecognizedWord {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// UTF-8 byte offset of this word in the full text.
    pub utf8_offset: usize,
    /// UTF-8 byte length of this word.
    pub utf8_length: usize,
}

/// Result of a Vision OCR recognition.
#[derive(Debug, Clone)]
pub struct VisionOcrResult {
    pub text: String,
    pub words: Vec<RecognizedWord>,
}

/// Recognize text from a raw BGRA pixel buffer.
///
/// `pixels` is BGRA, 4 bytes per pixel, row-major, top-to-bottom.
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

    // SAFETY: raw.text is a valid null-terminated UTF-8 string from strdup.
    let text = unsafe {
        let c_str = std::ffi::CStr::from_ptr(raw.text);
        c_str.to_string_lossy().into_owned()
    };

    // Extract per-word data (6 floats per word: x, y, w, h, utf8_offset, utf8_length).
    let mut words = Vec::new();
    if raw.word_count > 0 && !raw.word_data.is_null() {
        // SAFETY: word_data has word_count * 6 floats.
        let data =
            unsafe { std::slice::from_raw_parts(raw.word_data, (raw.word_count as usize) * 6) };
        for i in 0..raw.word_count as usize {
            let base = i * 6;
            let x = data[base] as i32;
            let y = data[base + 1] as i32;
            let w = data[base + 2] as i32;
            let h = data[base + 3] as i32;
            let utf8_offset = data[base + 4] as usize;
            let utf8_length = data[base + 5] as usize;

            // Extract word text from the full text using UTF-8 offsets.
            let word_text = text
                .get(utf8_offset..utf8_offset + utf8_length)
                .unwrap_or("")
                .to_string();

            words.push(RecognizedWord {
                text: word_text,
                x,
                y,
                width: w,
                height: h,
                utf8_offset,
                utf8_length,
            });
        }
    }

    let mut raw_mut = raw;
    // SAFETY: raw_mut owns the C result from lens_vision_recognize.
    unsafe { lens_ocr_result_free(&mut raw_mut) };

    Some(VisionOcrResult { text, words })
}
