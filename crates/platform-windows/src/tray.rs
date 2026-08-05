//! The tray icon: the only part of Lens a user can see when it is working.
//!
//! Everything else about this product is deliberately invisible. It does not
//! take focus, it does not appear in the taskbar, and when the pointer is
//! moving it draws nothing at all. That is the design, and it has one cost: a
//! user cannot tell a Lens that is listening from a Lens that has crashed, and
//! cannot stop it without finding the terminal it was started from.
//!
//! So the tray icon is not decoration. It is the answer to "is it on", the
//! answer to "make it stop", and the only affordance that does not require
//! remembering a hotkey.
//!
//! # Focus
//!
//! Showing the menu calls `SetForegroundWindow`, which everywhere else in this
//! product would be a defect: the card must never take focus
//! ([ADR-0004](../../../docs/decisions/ADR-0004-passive-overlay.md)). The
//! difference is who asked. A card appears because a pointer stopped moving; a
//! menu appears because a person clicked on it. Windows also requires the call,
//! since a popup menu whose owner is not foreground never receives the click
//! that dismisses it and hangs on screen until the next one.
//!
//! # Why a thread of its own
//!
//! `Shell_NotifyIconW` delivers its events as window messages, so the icon
//! needs a window and the window needs a pump. Running that on the hover loop
//! would mean the tray stops responding while an acquisition is in flight,
//! which is exactly the moment a user reaches for it.

use std::sync::mpsc::{self, Receiver, Sender};

use lens_core::icon::{tray_glyph, Glyph};
use lens_core::theme::Rgb;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics,
    PostMessageW, PostThreadMessageW, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    TrackPopupMenu, TranslateMessage, HICON, ICONINFO, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON,
    TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP,
    WM_LBUTTONUP, WM_NULL, WM_QUIT, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

/// The message `Shell_NotifyIconW` sends back when the user touches the icon.
///
/// Visible to the crate so the live check can replay a click with the same
/// packing the shell uses, which is the part of this module that is easy to
/// get wrong and impossible to see.
pub(crate) const WM_TRAY: u32 = WM_APP + 0x20;
/// Posted by [`Tray`] to make the pump look at its command queue.
const WM_TRAY_REFRESH: u32 = WM_APP + 0x21;

/// Menu command ids. Arbitrary, but distinct from zero, which `TrackPopupMenu`
/// returns to mean "dismissed without choosing".
const ID_TOGGLE: usize = 1;
const ID_QUIT: usize = 2;

/// The icon's own id within this process. One icon, so one id.
const ICON_ID: u32 = 1;

/// What the user asked for by clicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Pause if listening, resume if paused. The same request the Ctrl+Alt+L
    /// hotkey makes, so both paths land in one place in the loop.
    TogglePause,
    /// Quit. A tray application with no way out of it is a hostage situation.
    Quit,
}

/// What the loop asks the tray to do.
enum Command {
    SetState { glyph: Glyph, tip: String },
}

/// A live tray icon on its own thread.
pub struct Tray {
    commands: Sender<Command>,
    events: Receiver<TrayEvent>,
    thread_id: u32,
    window_id: i64,
    join: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tray").finish_non_exhaustive()
    }
}

impl Tray {
    /// Put the icon in the notification area.
    ///
    /// Returns `None` when the icon could not be created, which is not fatal
    /// to the product: Lens still works, the user just cannot see or stop it
    /// without the terminal. The caller is expected to say so rather than to
    /// exit.
    pub fn start(colour: Rgb, tip: &str) -> Option<Self> {
        let (commands, command_rx) = mpsc::channel::<Command>();
        let (events, event_rx) = mpsc::channel::<TrayEvent>();
        let (ready, ready_rx) = mpsc::channel::<Option<(u32, i64)>>();
        let tip = tip.to_owned();

        let join = std::thread::spawn(move || {
            // SAFETY: window creation, the icon, and the pump all happen on
            // this thread, and everything created is destroyed before return.
            unsafe {
                let Some(window) = create_window() else {
                    let _ = ready.send(None);
                    return;
                };
                let mut state = IconState::new(colour);
                if !state.add(window, Glyph::Listening, &tip) {
                    let _ = DestroyWindow(window);
                    let _ = ready.send(None);
                    return;
                }
                let _ = ready.send(Some((GetCurrentThreadId(), window.0 as i64)));
                pump(window, &mut state, &command_rx, &events);
                state.remove(window);
                let _ = DestroyWindow(window);
            }
        });

        let (thread_id, window_id) = ready_rx.recv().ok()??;
        Some(Self {
            commands,
            events: event_rx,
            thread_id,
            window_id,
            join: Some(join),
        })
    }

