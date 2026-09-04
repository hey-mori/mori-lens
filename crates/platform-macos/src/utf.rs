//! UTF-8 / UTF-16 / CFRange conversion helpers.
//!
//! macOS Accessibility APIs use UTF-16 internally (via `CFString`), while Rust
//! uses UTF-8. This module provides the conversions between the two, plus
//! helpers for translating between byte offsets (Rust) and character offsets
//! (AX/CFRange).
//!
//! The key invariant: **CFRange uses UTF-16 code unit offsets**, not Unicode
//! scalar offsets. A supplementary-plane character (e.g. emoji) occupies two
//! UTF-16 code units, so a CFRange length of 2 may represent one Rust `char`.

use crate::ffi::{self, CFIndex};

/// Convert a `CFStringRef` to a Rust `String`.
///
/// Returns `None` if the reference is null or the string cannot be decoded.
///
/// # Safety
///
/// The caller must ensure `cf_string` is either null or a valid, retained
/// `CFStringRef`. This function does not retain or release it.
pub unsafe fn cf_string_to_string(cf_string: ffi::CFStringRef) -> Option<String> {
    if cf_string.is_null() {
        return None;
    }
    // SAFETY: cf_string is valid per the caller's contract.
    let len = unsafe { ffi::CFStringGetLength(cf_string) };
    if len == 0 {
        return Some(String::new());
    }

    let mut buffer = vec![0u16; len as usize];
    let range = ffi::CFRange {
        location: 0,
        length: len,
    };
    // SAFETY: cf_string is valid per the caller's contract. The buffer has
    // exactly `len` elements, matching the range.
    unsafe {
        ffi::CFStringGetCharacters(cf_string, range, buffer.as_mut_ptr());
    }

    String::from_utf16(&buffer).ok()
}

/// Convert a Rust `&str` to a `Vec<u16>` (UTF-16 code units).
pub fn str_to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Convert a UTF-16 code unit offset to a UTF-8 byte offset in the given
/// string.
///
/// Returns the byte offset clamped to the string length. If the UTF-16 offset
/// falls in the middle of a surrogate pair, the result is the byte offset of
/// the *next* character boundary.
pub fn utf16_offset_to_byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_offset, ch) in text.char_indices() {
        if utf16_count >= utf16_offset {
            return byte_offset;
        }
        utf16_count += ch.len_utf16();
    }
    // The UTF-16 offset is at or past the end of the string.
    text.len()
}

/// Convert a UTF-8 byte offset to a UTF-16 code unit offset in the given
/// string.
///
/// If the byte offset falls in the middle of a multi-byte character, the
/// result is the UTF-16 offset of the *next* character boundary.
pub fn byte_offset_to_utf16_offset(text: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(text.len());
    text[..clamped].encode_utf16().count()
}

/// Convert a `CFRange` (UTF-16 offsets) to a Rust byte range `(start, end)`
/// in the given string.
///
/// The result is clamped to the string's byte length.
pub fn cf_range_to_byte_range(text: &str, range: ffi::CFRange) -> (usize, usize) {
    let start = utf16_offset_to_byte_offset(text, range.location as usize);
    let end_utf16 = (range.location + range.length) as usize;
    let end = utf16_offset_to_byte_offset(text, end_utf16);
    (start, end.min(text.len()))
}

/// Convert a Rust byte range `(start, end)` to a `CFRange` (UTF-16 offsets).
pub fn byte_range_to_cf_range(text: &str, start: usize, end: usize) -> ffi::CFRange {
    let utf16_start = byte_offset_to_utf16_offset(text, start) as CFIndex;
    let utf16_end = byte_offset_to_utf16_offset(text, end) as CFIndex;
    ffi::CFRange {
        location: utf16_start,
        length: utf16_end - utf16_start,
    }
}

