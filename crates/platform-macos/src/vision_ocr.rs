//! macOS Vision framework OCR.
//!
//! Recognizes text from a raw BGRA pixel buffer using `VNRecognizeTextRequest`.
//! No image is written to disk: the pixel buffer is passed directly from
//! Rust to Vision in memory (privacy invariant).
//!
//! Per-word geometry: each word has a bounding box and UTF-8 offset/length
//! in the full text, obtained via `boundingBoxForRange:`.

use std::os::raw::c_char;

/// Raw per-word OCR result from the C bridge.
/// Must match `LensOcrWord` in vision_ocr.h exactly.
#[repr(C)]
struct LensOcrWord {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    utf8_offset: u32,
    utf8_length: u32,
}

/// Raw OCR result from the C bridge.
/// Must match `LensOcrResult` in vision_ocr.h exactly.
#[repr(C)]
struct LensOcrResult {
    text: *mut c_char,
    word_count: i32,
    words: *mut LensOcrWord,
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
/// # Safety contract
/// `pixels` must point to a valid buffer of at least `stride * height` bytes.
/// `width` and `height` must be positive. `stride` must be >= `width * 4`.
/// The native side performs its own validation; this function returns `None`
/// for any invalid input.
pub fn recognize(pixels: &[u8], width: i32, height: i32, stride: i32) -> Option<VisionOcrResult> {
    if pixels.is_empty() || width <= 0 || height <= 0 || stride <= 0 {
        return None;
    }
    // BGRA requires at least 4 bytes per pixel.
    if stride < width * 4 {
        return None;
    }
    // Overflow check: stride * height must not overflow.
    let required = (stride as u64).checked_mul(height as u64)?;
    if required > pixels.len() as u64 {
        return None;
    }

    // SAFETY: pixels points to a valid buffer of at least stride * height
    // bytes (checked above). The C function reads it and returns a result.
    let raw = unsafe { lens_vision_recognize(pixels.as_ptr(), width, height, stride) };

    if raw.text.is_null() {
        return None;
    }

    // SAFETY: raw.text is a valid null-terminated UTF-8 string from strdup.
    let text = unsafe {
        let c_str = std::ffi::CStr::from_ptr(raw.text);
        c_str.to_string_lossy().into_owned()
    };

    // Extract per-word data from the typed struct array.
    let mut words = Vec::new();
    if raw.word_count > 0 && !raw.words.is_null() {
        // SAFETY: words has word_count elements of type LensOcrWord.
        let word_slice = unsafe { std::slice::from_raw_parts(raw.words, raw.word_count as usize) };
        for w in word_slice {
            let word_text = text
                .get(w.utf8_offset as usize..(w.utf8_offset + w.utf8_length) as usize)
                .unwrap_or("")
                .to_string();

            words.push(RecognizedWord {
                text: word_text,
                x: w.x as i32,
                y: w.y as i32,
                width: w.width as i32,
                height: w.height as i32,
                utf8_offset: w.utf8_offset as usize,
                utf8_length: w.utf8_length as usize,
            });
        }
    }

    // Free the C result. Always call free, even on early paths.
    let mut raw_mut = raw;
    // SAFETY: raw_mut owns the C result from lens_vision_recognize.
    unsafe { lens_ocr_result_free(&mut raw_mut) };

    Some(VisionOcrResult { text, words })
}
