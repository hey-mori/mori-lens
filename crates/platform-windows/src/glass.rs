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
//! alpha, so a rounded corner is a jagged one.
//!
//! # What this does instead
//!
//! Direct2D draws into a 32-bit premultiplied bitmap, and `UpdateLayeredWindow`
//! hands that bitmap to the compositor with its alpha channel intact. Every
//! pixel carries its own opacity, so:
//!
//! - the surface's opacity is the theme's to choose, and the **text on it is
//!   never faded with it**
//! - corners are round and antialiased rather than stepped
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
//! outside the OCR path. The preset names must not promise the material before
//! it exists, which ADR-0011 also says.
//!
//! **Not shadowed, any more.** There was a soft drop shadow here, on the usual
//! argument that it is what makes a panel read as floating rather than as
//! pasted on. It was rebuilt three times: as filled rings, as a distance field
//! with a smoothstep falloff, then with a power curve, tuned down at every
//! step. The same user called it a weird box every time, and the last screenshot
//! came with the question of why any of it looked like that.
//!
//! Three rounds of tuning that never converged is the finding. A shadow spills
//! onto the page **the reader is reading**, and it is the one element here whose
//! job is decoration rather than legibility, so it is the one to remove rather
//! than to keep tuning. What is left is a surface, one hairline, and a rounded
//! corner, which is what the system's own tooltips are and is not mistakable for
//! a box inside a box.
//!
//! # The original is never shortened
//!
//! It was capped at two lines once, to make the card smaller, and it was wrong
//! within a day. A user screenshotted a card whose visible original read
//! "…which module owns these migrations in areabase to confirm they're stripped
//! during the build proce…" beside a translation beginning "이것은 중요한
//! 발견입니다: 공유 데이터베이스 토폴로지의 격리가…". Both correct, and **from
//! different halves of the same sentence**: the answer covered text the ellipsis
//! had hidden.
//!
//! The original is on the card so a reader can check the answer belongs to what
//! they pointed at (`docs/privacy-model.md` §1). A truncated one does not make
//! that harder, it makes it impossible, and it reads as a mismatched pair. Size
//! has to come from somewhere that is not the evidence.
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
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_HIT_TEST_METRICS, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

/// Blank pixels kept around the card, in pixels at 100% scaling.
///
/// Two, and only because a rounded corner is antialiased: drawn hard against
/// the bitmap's edge its outermost pixels are clipped rather than blended, and
/// a clipped antialiased curve is a stepped one. This is not a shadow margin.
/// There is no shadow.
const EDGE_MARGIN: f32 = 2.0;

/// Corner radius in pixels at 100% scaling.
const CORNER: f32 = 10.0;

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

/// Line spacing, as a multiple of the font size.
///
/// 1.38 rather than the font's own, which is close to 1.2 for Segoe UI and
/// tight for Hangul: Korean glyphs fill more of their box than Latin ones, so
/// lines that look airy in English look packed in Korean. Every card here has
/// both, one above the other, and the difference shows.
const LINE_HEIGHT: f32 = 1.38;

/// Gap under the original, before the translation.
///
/// **The largest gap on the card**, and it was the smallest until 2026-08-11.
/// This is the one boundary a reader actually crosses: above it is what they
/// pointed at, below it is what this product made of it. The footer is a line
/// most readers never reach.
///
/// A rule was drawn here first and taken out again. This file already records
/// what happens when a card says one thing three ways: the answer was semibold
/// and larger and darker, and a reader called it too heavy and too large in the
/// same sentence. Space separates as well as a line does and adds no ink.
const GAP_AFTER_ORIGINAL: f32 = 11.0;
/// Gap under the translation, before the footer.
const GAP_BEFORE_FOOTER: f32 = 7.0;
/// Gap between two rows of an analysis.
///
/// Much smaller than either of the others, because those separate *kinds* of
/// thing and this separates items of one kind. A list whose rows are as far
/// apart as its sections is a list that has stopped looking like one.
const GAP_BETWEEN_ROWS: f32 = 3.0;
/// Space between an analysis label and the value beside it.
const LABEL_GUTTER: f32 = 7.0;

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
    /// Per-link colour underline metrics, collected during build and consumed
    /// by draw. Each entry is a single line of a link's target range in the
    /// translation text (or source range in the original). Multi-line links
    /// produce multiple entries. Coordinates are relative to the run they
    /// belong to; the draw function adds the run's offset.
    link_underlines: Vec<LinkUnderline>,
    /// Card size, without the shadow margin.
    pub width: i32,
    pub height: i32,
    scale: f32,
}