/// Create a `CFRange` from location and length (both UTF-16 code units).
pub fn cf_range(location: i64, length: i64) -> ffi::CFRange {
    ffi::CFRange { location, length }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trip() {
        let text = "Hello, world!";
        let utf16 = str_to_utf16(text);
        assert_eq!(utf16.len(), 13);
        // ASCII: each char is one UTF-16 code unit.
        assert_eq!(byte_offset_to_utf16_offset(text, 7), 7);
        assert_eq!(utf16_offset_to_byte_offset(text, 7), 7);
    }

    #[test]
    fn korean_characters_are_single_utf16_units() {
        let text = "안녕하세요";
        let utf16 = str_to_utf16(text);
        // Each Hangul syllable is in BMP, so one UTF-16 code unit each.
        assert_eq!(utf16.len(), 5);
        assert_eq!(byte_offset_to_utf16_offset(text, 0), 0);
        // Each Hangul syllable is 3 bytes in UTF-8.
        assert_eq!(byte_offset_to_utf16_offset(text, 3), 1);
        assert_eq!(byte_offset_to_utf16_offset(text, 6), 2);
        assert_eq!(utf16_offset_to_byte_offset(text, 1), 3);
        assert_eq!(utf16_offset_to_byte_offset(text, 2), 6);
    }

    #[test]
    fn japanese_characters_are_single_utf16_units() {
        let text = "こんにちは";
        let utf16 = str_to_utf16(text);
        assert_eq!(utf16.len(), 5);
        // Each Hiragana is 3 bytes in UTF-8, 1 code unit in UTF-16.
        assert_eq!(byte_offset_to_utf16_offset(text, 3), 1);
        assert_eq!(utf16_offset_to_byte_offset(text, 1), 3);
    }

    #[test]
    fn emoji_are_double_utf16_units() {
        let text = "Hello 🌍!";
        let utf16 = str_to_utf16(text);
        // "Hello " = 6 code units, 🌍 = 2 code units (surrogate pair), "!" = 1.
        assert_eq!(utf16.len(), 9);
        // Byte offset of 🌍 is 6 (after "Hello ").
        assert_eq!(byte_offset_to_utf16_offset(text, 6), 6);
        // Byte offset after 🌍 is 10 (4-byte UTF-8 sequence).
        assert_eq!(byte_offset_to_utf16_offset(text, 10), 8);
        // UTF-16 offset 7 is in the middle of the surrogate pair;
        // utf16_offset_to_byte_offset should advance past the character.
        assert_eq!(utf16_offset_to_byte_offset(text, 7), 10);
        assert_eq!(utf16_offset_to_byte_offset(text, 8), 10);
    }

    #[test]
    fn combining_characters() {
        // e + combining acute accent (U+0301) = "é" as two code points.
        let text = "e\u{0301}";
        assert_eq!(text.chars().count(), 2);
        let utf16 = str_to_utf16(text);
        // Both characters are in BMP, so 2 UTF-16 code units.
        assert_eq!(utf16.len(), 2);
        assert_eq!(byte_offset_to_utf16_offset(text, 1), 1);
        assert_eq!(byte_offset_to_utf16_offset(text, 3), 2);
    }

    #[test]
    fn mixed_korean_and_latin() {
        let text = "Hello 안녕 World";
        let utf16 = str_to_utf16(text);
        // "Hello " = 6, "안녕" = 2, " World" = 6 = 14 total.
        assert_eq!(utf16.len(), 14);
        // "안" starts at byte 6, UTF-16 offset 6.
        assert_eq!(byte_offset_to_utf16_offset(text, 6), 6);
        // "안" is 3 bytes, so "녕" starts at byte 9, UTF-16 offset 7.
        assert_eq!(byte_offset_to_utf16_offset(text, 9), 7);
    }

    #[test]
    fn cf_range_conversion_round_trip() {
        let text = "Hello 🌍 World";
        // UTF-16: H(0) e(1) l(2) l(3) o(4) ' '(5) 🌍(6,7) ' '(8) W(9) o(10) r(11) l(12) d(13)
        // Byte:   H(0) e(1) l(2) l(3) o(4) ' '(5) 🌍(6..10) ' '(10) W(11) o(12) r(13) l(14) d(15)
        let range = cf_range(6, 2); // The emoji in UTF-16
        let (start, end) = cf_range_to_byte_range(text, range);
        assert_eq!(start, 6);
        assert_eq!(end, 10);
        let back = byte_range_to_cf_range(text, start, end);
        assert_eq!(back.location, 6);
        assert_eq!(back.length, 2);
    }

    #[test]
    fn cf_range_clamped_to_string_bounds() {
        let text = "Hi";
        let range = cf_range(0, 100);
        let (start, end) = cf_range_to_byte_range(text, range);
        assert_eq!(start, 0);
        assert_eq!(end, 2);
    }

    #[test]
    fn empty_string_conversions() {
        assert_eq!(byte_offset_to_utf16_offset("", 0), 0);
        assert_eq!(utf16_offset_to_byte_offset("", 0), 0);
        let range = cf_range(0, 0);
        let (start, end) = cf_range_to_byte_range("", range);
        assert_eq!((start, end), (0, 0));
    }

    #[test]
    fn byte_offset_past_end_clamps() {
        let text = "Hello";
        assert_eq!(byte_offset_to_utf16_offset(text, 999), 5);
        assert_eq!(utf16_offset_to_byte_offset(text, 999), 5);
    }

    // -- Property-style tests --

    #[test]
    fn utf16_byte_round_trip_at_every_char_boundary() {
        // For every character boundary in a string, converting byte→utf16→byte
        // must return the same boundary.
        let texts = [
            "Hello, world!",
            "안녕하세요 세계",
            "こんにちは世界",
            "Hello 🌍 World 🎉",
            "e\u{0301}galite\u{0301}",
            "mixed 한글 Latin 日本語 emoji 🚀",
            "",
            "a",
            "\u{10000}\u{10FFFF}",
        ];
        for text in texts {
            let mut byte_offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            byte_offsets.push(text.len());
            for &byte_off in &byte_offsets {
                let utf16_off = byte_offset_to_utf16_offset(text, byte_off);
                let back = utf16_offset_to_byte_offset(text, utf16_off);
                assert_eq!(
                    back, byte_off,
                    "round-trip failed for text={text:?} byte_off={byte_off} utf16_off={utf16_off}"
                );
            }
        }
    }

    #[test]
    fn cf_range_byte_range_is_always_within_bounds() {
        let texts = [
            "Hello",
            "안녕하세요",
            "🌍🎉🚀",
            "mixed 한글 test",
            "a\u{0301}b\u{0302}c\u{0303}",
        ];
        for text in texts {
            for loc in 0..=(text.encode_utf16().count() as i64) {
                for len in 0..=((text.encode_utf16().count() as i64) - loc) {
                    let range = cf_range(loc, len);
                    let (start, end) = cf_range_to_byte_range(text, range);
                    assert!(
                        start <= end,
                        "start > end for text={text:?} range={range:?}"
                    );
                    assert!(
                        end <= text.len(),
                        "end past string length for text={text:?} range={range:?}"
                    );
                }
            }
        }
    }

    // -- Stress tests --

    #[test]
    fn stress_utf16_conversion_large_string() {
        // Build a string with 10,000 characters including CJK and emoji.
        let text: String = (0..2000)
            .map(|i| match i % 4 {
                0 => 'A',
                1 => '한',
                2 => '日',
                _ => '🌍',
            })
            .collect();
        let utf16 = str_to_utf16(&text);
        // Verify length: A=1utf16, 한=1utf16, 日=1utf16, 🌍=2utf16
        let expected_utf16_len = 500 * (1 + 1 + 1 + 2);
        assert_eq!(utf16.len(), expected_utf16_len);

        // Round-trip at every char boundary (not every 100th byte, since
        // fixed-step byte offsets can land inside multi-byte characters).
        for (byte_off, _) in text.char_indices() {
            let utf16_off = byte_offset_to_utf16_offset(&text, byte_off);
            let back = utf16_offset_to_byte_offset(&text, utf16_off);
            assert_eq!(back, byte_off, "round-trip failed at byte {byte_off}");
        }
    }

    #[test]
    fn stress_cf_range_conversion_repeated() {
        let text = "The quick brown fox jumps over the lazy dog. 안녕하세요.";
        // Run the conversion 10,000 times to check for leaks or inconsistencies.
        for _ in 0..10_000 {
            let range = cf_range(4, 10);
            let (start, end) = cf_range_to_byte_range(text, range);
            let back = byte_range_to_cf_range(text, start, end);
            assert_eq!(back.location, 4);
            assert_eq!(back.length, 10);
        }
    }
}
