//! The tray glyph, as pixels rather than as a file.
//!
//! Lens has no icon asset and this is deliberate. A tray icon has to be sharp
//! at whatever size Windows asks for, which on a mixed-DPI desktop is not one
//! size, and it has to say at a glance whether the product is listening. A
//! `.ico` committed at two or three fixed sizes answers neither: it is resampled
//! at every other size, and a second state means a second file that drifts from
//! the first.
//!
//! So the glyph is computed. It is drawn at exactly the size requested, in the
//! colour the theme resolved, and the paused state is the same shape rather
//! than a different picture, because a state indicator that changes shape is
//! read as a different application.
//!
//! Pure, and therefore tested. `lens-core` holds no OS types
//! ([ADR-0003](../../../docs/decisions/ADR-0003-rust-core-platform-adapters.md)),
//! so what leaves here is a buffer of pixels and the adapter turns it into
//! whatever the platform calls an icon.

use crate::theme::Rgb;

/// What the icon is saying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// Listening. The pointer will be read when it settles.
    Listening,
    /// Paused. Nothing is read at all, which is a stronger promise than
    /// "no card is shown", so the icon says it plainly.
    Paused,
}

/// How opaque the paused glyph is, against the listening one.
///
/// Dimmed rather than redrawn. A tray that swaps in a different shape reads as
/// a different application, and the user has to learn two pictures instead of
/// one picture with two weights.
const PAUSED_ALPHA: f64 = 0.38;

/// Outer edge of the ring, as a fraction of the icon's half-width.
///
/// Not 1.0: Windows draws tray icons hard against their neighbours, and a glyph
/// touching its own bounds looks larger and dirtier than the system ones beside
/// it.
const OUTER: f64 = 0.86;

/// Inner edge, leaving a ring thick enough to survive a 16 pixel icon.
///
/// At 16px the ring is about 2.5 pixels across. Thinner and anti-aliasing eats
/// it; thicker and it closes into a blob at small sizes.
const INNER: f64 = 0.54;

/// Samples per axis inside each pixel.
///
/// A ring is all curve, and at 16 pixels an unsampled edge is visibly stepped.
/// Three by three is nine samples per pixel, which is enough for an edge this
/// simple and cheap enough to do on every theme change.
const SUPERSAMPLE: u32 = 3;

/// The glyph as premultiplied BGRA pixels, top row first.
///
/// Premultiplied because that is what every compositor this will meet expects,
/// and because doing it here means the adapter cannot forget. A caller that
/// wants straight alpha has to divide it back out, which is the rarer case.
///
/// `size` is the icon's width and height in physical pixels. A zero size yields
/// an empty buffer rather than a panic: the platform is allowed to ask for
/// something silly and a tray icon is not worth a crash.
pub fn tray_glyph(size: u32, colour: Rgb, glyph: Glyph) -> Vec<u32> {
    if size == 0 {
        return Vec::new();
    }

    let alpha_scale = match glyph {
        Glyph::Listening => 1.0,
        Glyph::Paused => PAUSED_ALPHA,
    };

    let half = f64::from(size) / 2.0;
    let outer = half * OUTER;
    let inner = half * INNER;
    let step = 1.0 / f64::from(SUPERSAMPLE);

    let mut pixels = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0u32;
            for sub_y in 0..SUPERSAMPLE {
                for sub_x in 0..SUPERSAMPLE {
                    // The sample sits at the centre of its sub-cell, not its
                    // corner. Sampling corners biases the whole ring half a
                    // pixel up and left, which is invisible at 64 and obvious
                    // at 16.
                    let px = f64::from(x) + (f64::from(sub_x) + 0.5) * step;
                    let py = f64::from(y) + (f64::from(sub_y) + 0.5) * step;
                    let distance = (px - half).hypot(py - half);
                    if distance <= outer && distance >= inner {
                        hits += 1;
                    }
                }
            }

            let coverage = f64::from(hits) / f64::from(SUPERSAMPLE * SUPERSAMPLE) * alpha_scale;
            pixels.push(premultiplied(colour, coverage));
        }
    }
    pixels
}

