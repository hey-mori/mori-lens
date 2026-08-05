//! Words out of a captured rectangle, with where each one is on screen.
//!
//! The spike ([#62](https://github.com/swarea/mori-lens/issues/62)) settled
//! that `Windows.Media.Ocr` works from an unpackaged binary and that this
//! machine has both English and Korean recognisers. This is the same engine
//! used for what the product actually needs: not a page of text, but **words
//! with rectangles**, so the pointer can be resolved to one of them exactly as
//! it is on the accessibility path.
//!
//! # Coordinates
//!
//! The recogniser answers in the captured bitmap's own coordinates. Everything
//! that leaves this module is in **screen** coordinates, translated by the
//! rectangle that was captured, because that is the space the pointer, the card
//! and every rule in `lens-core` are expressed in. Returning bitmap coordinates
//! and translating at each call site is how one call site forgets.
//!
//! # What it does not do
//!
//! It does not capture, and it does not decide when to run. Both belong to the
//! caller, which is what keeps this file honest about
//! [ADR-0002](../../../docs/decisions/ADR-0002-accessibility-first-acquisition.md):
//! OCR is a fallback the user asks for, and nothing here can be reached from a
//! hover.

use lens_core::geometry::{ScreenPoint, ScreenRect};
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

use crate::capture::Capture;

/// `AsyncStatus::Started`, compared as the integer it is.
///
/// The type lives in the `windows` crate's own vendored `windows-future`, and
/// depending on that crate separately produces a second, incompatible copy.
/// The value is part of the WinRT ABI and does not move.
const ASYNC_STARTED: i32 = 0;

/// How long to wait for a recogniser before giving up on it.
///
/// A hover's guard is 800ms because a reader is waiting on it. OCR is asked for
/// deliberately and runs on a whole region, so it is allowed longer, and it
/// still may not hang the caller forever.
const RECOGNISE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// One recognised word and where it is, in screen coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrWord {
    pub text: String,
    pub bounds: ScreenRect,
}

/// One recognised line: its words, in order, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrLine {
    pub text: String,
    pub words: Vec<OcrWord>,
}

impl OcrLine {
    /// The rectangle covering the whole line, or `None` for a line with no
    /// words, which the recogniser does occasionally return.
    pub fn bounds(&self) -> Option<ScreenRect> {
        let first = self.words.first()?.bounds;
        Some(self.words.iter().skip(1).fold(first, |joined, word| {
            let left = joined.x.min(word.bounds.x);
            let top = joined.y.min(word.bounds.y);
            let right = joined.right().max(word.bounds.right());
            let bottom = joined.bottom().max(word.bounds.bottom());
            ScreenRect::new(left, top, right - left, bottom - top)
        }))
    }
}

/// Everything read out of one captured rectangle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recognised {
    pub lines: Vec<OcrLine>,
}

impl Recognised {
    /// The line the pointer is on, and the word within it.
    ///
    /// The same question the accessibility path answers with
    /// `RangeFromPoint`, asked of pixels instead. Containment first, and then
    /// the nearest word on the same line, because a pointer between two words
    /// is on the line and a reader means the word beside the gap.
    ///
    /// `None` when the point is on no line at all, which is the honest answer
    /// for a pointer in a margin: the accessibility path's equivalent mistake
    /// was answering with the nearest text anywhere on the page, and
    /// [ADR-0009](../../../docs/decisions/ADR-0009-pointer-distance-gate.md)
    /// exists because of it.
    pub fn word_at(&self, point: ScreenPoint) -> Option<(&OcrLine, &OcrWord)> {
        let line = self
            .lines
            .iter()
            .find(|line| line.bounds().is_some_and(|bounds| bounds.contains(point)))?;
        let word = line.words.iter().min_by(|a, b| {
            a.bounds
                .distance_to(point)
                .total_cmp(&b.bounds.distance_to(point))
        })?;
        Some((line, word))
    }

    /// Everything read, as one string, for the rare caller that wants it.
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Which languages this machine can recognise, in BCP-47.
pub fn available_languages() -> Vec<String> {
    let Ok(list) = OcrEngine::AvailableRecognizerLanguages() else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|language| language.LanguageTag().ok())
        .map(|tag| tag.to_string_lossy())
        .collect()
}

/// Whether a language can be recognised at all on this machine.
pub fn can_recognise(tag: &str) -> bool {
    Language::CreateLanguage(&HSTRING::from(tag))
        .and_then(|language| OcrEngine::IsLanguageSupported(&language))
        .unwrap_or(false)
}

/// Read a captured rectangle.
///
/// `origin` is where the capture came from, and every rectangle that comes back
/// is offset by it.
pub fn read(capture: &Capture, origin: ScreenPoint) -> Option<Recognised> {
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().ok()?;
    read_with(&engine, capture, origin)
}

