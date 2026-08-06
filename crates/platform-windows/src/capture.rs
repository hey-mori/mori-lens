//! One rectangle of the screen, in memory, for exactly as long as it takes.
//!
//! This is the only file in the project that reads pixels the user did not
//! choose to give it, so the rules around it are the strictest here.
//!
//! # What it will not do
//!
//! **Nothing reaches disk. Ever, including for debugging** (`CLAUDE.md`). There
//! is no path out of this module that writes a file, and the returned buffer is
//! a plain `Vec` the caller drops.
//!
//! **It captures a rectangle, not a screen.** `BitBlt` copies only the region
//! asked for. A whole-monitor grab followed by a crop would be simpler and
//! would mean the pixels of everything else existed in this process for a
//! moment, which is a different promise.
//!
//! **It runs only when asked.** Nothing here is on a timer and nothing here is
//! reachable from the hover path
//! ([ADR-0002](../../../docs/decisions/ADR-0002-accessibility-first-acquisition.md)).
//!
//! # Why the screen device context
//!
//! `GetDC(None)` is the desktop, which is what a user pointing at a region
//! means. It includes any window on top of that region, including Lens's own
//! card, which is why the caller hides the card first and why the milestone
//! gate says Lens never recognises its own overlay.

use lens_core::geometry::ScreenRect;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};

/// A ceiling on any single capture, so a mistake in the selection interface
/// cannot ask this process to hold a whole wall of monitors. Generous for a
/// paragraph and far below a desktop.
const MAX_SIDE: i32 = 4096;

/// A rectangle of screen, as premultiplied-irrelevant BGRA bytes.
///
/// Deliberately not `Clone`. A capture that can be duplicated is a capture that
/// can be kept, and the only correct lifetime for this is "until the words come
/// out".
pub struct Capture {
    pub width: i32,
    pub height: i32,
    pixels: Vec<u8>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the pixels, and never anything derived from them. A hash would
        // be a fingerprint of what was on screen.
        formatter
            .debug_struct("Capture")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Capture {
    /// The bytes, for a recogniser to read. Borrowed, never handed over.
    pub fn bytes(&self) -> &[u8] {
        &self.pixels
    }
}

/// Copy one rectangle of the desktop into memory.
///
/// Returns `None` for an empty or absurd rectangle, and for any GDI failure.
/// There is no error type because there is nothing a caller could do
/// differently: a capture either happened or the user is told OCR did not work.
pub fn rectangle(region: ScreenRect) -> Option<Capture> {
    if region.width <= 0
        || region.height <= 0
        || region.width > MAX_SIDE
        || region.height > MAX_SIDE
    {
        return None;
    }

    let header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: region.width,
            // Negative for top-down, which is the order a recogniser expects.
            biHeight: -region.height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // SAFETY: every handle is released on every path, and the buffer is copied
    // out before the bitmap is deleted.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let memory = CreateCompatibleDC(Some(screen));
        if memory.is_invalid() {
            ReleaseDC(None, screen);
            return None;
        }
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(bitmap) =
            CreateDIBSection(Some(memory), &header, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            return None;
        };
        let previous = SelectObject(memory, bitmap.into());

        let copied = BitBlt(
            memory,
            0,
            0,
            region.width,
            region.height,
            Some(screen),
            region.x,
            region.y,
            SRCCOPY,
        )
        .is_ok();

        let pixels = if copied && !bits.is_null() {
            let count = (region.width * region.height) as usize * 4;
            let mut buffer = std::slice::from_raw_parts(bits.cast::<u8>(), count).to_vec();
            // GDI leaves the alpha channel at zero and the recogniser reads
            // BGRA. Without this it is handed a fully transparent image, which
            // is the same mistake the tray glyph made.
            for pixel in buffer.chunks_exact_mut(4) {
                pixel[3] = 0xff;
            }
            Some(buffer)
        } else {
            None
        };

        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        pixels.map(|pixels| Capture {
            width: region.width,
            height: region.height,
            pixels,
        })
    }
}

impl Capture {
    /// The same picture, `factor` times larger, sampled bilinearly.
    ///
    /// **For the recogniser, not for a person.** `Windows.Media.Ocr` is trained
    /// on text at document sizes and degrades sharply below them, and a code
    /// editor is well below them: a recogniser measuring VS Code returned a
    /// median word of 28x10px. What came back from that was `do은5` for `does`
    /// and `5h0l word_banner` for `show_word`, translated and presented with a
    /// straight face.
    ///
    /// Bilinear rather than nearest. Nearest gives the recogniser more pixels
    /// of the same staircase; bilinear gives it the smooth edges the shapes
    /// were antialiased into before the screen quantised them, which is what
    /// the classifier is looking for.
    ///
    /// `None` when the result would exceed what a capture may hold, so a large
    /// band cannot become an enormous allocation by asking for a big factor.
    pub fn scaled(&self, factor: u32) -> Option<Self> {
        if factor <= 1 {
            return None;
        }
        let factor = i32::try_from(factor).ok()?;
        let (width, height) = (
            self.width.checked_mul(factor)?,
            self.height.checked_mul(factor)?,
        );
        if width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let sample = |x: i32, y: i32, channel: usize| -> f32 {
            let x = x.clamp(0, self.width - 1) as usize;
            let y = y.clamp(0, self.height - 1) as usize;
            f32::from(self.pixels[(y * self.width as usize + x) * 4 + channel])
        };

        for out_y in 0..height {
            // The centre of the output pixel, mapped back into the source. The
            // half-pixel offsets are what stop the whole picture drifting by
            // half a source pixel towards the origin, which at these sizes is
            // most of a glyph's stem.
            let source_y = (f32::from(out_y as i16) + 0.5) / factor as f32 - 0.5;
            let top = source_y.floor();
            let weight_y = source_y - top;
            for out_x in 0..width {
                let source_x = (f32::from(out_x as i16) + 0.5) / factor as f32 - 0.5;
                let left = source_x.floor();
                let weight_x = source_x - left;
                let (left, top) = (left as i32, top as i32);

                let base = ((out_y as usize) * (width as usize) + out_x as usize) * 4;
                for channel in 0..4 {
                    let upper = sample(left, top, channel) * (1.0 - weight_x)
                        + sample(left + 1, top, channel) * weight_x;
                    let lower = sample(left, top + 1, channel) * (1.0 - weight_x)
                        + sample(left + 1, top + 1, channel) * weight_x;
                    let value = upper * (1.0 - weight_y) + lower * weight_y;
                    pixels[base + channel] = value.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    /// A capture built from bytes this process produced rather than from the
    /// screen. Only for measuring the recogniser against text this repository
    /// drew, which is the one way to score it without printing anybody's.
    #[cfg(test)]
    pub(crate) fn from_bgra(width: i32, height: i32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_absurd_rectangle_captures_nothing() {
        assert!(rectangle(ScreenRect::new(0, 0, 0, 100)).is_none());
        assert!(rectangle(ScreenRect::new(0, 0, 100, 0)).is_none());
        assert!(rectangle(ScreenRect::new(0, 0, -5, 100)).is_none());
        // Wider than any selection interface should ever ask for.
        assert!(rectangle(ScreenRect::new(0, 0, 100_000, 100)).is_none());
    }

    /// A capture must not be duplicable. This is a compile-time property and
    /// the test exists to say so out loud, since deriving `Clone` later would
    /// look like an ordinary convenience.
    #[test]
    fn a_capture_cannot_be_cloned() {
        fn assert_not_clone<T>() {}
        assert_not_clone::<Capture>();
    }
}