/// One pixel, as `0xAARRGGBB` with the colour already multiplied by the alpha.
fn premultiplied(colour: Rgb, coverage: f64) -> u32 {
    let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u32;
    let scale = |channel: u8| (u32::from(channel) * alpha) / 255;
    (alpha << 24) | (scale(colour.r) << 16) | (scale(colour.g) << 8) | scale(colour.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INK: Rgb = Rgb {
        r: 200,
        g: 220,
        b: 255,
    };

    fn alpha_at(pixels: &[u32], size: u32, x: u32, y: u32) -> u32 {
        pixels[(y * size + x) as usize] >> 24
    }

    #[test]
    fn a_zero_sized_request_is_empty_rather_than_a_panic() {
        assert!(tray_glyph(0, INK, Glyph::Listening).is_empty());
    }

    #[test]
    fn the_buffer_is_square_and_the_size_that_was_asked_for() {
        for size in [16u32, 20, 24, 32, 48] {
            assert_eq!(
                tray_glyph(size, INK, Glyph::Listening).len(),
                (size * size) as usize,
                "{size}px",
            );
        }
    }

    /// It is a ring, so the middle must be a hole. A filled circle would read
    /// as a dot, which is what half the tray already looks like.
    #[test]
    fn the_centre_is_transparent() {
        let size = 32;
        let pixels = tray_glyph(size, INK, Glyph::Listening);
        assert_eq!(alpha_at(&pixels, size, size / 2, size / 2), 0);
    }

    #[test]
    fn the_corners_are_transparent() {
        let size = 32;
        let pixels = tray_glyph(size, INK, Glyph::Listening);
        for (x, y) in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)] {
            assert_eq!(alpha_at(&pixels, size, x, y), 0, "corner ({x}, {y})");
        }
    }

    /// The ring itself, sampled where it must be solid: halfway between the
    /// inner and outer edges, on the horizontal through the centre.
    #[test]
    fn the_ring_is_opaque_where_it_is_thickest() {
        let size = 32;
        let pixels = tray_glyph(size, INK, Glyph::Listening);
        let half = f64::from(size) / 2.0;
        let radius = half * (OUTER + INNER) / 2.0;
        let x = (half + radius).round() as u32 - 1;
        assert_eq!(alpha_at(&pixels, size, x, size / 2), 255);
    }

    /// Survives the size that matters. At 16 pixels the ring is thin enough
    /// that a careless constant would anti-alias it away entirely.
    #[test]
    fn the_ring_still_reaches_full_opacity_at_sixteen_pixels() {
        let size = 16;
        let pixels = tray_glyph(size, INK, Glyph::Listening);
        let solid = pixels.iter().filter(|pixel| (*pixel >> 24) == 255).count();
        assert!(
            solid >= 8,
            "only {solid} fully opaque pixels at 16px, so the glyph is a smudge",
        );
    }

    #[test]
    fn paused_is_the_same_shape_dimmed_rather_than_a_different_picture() {
        let size = 32;
        let listening = tray_glyph(size, INK, Glyph::Listening);
        let paused = tray_glyph(size, INK, Glyph::Paused);

        let shape = |pixels: &[u32]| -> Vec<bool> {
            pixels.iter().map(|pixel| (pixel >> 24) > 0).collect()
        };
        assert_eq!(
            shape(&listening),
            shape(&paused),
            "the two states cover different pixels, so they are two icons",
        );

        let brightness = |pixels: &[u32]| -> u32 { pixels.iter().map(|pixel| pixel >> 24).sum() };
        assert!(
            brightness(&paused) < brightness(&listening),
            "paused is not visibly quieter than listening",
        );
    }

    /// Premultiplied, and provably so: a half-covered pixel carries about half
    /// the colour. An adapter handed straight alpha would show a bright fringe
    /// around the ring, which is the classic symptom and is hard to spot in a
    /// 16 pixel icon.
    #[test]
    fn partial_coverage_carries_partial_colour() {
        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let size = 32;
        let pixels = tray_glyph(size, white, Glyph::Listening);
        let partial = pixels
            .iter()
            .find(|pixel| {
                let alpha = *pixel >> 24;
                alpha > 60 && alpha < 200
            })
            .copied()
            .expect("an anti-aliased edge pixel");

        let alpha = partial >> 24;
        let red = (partial >> 16) & 0xff;
        assert_eq!(
            red, alpha,
            "a white pixel's red channel must equal its alpha once premultiplied",
        );
    }

    #[test]
    fn a_fully_transparent_pixel_carries_no_colour_at_all() {
        // Otherwise a compositor that ignores alpha paints the whole square.
        let size = 32;
        let pixels = tray_glyph(size, INK, Glyph::Listening);
        assert_eq!(pixels[0], 0);
    }
}
