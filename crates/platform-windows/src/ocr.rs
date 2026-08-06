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

/// How far outside a line's own rectangle still counts as on that line, as a
/// multiple of the line's height.
///
/// Half. A line's glyph box is roughly two thirds of its pitch, so the gap
/// between two lines is about half a box: half above and half below closes it
/// exactly, and no further. Anything more would reach past the neighbouring
/// line, and the nearest-line rule would still resolve it correctly, but the
/// margin at the end of a paragraph would start answering too.
const LINE_SLACK: f64 = 0.5;

impl Recognised {
    /// The line the pointer is on, and the word within it.
    ///
    /// The same question the accessibility path answers with
    /// `RangeFromPoint`, asked of pixels instead: the nearest line within a
    /// tolerance, and then the nearest word on it, because a pointer between
    /// two words is still on the line and a reader means the word beside the
    /// gap.
    ///
    /// `None` when the point is on no line, which is the honest answer for a
    /// pointer in a margin. The accessibility path's equivalent mistake was
    /// answering with the nearest text anywhere on the page, and
    /// [ADR-0009](../../../docs/decisions/ADR-0009-pointer-distance-gate.md)
    /// exists because of it. The tolerance below is what keeps this on the
    /// right side of that line.
    ///
    /// # Why there is a tolerance at all
    ///
    /// This required the point to be **inside** a line's rectangle, and that
    /// rectangle is the glyphs, not the line. A recogniser measuring VS Code
    /// returned a median word of 28x10px against a line pitch nearer 16, so
    /// between every pair of lines is a band several pixels tall where a
    /// pointer is over text, looks over text, and resolves to nothing. Two
    /// consecutive `not-under-the-pointer` refusals in an ordinary session is
    /// what that looks like from outside.
    ///
    /// So the tolerance is a multiple of **the line's own height**, exactly as
    /// `is_on_target` uses a multiple of line height rather than a pixel
    /// constant: the slack being forgiven is leading, and leading scales with
    /// the font and with the display. A constant tuned on 16px text is wrong
    /// on a 40px heading in both directions.
    pub fn word_at(&self, point: ScreenPoint) -> Option<(&OcrLine, &OcrWord)> {
        let mut best: Option<(f64, &OcrLine)> = None;

        for line in &self.lines {
            let Some(bounds) = line.bounds() else {
                continue;
            };
            let slack = f64::from(bounds.height.max(1)) * LINE_SLACK;

            // Horizontal first, and unforgiving beyond one slack: past the end
            // of a line is the margin, and the margin is where answering with
            // the nearest text is the mistake ADR-0009 names.
            let x = f64::from(point.x);
            if x < f64::from(bounds.x) - slack || x > f64::from(bounds.right()) + slack {
                continue;
            }

            let above = f64::from(bounds.y) - f64::from(point.y);
            let below = f64::from(point.y) - f64::from(bounds.bottom());
            let gap = above.max(below).max(0.0);
            if gap > slack {
                continue;
            }
            // Nearest wins, which is what makes the tolerance safe to set wide
            // enough to close the gap between two lines: a point in that gap
            // is inside both tolerances and belongs to the closer line.
            if best.is_none() || best.is_some_and(|(closest, _)| gap < closest) {
                best = Some((gap, line));
            }
        }

        let (_, line) = best?;
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

#[cfg(test)]
mod pointer_tests {
    use super::*;

    /// Three lines of 10px glyphs on a 16px pitch, which is what a recogniser
    /// reported from a real editor: median word 28x10px.
    fn page() -> Recognised {
        let line = |row: i32, text: &str| OcrLine {
            text: text.to_owned(),
            words: text
                .split(' ')
                .enumerate()
                .map(|(index, word)| OcrWord {
                    text: word.to_owned(),
                    bounds: ScreenRect::new(100 + index as i32 * 30, 100 + row * 16, 28, 10),
                })
                .collect(),
        };
        Recognised {
            lines: vec![
                line(0, "alpha beta"),
                line(1, "gamma delta"),
                line(2, "epsilon zeta"),
            ],
        }
    }

    #[test]
    fn a_point_on_a_word_resolves_to_it() {
        let read = page();
        let (_, word) = read.word_at(ScreenPoint::new(110, 105)).expect("a word");
        assert_eq!(word.text, "alpha");
        let (_, word) = read.word_at(ScreenPoint::new(140, 105)).expect("a word");
        assert_eq!(word.text, "beta");
    }

    /// The defect this tolerance exists for. Between two 10px lines on a 16px
    /// pitch there is a 6px band that is visually text and resolved to
    /// nothing, and a pointer lands in it a third of the time.
    #[test]
    fn a_point_between_two_lines_belongs_to_the_nearer_one() {
        let read = page();
        // 112 is two pixels under the first line's box and four above the
        // second's.
        let (line, _) = read.word_at(ScreenPoint::new(110, 112)).expect("a line");
        assert_eq!(line.text, "alpha beta");
        // 114 is four under the first and two above the second.
        let (line, _) = read.word_at(ScreenPoint::new(110, 114)).expect("a line");
        assert_eq!(line.text, "gamma delta");
    }

    #[test]
    fn every_pixel_between_the_first_and_last_line_resolves() {
        // The whole point: no dead bands left inside a paragraph.
        let read = page();
        for y in 100..=141 {
            assert!(
                read.word_at(ScreenPoint::new(110, y)).is_some(),
                "y={y} is inside the paragraph and resolved to nothing",
            );
        }
    }

    #[test]
    fn a_point_in_the_margin_beside_a_line_is_still_nothing() {
        // The tolerance must not turn into the mistake ADR-0009 names: an
        // answer for a pointer that is not on text.
        let read = page();
        assert!(
            read.word_at(ScreenPoint::new(400, 105)).is_none(),
            "far right"
        );
        assert!(
            read.word_at(ScreenPoint::new(20, 105)).is_none(),
            "far left"
        );
    }

    #[test]
    fn a_point_well_above_or_below_the_text_is_nothing() {
        let read = page();
        assert!(read.word_at(ScreenPoint::new(110, 80)).is_none(), "above");
        assert!(read.word_at(ScreenPoint::new(110, 200)).is_none(), "below");
    }

    #[test]
    fn the_tolerance_scales_with_the_text_rather_than_being_a_constant() {
        // A heading. Five pixels off a 40px line is well inside its leading
        // and would be far outside a 10px line's.
        let heading = Recognised {
            lines: vec![OcrLine {
                text: "Heading".to_owned(),
                words: vec![OcrWord {
                    text: "Heading".to_owned(),
                    bounds: ScreenRect::new(100, 100, 200, 40),
                }],
            }],
        };
        assert!(heading.word_at(ScreenPoint::new(150, 155)).is_some());
        assert!(
            heading.word_at(ScreenPoint::new(150, 175)).is_none(),
            "half a line's height is the limit, whatever the line",
        );
    }

    #[test]
    fn nothing_recognised_resolves_to_nothing() {
        let empty = Recognised { lines: Vec::new() };
        assert!(empty.word_at(ScreenPoint::new(110, 105)).is_none());
    }
}

/// The size a glyph has to be before `Windows.Media.Ocr` reads it reliably.
///
/// Measured, not guessed. The same card drawn at 8pt was read back as 0 of 7
/// English words at native size and 6 of 7 at three times it, and the
/// difference between those two runs is nothing but pixels per glyph. At 1.0
/// scaling the same card scored 5 of 7 native, so the cliff is somewhere just
/// under this.
const SMALL_TEXT_PX: i32 = 16;

/// What a glyph should be enlarged to.
///
/// Three times a 10px word is 30, which is where the measurement above stopped
/// improving. Higher costs recognition time for nothing.
const COMFORTABLE_PX: i32 = 30;

/// The most a capture may be enlarged.
///
/// Three. Not a budget: past this the picture is mostly interpolation, and a
/// recogniser given more of that reads the interpolation.
const MOST_ENLARGEMENT: u32 = 3;

/// Read, and read again enlarged if what came back was small.
///
/// One pass is not enough and two passes on everything is waste, so the first
/// pass decides. It answers the question the caller cannot: how big the glyphs
/// on this screen actually are. Below [`SMALL_TEXT_PX`] the same picture is
/// enlarged and asked again, and that second answer is the one used.
///
/// The case this exists for is a code editor. A recogniser measuring VS Code
/// returned a median word of 28x10px, and what it read at that size was `do은5`
/// for `does` and `5h0l word_banner` for `show_word`. Lens translated both and
/// showed the result with no sign that anything had gone wrong, which is worse
/// than reading nothing.
pub fn read_best(capture: &Capture, origin: ScreenPoint) -> Option<Recognised> {
    let first = read(capture, origin)?;
    let Some(median) = first.median_word_height() else {
        return Some(first);
    };
    if median >= SMALL_TEXT_PX {
        return Some(first);
    }

    let wanted = (COMFORTABLE_PX + median - 1) / median.max(1);
    let factor = u32::try_from(wanted)
        .unwrap_or(1)
        .clamp(2, MOST_ENLARGEMENT);
    let Some(enlarged) = capture.scaled(factor) else {
        return Some(first);
    };
    // Read at the origin, then divided back: the rectangles come out in the
    // enlarged picture's pixels and every caller of this works in screen ones.
    let second = read(&enlarged, ScreenPoint::new(0, 0))?;
    Some(second.shrunk_by(factor, origin))
}

impl Recognised {
    /// The median height of a recognised word, or `None` when nothing was read.
    pub fn median_word_height(&self) -> Option<i32> {
        let mut heights: Vec<i32> = self
            .lines
            .iter()
            .flat_map(|line| &line.words)
            .map(|word| word.bounds.height)
            .collect();
        if heights.is_empty() {
            return None;
        }
        heights.sort_unstable();
        Some(heights[heights.len() / 2])
    }

    /// The same reading, mapped from an enlarged picture back to the screen.
    fn shrunk_by(self, factor: u32, origin: ScreenPoint) -> Self {
        let factor = factor.max(1) as i32;
        let map = |rect: ScreenRect| {
            ScreenRect::new(
                origin.x + rect.x / factor,
                origin.y + rect.y / factor,
                (rect.width / factor).max(1),
                (rect.height / factor).max(1),
            )
        };
        Self {
            lines: self
                .lines
                .into_iter()
                .map(|line| OcrLine {
                    text: line.text,
                    words: line
                        .words
                        .into_iter()
                        .map(|word| OcrWord {
                            text: word.text,
                            bounds: map(word.bounds),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// Whether a recognised word is a misreading rather than a word.
///
/// **The last lever, because preprocessing is spent.** Enlarging took small
/// text from 0 of 7 words to 6 of 7 and nothing beyond 3x helps; contrast is a
/// cliff below 3:1 and flat above it; inverting a dark capture changes nothing
/// at all. At the size a code editor renders, on a dark theme, the recogniser
/// returns four or five of seven words and no amount of preparing the picture
/// moves that. See `docs/benchmarks/2026-08-06-ocr-small-text.md`.
///
/// So the remaining thing Lens can do is stop presenting a misreading as a
/// reading. `Windows.Media.Ocr` gives no per-word confidence, so this is a
/// shape rule.
///
/// # Hangul in the middle of a Latin word
///
/// `do은5` for `does`, `k국05t` for part of a URL, `de님op디5` for `developers`.
/// Each has a Hangul syllable with a letter or a digit **on both sides of it**,
/// which is a shape real text does not have.
///
/// A suffix does not count, and that distinction is the whole rule. Korean
/// technical writing attaches particles to English identifiers constantly:
/// `export_csv를`, `orbbase에`, `RowQuery의`, `k8s를`. Those are Hangul at the
/// end of a Latin token and they are correct readings of correct text.
/// Flagging them would fire on most of what this product's first reader
/// writes, which is worse than the silence it is trying to fix.
///
/// # What it cannot see, written down because the gap is the useful part
///
/// - **`님hOle` for `whole`.** The Hangul is first, with nothing before it. A
///   rule that caught a leading syllable would also catch a Korean word run
///   into an English one, and a false positive here suppresses a correct
///   reading.
/// - **`5h0l` for `show`, `0f` for `Off`.** Digits standing in for letters,
///   with no Hangul anywhere. Catching that needs a rule about digits inside
///   Latin words, and `k8s`, `qwen2.5` and `sha256` are all that shape.
///
/// So this is a floor on what can be detected rather than a ceiling. It
/// catches the misreadings whose shape is impossible and leaves the ones whose
/// shape is merely unlikely.
pub fn is_misread(word: &str) -> bool {
    let hangul =
        |c: char| ('\u{AC00}'..='\u{D7A3}').contains(&c) || ('\u{3130}'..='\u{318F}').contains(&c);
    let latin = |c: char| c.is_ascii_alphabetic();
    let filler = |c: char| c.is_ascii_alphanumeric();

    let characters: Vec<char> = word.chars().collect();
    // At least one Latin letter somewhere, so a Korean word beside a number is
    // never a candidate.
    if !characters.iter().any(|c| latin(*c)) {
        return false;
    }
    characters
        .iter()
        .enumerate()
        .filter(|(_, c)| hangul(**c))
        .any(|(at, _)| {
            // Digits count as filler, not only letters: `do은5` has a digit
            // after the syllable and is exactly the shape this is for.
            let before = characters[..at].iter().any(|c| filler(*c));
            let after = characters[at + 1..].iter().any(|c| filler(*c));
            before && after
        })
}

#[cfg(test)]
mod misreading_tests {
    use super::is_misread;

    /// Words a reader photographed, off their own screen, in cards that
    /// presented a confident translation of them.
    #[test]
    fn the_misreadings_from_the_cards_are_caught() {
        for word in ["do은5", "k국05t", "de님op디5", "En글lish"] {
            assert!(is_misread(word), "{word} was not caught");
        }
    }

    /// The misreadings this rule cannot see, asserted so the gap stays honest.
    ///
    /// A test that shows only what a rule catches invites a reader to believe
    /// it catches everything.
    #[test]
    fn the_misreadings_it_cannot_see_are_named() {
        // Hangul first, with nothing before it. A rule that caught a leading
        // syllable would also catch a Korean word run into an English one,
        // and a false positive here suppresses a correct reading.
        assert!(!is_misread("님hOle"));
        // Digits standing in for letters, with no Hangul anywhere. Catching
        // that needs a rule about digits inside Latin words, and `k8s`,
        // `qwen2.5` and `sha256` are all that shape.
        assert!(!is_misread("5h0l"));
        assert!(!is_misread("0f"));
    }

    /// Korean technical writing, which does this constantly and correctly.
    #[test]
    fn a_particle_on_an_identifier_is_not_a_misreading() {
        for word in [
            "export_csv를",
            "orbbase에",
            "RowQuery의",
            "systemd에서",
            "k8s를",
            "GitHub과",
        ] {
            assert!(!is_misread(word), "{word} was flagged");
        }
    }

    #[test]
    fn ordinary_words_in_either_script_are_not_misreadings() {
        for word in [
            "translation",
            "테스트",
            "127.0.0.1:11434",
            "src/main.rs",
            "8pt",
        ] {
            assert!(!is_misread(word), "{word} was flagged");
        }
    }

    /// A Latin word inside Korean is not a misreading either, and the rule
    /// only looks at Hangul with Latin on **both** sides of it.
    #[test]
    fn latin_inside_korean_is_not_flagged() {
        assert!(
            !is_misread("한글English한글"),
            "Latin inside Hangul is a different shape, and a legitimate one",
        );
        assert!(is_misread("En글lish"), "Hangul inside Latin is the shape");
    }
}