fn read_with(engine: &OcrEngine, capture: &Capture, origin: ScreenPoint) -> Option<Recognised> {
    let bitmap = to_bitmap(capture).ok()?;
    let operation = engine.RecognizeAsync(&bitmap).ok()?;

    // Waited for rather than assumed finished. `GetResults` on an operation
    // still running answers `0x8000000E`, which during the spike briefly looked
    // like somebody else's constraint.
    let started = std::time::Instant::now();
    while operation.Status().ok()?.0 == ASYNC_STARTED {
        if started.elapsed() > RECOGNISE_TIMEOUT {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let result = operation.GetResults().ok()?;
    let mut lines = Vec::new();
    for line in result.Lines().ok()? {
        let mut words = Vec::new();
        for word in line.Words().ok()? {
            let (Ok(text), Ok(rect)) = (word.Text(), word.BoundingRect()) else {
                continue;
            };
            words.push(OcrWord {
                text: text.to_string_lossy(),
                bounds: ScreenRect::new(
                    origin.x + rect.X.round() as i32,
                    origin.y + rect.Y.round() as i32,
                    rect.Width.round() as i32,
                    rect.Height.round() as i32,
                ),
            });
        }
        lines.push(OcrLine {
            text: line.Text().ok()?.to_string_lossy(),
            words,
        });
    }
    Some(Recognised { lines })
}

fn to_bitmap(capture: &Capture) -> windows::core::Result<SoftwareBitmap> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(capture.bytes())?;
    let buffer = writer.DetachBuffer()?;
    SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        capture.width,
        capture.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x: i32, y: i32, width: i32) -> OcrWord {
        OcrWord {
            text: text.to_owned(),
            bounds: ScreenRect::new(x, y, width, 20),
        }
    }

    fn two_lines() -> Recognised {
        Recognised {
            lines: vec![
                OcrLine {
                    text: "the output files".to_owned(),
                    words: vec![
                        word("the", 100, 100, 40),
                        word("output", 150, 100, 80),
                        word("files", 240, 100, 60),
                    ],
                },
                OcrLine {
                    text: "came back empty".to_owned(),
                    words: vec![
                        word("came", 100, 130, 60),
                        word("back", 170, 130, 50),
                        word("empty", 230, 130, 70),
                    ],
                },
            ],
        }
    }

    #[test]
    fn a_line_covers_all_of_its_words() {
        let bounds = two_lines().lines[0].bounds().expect("bounds");
        assert_eq!(bounds.x, 100);
        assert_eq!(bounds.right(), 300);
    }

    #[test]
    fn a_line_with_no_words_has_no_bounds() {
        let empty = OcrLine {
            text: String::new(),
            words: Vec::new(),
        };
        assert_eq!(empty.bounds(), None);
    }

    #[test]
    fn the_pointer_resolves_to_the_word_it_is_on() {
        let read = two_lines();
        let (line, word) = read.word_at(ScreenPoint::new(180, 110)).expect("a word");
        assert_eq!(word.text, "output");
        assert_eq!(line.text, "the output files");
    }

    /// A pointer in the gap between two words is still on the line, and the
    /// reader means whichever word the gap is nearer to.
    ///
    /// Both directions, because the first version of this test asserted only
    /// one and asserted the wrong one: `output` ends at 230 and `files` starts
    /// at 240, so a point at 234 is nearer the word before it, not after.
    #[test]
    fn a_pointer_between_two_words_takes_the_nearer_one() {
        let read = two_lines();

        let (_, before) = read.word_at(ScreenPoint::new(233, 110)).expect("a word");
        assert_eq!(before.text, "output", "3px past the end of it");

        let (_, after) = read.word_at(ScreenPoint::new(237, 110)).expect("a word");
        assert_eq!(after.text, "files", "3px before the start of it");
    }

    #[test]
    fn the_second_line_is_reachable() {
        let read = two_lines();
        let (line, word) = read.word_at(ScreenPoint::new(240, 140)).expect("a word");
        assert_eq!(word.text, "empty");
        assert_eq!(line.text, "came back empty");
    }

    /// The mistake ADR-0009 exists for, on the other path. A pointer in a
    /// margin is on nothing, and answering with the nearest word anywhere
    /// would explain a sentence the reader was not looking at.
    #[test]
    fn a_pointer_in_the_margin_is_on_no_word_at_all() {
        assert!(two_lines().word_at(ScreenPoint::new(600, 400)).is_none());
        assert!(two_lines().word_at(ScreenPoint::new(180, 300)).is_none());
    }

    #[test]
    fn the_whole_reading_is_available_as_text() {
        assert_eq!(two_lines().text(), "the output files\ncame back empty");
    }
}
