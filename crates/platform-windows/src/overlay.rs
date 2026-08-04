//! The card, as a native Win32 window.
//!
//! Native rather than a webview
//! (`docs/decisions/ADR-0004-native-overlay.md`): this window appears and
//! disappears on a 320ms rhythm, must never take focus from what the user is
//! typing into, and has to sit correctly on a monitor whose origin is negative.
//!
//! # The property everything else is subordinate to
//!
//! **It must not steal focus.** A card that activates itself while the user is
//! typing eats a keystroke and moves the caret, which is worse than showing no
//! card at all. Three things together buy that, and all three are load-bearing:
//!
//! - `WS_EX_NOACTIVATE`, so clicking it does not activate the owner.
//! - `SW_SHOWNOACTIVATE` rather than `SW_SHOW`.
//! - `SWP_NOACTIVATE` on every position change.
//!
//! It is asserted rather than trusted: `live_tests.rs` records the foreground
//! window before and after showing a card and fails if it moved.
//!
//! # Why a thread of its own
//!
//! A window belongs to the thread that created it, and that thread must pump
//! messages or the window never paints and the desktop treats it as hung. The
//! hover loop cannot be that thread, so the overlay owns one and is driven
//! through a channel.

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;

use lens_core::card::{Card, CardBody};
use lens_core::geometry::ScreenRect;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, GetDC, InvalidateRect,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CALCRECT,
    DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_WORDBREAK, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD,
    HBRUSH, HFONT, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
    SystemParametersInfoW, HWND_TOPMOST, LWA_ALPHA, MSG, SPI_GETWORKAREA, SWP_NOACTIVATE, SW_HIDE,
    SW_SHOWNOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_PAINT, WM_QUIT, WM_USER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// Card geometry, in physical pixels at 100%. Deliberately fixed for now: a
/// card that resizes to its content on every hover flickers, and the sizing
/// rules are a design question rather than a plumbing one.
const CARD_WIDTH: i32 = 420;
const PADDING: i32 = 14;
const MAX_HEIGHT: i32 = 420;

/// Posted to the overlay thread when the content has changed.
const WM_LENS_REFRESH: u32 = WM_USER + 1;

/// The card being drawn. One overlay, so one slot.
///
/// A mutex rather than a channel payload because `WM_PAINT` arrives on the
/// overlay thread at times nobody chose, and the paint handler needs whatever
/// is current then.
static CONTENT: Mutex<Option<Card>> = Mutex::new(None);

/// What the hover loop asks the overlay to do.
#[derive(Debug)]
enum Command {
    Show(Box<Card>, ScreenRect),
    Hide,
}

/// A live overlay window on its own thread.
#[derive(Debug)]
pub struct Overlay {
    commands: Sender<Command>,
    thread_id: u32,
    window_id: i64,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Overlay {
    /// Create the window and start pumping its messages.
    pub fn start() -> Option<Self> {
        let (commands, rx) = mpsc::channel::<Command>();
        let (ready, ready_rx) = mpsc::channel::<Option<(u32, i64)>>();

        let join = std::thread::spawn(move || {
            // SAFETY: window creation and the pump, all on this thread, with
            // the window destroyed before it returns.
            unsafe {
                let Some(window) = create_window() else {
                    let _ = ready.send(None);
                    return;
                };
                let _ = ready.send(Some((GetCurrentThreadId(), window.0 as i64)));
                pump(window, &rx);
                let _ = DestroyWindow(window);
            }
        });

        let (thread_id, window_id) = ready_rx.recv().ok()??;
        Some(Self {
            commands,
            thread_id,
            window_id,
            join: Some(join),
        })
    }

    /// Show a card anchored to the text it explains.
    ///
    /// Refuses a card that is not showable, which is how the rule that a
    /// translation never appears without its original is enforced at the last
    /// possible moment as well as the first
    /// (`lens_core::card::Card::is_showable`).
    pub fn show(&self, card: Card, anchor: ScreenRect) -> bool {
        if !card.is_showable() {
            return false;
        }
        self.commands
            .send(Command::Show(Box::new(card), anchor))
            .is_ok()
            && self.wake()
    }

    /// The card window, as an opaque id.
    ///
    /// Only for the live check that reads its pixels back to prove text was
    /// actually painted. Nothing in the product needs it.
    pub fn window_id(&self) -> i64 {
        self.window_id
    }

    pub fn hide(&self) -> bool {
        self.commands.send(Command::Hide).is_ok() && self.wake()
    }

    /// Nudge the pump so it notices a queued command.
    fn wake(&self) -> bool {
        // SAFETY: posting a documented message to a thread this struct owns.
        unsafe { PostThreadMessageW(self.thread_id, WM_LENS_REFRESH, WPARAM(0), LPARAM(0)) }.is_ok()
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: posting to a thread this struct owns; failure means it is
        // already gone, which is the state being asked for.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// # Safety
/// Must run on the thread that will pump this window's messages.
unsafe fn create_window() -> Option<HWND> {
    // SAFETY: a documented class registration and window creation; a repeat
    // registration fails harmlessly.
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            lpszClassName: w!("MoriLensOverlay"),
            ..Default::default()
        };
        RegisterClassW(&class);

        let window = CreateWindowExW(
            // Every flag here is load-bearing:
            //   NOACTIVATE  the card never takes focus from what is being typed
            //   TOOLWINDOW  it is not a task-switcher entry
            //   TOPMOST     it sits above the application it is explaining
            //   LAYERED     it can be drawn with alpha
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
            w!("MoriLensOverlay"),
            PCWSTR::null(),
            // POPUP, so there is no caption, no border, and no system menu.
            WS_POPUP,
            0,
            0,
            CARD_WIDTH,
            120,
            None,
            None,
            None,
            None,
        )
        .ok()?;

        let _ = SetLayeredWindowAttributes(window, COLORREF(0), 244, LWA_ALPHA);
        Some(window)
    }
}

/// # Safety
/// `window` must be owned by this thread.
unsafe fn pump(window: HWND, commands: &mpsc::Receiver<Command>) {
    let mut message = MSG::default();
    // SAFETY: the standard message loop for a window owned by this thread.
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            // Commands are drained on every wake rather than handled as
            // messages, so a burst of pointer movement collapses into the last
            // instruction instead of queueing a card per sample.
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Show(card, anchor) => show_card(window, *card, anchor),
                    Command::Hide => {
                        let _ = ShowWindow(window, SW_HIDE);
                    }
                }
            }
            DispatchMessageW(&message);
        }
    }
}

