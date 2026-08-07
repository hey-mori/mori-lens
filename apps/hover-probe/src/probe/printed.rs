//! Asking a window to draw itself, so a compatibility run need not rearrange a
//! desktop.
//!
//! # Why this lives in the probe and not in the product
//!
//! `docs/privacy-model.md` bounds what Lens captures to a band around the
//! pointer, 760 by 180 pixels, and says so in a section about how much of
//! somebody's screen passes through memory in an afternoon. A function that
//! renders a whole window belongs nowhere near that promise.
//!
//! The probe is a measurement instrument a developer runs deliberately, which
//! is a different thing from a tool that runs while somebody reads. So the
//! capability is here, and `lens-platform-windows` gains only
//! [`capture::Capture::from_bgra`], which wraps bytes rather than obtaining
//! them.
//!
//! # What it is for
//!
//! The compatibility matrix's own occlusion limit:
//!
//! > The consequence for a compatibility session: bring one application
//! > forward, sweep it, then the next.
//!
//! That friction produced a wrong reading on 2026-08-06, when a sweep of a
//! 74%-covered VS Code reported 22% acquisition against the 2.8% on record.
//!
//! It does not fix the sweep, and nothing can: `ElementFromPoint` answers with
//! the topmost window whatever the pixels say. It fixes the OCR half, which is
//! the half that reads pixels.
//!
//! # Measured before it was built
//!
//! `PrintWindow` is documented as unreliable on GPU-composited surfaces, and
//! Chromium and Electron are exactly that, which is what issues #14 and #34 are
//! about. Every top-level window on the development machine rendered, VS Code
//! at 99.4% ink. See
//! `docs/benchmarks/2026-08-07-reading-a-covered-window.md`.

use lens_core::geometry::ScreenRect;
use lens_platform_windows::capture::Capture;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};

/// `PW_RENDERFULLCONTENT`.
///
/// The flag that makes this worth trying at all: without it a composited
/// window renders as a blank rectangle, which is where the mechanism's
/// reputation comes from.
const FULL_CONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(0x0000_0002);

/// The same ceiling `capture::rectangle` uses, for the same reason.
const MAX_SIDE: i32 = 4096;

/// Ask `window` to render itself, whatever is in front of it.
///
/// `size` is the client area to render, and the result is that many pixels
/// whether or not any of them are on screen.
///
/// `None` for an absurd size, for any GDI failure, and when the window
/// declined to draw. A caller cannot do anything different about the three, and
/// the answer either way is that this window could not be read this way.
pub fn window(id: i64, size: ScreenRect) -> Option<Capture> {
    let (width, height) = (size.width, size.height);
    if width <= 0 || height <= 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }

    let header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative for top-down, which is the order a recogniser expects.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // SAFETY: every handle is released on every path, and the pixels are copied
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

        let drawn = PrintWindow(HWND(id as *mut _), memory, FULL_CONTENT).as_bool();

        let pixels = if drawn && !bits.is_null() {
            let count = (width * height) as usize * 4;
            let mut buffer = std::slice::from_raw_parts(bits.cast::<u8>(), count).to_vec();
            // GDI leaves the alpha channel at zero and the recogniser reads it,
            // exactly as `capture::rectangle` has to fix up.
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

        pixels.and_then(|pixels| Capture::from_bgra(width, height, pixels))
    }
}

/// Whether a rendered capture has anything on it.
///
/// A window may answer `PrintWindow` and draw nothing, which is the documented
/// failure and looks identical to a blank window. This is how a caller tells
/// "rendered nothing" from "rendered a blank page", and it reports a **share**
/// rather than anything about what the pixels were
/// (`docs/privacy-model.md`).
pub fn ink(capture: &Capture) -> f64 {
    let bytes = capture.bytes();
    let Some(corner) = bytes.get(..3) else {
        return 0.0;
    };
    let total = (capture.width * capture.height) as f64;
    if total <= 0.0 {
        return 0.0;
    }
    let different = bytes
        .chunks_exact(4)
        .filter(|pixel| pixel[..3] != *corner)
        .count();
    different as f64 * 100.0 / total
}