    /// Whatever the user has clicked since this was last asked.
    ///
    /// Non-blocking, and drained rather than one at a time: the loop polls
    /// every 25ms and a double click should not take two of those to be seen.
    pub fn poll(&self) -> Vec<TrayEvent> {
        self.events.try_iter().collect()
    }

    /// Change what the icon is saying.
    ///
    /// Called when pausing and resuming, so the icon and the state cannot
    /// disagree. A tray icon that still looks live while the product is paused
    /// is worse than none, because it is confidently wrong.
    pub fn set_state(&self, glyph: Glyph, tip: &str) -> bool {
        self.commands
            .send(Command::SetState {
                glyph,
                tip: tip.to_owned(),
            })
            .is_ok()
            && self.wake()
    }

    /// The message-only window the shell sends callbacks to, as an opaque id.
    ///
    /// Only for the live check that replays a click without a hand. Nothing in
    /// the product needs it, which is why it is not part of how the tray is
    /// used.
    pub fn window_id(&self) -> i64 {
        self.window_id
    }

    fn wake(&self) -> bool {
        // SAFETY: posting a documented message to a thread this struct owns.
        unsafe { PostThreadMessageW(self.thread_id, WM_TRAY_REFRESH, WPARAM(0), LPARAM(0)) }.is_ok()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: posting to a thread this struct owns; a failure means it is
        // already gone, which is the state being asked for.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// The icon currently in the notification area, and what it took to make it.
struct IconState {
    colour: Rgb,
    icon: Option<HICON>,
    glyph: Glyph,
}

impl IconState {
    fn new(colour: Rgb) -> Self {
        Self {
            colour,
            icon: None,
            glyph: Glyph::Listening,
        }
    }

    /// # Safety
    /// Must run on the thread owning `window`.
    unsafe fn add(&mut self, window: HWND, glyph: Glyph, tip: &str) -> bool {
        // SAFETY: the caller guarantees the thread; every handle is this
        // struct's own.
        unsafe { self.notify(window, glyph, tip, NIM_ADD) }
    }

    /// # Safety
    /// Must run on the thread owning `window`.
    unsafe fn modify(&mut self, window: HWND, glyph: Glyph, tip: &str) -> bool {
        // SAFETY: as above.
        unsafe { self.notify(window, glyph, tip, NIM_MODIFY) }
    }

    /// # Safety
    /// Must run on the thread owning `window`.
    unsafe fn notify(
        &mut self,
        window: HWND,
        glyph: Glyph,
        tip: &str,
        action: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE,
    ) -> bool {
        // SAFETY: `data` is fully initialised below and the icon it points at
        // outlives the call, because it is replaced only after the shell has
        // been told about the new one.
        unsafe {
            let fresh = make_icon(self.colour, glyph);
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: ICON_ID,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAY,
                hIcon: fresh.unwrap_or_default(),
                ..Default::default()
            };
            write_tip(&mut data.szTip, tip);

            let sent = Shell_NotifyIconW(action, &data).as_bool();

            // The previous icon is destroyed only now. Freeing it before the
            // shell has the replacement leaves a blank square in the tray for
            // as long as the shell takes to repaint.
            if let Some(previous) = self.icon.replace(fresh.unwrap_or_default()) {
                if !previous.is_invalid() {
                    let _ = DestroyIcon(previous);
                }
            }
            self.glyph = glyph;
            sent
        }
    }

    /// # Safety
    /// Must run on the thread owning `window`.
    unsafe fn remove(&mut self, window: HWND) {
        // SAFETY: removing this process's own icon and freeing its handle.
        unsafe {
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: ICON_ID,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            if let Some(icon) = self.icon.take() {
                if !icon.is_invalid() {
                    let _ = DestroyIcon(icon);
                }
            }
        }
    }
}

/// Copy a tooltip into the fixed buffer the shell expects.
///
/// Truncated at a `char` boundary rather than a byte one, and the terminator is
/// always written: `szTip` is not a Rust string and an unterminated one is read
/// until it finds a zero somewhere else in the struct.
fn write_tip(buffer: &mut [u16; 128], tip: &str) {
    let mut written = 0usize;
    for unit in tip.encode_utf16() {
        if written + 1 >= buffer.len() {
            break;
        }
        buffer[written] = unit;
        written += 1;
    }
    buffer[written] = 0;
}

/// Build an `HICON` from the glyph `lens-core` computed.
///
/// # Safety
/// Creates and releases GDI objects; the returned icon is the caller's.
unsafe fn make_icon(colour: Rgb, glyph: Glyph) -> Option<HICON> {
    // SAFETY: every handle created here is destroyed on every path, and the
    // pixel buffer is exactly `size * size` words as the header declares.
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
        let pixels = tray_glyph(size, colour, glyph);

        let header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                // Negative, so row zero is the top one. A positive height flips
                // the glyph, which for a ring is invisible and would hide the
                // mistake until something asymmetric was drawn.
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let colour_bitmap =
            CreateDIBSection(None, &header, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(colour_bitmap.into());
            return None;
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());

        // An all-zero mask means "take every pixel from the colour bitmap",
        // which is what a 32-bit icon with its own alpha channel wants.
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);