/// # Safety
/// `window` must be owned by this thread.
unsafe fn show_card(window: HWND, card: Card, anchor: ScreenRect) {
    let height = {
        // SAFETY: measuring against the window's own device context.
        let measured = unsafe { measure(window, &card) };
        measured.clamp(80, MAX_HEIGHT)
    };

    if let Ok(mut slot) = CONTENT.lock() {
        *slot = Some(card);
    }

    let placed = lens_core::card::place_card(
        anchor,
        anchor.center(),
        (CARD_WIDTH, height),
        // SAFETY: reads the primary work area into a local rectangle.
        unsafe { work_area() },
    );

    // SAFETY: NOACTIVATE on every call is what keeps focus where it was.
    unsafe {
        // Only move the window when it actually needs moving. A streaming
        // answer redraws several times, and repositioning a layered window on
        // every redraw is visible as a jump even when the numbers are equal.
        let mut current = RECT::default();
        let same = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(window, &mut current)
            .is_ok()
            && current.left == placed.x
            && current.top == placed.y
            && current.right - current.left == placed.width
            && current.bottom - current.top == placed.height;

        if !same {
            let _ = SetWindowPos(
                window,
                Some(HWND_TOPMOST),
                placed.x,
                placed.y,
                placed.width,
                placed.height,
                SWP_NOACTIVATE,
            );
        }
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
        // `false`: the whole client area is painted every time, so letting
        // Windows erase it first only guarantees a frame of blank card.
        let _ = InvalidateRect(Some(window), None, false);
    }
}

/// The primary monitor's work area, excluding the taskbar.
///
/// # Safety
/// Writes only into a local rectangle.
unsafe fn work_area() -> ScreenRect {
    let mut rect = RECT::default();
    // SAFETY: `SPI_GETWORKAREA` writes a RECT of the size given.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(std::ptr::addr_of_mut!(rect).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        // A plausible default is worse than none here, so fall back to
        // something large enough not to clamp anything into the wrong place.
        return ScreenRect::new(0, 0, 1920, 1080);
    }
    ScreenRect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom)
}

/// How tall the card needs to be for this content.
///
/// # Safety
/// `window` must be a valid window.
unsafe fn measure(window: HWND, card: &Card) -> i32 {
    // SAFETY: a paint cycle purely to obtain a device context for measurement.
    unsafe {
        let dc = GetDC(Some(window));
        if dc.is_invalid() {
            return 160;
        }
        let inner = CARD_WIDTH - PADDING * 2;
        let body = match &card.body {
            CardBody::Translation(text) | CardBody::Problem(text) => text,
        };
        let original = text_height(dc, &card.original, inner, false);
        let translated = text_height(dc, body, inner, true);
        let footer = 18;
        ReleaseDC(Some(window), dc);
        PADDING * 2 + original + 8 + translated + 10 + footer
    }
}

