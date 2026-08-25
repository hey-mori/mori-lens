//! The pinned detail, as a window that stays until it is closed.
//!
//! [ADR-0004](../../../docs/decisions/ADR-0004-native-overlay.md) named this
//! surface on 2026-08-04 and nothing built it, so four capabilities became
//! keyboard chords instead. `lens_core::detail` holds what it shows and why;
//! this puts it on screen.
//!
//! # Why a window and not a bigger card
//!
//! The card is `WS_EX_TRANSPARENT` and never activates, which is what lets a
//! reader keep typing through it. A window that can be focused or closed cannot
//! be that window, and a button inside a click-through one cannot be clicked.
//! ADR-0004 says exactly this.
//!
//! # GDI, not Direct2D
//!
//! The card is a floating tooltip and earns the glass renderer
//! ([ADR-0011](../../../docs/decisions/ADR-0011-glass-needs-direct2d.md)). This
//! is an ordinary window with a border, which GDI draws in a fraction of the
//! code, and `welcome.rs` is the working precedent for that shape.
//!
//! # The slot is filled before the window is shown
//!
//! `ShowWindow` paints. The welcome window shipped blank because its content was
//! published after that call and nothing invalidated again, and its live check
//! passed throughout because it measured whether a pointer could reach the
//! lesson rather than whether an eye could see it. The same mistake is available
//! here and the ordering in `open` is why it is not made.

use std::sync::{mpsc, Mutex};

use lens_core::detail::Detail;
use lens_core::theme::{Rgb, Theme};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, GetDC,
    GetDeviceCaps, InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_CALCRECT,
    DT_LEFT, DT_NOPREFIX, DT_WORDBREAK, HDC, LOGPIXELSY, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetSystemMetrics, IsWindow, PostQuitMessage, PostThreadMessageW, RegisterClassW,
    SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage, MSG, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SW_SHOW, WM_DESTROY, WM_KEYDOWN, WM_PAINT, WM_QUIT,
    WNDCLASSW, WS_BORDER, WS_EX_TOPMOST, WS_POPUP,
};

/// What the window paints, published before it is shown and replaced on update.
static SHOWING: Mutex<Option<Showing>> = Mutex::new(None);

struct Showing {
    detail: Detail,
    theme: Theme,
    /// What the panel says where an explanation would be, before there is one.
    hint: String,
}

/// Logical width of the panel, before the display's scaling.
///
/// Wider than the card's 300, because the reason to open this is the answer
/// that did not fit in 300.
const WIDTH: i32 = 560;
/// Space around the edge, logical.
const PADDING: i32 = 16;
/// Space between one block and the next.
const GAP: i32 = 10;
/// Most of the screen this may take, so a long sentence does not fill it.
const MOST_OF_THE_SCREEN: f32 = 0.8;

/// A detail panel on its own thread.
#[derive(Debug)]
pub struct DetailPanel {
    window_id: i64,
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
}

impl DetailPanel {
    /// Put a detail on screen and wait until it is there.
    ///
    /// `None` when the window cannot be made, which is not worth failing a
    /// hover for: the card is still the product.
    pub fn open(detail: Detail, theme: Theme, hint: String) -> Option<Self> {
        crate::window::declare_dpi_awareness();
        // Published first. See the module comment: `ShowWindow` paints, and a
        // slot filled after it leaves a window that is correct and empty.
        if let Ok(mut slot) = SHOWING.lock() {
            *slot = Some(Showing {
                detail,
                theme,
                hint,
            });
        }

        let (ready, ready_rx) = mpsc::channel::<Option<(i64, u32)>>();
        let join = std::thread::spawn(move || {
            let made = create();
            let Some(window) = made else {
                let _ = ready.send(None);
                return;
            };
            // SAFETY: a read-only query for this thread's own id.
            let thread_id = unsafe { GetCurrentThreadId() };
            let _ = ready.send(Some((window.0 as i64, thread_id)));
            // SAFETY: an ordinary pump on the thread that owns the window, and
            // the window is destroyed here rather than by another thread.
            unsafe {
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = DestroyWindow(window);
            }
        });

        let (window_id, thread_id) = ready_rx.recv().ok()??;
        Some(Self {
            window_id,
            thread_id,
            join: Some(join),
        })
    }

