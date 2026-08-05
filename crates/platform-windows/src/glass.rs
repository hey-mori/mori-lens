//! The card, painted with alpha that means something.
//!
//! [ADR-0011](../../../docs/decisions/ADR-0011-themed-card-glass-later.md)
//! split the card's appearance in two and shipped the half that needed no new
//! renderer. This is the other half.
//!
//! # What was wrong with the GDI card
//!
//! `SetLayeredWindowAttributes(alpha)` applies one opacity to the **whole
//! window**, so a translucent card is a card whose text is translucent too.
//! That is why every contrast figure in `lens_core::theme` composites the ink
//! as well as the surface: the ink really was being faded, and the model was
//! telling the truth about a weakness rather than describing a design.
//!
//! It also means the card can never be more than a rectangle. GDI has no
//! alpha, so a rounded corner is a jagged one, and a shadow is not available at
//! all.
//!
//! # What this does instead
//!
//! Direct2D draws into a 32-bit premultiplied bitmap, and `UpdateLayeredWindow`
//! hands that bitmap to the compositor with its alpha channel intact. Every
//! pixel carries its own opacity, so:
//!
//! - the **surface** is translucent and the **text on it is not**, which is
//!   both more readable and more like the material it is imitating
//! - corners are round and antialiased rather than stepped
//! - there is a soft shadow, which is most of what makes a floating panel read
//!   as floating rather than as pasted on
//!
//! Text is DirectWrite rather than `DrawTextW`. That was the quieter half of
//! ADR-0011's argument: this is a reading tool, and DirectWrite lays out a
//! paragraph mixing Hangul and Latin properly rather than by accident.
//!
//! # What it deliberately is not
//!
//! **Not blurred.** Real acrylic blurs what is behind the window, and the only
//! ways to get that are a DWM backdrop, which requires giving up the layered
//! window this needs, or sampling the screen, which this product does not do
//! outside the OCR path. So "glass" here means translucent, rounded, and
//! shadowed. The preset names must not promise the material before it exists,
//! which ADR-0011 also says.
//!
//! # Failure
//!
//! Every entry point returns an `Option`. A machine where Direct2D cannot be
//! created keeps the GDI painter, because a plain card is a product and no card
//! is not.

use lens_core::card::{Card, CardBody};
use lens_core::geometry::ScreenRect;
use lens_core::theme::{Rgb, Theme};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextLayout, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

/// How far the shadow reaches beyond the card, in pixels at 100% scaling.
///
/// The window is this much larger than the card on every side, and those extra
/// pixels are almost transparent. It is the single change that makes a panel
/// read as floating above the page rather than pasted onto it.
const SHADOW_REACH: f32 = 14.0;

/// How dark the shadow is directly under the card's edge.
///
/// Low, and deliberately so. A reading tool that drops a heavy shadow over the
/// sentence next to the one being explained has made the page harder to read in
/// order to look better.
const SHADOW_STRENGTH: f32 = 0.30;

/// How far down the shadow is offset. Light from above, as every other panel on
/// this desktop assumes.
const SHADOW_DROP: f32 = 3.0;

/// Corner radius in pixels at 100% scaling.
const CORNER: f32 = 10.0;

/// Rings drawn to fake a gaussian falloff.
///
/// Direct2D can blur properly, through an effect graph on a device context.
/// That needs a Direct3D device, which needs a DXGI swap chain or a bitmap
/// surface, which is a great deal of machinery for a shadow on a card that is
/// three hundred pixels wide. Concentric rounded rectangles with a quadratic
/// falloff are indistinguishable at this size.
const SHADOW_RINGS: i32 = 14;

/// The typeface. Segoe UI is the system's own, and DirectWrite falls back to an
/// installed Hangul face for runs it cannot cover, which is the behaviour that
/// makes a mixed Korean and Latin paragraph lay out correctly.
const FONT: PCWSTR = windows::core::w!("Segoe UI");
const LOCALE: PCWSTR = windows::core::w!("");