        let info = ICONINFO {
            fIcon: true.into(),
            hbmMask: mask,
            hbmColor: colour_bitmap,
            ..Default::default()
        };
        let icon = CreateIconIndirect(&info).ok();

        // `CreateIconIndirect` copies the bitmaps, so ours are ours to free.
        let _ = DeleteObject(colour_bitmap.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

/// A window with no pixels, only a message queue.
///
/// # Safety
/// Must run on the thread that will pump it.
unsafe fn create_window() -> Option<HWND> {
    // SAFETY: a documented class registration and window creation; a duplicate
    // registration fails harmlessly and the create still succeeds.
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            lpszClassName: w!("MoriLensTray"),
            ..Default::default()
        };
        RegisterClassW(&class);

        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MoriLensTray"),
            w!("Mori Lens"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            // Message-only: never shown, never in the taskbar, never in
            // Alt+Tab. It exists to receive the shell's callbacks.
            Some(HWND(-3isize as *mut _)),
            None,
            None,
            None,
        )
        .ok()
    }
}

/// Where the user's click is turned into a [`TrayEvent`].
///
/// # Safety
/// Must run on the thread owning `window`.
unsafe fn pump(
    window: HWND,
    state: &mut IconState,
    commands: &Receiver<Command>,
    events: &Sender<TrayEvent>,
) {
    // SAFETY: the caller guarantees this thread owns the window.
    unsafe {
        // Explorer restarting takes every tray icon with it and broadcasts
        // this. Without handling it, Lens is still running and completely
        // invisible, which is the failure this whole module exists to prevent.
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));

        let mut tip = String::new();
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            match message.message {
                WM_TRAY_REFRESH => {
                    while let Ok(Command::SetState { glyph, tip: fresh }) = commands.try_recv() {
                        tip = fresh;
                        state.modify(window, glyph, &tip);
                    }
                }
                other if other == taskbar_created => {
                    // Re-add rather than modify: as far as the new shell is
                    // concerned this icon has never existed.
                    let glyph = state.glyph;
                    state.add(window, glyph, &tip);
                }
                WM_TRAY => {
                    if let Some(event) = clicked(window, message.lParam) {
                        if events.send(event).is_err() {
                            return;
                        }
                    }
                }
                _ => {}
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// Turn a click on the icon into a request, showing the menu when asked.
///
/// # Safety
/// Must run on the thread owning `window`.
unsafe fn clicked(window: HWND, lparam: LPARAM) -> Option<TrayEvent> {
    // The shell packs the mouse message into the low word.
    let mouse = (lparam.0 as u32) & 0xffff;
    match mouse {
        // Left click is the fast path, because pause and resume is the only
        // thing a user does often enough to deserve one.
        WM_LBUTTONUP => Some(TrayEvent::TogglePause),
        WM_RBUTTONUP => {
            // SAFETY: menu creation and tracking on the owning thread; the
            // menu is destroyed on every path.
            unsafe { show_menu(window) }
        }
        _ => None,
    }
}

/// # Safety
/// Must run on the thread owning `window`.
unsafe fn show_menu(window: HWND) -> Option<TrayEvent> {
    // SAFETY: all documented menu calls; `DestroyMenu` runs before returning.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        // Korean, because this is product UI rather than a repository
        // artifact (`docs/git-workflow.md` §7).
        let _ = AppendMenuW(menu, MF_STRING, ID_TOGGLE, w!("일시정지 · 다시 시작"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("종료"));

        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);

        // Required, and the one place in this product where taking focus is
        // correct: a popup menu whose owner is not foreground never gets the
        // click that dismisses it and stays on screen. The user asked for this
        // menu by clicking, which is the difference from a card.
        let _ = SetForegroundWindow(window);

        let chosen = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD,
            point.x,
            point.y,
            None,
            window,
            None,
        );
        let _ = DestroyMenu(menu);

        // The documented dance that lets the menu close properly when the user
        // clicks away from it.
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));

        match chosen.0 as usize {
            ID_TOGGLE => Some(TrayEvent::TogglePause),
            ID_QUIT => Some(TrayEvent::Quit),
            _ => None,
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: the documented default handler.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