/// A single visual fragment of a link underline.
///
/// One `TextLink` in the analysis may span multiple wrapped lines, producing
/// multiple `LinkUnderline` entries. Each stores the pixel position and width
/// of the text range within its containing run, plus the palette index that
/// picks the colour from `Theme::link_colours`.
struct LinkUnderline {
    /// Left offset within the run, in pixels.
    left: f32,
    /// Top offset within the run, in pixels.
    top: f32,
    /// Width of the text range, in pixels.
    width: f32,
    /// Height of the text range, in pixels. The underline is drawn at the
    /// bottom of this box.
    height: f32,
    /// Index into `Theme::link_colours`.
    colour_index: usize,
    /// The run's x offset within the card.
    run_x: f32,
    /// The run's y offset within the card.
    run_y: f32,
}

/// A pending link hit-test, collected before run positions are final.
///
/// Stored during the Analysis arm of the layout, then resolved after the
/// positioning loop when `run.x` and `run.y` are known.
struct PendingLink {
    /// Byte range in the text: (start, end).
    range: (usize, usize),
    /// Index into `Theme::link_colours`.
    colour_index: usize,
    /// The run's index in the `runs` vector.
    run_index: usize,
    /// The text that contains the range. Needed for byte-to-UTF-16 conversion.
    text: String,
}

struct Run {
    layout: IDWriteTextLayout,
    /// Offset from the card's left padding.
    ///
    /// Zero for everything that is a paragraph. Non-zero for the value half of
    /// an analysis row, which sits beside its label rather than under it.
    x: f32,
    y: f32,
    /// Which line of the card this shares a top edge with.
    ///
    /// Runs are a flat list and rows are what the card is actually made of: a
    /// label and its value are two runs on one row. Before this the two were
    /// the same thing, and the gap under a run was chosen by its **index**,
    /// which is why a card with no original gave the original's wider gap to a
    /// boundary that was not there.
    row: usize,
    /// Space under this run's row. The widest wins where a row has two.
    gap_after: f32,
    colour: Rgb,
    /// Text is opaque even when the surface is not. That is the whole point.
    opaque: bool,
}

impl Laid {
    /// Where every run landed, as `(x, y)` in card coordinates.
    ///
    /// For the live checks and nothing else. Two runs sharing a `y` are a
    /// label and the value beside it, which is the whole claim the analysis
    /// layout makes and the one thing a screenshot would show and a test
    /// otherwise could not.
    #[cfg(test)]
    pub(crate) fn placements(&self) -> Vec<(f32, f32)> {
        self.runs.iter().map(|run| (run.x, run.y)).collect()
    }

    /// The display scaling this was laid out for, so an animation moves the
    /// same physical distance on every monitor.
    pub fn scale(&self) -> f32 {
        self.scale
    }
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
    /// Lay a card out at whatever width its contents need, up to a limit.
    ///
    /// The product always uses [`Self::lay_out_between`], because a card on
    /// screen has a width it must keep. This is the unconstrained case, which
    /// is what the live checks measure.
    #[cfg(test)]
    pub fn lay_out(&self, card: &Card, theme: &Theme, max_width: i32, scale: f32) -> Option<Laid> {
        self.lay_out_between(card, theme, (0, max_width), scale)
    }