/// # Safety
/// `dc` must be a valid device context.
unsafe fn text_height(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &str,
    width: i32,
    large: bool,
) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: creates a font this function owns and deletes below.
    let font = unsafe { font(large) };
    // SAFETY: the font is selected, used, and restored before deletion.
    unsafe {
        let previous = SelectObject(dc, font.into());
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: 0,
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
        );
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        rect.bottom - rect.top
    }
}

/// # Safety
/// The returned font must be deleted by the caller.
unsafe fn font(large: bool) -> HFONT {
    // Malgun Gothic carries Hangul; Segoe UI does not. A card whose target
    // language is Korean rendered in a Latin-only face shows boxes.
    // SAFETY: a documented font creation with a static face name.
    unsafe {
        CreateFontW(
            if large { -19 } else { -15 },
            0,
            0,
            0,
            if large {
                FW_SEMIBOLD.0 as i32
            } else {
                FW_NORMAL.0 as i32
            },
            0,
            0,
            0,
            DEFAULT_CHARSET,
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(0),
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            w!("Malgun Gothic"),
        )
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_PAINT {
        // SAFETY: a standard paint cycle on this thread's own window.
        unsafe { paint(window) };
        return LRESULT(0);
    }
    // SAFETY: the documented default handler.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Paint the card, through a memory bitmap.
///
/// **Double-buffered, and that is the point.** Drawing straight to the window
/// means filling the background and then drawing text onto it, and the window
/// is on screen throughout: a reader sees an empty card for the time it takes
/// to lay out three runs of text. Repeat that for every streamed partial and
/// the card flickers. Composing into a bitmap and copying it once means the
/// screen only ever shows a finished frame.
///
/// # Safety
/// `window` must be owned by this thread.
unsafe fn paint(window: HWND) {
    // SAFETY: every GDI object created here is deleted before returning.
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(window, &mut ps);
        if dc.is_invalid() {
            return;
        }

        let mut rect = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(window, &mut rect);
        let (width, height) = (rect.right, rect.bottom);

        let memory = CreateCompatibleDC(Some(dc));
        let bitmap = CreateCompatibleBitmap(dc, width.max(1), height.max(1));
        let previous = SelectObject(memory, bitmap.into());
        let target = if memory.is_invalid() { dc } else { memory };

        let background: HBRUSH = CreateSolidBrush(COLORREF(0x00201c1a));
        let border: HBRUSH = CreateSolidBrush(COLORREF(0x00453e3a));
        FillRect(target, &rect, background);
        FrameRect(target, &rect, border);
        let _ = DeleteObject(background.into());
        let _ = DeleteObject(border.into());

        SetBkMode(target, TRANSPARENT);

        if let Some(card) = CONTENT.lock().ok().and_then(|slot| slot.clone()) {
            let inner = width - PADDING * 2;
            let mut y = PADDING;

            // The original first and dimmer. It is above the answer because
            // the reader's eye is already on it, and it is present at all
            // because it is the only way to notice an answer that is not a
            // translation (docs/privacy-model.md §1).
            if !card.original.trim().is_empty() {
                y += draw(target, &card.original, PADDING, y, inner, false, 0x00908880) + 8;
            }

            let (body, colour) = match &card.body {
                CardBody::Translation(text) => (text, 0x00f2ece8),
                CardBody::Problem(text) => (text, 0x008fb8e8),
            };
            y += draw(target, body, PADDING, y, inner, true, colour) + 10;

            draw(target, &card.footer, PADDING, y, inner, false, 0x00706a66);
        }

        // One copy, so the screen never shows a half-drawn card.
        if !memory.is_invalid() {
            let _ = BitBlt(dc, 0, 0, width, height, Some(memory), 0, 0, SRCCOPY);
            SelectObject(memory, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
        }

        let _ = EndPaint(window, &ps);
    }
}

/// Draw one run of text, returning its height.
///
/// # Safety
/// `dc` must be a valid device context.
#[allow(clippy::too_many_arguments)]
unsafe fn draw(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    large: bool,
    colour: u32,
) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: the font is selected, used, restored, and deleted.
    unsafe {
        let font = font(large);
        let previous = SelectObject(dc, font.into());
        SetTextColor(dc, COLORREF(colour));

        let mut measure_rect = RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y,
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut measure_rect,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
        );

        let mut draw_rect = RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: measure_rect.bottom,
        };
        DrawTextW(
            dc,
            &mut wide,
            &mut draw_rect,
            DT_WORDBREAK | DT_NOPREFIX | DT_LEFT | DT_END_ELLIPSIS,
        );

        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        measure_rect.bottom - measure_rect.top
    }
}

/// Whether the overlay class is registered with the styles it must have.
///
/// Exposed for the live check, which asserts the focus properties rather than
/// trusting the flags above to stay put.
pub fn required_ex_styles() -> u32 {
    (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED).0
}
