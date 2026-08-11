//! The window that stages a reader's first hover.
//!
//! [`lens_core::welcome`] decides *what* the lesson says and why. This puts it
//! on screen and reports where each line landed, so the ordinary hover path can
//! answer it.
//!
//! # Why it cannot be dragged and does not close on a click
//!
//! **Dragging.** The line rectangles are measured once and handed to the
//! acquirer on another thread. A window the reader can move takes its text with
//! it and leaves the rectangles behind, and the first hover then acquires a
//! sentence from where the window used to be. There is no caption bar, so there
//! is nothing to drag it by.
//!
//! **Clicking.** A click is exactly the reflex this window exists to unteach,
//! and it is the first thing a reader will try on a sentence that looks like a
//! link. A window that dismissed itself on a click would vanish before the
//! reader ever held still, which would teach the opposite lesson and leave no
//! way back. `Esc` closes it, and the footer says so.
//!
//! # It is not the overlay
//!
//! [ADR-0004](../../../docs/decisions/ADR-0004-passive-overlay.md) requires the
//! **card** to be passive and click-through, and it still is. This is an
//! ordinary window that the reader looks at once, and the card will be drawn
//! over it by the same overlay as everywhere else.

use std::sync::{mpsc, Mutex};

use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::theme::{Rgb, Theme};
use lens_core::welcome::Lesson;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    FrameRect, GetDC, GetDeviceCaps, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    UpdateWindow, DT_CALCRECT, DT_LEFT, DT_NOPREFIX, DT_WORDBREAK, LOGPIXELSY, PAINTSTRUCT,
    TRANSPARENT,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, GetSystemMetrics, IsWindow, PostQuitMessage, PostThreadMessageW,
    RegisterClassW, SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage, HWND_TOP,
    HWND_TOPMOST, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SW_SHOW, WM_DESTROY, WM_KEYDOWN, WM_PAINT, WM_QUIT, WNDCLASSW, WS_BORDER,
    WS_EX_TOPMOST, WS_POPUP,
};

/// Logical width of the window, before scaling.
const WIDTH: i32 = 620;
/// Space around everything, and between one lesson line and the next.
const PADDING: i32 = 30;
const BETWEEN_LINES: i32 = 22;
/// Lesson lines, the size of a sentence somebody is meant to aim at.
const LESSON_POINTS: i32 = 15;
/// Chrome above and below, in the reader's own language.
const CHROME_POINTS: i32 = 11;

/// The heading, and the two lines under the lesson.
///
/// Korean, because this is product UI copy rather than a repository artefact
/// (`docs/git-workflow.md` §7). The heading names the gesture in words and the
/// lines beneath name the only key this window answers to.
const HEADING: &str = "포인터를 문장 위에 올려 두면 카드가 나타납니다";
const FOOTER_KEY: &str = "닫으려면 Esc";
const FOOTER_WHY: &str = "누르지 않아도 됩니다. 멈추기만 하면 됩니다.";

/// What the window paints, read by its own thread inside `WM_PAINT`.
///
/// One welcome exists at a time, so one slot is enough. A window procedure is a
/// plain function with no place to put state, and this is the state.
static PAINTING: Mutex<Option<Painted>> = Mutex::new(None);