/// The narrowest a card may be, in pixels at 100% scaling.
///
/// A card fits its contents, and without a floor a two-character answer
/// produces a card the size of a tooltip, with the footer wrapping onto three
/// lines beneath it.
const MIN_CARD_WIDTH: i32 = 220;

/// Gap under the original, before the translation.
const GAP_AFTER_ORIGINAL: f32 = 7.0;
/// Gap under the translation, before the footer.
const GAP_BEFORE_FOOTER: f32 = 9.0;

/// How much fully opaque ink the last presented frame carried.
///
/// A layered window's content lives in the compositor once
/// `UpdateLayeredWindow` has it, so the window's own device context reads back
/// as `CLR_INVALID` and the two live checks that sampled it were measuring
/// nothing. Reading the screen instead would work and is refused: those checks
/// deliberately sample the product's own surface and never the user's desktop.
///
/// So the renderer reports what it handed over. A frame with no opaque pixels
/// is a blank card, which is the flicker a user reported and the reason those
/// checks exist.
///
/// One pass over the bitmap, some tens of microseconds against a translation
/// that takes a second and a half.
static LAST_FRAME_INK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Opaque pixels in the most recently presented frame.
pub fn last_frame_ink() -> usize {
    LAST_FRAME_INK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Forget the last frame, so a check can prove the next one arrived rather
/// than reading one left over from before it started.
pub fn forget_last_frame() {
    LAST_FRAME_INK.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// The Direct2D and DirectWrite factories, created once.
pub struct Glass {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
}

/// A card laid out but not yet drawn.
///
/// Separated because the window has to be sized before it can be painted, and
/// measuring twice with two different layout engines is how a card ends up with
/// its last line clipped.
pub struct Laid {
    runs: Vec<Run>,
    /// Card size, without the shadow margin.
    pub width: i32,
    pub height: i32,
    scale: f32,
}

struct Run {
    layout: IDWriteTextLayout,
    y: f32,
    colour: Rgb,
    /// Text is opaque even when the surface is not. That is the whole point.
    opaque: bool,
}

impl Glass {
    pub fn new() -> Option<Self> {
        // SAFETY: both are documented factory constructors with no arguments
        // this code owns.
        unsafe {
            let d2d =
                D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let dwrite = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            Some(Self { d2d, dwrite })
        }
    }

    /// Lay the card out at a given width, and say how tall it needs to be.
    ///
    /// `scale` is the monitor's DPI over 96, applied to every size here so a
    /// card on a 150% display is the same physical size as one on a 100%
    /// display beside it.
    pub fn lay_out(&self, card: &Card, theme: &Theme, max_width: i32, scale: f32) -> Option<Laid> {
        let padding = theme.padding as f32 * scale;
        let widest = (max_width as f32 - padding * 2.0).max(1.0);

        // Every run, laid out at the widest the card is allowed to be. What
        // comes back is not how wide it *is*: `DWRITE_TEXT_METRICS::width` is
        // the width actually used, which for a short answer is a fraction of
        // what it was offered.
        let mut runs = Vec::new();
        let mut push = |text: &str, points: i32, semibold: bool, colour: Rgb| {
            if text.trim().is_empty() {
                return;
            }
            let size_px = points as f32 * scale * 96.0 / 72.0;
            // SAFETY: documented layout creation; the interface is owned by the
            // `Run` it goes into.
            let made = unsafe { self.layout(text, size_px, semibold, widest) };
            if let Some((layout, _)) = made {
                runs.push(Run {
                    layout,
                    y: 0.0,
                    colour,
                    opaque: true,
                });
            }
        };

        // The original first and quieter. Above the answer because the reader's
        // eye is already there, and present at all because it is the only way
        // to notice an answer that is not a translation.
        //
        // Quieter by colour, not by opacity. Fading it was the old renderer's
        // only way to say "secondary" and it cost legibility to say it.
        push(&card.original, theme.small_pt, false, theme.original);

        let waiting = Card::WAITING_TEXT.to_owned();
        let (body, colour, weight, points) = match &card.body {
            CardBody::Translation(text) => (text, theme.body, true, theme.body_pt),
            CardBody::Problem(text) => (text, theme.problem, true, theme.body_pt),
            // Quiet, and at the footer's colour and size: it is a status, not
            // an answer, and it must not read as one for the second it is up.
            CardBody::Waiting => (&waiting, theme.footer, false, theme.small_pt),
        };
        push(body, points, weight, colour);
        push(&card.footer, theme.small_pt, false, theme.footer);

        // Fit the card to what is in it. A fixed 420px box holding one short
        // line is mostly empty, which reads as a window that failed to load
        // rather than as an answer.
        //
        // Never below the floor: a card that shrinks to the width of `네` is a
        // tooltip, and it also makes the footer wrap.
        let natural = runs
            .iter()
            // SAFETY: metrics on a layout this function owns.
            .filter_map(|run| unsafe { text_width(&run.layout) })
            .fold(0.0f32, f32::max);
        let floor = MIN_CARD_WIDTH as f32 * scale - padding * 2.0;
        let inner = natural.max(floor).min(widest).ceil();

        // Re-flow at the width actually chosen, and only now find out how tall
        // each run is. Measuring height at one width and drawing at another is
        // how a card ends up with its last line clipped.
        let mut y = padding;
        for (index, run) in runs.iter_mut().enumerate() {
            // SAFETY: documented calls on a layout this function owns.
            let height = unsafe {
                if run.layout.SetMaxWidth(inner).is_err() {
                    return None;
                }
                let mut metrics = DWRITE_TEXT_METRICS::default();
                run.layout.GetMetrics(&mut metrics).ok()?;
                metrics.height
            };
            run.y = y;
            y += height;
            // The gaps belong between the runs, not after the last one.
            if index + 1 < 3 {
                y += if index == 0 {
                    GAP_AFTER_ORIGINAL * scale
                } else {
                    GAP_BEFORE_FOOTER * scale
                };
            }
        }

        Some(Laid {
            runs,
            width: (inner + padding * 2.0).ceil() as i32,
            height: (y + padding).ceil() as i32,
            scale,
        })
    }

    /// # Safety
    /// Documented DirectWrite calls; the layout returned is owned by the caller.
    unsafe fn layout(
        &self,
        text: &str,
        size_px: f32,
        semibold: bool,
        width: f32,
    ) -> Option<(IDWriteTextLayout, f32)> {
        // SAFETY: as above.
        unsafe {
            let format = self
                .dwrite
                .CreateTextFormat(
                    FONT,
                    None,
                    if semibold {
                        DWRITE_FONT_WEIGHT_SEMI_BOLD
                    } else {
                        DWRITE_FONT_WEIGHT_NORMAL
                    },
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size_px.max(1.0),
                    LOCALE,
                )
                .ok()?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP).ok()?;

            let wide: Vec<u16> = text.encode_utf16().collect();
            let layout = self
                .dwrite
                .CreateTextLayout(&wide, &format, width, f32::MAX)
                .ok()?;

            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).ok()?;
            Some((layout, metrics.height))
        }
    }

    /// Paint the laid-out card and hand it to the compositor.
    ///
    /// `card_rect` is where the card itself goes. The window is grown by the
    /// shadow margin around it, so the card lands exactly where placement asked
    /// and the shadow spills outside.
    ///
    /// # Safety
    /// `window` must be a live `WS_EX_LAYERED` window owned by this thread.
    pub unsafe fn present(
        &self,
        window: HWND,
        laid: &Laid,
        theme: &Theme,
        card_rect: ScreenRect,
    ) -> bool {
        let margin = (SHADOW_REACH * laid.scale).ceil() as i32;
        let width = laid.width + margin * 2;
        let height = laid.height + margin * 2;

        // SAFETY: every GDI and Direct2D object created here is released on
        // every path out.
        unsafe {
            let Some((dc, bitmap, previous, bits)) = surface(width, height) else {
                return false;
            };

            let drawn = self.draw(dc, laid, theme, width, height, margin as f32);
            if drawn {
                let pixels =
                    std::slice::from_raw_parts(bits.cast::<u32>(), (width * height) as usize);
                let ink = pixels.iter().filter(|pixel| (*pixel >> 24) == 255).count();
                LAST_FRAME_INK.store(ink, std::sync::atomic::Ordering::Relaxed);
            }

            let mut sent = false;
            if drawn {
                let position = POINT {
                    x: card_rect.x - margin,
                    y: card_rect.y - margin,
                };
                let size = SIZE {
                    cx: width,
                    cy: height,
                };
                let origin = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    // 255: the per-pixel alpha in the bitmap is the whole
                    // story. A value below this would fade the text again,
                    // which is the thing this renderer exists to stop.
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                sent = UpdateLayeredWindow(
                    window,
                    None,
                    Some(&position),
                    Some(&size),
                    Some(dc),
                    Some(&origin),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                )
                .is_ok();
            }

            SelectObject(dc, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            sent
        }
    }

    /// # Safety
    /// `dc` must hold a 32-bit top-down DIB of exactly `width` by `height`.
    unsafe fn draw(
        &self,
        dc: HDC,
        laid: &Laid,
        theme: &Theme,
        width: i32,
        height: i32,
        margin: f32,
    ) -> bool {
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            // 96 on both axes, because every size in this file is already in
            // physical pixels. Letting Direct2D scale as well would apply the
            // display's scaling twice, which is the mistake the card's font
            // size made once already.
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };

        // SAFETY: documented Direct2D calls against a DC the caller owns.
        unsafe {
            let Ok(target) = self.d2d.CreateDCRenderTarget(&properties) else {
                return false;
            };
            let bounds = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            if target.BindDC(dc, &bounds).is_err() {
                return false;
            }

            target.BeginDraw();
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            // Fully transparent, so everything outside the card's rounded
            // outline disappears rather than becoming a black box.
            target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            let card = D2D_RECT_F {
                left: margin,
                top: margin,
                right: width as f32 - margin,
                bottom: height as f32 - margin,
            };
            let radius = CORNER * laid.scale;

            // The shadow, outermost ring first so the darker inner ones land on
            // top.
            let drop = SHADOW_DROP * laid.scale;
            for ring in (1..=SHADOW_RINGS).rev() {
                let spread = margin * ring as f32 / SHADOW_RINGS as f32;
                // Quadratic falloff. Linear reads as a grey halo with an edge.
                let fade = 1.0 - ring as f32 / SHADOW_RINGS as f32;
                let alpha = SHADOW_STRENGTH * fade * fade;
                let Ok(brush) = target.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: alpha,
                    },
                    None,
                ) else {
                    continue;
                };
                // Stroked, not filled. A filled rounded rectangle covers the
                // card as well as the space around it, so fourteen of them
                // stack underneath a translucent surface and darken it: the
                // card measured 250 alpha where the theme asked for 236. A
                // stroke paints only its own ring, so the surface is drawn at
                // the opacity the theme actually chose.
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: card.left - spread,
                            top: card.top - spread + drop,
                            right: card.right + spread,
                            bottom: card.bottom + spread + drop,
                        },
                        radiusX: radius + spread,
                        radiusY: radius + spread,
                    },
                    &brush,
                    // Wide enough that consecutive rings overlap rather than
                    // leaving gaps between them, which reads as banding.
                    (margin / SHADOW_RINGS as f32) * 2.2,
                    None,
                );
            }

            // The surface. Translucent by the theme's own figure, and the only
            // thing here that is.
            let surface_alpha = f32::from(theme.surface_alpha) / 255.0;
            if let Ok(brush) =
                target.CreateSolidColorBrush(&colour(theme.surface, surface_alpha), None)
            {
                target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: card,
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &brush,
                );
            }

            // A hairline, inset by half a pixel so the stroke lands on the
            // outline instead of straddling it and coming out two pixels of
            // grey.
            if let Ok(brush) = target.CreateSolidColorBrush(&colour(theme.border, 0.9), None) {
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: card.left + 0.5,
                            top: card.top + 0.5,
                            right: card.right - 0.5,
                            bottom: card.bottom - 0.5,
                        },
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &brush,
                    1.0,
                    None,
                );
            }

            for run in &laid.runs {
                let alpha = if run.opaque { 1.0 } else { surface_alpha };
                let Ok(brush) = target.CreateSolidColorBrush(&colour(run.colour, alpha), None)
                else {
                    continue;
                };
                target.DrawTextLayout(
                    windows_numerics::Vector2 {
                        X: margin + theme.padding as f32 * laid.scale,
                        Y: margin + run.y,
                    },
                    &run.layout,
                    &brush,
                    windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }

            target.EndDraw(None, None).is_ok()
        }
    }

    /// Render a card and return its pixels instead of presenting it.
    ///
    /// For verification only. Once `UpdateLayeredWindow` has the bitmap, the
    /// window's own device context no longer holds the content: it lives in
    /// the compositor, and `GetPixel` on the window answers `CLR_INVALID`.
    /// That is correct behaviour and it removes the only way the live checks
    /// had of seeing what was drawn.
    ///
    /// Reading the screen instead would work and is refused: the overlay tests
    /// deliberately sample the product's own surface and never the user's
    /// desktop, and a diagnostic is not a reason to give that up.
    ///
    /// Returns width, height, and premultiplied `0xAARRGGBB` pixels, top row
    /// first.
    ///
    /// # Safety
    /// Creates and releases its own GDI objects.
    #[cfg(test)]
    pub unsafe fn pixels(&self, laid: &Laid, theme: &Theme) -> Option<(i32, i32, Vec<u32>)> {
        let margin = (SHADOW_REACH * laid.scale).ceil() as i32;
        let width = laid.width + margin * 2;
        let height = laid.height + margin * 2;

        // SAFETY: everything created here is released before returning.
        unsafe {
            let (dc, bitmap, previous, bits) = surface(width, height)?;
            let drawn = self.draw(dc, laid, theme, width, height, margin as f32);
            let copied = drawn.then(|| {
                std::slice::from_raw_parts(bits.cast::<u32>(), (width * height) as usize).to_vec()
            });
            SelectObject(dc, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            copied.map(|pixels| (width, height, pixels))
        }
    }
}

