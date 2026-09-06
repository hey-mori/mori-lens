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
    // BGRA requires at least 4 bytes per pixel (widened to avoid i32 overflow).
    let min_stride = (width as u64).checked_mul(4)?;
    if (stride as u64) < min_stride {
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

    // RAII guard: always free the C result on any return path.
    struct RawGuard(LensOcrResult);
    impl Drop for RawGuard {
        fn drop(&mut self) {
            // SAFETY: self.0 owns the C result. lens_ocr_result_free is safe
            // on any state returned by lens_vision_recognize (including partial).
            unsafe { lens_ocr_result_free(&mut self.0) };
        }
    }
    let guard = RawGuard(raw);

    if guard.0.text.is_null() {
        return None;
    }

    // SAFETY: guard.0.text is a valid null-terminated UTF-8 string from strdup.
    let text = unsafe {
        let c_str = std::ffi::CStr::from_ptr(guard.0.text);
        c_str.to_string_lossy().into_owned()
    };

    // Extract per-word data from the typed struct array.
    let mut words = Vec::new();
    if guard.0.word_count > 0 && !guard.0.words.is_null() {
        // Defensive: reject negative or unreasonably large word counts.
        let count = guard.0.word_count as usize;
        if count > 1_000_000 {
            return None;
        }
        // SAFETY: words has word_count elements of type LensOcrWord.
        let word_slice = unsafe { std::slice::from_raw_parts(guard.0.words, count) };
        for w in word_slice {
            // Checked arithmetic for range bounds.
            let end = (w.utf8_offset as u64).checked_add(w.utf8_length as u64)?;
            if end > text.len() as u64 {
                // Invalid range — skip this word (fail-closed per word).
                continue;
            }
            let offset = w.utf8_offset as usize;
            let length = w.utf8_length as usize;
            // Verify UTF-8 char boundaries.
            if !text.is_char_boundary(offset) || !text.is_char_boundary(offset + length) {
                continue;
            }
            let word_text = text[offset..offset + length].to_string();

            words.push(RecognizedWord {
                text: word_text,
                x: w.x as i32,
                y: w.y as i32,
                width: w.width as i32,
                height: w.height as i32,
                utf8_offset: offset,
                utf8_length: length,
            });
        }
    }

    Some(VisionOcrResult { text, words })
    // guard drops here, freeing the C result.
}