#[derive(Clone)]
struct Painted {
    theme: Theme,
    lesson: [&'static str; 2],
    /// The window size these rectangles were measured for.
    ///
    /// Computed where the density is known rather than by the caller. Every
    /// length in this file is written at 96 dpi and scaled once, here.
    width: i32,
    height: i32,
    /// Client-coordinate rectangles, in the order they are drawn.
    heading: RECT,
    lines: [RECT; 2],
    footer_key: RECT,
    footer_why: RECT,
}

/// A staged first hover, and where its lines are.
#[derive(Debug)]
pub struct Welcome {
    window_id: i64,
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
    placement: Placement,
}

/// Everything the acquirer needs, and nothing that belongs to a thread.
///
/// Plain data on purpose: the hover worker runs on its own thread and holds
/// this, while the window and its message pump stay where they were created.
#[derive(Debug, Clone)]
pub struct Placement {
    pub window_id: i64,
    /// Screen rectangles for the lesson lines, in drawing order, paired with
    /// what each one says.
    pub lines: Vec<(ScreenRect, &'static str)>,
}

impl Placement {
    /// The line under a point, if this window is the one that owns the point.
    ///
    /// The ownership check is not decoration. A window on top of the welcome
    /// would otherwise have its own text answered for by this lesson, which is
    /// the failure mode `protected_harness` had to learn about the hard way.
    pub fn line_under(&self, point: ScreenPoint) -> Option<(ScreenRect, &'static str)> {
        if crate::window::root_window_at(point) != Some(self.window_id) {
            return None;
        }
        let rects: Vec<ScreenRect> = self.lines.iter().map(|(rect, _)| *rect).collect();
        lens_core::geometry::line_under(&rects, point).map(|index| self.lines[index])
    }
}

impl Welcome {
    /// Put the lesson on screen and wait until it is there.
    ///
    /// Returns `None` if the window cannot be created, which is not an error
    /// worth stopping a launch for: the product works without ever having
    /// taught anybody, it just works less often.
    pub fn open(lesson: Lesson, theme: Theme) -> Option<Self> {
        crate::window::declare_dpi_awareness();
        let (ready, ready_rx) = mpsc::channel::<Option<(Placement, u32)>>();

        let join = std::thread::spawn(move || {
            // SAFETY: every call below is a documented window API on a window
            // this thread creates, owns, and destroys before returning.
            unsafe {
                let Some((window, placement)) = create_window(lesson, theme) else {
                    let _ = ready.send(None);
                    return;
                };
                let _ = SetForegroundWindow(window);
                let _ = BringWindowToTop(window);
                let _ = ready.send(Some((placement, GetCurrentThreadId())));

                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = DestroyWindow(window);
            }
            if let Ok(mut slot) = PAINTING.lock() {
                *slot = None;
            }
        });

        let (placement, thread_id) = ready_rx.recv().ok()??;
        raise_until_owned(&placement);
        Some(Self {
            window_id: placement.window_id,
            thread_id,
            join: Some(join),
            placement,
        })
    }

    pub fn placement(&self) -> Placement {
        self.placement.clone()
    }