    /// Lay a card out somewhere between two widths.
    ///
    /// `narrowest` and `widest` are equal when the card is already on screen.
    /// A streamed answer arrives a few words at a time, and a card that fits
    /// itself to each of those is a card that changes width several times a
    /// second. Placement derives its **x** from the width, so the card also
    /// slides sideways while the reader is trying to read it. Pinning the
    /// width for the life of one hover is what stops that.
    pub fn lay_out_between(
        &self,
        card: &Card,
        theme: &Theme,
        (narrowest, max_width): (i32, i32),
        scale: f32,
    ) -> Option<Laid> {
        let padding = theme.padding as f32 * scale;
        let widest = (max_width as f32 - padding * 2.0).max(1.0);
        let floor_px = (narrowest as f32 - padding * 2.0).max(1.0);

        // Every run, laid out at the widest the card is allowed to be. What
        // comes back is not how wide it *is*: `DWRITE_TEXT_METRICS::width` is
        // the width actually used, which for a short answer is a fraction of
        // what it was offered.
        let mut runs: Vec<Run> = Vec::new();
        let mut link_underlines: Vec<LinkUnderline> = Vec::new();
        let mut pending_links: Vec<PendingLink> = Vec::new();
        let make = |runs: &mut Vec<Run>,
                    text: &str,
                    points: i32,
                    semibold: bool,
                    colour: Rgb|
         -> Option<usize> {
            if text.trim().is_empty() {
                return None;
            }
            let size_px = points as f32 * scale * 96.0 / 72.0;
            // SAFETY: documented layout creation; the interface is owned by the
            // `Run` it goes into.
            let (layout, _) = unsafe { self.layout(text, size_px, semibold, widest) }?;
            runs.push(Run {
                layout,
                x: 0.0,
                y: 0.0,
                row: 0,
                gap_after: 0.0,
                colour,
                opaque: true,
            });
            Some(runs.len() - 1)
        };

        // Rows are counted as they are made, so an empty original takes no row
        // and every gap below it belongs to the boundary it is actually under.
        let mut row = 0usize;
        // Kept so the pair can be marked once both halves exist. The word is in
        // the original and its counterpart is in the translation below it, and
        // neither is worth marking without the other.
        let mut original_run: Option<usize> = None;

        // The original first and quieter. Above the answer because the reader's
        // eye is already there, and present at all because it is the only way
        // to notice an answer that is not a translation.
        //
        // Quieter by colour, not by opacity. Fading it was the old renderer's
        // only way to say "secondary" and it cost legibility to say it.
        //
        // Never capped. See ORIGINAL_LINES: an elided original put a
        // translation of the end of a sentence beside the start of it.
        if let Some(index) = make(
            &mut runs,
            &card.original,
            theme.small_pt,
            false,
            theme.original,
        ) {
            runs[index].row = row;
            runs[index].gap_after = GAP_AFTER_ORIGINAL * scale;
            original_run = Some(index);
            row += 1;
        }

        match &card.body {
            // **The one body that is a list.** A label and its value are two
            // runs on one row: the labels are made first so the column they
            // share can be measured, and only then are the values placed
            // beside it. Every value therefore starts at the same x, and a
            // reader finds that x once instead of five times.
            CardBody::Analysis {
                translation,
                word_at,
                counterpart_at,
                links,
                rows: pairs,
            } => {
                // **The answer stays on the card.** `Ctrl+Alt+D` replaced the
                // translation with an account of it, so a reader who pressed it
                // to understand an answer lost the answer. At the answer's size
                // and colour, because on this card it is still the answer and
                // the rows below it are notes about it.
                if let Some(text) = translation {
                    if let Some(index) = make(&mut runs, text, theme.body_pt, false, theme.body) {
                        runs[index].row = row;
                        runs[index].gap_after = GAP_AFTER_ORIGINAL * scale;
                        // SAFETY: marking a range of a layout this function
                        // owns and has not yet drawn.
                        unsafe { underline(&runs[index].layout, text, *counterpart_at) };
                        // #148: record each link's target span for coloured
                        // underline drawing in draw(). The coloured line
                        // replaces the monochrome SetUnderline that was here
                        // before; the thickness (2px at 100% scale) is the
                        // non-colour cue.
                        for (link_idx, link) in links.iter().enumerate() {
                            pending_links.push(PendingLink {
                                range: link.target_range,
                                colour_index: link_idx % 6,
                                run_index: index,
                                text: text.to_string(),
                            });
                        }
                        row += 1;
                    }
                }
                // The other half of the pair, on the original above.
                if let Some(index) = original_run {
                    // SAFETY: as above.
                    unsafe { underline(&runs[index].layout, &card.original, *word_at) };
                    // #148: record each link's source span with the same colour
                    // as its target, so a reader can trace the pair.
                    for (link_idx, link) in links.iter().enumerate() {
                        pending_links.push(PendingLink {
                            range: link.source_range,
                            colour_index: link_idx % 6,
                            run_index: index,
                            text: card.original.clone(),
                        });
                    }
                }
                let first_row = row;
                let mut labelled: Vec<(usize, &String)> = Vec::new();
                for (label, value) in pairs {
                    if let Some(index) = make(&mut runs, label, theme.small_pt, false, theme.footer)
                    {
                        runs[index].row = row;
                        labelled.push((index, value));
                        row += 1;
                    }
                }
                // The widest label decides the column. Measured at `widest`,
                // where a label of two or three glyphs never wraps, so this is
                // its real width rather than a wrapped one.
                let column = labelled
                    .iter()
                    // SAFETY: metrics on a layout this function owns.
                    .filter_map(|(index, _)| unsafe { text_width(&runs[*index].layout) })
                    .fold(0.0f32, f32::max)
                    + LABEL_GUTTER * scale;
                for (offset, (_, value)) in labelled.iter().enumerate() {
                    let at = first_row + offset;
                    if let Some(index) = make(&mut runs, value, theme.small_pt, false, theme.body) {
                        runs[index].row = at;
                        runs[index].x = column;
                    }
                    // Set on the row rather than on either half, so a row whose
                    // value failed to lay out still ends where it should.
                    let gap = if at + 1 == row {
                        GAP_BEFORE_FOOTER
                    } else {
                        GAP_BETWEEN_ROWS
                    };
                    for run in runs.iter_mut().filter(|run| run.row == at) {
                        run.gap_after = gap * scale;
                    }
                }
            }
            // **Not bold.** The answer was semibold at a size above the
            // original, which is three signals (weight, size, colour) saying
            // one thing, and a reader called it too heavy and too large in the
            // same sentence. Two carry it: the answer is the darkest ink on the
            // card and a point larger than the original. Bold Hangul is also
            // heavier than bold Latin at the same weight, because the glyphs
            // fill more of their box, so the emphasis that looked measured in
            // English did not stay measured here.
            body => {
                let (text, colour, points) = match body {
                    CardBody::Translation(text) => (text.as_str(), theme.body, theme.body_pt),
                    CardBody::Problem(text) => (text.as_str(), theme.problem, theme.body_pt),
                    // Quiet, and at the footer's colour and size: it is a
                    // status, not an answer, and it must not read as one for
                    // the second it is up.
                    CardBody::Waiting(status) => (status.as_str(), theme.footer, theme.small_pt),
                    // Unreachable: the arm above takes it.
                    CardBody::Analysis { .. } => ("", theme.body, theme.small_pt),
                };
                if let Some(index) = make(&mut runs, text, points, false, colour) {
                    runs[index].row = row;
                    runs[index].gap_after = GAP_BEFORE_FOOTER * scale;
                    row += 1;
                }
            }
        }

        if let Some(index) = make(
            &mut runs,
            &card.footer,
            theme.footer_pt,
            false,
            theme.footer,
        ) {
            runs[index].row = row;
        }
        let rows_total = runs.iter().map(|run| run.row + 1).max().unwrap_or(0);

        // Fit the card to what is in it. A fixed 420px box holding one short
        // line is mostly empty, which reads as a window that failed to load
        // rather than as an answer.
        //
        // Per row, and including where the row starts: a value indented past a
        // label needs the column as well as itself, and measuring runs alone
        // would make every analysis card exactly one label-column too narrow.
        //
        // Never below the floor: a card that shrinks to the width of a single
        // glyph is a tooltip, and it also makes the footer wrap.
        let natural = (0..rows_total)
            .map(|at| {
                runs.iter()
                    .filter(|run| run.row == at)
                    // SAFETY: metrics on a layout this function owns.
                    .filter_map(|run| unsafe { text_width(&run.layout) }.map(|used| run.x + used))
                    .fold(0.0f32, f32::max)
            })
            .fold(0.0f32, f32::max);
        let floor = (MIN_CARD_WIDTH as f32 * scale - padding * 2.0).max(floor_px);
        let inner = natural.max(floor).min(widest).ceil();

        // Re-flow at the width actually chosen, and only now find out how tall
        // each run is. Measuring height at one width and drawing at another is
        // how a card ends up with its last line clipped.
        //
        // A row is as tall as its tallest run, and the gaps go between rows
        // rather than after the last one.
        let mut y = padding;
        for at in 0..rows_total {
            let mut tallest = 0.0f32;
            let mut gap = 0.0f32;
            for run in runs.iter_mut().filter(|run| run.row == at) {
                // SAFETY: documented calls on a layout this function owns.
                let height = unsafe {
                    if run.layout.SetMaxWidth((inner - run.x).max(1.0)).is_err() {
                        return None;
                    }
                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    run.layout.GetMetrics(&mut metrics).ok()?;
                    metrics.height
                };
                run.y = y;
                tallest = tallest.max(height);
                gap = gap.max(run.gap_after);
            }
            y += tallest;
            if at + 1 < rows_total {
                y += gap;
            }
        }

        // #148: resolve pending link hit-tests now that run positions are final.
        // Each pending link becomes one or more LinkUnderlines (one per wrapped
        // line of the text range). The hit-test metrics are relative to the
        // layout origin; draw() combines them with the run's absolute offset.
        // A failed hit-test drops the coloured underline for that link; the
        // monochrome underline from SetUnderline still draws.
        for pending in &pending_links {
            let run = &runs[pending.run_index];
            let (from, to) = pending.range;
            let Some(head) = pending.text.get(..from) else {
                continue;
            };
            let Some(span) = pending.text.get(from..to) else {
                continue;
            };
            let position = head.encode_utf16().count() as u32;
            let length = span.encode_utf16().count() as u32;
            if length == 0 {
                continue;
            }
            let mut hit_metrics = [DWRITE_HIT_TEST_METRICS::default(); 8];
            let mut actual_count: u32 = 0;
            // SAFETY: HitTestTextRange is a documented read-only query on a
            // layout this function owns. The output buffer is stack-allocated
            // and large enough for a reasonable number of wrapped lines.
            let ok = unsafe {
                run.layout.HitTestTextRange(
                    position,
                    length,
                    0.0,
                    0.0,
                    Some(&mut hit_metrics),
                    &mut actual_count,
                )
            };
            if ok.is_err() {
                continue;
            }
            for hit in &hit_metrics[..actual_count as usize] {
                link_underlines.push(LinkUnderline {
                    left: hit.left,
                    top: hit.top,
                    width: hit.width,
                    height: hit.height,
                    colour_index: pending.colour_index,
                    run_x: run.x,
                    run_y: run.y,
                });
            }
        }

        Some(Laid {
            runs,
            link_underlines,
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
            // Line spacing, set rather than left to the font's own metrics.
            // A card is read at a glance and the default leading is tight for
            // Hangul, whose glyphs fill more of their box than Latin does. The
            // baseline is the conventional 80% of the line, which is what
            // DirectWrite's own uniform mode uses.
            let line = size_px * LINE_HEIGHT;
            format
                .SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line, line * 0.8)
                .ok()?;

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
    /// Paint the laid-out card and hand it to the compositor.
    ///
    /// `fade` is the whole window's opacity, for the entrance only. It is
    /// **not** a way to make the card translucent: the surface's translucency
    /// is per pixel and lives in the bitmap, which is the entire reason this
    /// renderer exists. This multiplies everything including the text, which is
    /// what a fade is and what a steady state must never be.
    ///
    /// It is also why the entrance fades the *window* rather than the pixels.
    /// Fading the pixels would drop the bitmap's opaque-pixel count to zero for
    /// the length of the animation, and that count is what the flicker checks
    /// read: a polished entrance would have looked exactly like the bug those
    /// checks exist to catch.
    ///
    /// # Safety
    /// `window` must be a live `WS_EX_LAYERED` window owned by this thread.
    pub unsafe fn present_faded(
        &self,
        window: HWND,
        laid: &Laid,
        theme: &Theme,
        card_rect: ScreenRect,
        fade: u8,
    ) -> bool {
        let margin = (EDGE_MARGIN * laid.scale).ceil() as i32;
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
                    // 255 in the steady state: the per-pixel alpha in the
                    // bitmap is the whole story, and a lower value here fades
                    // the text with the surface, which is the thing this
                    // renderer exists to stop. Below 255 only while a card is
                    // arriving.
                    SourceConstantAlpha: fade,
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

    /// Paint a card at full opacity, which is every case but an entrance.
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
        // SAFETY: the caller's guarantee, passed straight through.
        unsafe { self.present_faded(window, laid, theme, card_rect, 255) }
    }

    /// Everything that makes a frame.
    ///
    /// **One path, used by both callers**, which is worth stating because it
    /// once was not: a compositing pass ran only where a frame was presented,
    /// so the entry point that renders a card for inspection produced a
    /// different image from the one on screen. A diagnostic that does not draw
    /// what the product draws is worse than none.
    ///
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

            // The card's only edge, inset by half a pixel so the stroke lands
            // on the outline instead of straddling it and coming out two pixels
            // of grey.
            //
            // It carries the whole boundary, so it was drawn firmly rather than
            // apologetically: 0.30 while a shadow drew a second edge outside
            // it, then 0.65 once the shadow went. 0.65 became the box on its
            // own.
            //
            // **Opaque, and softened in the colour instead.** Drawn at partial
            // alpha it took part of its colour from the page, so one line read
            // differently along its own length, which over a page of text is
            // blotchy rather than thin and is the recorded reason this product
            // stopped shipping a translucent card. `Theme::edge_colour` mixes
            // the softness in ahead of time, so the line is one colour at every
            // point and translucency costs it nothing.
            if let Ok(brush) = target.CreateSolidColorBrush(&colour(theme.edge_colour(), 1.0), None)
            {
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
                        // `run.x` is zero for everything that is a paragraph,
                        // and the label column for the value half of a row.
                        X: margin + theme.padding as f32 * laid.scale + run.x,
                        Y: margin + run.y,
                    },
                    &run.layout,
                    &brush,
                    windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }

            // #148: coloured underlines for each link. Drawn as thin filled
            // rectangles at the bottom of the text range, using the link
            // palette from the theme. The monochrome SetUnderline from build()
            // still draws underneath; the coloured line sits on top of it and
            // is what the reader actually sees.
            //
            // A non-colour cue: the coloured line is 2px thick at 100% scale
            // (clamped to 1px minimum), which is thicker than a default
            // underline. A reader who cannot distinguish the palette colours
            // still sees "thicker underline" versus "normal underline".
            let underline_thickness = (2.0 * laid.scale).max(1.0);
            for link in &laid.link_underlines {
                let rgb = theme.link_colours[link.colour_index % theme.link_colours.len()];
                let Ok(brush) = target.CreateSolidColorBrush(&colour(rgb, 1.0), None) else {
                    continue;
                };
                let x = margin + theme.padding as f32 * laid.scale + link.run_x + link.left;
                let y = margin + link.run_y + link.top + link.height - underline_thickness;
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + link.width,
                        bottom: y + underline_thickness,
                    },
                    &brush,
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
        let margin = (EDGE_MARGIN * laid.scale).ceil() as i32;
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

/// Underline one range of a laid-out line, in the text's own colour.
///
/// **Underline and not colour, and that is a design decision rather than a
/// shortcut.** The plan was four colours with four underline styles beside
/// them, because a reader who cannot see hue needs a second cue and four
/// coloured pairs need telling apart. Measurement cut the feature down to
/// **one** pair
/// (`docs/benchmarks/2026-08-11-one-word-and-its-counterpart.md`), and with one
/// pair there is nothing to tell it apart from: two marks on two lines are the
/// pair. So the second cue is the only cue, which survives a colour-blind
/// reader, high contrast mode, and this file's own rule about saying one thing
/// three ways.
///
/// Byte offsets in, UTF-16 offsets out. `lens-core` indexes strings by bytes
/// because that is what Rust strings are; DirectWrite counts UTF-16 code units.
/// A Korean counterpart is three bytes and one unit per character, so handing
/// the byte offsets straight over would underline roughly three times too much,
/// starting in the wrong place.
///
/// Does nothing when there is no range. A failure to mark is not a failure to
/// draw: the card is the same card with one fewer thing said on it.
///
/// # Safety
/// `layout` must be a layout this caller owns and has not yet drawn.
unsafe fn underline(layout: &IDWriteTextLayout, text: &str, at: Option<(usize, usize)>) {
    let Some((from, to)) = at else { return };
    let Some(head) = text.get(..from) else { return };
    let Some(span) = text.get(from..to) else {
        return;
    };
    let range = DWRITE_TEXT_RANGE {
        startPosition: head.encode_utf16().count() as u32,
        length: span.encode_utf16().count() as u32,
    };
    if range.length == 0 {
        return;
    }
    // SAFETY: a documented range attribute on a layout the caller owns. A
    // rejected range leaves the line unmarked, which is what happens anyway
    // when there is nothing to mark.
    unsafe {
        let _ = layout.SetUnderline(true, range);
    }
}