    /// Replace what the panel shows, without closing it.
    ///
    /// The reason this surface exists: an explanation arrives and the
    /// translation the reader was already reading stays where it is.
    pub fn update(&self, detail: Detail) {
        if let Ok(mut slot) = SHOWING.lock() {
            if let Some(showing) = slot.as_mut() {
                showing.detail = detail;
            }
        }
        // **Resized, not just repainted.** The window is sized to its content
        // when it is made, so an explanation arriving afterwards is four rows
        // taller than the window it is drawn into and the last of them lands
        // below the visible edge. Measured 2026-08-18: adding four rows moved
        // the panel's ink from 0.062 of sampled pixels to 0.061, which is the
        // rows being drawn where nobody can see them.
        let window = HWND(self.window_id as *mut _);
        if let Some((width, height)) = measure() {
            // SAFETY: resizing a window this type owns, keeping its position
            // and its place in the z-order.
            unsafe {
                let _ = SetWindowPos(
                    window,
                    None,
                    0,
                    0,
                    width,
                    height,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        // SAFETY: asking a window this type owns to repaint. A destroyed handle
        // makes this a no-op rather than an error.
        unsafe {
            let _ = InvalidateRect(Some(window), None, true);
        }
    }

    /// The window's handle, for a check that needs to look at it.
    pub fn window_id(&self) -> i64 {
        self.window_id
    }

    /// Whether it is still there.
    ///
    /// A reader closes it with `Esc`, on its own thread, without telling
    /// anybody. So this asks the window rather than tracking a flag that would
    /// be wrong the moment they did.
    pub fn is_open(&self) -> bool {
        // SAFETY: a read-only query that answers false for a destroyed handle,
        // which is precisely the case being asked about.
        unsafe { IsWindow(Some(HWND(self.window_id as *mut _))).as_bool() }
    }
}

impl Drop for DetailPanel {
    fn drop(&mut self) {
        // SAFETY: the documented quit message to a thread this type started. A
        // thread that has already gone makes this a no-op.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        if let Ok(mut slot) = SHOWING.lock() {
            *slot = None;
        }
    }
}

/// Make the window, sized to what the slot already holds.
///
/// Safe to call, and only correct from the thread that will pump its messages:
/// a threading precondition rather than a memory-safety one.
fn create() -> Option<HWND> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        lpszClassName: w!("MoriLensDetail"),
        ..Default::default()
    };
    // SAFETY: a class registration with a static name. A second registration is
    // an error this ignores, because the first one is the one that counts.
    unsafe {
        RegisterClassW(&class);
    }

    let (width, height) = measure()?;
    // SAFETY: read-only metrics, used to put the panel in the middle.
    let (screen_w, screen_h) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };

    // SAFETY: creating a window of the class registered above. Topmost rather
    // than foreground at creation: `SetForegroundWindow` is refused to a process
    // not already in front, which is what left the welcome window correct,
    // drawn, and behind a terminal.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            w!("MoriLensDetail"),
            w!("Mori Lens"),
            WS_POPUP | WS_BORDER,
            (screen_w - width) / 2,
            (screen_h - height) / 3,
            width,
            height,
            None,
            None,
            None,
            None,
        )
        .ok()?
    };

    // SAFETY: showing and focusing a window this thread just made. The slot was
    // filled before the thread started, so this paint has content.
    unsafe {
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
    Some(window)
}

/// The blocks the panel draws, in order, as `(heading, body, quiet)`.
///
/// Built once and used by both the measuring pass and the drawing one, because
/// two lists would be two chances to disagree and the disagreement shows as a
/// panel whose last line is cut off.
fn blocks(showing: &Showing) -> Vec<(&'static str, String, bool)> {
    let detail = &showing.detail;
    let mut out: Vec<(&'static str, String, bool)> = Vec::new();
    out.push(("원문", detail.original.clone(), true));
    if let Some(translation) = &detail.translation {
        out.push(("번역", translation.clone(), false));
    }
    if detail.is_explained() {
        let rows = detail
            .rows
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(label, value)| format!("{label}   {value}"))
            .collect::<Vec<_>>()
            .join("\n");
        out.push(("설명", rows, false));
    } else {
        out.push(("설명", showing.hint.clone(), true));
    }
    out
}