    /// Whether the reader still has it open.
    ///
    /// They close it with `Esc`, on their own thread, without telling anybody.
    /// So this asks the window rather than tracking a flag that would be wrong
    /// the moment they did.
    pub fn is_open(&self) -> bool {
        // SAFETY: a read-only query that answers false for a destroyed handle,
        // which is precisely the case being asked about.
        unsafe { IsWindow(Some(HWND(self.window_id as *mut _))).as_bool() }
    }
}

impl Drop for Welcome {
    fn drop(&mut self) {
        // SAFETY: posting the documented quit message to a thread this type
        // started. A thread that has already gone makes this a no-op.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Put the welcome in front until a pointer would actually reach its lines.
///
/// `SetForegroundWindow` is refused when the calling process is not already the
/// foreground one, which is exactly the case here: Lens has just started from a
/// shortcut, a terminal, or a scheduled task, and something else owns the
/// screen. The first live run of the check below found the window created,
/// drawn, correctly measured, and behind the terminal that launched it, which
/// reads identically to the layout being wrong.
///
/// Gives up after a few attempts rather than looping. Something determinedly
/// on top is the reader's business, and a welcome that fought it would be worse
/// than one that waits behind it.
fn raise_until_owned(placement: &Placement) {
    let reachable = |placement: &Placement| {
        placement.lines.iter().all(|(rect, _)| {
            placement
                .line_under(ScreenPoint::new(
                    rect.x + rect.width / 2,
                    rect.y + rect.height / 2,
                ))
                .is_some()
        })
    };
    for _ in 0..8 {
        if reachable(placement) {
            return;
        }
        let window = HWND(placement.window_id as *mut _);
        // SAFETY: documented window APIs on a window this type owns. Each is
        // allowed to fail, which is what the retry is for.
        unsafe {
            let _ = SetWindowPos(
                window,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = BringWindowToTop(window);
            let _ = SetForegroundWindow(window);
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

/// # Safety
/// Runs on the window's own thread, which owns everything it creates here.
unsafe fn create_window(lesson: Lesson, theme: Theme) -> Option<(HWND, Placement)> {
    let class = w!("MoriLensWelcome");
    // SAFETY: a documented class registration with a static name. Registering
    // twice returns an error that is ignored, which is what re-opening from the
    // tray would do.
    unsafe {
        let mut spec = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            lpszClassName: class,
            ..Default::default()
        };
        spec.hInstance = Default::default();
        RegisterClassW(&spec);
    }

    // SAFETY: created with a provisional height, then measured and resized.
    // The height cannot be known before a device context exists to measure the
    // text with, and the context has to come from the window it will be drawn
    // in so that it carries that monitor's density.
    let window = unsafe {
        CreateWindowExW(
            // **Topmost, and it has to be.**
            //
            // `SetForegroundWindow` is refused to a process that is not
            // already the foreground one, and Lens has just started: whatever
            // the reader launched it from still owns the screen. The first
            // live run of `the_welcome_puts_its_lines_where_it_says_it_did`
            // found the window created, drawn, correctly measured, and behind
            // the terminal, which is indistinguishable from a broken one.
            //
            // The card still wins. `overlay` re-asserts `HWND_TOPMOST` every
            // time it shows one, so the answer to a staged hover appears over
            // the lesson rather than under it.
            WS_EX_TOPMOST,
            class,
            w!("Mori Lens"),
            WS_POPUP | WS_BORDER,
            0,
            0,
            WIDTH,
            200,
            None,
            None,
            None,
            None,
        )
    }
    .ok()?;

    // SAFETY: measuring against a window this function just created and owns.
    let painted = unsafe { lay_out(window, lesson, theme) }?;
    let (width, height) = (painted.width, painted.height);

    // SAFETY: read-only metrics for the primary display.
    let (screen_width, screen_height) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let left = ((screen_width - width) / 2).max(0);
    let top = ((screen_height - height) / 2).max(0);

    // SAFETY: moving and showing a window this function owns.
    unsafe {
        let _ = SetWindowPos(
            window,
            Some(HWND_TOP),
            left,
            top,
            width,
            height,
            SWP_NOZORDER,
        );
        let _ = ShowWindow(window, SW_SHOW);
        let _ = UpdateWindow(window);
    }

    let mut lines = Vec::with_capacity(2);
    for (index, rect) in painted.lines.iter().enumerate() {
        // SAFETY: converting this window's client coordinates to screen ones.
        let screen = unsafe { to_screen(window, *rect) };
        lines.push((screen, lesson.lines[index]));
    }

    if let Ok(mut slot) = PAINTING.lock() {
        *slot = Some(painted);
    }

    Some((
        window,
        Placement {
            window_id: window.0 as i64,
            lines,
        },
    ))
}

/// Measure every string and decide where it goes, in client coordinates.
///
/// # Safety
/// `window` must be a valid window this thread owns.
unsafe fn lay_out(window: HWND, lesson: Lesson, theme: Theme) -> Option<Painted> {
    // SAFETY: a device context for this window, released below.
    let dc = unsafe { GetDC(Some(window)) };
    if dc.is_invalid() {
        return None;
    }
    // **Every length in this file is written at 96 dpi and scaled here.**
    //
    // `logical_height` already scales the *font* by this display's density, so
    // a box left unscaled shrinks against its own text: on a 125% display the
    // first lesson line wrapped onto three rows, and the rectangle handed to
    // the acquirer covered all three. The first live run of this measured a
    // 560x70 line where 35 was expected, which is what that looks like from
    // outside.
    // SAFETY: a read-only capability query on the context above.
    let dpi = unsafe { GetDeviceCaps(Some(dc), LOGPIXELSY) };
    let dpi = if dpi > 0 { dpi } else { 96 };
    let scaled = |value: i32| value * dpi / 96;

    let padding = scaled(PADDING);
    let between = scaled(BETWEEN_LINES);
    let width = scaled(WIDTH);
    let inner = width - padding * 2;
    let mut y = padding;

    // SAFETY: measuring only, against the context taken above, which is valid
    // for the whole block and released at the end of it. Each `measure` selects
    // and restores its own font and draws nothing.
    let (heading, first, second, footer_why, footer_key) = unsafe {
        let heading = measure(dc, HEADING, inner, CHROME_POINTS, padding, &mut y);
        y += between;
        let first = measure(dc, lesson.lines[0], inner, LESSON_POINTS, padding, &mut y);
        y += between;
        let second = measure(dc, lesson.lines[1], inner, LESSON_POINTS, padding, &mut y);
        y += between + between / 2;
        let footer_why = measure(dc, FOOTER_WHY, inner, CHROME_POINTS, padding, &mut y);
        y += scaled(6);
        let footer_key = measure(dc, FOOTER_KEY, inner, CHROME_POINTS, padding, &mut y);
        ReleaseDC(Some(window), dc);
        (heading, first, second, footer_why, footer_key)
    };

    Some(Painted {
        theme,
        lesson: lesson.lines,
        width,
        // **`footer_key`, not `footer_why`.** The key line is drawn last and
        // sits lowest, so measuring to the line above it cut "닫으려면 Esc"
        // off the bottom of a window whose whole purpose is to be understood
        // at a glance.
        height: footer_key.bottom + padding,
        heading,
        lines: [first, second],
        footer_key,
        footer_why,
    })
}

/// Where one string lands, advancing `y` past it.
///
/// # Safety
/// `dc` must be a valid device context.
unsafe fn measure(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &str,
    width: i32,
    points: i32,
    left: i32,
    y: &mut i32,
) -> RECT {
    // SAFETY: creates a font this function owns, selects it, and restores and
    // deletes it before returning.
    let font = unsafe { crate::overlay::font(crate::overlay::logical_height(dc, points), false) };
    let mut rect = RECT {
        left,
        top: *y,
        right: left + width,
        bottom: *y,
    };
    // SAFETY: DT_CALCRECT writes the measured box and draws nothing.
    unsafe {
        let previous = SelectObject(dc, font.into());
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
        );
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
    }
    rect.right = left + width;
    *y = rect.bottom;
    rect
}

/// # Safety
/// `window` must be a valid window.
unsafe fn to_screen(window: HWND, rect: RECT) -> ScreenRect {
    let mut corner = POINT {
        x: rect.left,
        y: rect.top,
    };
    // SAFETY: a documented coordinate conversion on a window this thread owns.
    unsafe {
        let _ = ClientToScreen(window, &mut corner);
    }
    ScreenRect::new(
        corner.x,
        corner.y,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            // SAFETY: a paint cycle on the window being painted, closed by
            // EndPaint on every path out.
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(window, &mut paint);
                if let Ok(slot) = PAINTING.lock() {
                    if let Some(painted) = slot.as_ref() {
                        draw(dc, window, painted);
                    }
                }
                let _ = EndPaint(window, &paint);
            }
            LRESULT(0)
        }
        // Esc and nothing else. A click must not close this: see the module
        // comment, and the reader who clicks the sentence first.
        WM_KEYDOWN if w_param.0 as u16 == VK_ESCAPE.0 => {
            // SAFETY: ends this window's own message loop.
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: ends this window's own message loop.
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        // SAFETY: the documented default handler.
        _ => unsafe { DefWindowProcW(window, message, w_param, l_param) },
    }
}

/// # Safety
/// `dc` must be the device context of `window`, inside a paint cycle.
unsafe fn draw(dc: windows::Win32::Graphics::Gdi::HDC, window: HWND, painted: &Painted) {
    let theme = painted.theme;
    let mut client = RECT::default();
    // SAFETY: a read-only query on the window being painted.
    unsafe {
        let _ = GetClientRect(window, &mut client);
    }

    // SAFETY: brushes created here are deleted here; the background and border
    // are filled before any text is drawn over them.
    unsafe {
        let surface = CreateSolidBrush(crate::overlay::colorref(theme.surface));
        FillRect(dc, &client, surface);
        let _ = DeleteObject(surface.into());
        let border = CreateSolidBrush(crate::overlay::colorref(theme.border));
        FrameRect(dc, &client, border);
        let _ = DeleteObject(border.into());
        SetBkMode(dc, TRANSPARENT);
    }

    // SAFETY: each call owns, selects, restores and deletes one font.
    unsafe {
        write(dc, HEADING, painted.heading, CHROME_POINTS, theme.footer);
        write(
            dc,
            painted.lesson[0],
            painted.lines[0],
            LESSON_POINTS,
            theme.body,
        );
        write(
            dc,
            painted.lesson[1],
            painted.lines[1],
            LESSON_POINTS,
            theme.body,
        );
        write(
            dc,
            FOOTER_WHY,
            painted.footer_why,
            CHROME_POINTS,
            theme.original,
        );
        write(
            dc,
            FOOTER_KEY,
            painted.footer_key,
            CHROME_POINTS,
            theme.footer,
        );
    }
}

/// # Safety
/// `dc` must be a valid device context inside a paint cycle.
unsafe fn write(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &str,
    at: RECT,
    points: i32,
    ink: Rgb,
) {
    // SAFETY: creates one font, selects it, restores the previous, deletes it.
    unsafe {
        let font = crate::overlay::font(crate::overlay::logical_height(dc, points), false);
        let previous = SelectObject(dc, font.into());
        SetTextColor(dc, crate::overlay::colorref(ink));
        let mut rect = at;
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
        );
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
    }
}