/// How wide a laid-out run actually is, rather than how wide it was offered.
///
/// # Safety
/// `layout` must be live.
unsafe fn text_width(layout: &IDWriteTextLayout) -> Option<f32> {
    // SAFETY: a documented read-only query.
    unsafe {
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics).ok()?;
        Some(metrics.width)
    }
}

/// A colour Direct2D understands, with straight alpha. Direct2D premultiplies
/// on the way into a premultiplied target, so this must not be premultiplied
/// here as well.
fn colour(rgb: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(rgb.r) / 255.0,
        g: f32::from(rgb.g) / 255.0,
        b: f32::from(rgb.b) / 255.0,
        a: alpha,
    }
}

/// A memory DC holding a 32-bit top-down DIB, ready to be drawn into and handed
/// to `UpdateLayeredWindow`.
///
/// # Safety
/// The caller must restore `previous`, delete the bitmap, and delete the DC.
unsafe fn surface(
    width: i32,
    height: i32,
) -> Option<(HDC, HBITMAP, HGDIOBJ, *mut core::ffi::c_void)> {
    let header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width.max(1),
            // Negative for top-down, matching how Direct2D addresses it.
            biHeight: -height.max(1),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // SAFETY: documented GDI calls; the caller owns what comes back.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &header, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(dc);
            return None;
        };
        let previous = SelectObject(dc, bitmap.into());
        Some((dc, bitmap, previous, bits))
    }
}