/// How big the panel has to be for what it is showing.
fn measure() -> Option<(i32, i32)> {
    let slot = SHOWING.lock().ok()?;
    let showing = slot.as_ref()?;
    // SAFETY: a screen device context, released below, used only to measure.
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let scale = f32::from(GetDeviceCaps(Some(dc), LOGPIXELSY) as i16) / 96.0;
        let px = |logical: i32| (logical as f32 * scale).round() as i32;
        let width = px(WIDTH);
        let inner = width - px(PADDING) * 2;

        let mut height = px(PADDING);
        for (heading, body, _) in blocks(showing) {
            height += height_of(dc, heading, inner, showing.theme.small_pt);
            height += height_of(dc, &body, inner, showing.theme.body_pt);
            height += px(GAP);
        }
        height += height_of(dc, &showing.detail.footer, inner, showing.theme.small_pt);
        height += px(PADDING);
        ReleaseDC(None, dc);

        let ceiling = (GetSystemMetrics(SM_CYSCREEN) as f32 * MOST_OF_THE_SCREEN) as i32;
        Some((width, height.min(ceiling)))
    }
}

/// How tall one block is at this width, measured and not drawn.
///
/// # Safety
///
/// `dc` must be a valid device context.
unsafe fn height_of(dc: HDC, text: &str, width: i32, points: i32) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: one font, selected, restored and deleted. `DT_CALCRECT` measures
    // and draws nothing.
    unsafe {
        let font = crate::overlay::font(crate::overlay::logical_height(dc, points), false);
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

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            // SAFETY: a paint cycle on this window, begun and ended here.
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(window, &mut paint);
                if let Ok(slot) = SHOWING.lock() {
                    if let Some(showing) = slot.as_ref() {
                        draw(dc, window, showing);
                    }
                }
                let _ = EndPaint(window, &paint);
            }
            LRESULT(0)
        }
        WM_KEYDOWN if w_param.0 as u16 == VK_ESCAPE.0 => {
            // SAFETY: destroying this window, which ends its own pump.
            unsafe {
                let _ = DestroyWindow(window);
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
///
/// `dc` must be `window`'s device context, inside a paint cycle.
unsafe fn draw(dc: HDC, window: HWND, showing: &Showing) {
    let theme = showing.theme;
    let mut client = RECT::default();
    // SAFETY: a read-only query on the window being painted.
    unsafe {
        let _ = GetClientRect(window, &mut client);
    }
    // SAFETY: brushes created and deleted here, before any text is drawn over
    // them. The edge takes `Theme::edge_colour` so this window and the card
    // agree on how firm a border is.
    unsafe {
        let surface = CreateSolidBrush(crate::overlay::colorref(theme.surface));
        FillRect(dc, &client, surface);
        let _ = DeleteObject(surface.into());
        let border = CreateSolidBrush(crate::overlay::colorref(theme.edge_colour()));
        FrameRect(dc, &client, border);
        let _ = DeleteObject(border.into());
        SetBkMode(dc, TRANSPARENT);
    }

    // SAFETY: a read-only query on the device context being drawn into.
    let scale = f32::from(unsafe { GetDeviceCaps(Some(dc), LOGPIXELSY) } as i16) / 96.0;
    let px = |logical: i32| (logical as f32 * scale).round() as i32;
    let left = px(PADDING);
    let inner = client.right - px(PADDING) * 2;
    let mut y = px(PADDING);

    for (heading, body, quiet) in blocks(showing) {
        // SAFETY: measuring and drawing with the same device context.
        unsafe {
            y += write(dc, heading, left, y, inner, theme.small_pt, theme.footer);
            let ink = if quiet { theme.original } else { theme.body };
            y += write(dc, &body, left, y, inner, theme.body_pt, ink);
        }
        y += px(GAP);
    }
    // SAFETY: as above.
    unsafe {
        write(
            dc,
            &showing.detail.footer,
            left,
            y,
            inner,
            theme.small_pt,
            theme.footer,
        );
    }
}

/// Draw one block and answer how tall it was.
///
/// # Safety
///
/// `dc` must be a valid device context inside a paint cycle.
unsafe fn write(
    dc: HDC,
    text: &str,
    left: i32,
    top: i32,
    width: i32,
    points: i32,
    ink: Rgb,
) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: one font, selected, restored and deleted, and a height measured
    // with the same rule the layout used.
    unsafe {
        let height = height_of(dc, text, width, points);
        let font = crate::overlay::font(crate::overlay::logical_height(dc, points), false);
        let previous = SelectObject(dc, font.into());
        SetTextColor(dc, crate::overlay::colorref(ink));
        let mut rect = RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
        );
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        height
    }
}
