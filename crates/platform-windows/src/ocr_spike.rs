//! Does Windows' own OCR work from an unpackaged single executable?
//!
//! The question that blocks Milestone 4
//! ([#62](https://github.com/swarea/mori-lens/issues/62)).
//! [`Windows.Media.Ocr`](https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr)
//! is the obvious backend: in the operating system already, no model to
//! download, Korean and English covered. Its documentation states a **package
//! identity** requirement for desktop applications, and Lens ships as a single
//! unpackaged `lens.exe`, which is what the README promises.
//!
//! A cargo test binary has no package identity either, so it is a fair proxy
//! for `lens.exe`. If OCR works here it works there.
//!
//! # It draws its own bitmap
//!
//! Nothing is captured. The text is rendered with GDI into a memory bitmap and
//! handed straight to the recogniser, so this spike answers the question
//! without reading a pixel of anybody's screen. That also makes it a real test
//! of the recogniser rather than of the capture path, which is a separate
//! problem with its own permission story.
//!
//! # What it does not answer
//!
//! Whether OCR is *good enough*: accuracy on small text, on dark themes, on
//! the code editor that prompted the milestone. Those need real screen content
//! and belong in the milestone, not in the spike that decides whether the
//! milestone can use this backend at all.

#![cfg(windows)]

use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, FillRect,
    SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_LEFT, DT_NOPREFIX, DT_WORDBREAK,
    FF_DONTCARE, FW_NORMAL, HBRUSH, OUT_TT_PRECIS, TRANSPARENT,
};

/// `AsyncStatus::Started`, compared as the integer it is.
///
/// The type lives in the `windows` crate's own vendored `windows-future`, and
/// depending on that crate separately produces a second, incompatible copy of
/// it. The value is part of the WinRT ABI and does not move.
const ASYNC_STARTED: i32 = 0;

/// What the spike found.
#[derive(Debug)]
pub struct OcrVerdict {
    /// Whether an engine could be created at all. This is the package identity
    /// question.
    pub engine_available: bool,
    /// The languages this machine can recognise, in BCP-47.
    pub languages: Vec<String>,
    /// What came back for the bitmap below, when an engine existed.
    pub recognised: Option<String>,
    /// The failure, when there was one, as the platform reported it.
    pub failure: Option<String>,
}

/// Render a line of text and ask Windows to read it back.
pub fn probe(text: &str) -> OcrVerdict {
    let languages = available_languages();

    let engine = match OcrEngine::TryCreateFromUserProfileLanguages() {
        Ok(engine) => engine,
        Err(error) => {
            // The answer either way. A package identity failure shows up here,
            // and so does "no recognition pack installed", which is a
            // different problem with a different fix.
            return OcrVerdict {
                engine_available: false,
                languages,
                recognised: None,
                failure: Some(format!("{error:?}")),
            };
        }
    };

    // SAFETY: every GDI object is released before the pixels are used.
    let Some((pixels, width, height)) = (unsafe { render(text) }) else {
        return OcrVerdict {
            engine_available: true,
            languages,
            recognised: None,
            failure: Some("could not render the test bitmap".to_owned()),
        };
    };

    match recognise(&engine, &pixels, width, height) {
        Ok(found) => OcrVerdict {
            engine_available: true,
            languages,
            recognised: Some(found),
            failure: None,
        },
        Err(error) => OcrVerdict {
            engine_available: true,
            languages,
            recognised: None,
            failure: Some(format!("{error:?}")),
        },
    }
}

fn available_languages() -> Vec<String> {
    let Ok(list) = OcrEngine::AvailableRecognizerLanguages() else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|language| language.LanguageTag().ok())
        .map(|tag| tag.to_string_lossy())
        .collect()
}

/// Whether a specific language can be recognised, which decides whether this
/// backend can serve a Korean reader at all.
pub fn can_recognise(tag: &str) -> bool {
    Language::CreateLanguage(&HSTRING::from(tag))
        .and_then(|language| OcrEngine::IsLanguageSupported(&language))
        .unwrap_or(false)
}

fn recognise(
    engine: &OcrEngine,
    pixels: &[u8],
    width: i32,
    height: i32,
) -> windows::core::Result<String> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(pixels)?;
    let buffer = writer.DetachBuffer()?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width, height)?;
    // Waited for rather than assumed finished. `GetResults` on an operation
    // still running answers `0x8000000E`, "method called at an unexpected
    // time", which is what the first run of this spike reported and briefly
    // looked like the package identity problem it was written to find.
    //
    // Polled rather than awaited: a spike may block, and where this call
    // belongs in the product is a question for the milestone, not for the
    // thing deciding whether the milestone can use this backend.
    let operation = engine.RecognizeAsync(&bitmap)?;
    let started = std::time::Instant::now();
    while operation.Status()?.0 == ASYNC_STARTED {
        if started.elapsed() > std::time::Duration::from_secs(20) {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::RPC_E_TIMEOUT,
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let result = operation.GetResults()?;
    Ok(result.Text()?.to_string_lossy())
}

/// Black text on white, drawn rather than captured.
///
/// # Safety
/// Creates and releases its own GDI objects.
unsafe fn render(text: &str) -> Option<(Vec<u8>, i32, i32)> {
    const WIDTH: i32 = 720;
    const HEIGHT: i32 = 120;

    let header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: WIDTH,
            biHeight: -HEIGHT,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // SAFETY: documented GDI calls; every object is deleted below.
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

        let whole = RECT {
            left: 0,
            top: 0,
            right: WIDTH,
            bottom: HEIGHT,
        };
        // White page, black ink: the easiest thing any recogniser will ever be
        // given. A spike that fails on this has answered the question.
        let white = windows::Win32::Graphics::Gdi::CreateSolidBrush(COLORREF(0x00ff_ffff));
        FillRect(dc, &whole, HBRUSH(white.0));
        let _ = DeleteObject(white.into());

        let font = CreateFontW(
            -36,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FF_DONTCARE.0.into(),
            windows::core::w!("Segoe UI"),
        );
        let previous_font = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0));

        let mut rect = RECT {
            left: 20,
            top: 20,
            right: WIDTH - 20,
            bottom: HEIGHT - 20,
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
        );

        SelectObject(dc, previous_font);
        let _ = DeleteObject(font.into());

        let count = (WIDTH * HEIGHT) as usize;
        let words = std::slice::from_raw_parts_mut(bits.cast::<u32>(), count);
        // GDI leaves the alpha channel at zero, and `Bgra8` means it. Without
        // this the recogniser is handed a fully transparent image.
        for word in words.iter_mut() {
            *word |= 0xff00_0000;
        }
        let bytes = std::slice::from_raw_parts(bits.cast::<u8>(), count * 4).to_vec();

        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        Some((bytes, WIDTH, HEIGHT))
    }
}
