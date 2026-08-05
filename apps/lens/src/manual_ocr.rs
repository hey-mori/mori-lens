//! Reading what the accessibility layer will not give up.
//!
//! The fallback [ADR-0002](../../../docs/decisions/ADR-0002-accessibility-first-acquisition.md)
//! always intended, brought forward because this repository's own matrix says
//! the two applications its first user lives in expose almost nothing: VS Code
//! answered 7 of 247 sampled points and all of them labels, and KakaoTalk gave
//! one separable target in a whole window.
//!
//! # It is asked for, never guessed at
//!
//! There is no automatic fall-through from a failed hover to OCR. A hover that
//! finds nothing says so and stops. Capturing pixels because an application was
//! quiet would make every unsupported window a screenshot, on a timer, which is
//! the product this one is defined against.
//!
//! # A crop around the pointer, not a drag
//!
//! Other screen translators start with a rectangle the user drags. This one
//! does not, and the reason is the same one the whole product rests on: **the
//! pointer is already the question.** The reader is looking at a word. Asking
//! them to then draw a box around it is asking them to say it twice.
//!
//! So the key means "read here", and the region is a band around the pointer
//! big enough to hold the line it is on and its neighbours.
//!
//! # The card comes down first
//!
//! Measured, and not obvious: a capture of a region under the card returns a
//! **mixture** of the card and the desktop behind it, because the surface is
//! translucent. Which one wins varies. So the card is hidden before the
//! capture, which is also the milestone's own rule that Lens never recognises
//! its own overlay.

use std::time::Duration;

use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_platform_windows::{capture, ocr, Overlay};

/// The band captured around the pointer, in pixels at 100% scaling.
///
/// Wide enough for a line of prose at a readable size, and tall enough for the
/// line above and below so a sentence that wraps is not cut in half.
///
/// Small enough to be a crop rather than a screenshot: **6.6% of a 1080p
/// screen**. The first version of this comment said "about 4%", which was a
/// number nobody had divided.
const BAND_WIDTH: i32 = 760;
const BAND_HEIGHT: i32 = 180;

/// What the pointer was on, read from pixels.
#[derive(Debug)]
pub struct Reading {
    /// The line the pointer was on. The analogue of the accessibility path's
    /// context: what gets translated.
    pub sentence: String,
    /// The word under the pointer, for the focus hint.
    pub word: String,
    /// Where the line is, for the card to sit clear of.
    pub anchor: ScreenRect,
}

/// Why a reading did not happen, in words a card can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoReading {
    /// The region could not be captured at all.
    NotCaptured,
    /// The recogniser is unavailable, or has no language pack.
    NoRecogniser,
    /// It read the region and found no text.
    NothingThere,
    /// It found text, but none of it under the pointer.
    NotUnderThePointer,
}

impl NoReading {
    /// A sentence for the card, in the reader's language.
    pub fn user_message(self) -> &'static str {
        match self {
            Self::NotCaptured => "화면을 읽지 못했습니다",
            Self::NoRecogniser => "이 시스템에 문자 인식기가 없습니다",
            Self::NothingThere => "이 부분에서 글자를 찾지 못했습니다",
            Self::NotUnderThePointer => "포인터 위에 글자가 없습니다",
        }
    }

    /// The short form, for a log line.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotCaptured => "not-captured",
            Self::NoRecogniser => "no-recogniser",
            Self::NothingThere => "nothing-there",
            Self::NotUnderThePointer => "not-under-the-pointer",
        }
    }
}

/// Read the region around `pointer`, with the card out of the way.
///
/// `overlay` is hidden before the capture and left hidden: whatever the caller
/// does next will show a card of its own, and leaving a stale one up while a
/// translation runs would put the previous answer beside the new sentence.
pub fn read_around(
    overlay: &Overlay,
    pointer: ScreenPoint,
    scale: f32,
) -> Result<Reading, NoReading> {
    overlay.hide();
    // The compositor needs a moment to take it down. Without this the capture
    // races the hide and gets the card it was trying to avoid.
    std::thread::sleep(Duration::from_millis(60));

    let width = (BAND_WIDTH as f32 * scale).round() as i32;
    let height = (BAND_HEIGHT as f32 * scale).round() as i32;
    let region = ScreenRect::new(pointer.x - width / 2, pointer.y - height / 2, width, height);

    let captured = capture::rectangle(region).ok_or(NoReading::NotCaptured)?;
    let read = ocr::read(&captured, ScreenPoint::new(region.x, region.y))
        .ok_or(NoReading::NoRecogniser)?;

    if read.lines.iter().all(|line| line.words.is_empty()) {
        return Err(NoReading::NothingThere);
    }

    let (line, word) = read.word_at(pointer).ok_or(NoReading::NotUnderThePointer)?;
    Ok(Reading {
        sentence: line.text.trim().to_owned(),
        word: word.text.clone(),
        // The line, so the card sits clear of what is being read. The
        // accessibility path clears the paragraph when it can; a crop has no
        // paragraph to clear, only the lines it happened to catch.
        anchor: line
            .bounds()
            .unwrap_or(ScreenRect::new(pointer.x, pointer.y, 1, 1)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every refusal has to be sayable, because the card shows one and a code
    /// on a card tells a reader nothing.
    #[test]
    fn every_refusal_has_a_sentence_and_a_reason() {
        for refusal in [
            NoReading::NotCaptured,
            NoReading::NoRecogniser,
            NoReading::NothingThere,
            NoReading::NotUnderThePointer,
        ] {
            assert!(!refusal.user_message().is_empty(), "{refusal:?}");
            assert!(!refusal.reason().is_empty(), "{refusal:?}");
            // The reason is for a log and the message is for a person; a
            // refusal that used one for both would leak a code onto a card.
            assert_ne!(refusal.user_message(), refusal.reason());
        }
    }

    /// The band is a crop, not a screenshot.
    ///
    /// A tenth of a 1080p screen is the line this holds: enough for a
    /// paragraph, nowhere near enough to be a way of reading a window. It is
    /// asserted rather than left to a comment because the tempting fix for
    /// "OCR missed the end of the line" is to make this bigger.
    #[test]
    fn the_captured_band_is_a_small_part_of_a_screen() {
        let band = BAND_WIDTH * BAND_HEIGHT;
        let screen = 1920 * 1080;
        assert!(
            band * 10 < screen,
            "{band} pixels is {:.1}% of a {screen} pixel screen, which is not a crop",
            band as f64 / screen as f64 * 100.0,
        );
    }
}
