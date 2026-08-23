//! A borderless full-screen window, so the last open M2 condition can be run.
//!
//! The milestone's gate asks for the card to be verified "in a full-screen
//! application" and that row has read **open** since it was written, because
//! checking it meant a person putting something into full screen and looking.
//!
//! This builds one instead: a `WS_POPUP` window covering the primary monitor,
//! which is what borderless full screen is to Windows and what a browser's F11,
//! a video player and most modern games actually use.
//!
//! **It is not exclusive full screen.** A DirectX flip-model swap chain takes
//! the display out from under the desktop compositor, and nothing drawn by
//! another process is guaranteed to appear over it. That case needs a real
//! game and a person, and saying which of the two was measured is the point of
//! naming it here.
//!
//! # Why the window lives on its own thread
//!
//! The same reason `protected_harness` does: a window's messages are pumped by
//! the thread that created it, and a test that owns the window and then waits
//! on it is waiting on itself.

use std::sync::mpsc;

use lens_core::geometry::ScreenRect;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HBRUSH, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, TranslateMessage, MSG, WINDOW_EX_STYLE, WNDCLASSW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};

/// A window covering the primary monitor, owned by its own thread.
pub struct FullScreenHarness {
    window: HWND,
    covers: ScreenRect,
}

impl FullScreenHarness {
    /// Cover the primary monitor. `topmost` is the difference between a video
    /// player and an ordinary maximised window, and both are worth a run.
    pub fn start(topmost: bool) -> Option<Self> {
        // Before the window exists, and on the thread that asks for it. Without
        // this the monitor rectangle comes back virtualised: on this 125%
        // display it read 1536x960 for a 1920x1200 screen, and the window was
        // stretched to cover it anyway, so the run looked correct and the
        // numbers in it were not. `window::declare_dpi_awareness` carries the
        // longer version of this, including the forty-of-forty failure that
        // paid for it.
        crate::window::declare_dpi_awareness();

        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let made = create(topmost);
            let ok = made.is_some();
            let _ = sender.send(made.map(|(window, covers)| (window.0 as i64, covers)));
            if !ok {
                return;
            }
            // SAFETY: an ordinary message pump on the owning thread.
            unsafe {
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        });

        let (window, covers) = receiver.recv().ok()??;
        // Long enough for the window to be created, shown and painted once.
        std::thread::sleep(std::time::Duration::from_millis(400));
        Some(Self {
            window: HWND(window as *mut _),
            covers,
        })
    }

    /// The window's handle, as the overlay reports its own.
    pub fn window_id(&self) -> i64 {
        self.window.0 as i64
    }

    /// The monitor rectangle this was asked to cover, in physical pixels.
    pub fn covers(&self) -> ScreenRect {
        self.covers
    }
}

impl Drop for FullScreenHarness {
    fn drop(&mut self) {
        // SAFETY: destroying a window this process created. The pump exits on
        // WM_DESTROY, which the window procedure turns into WM_QUIT.
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }
}

/// Make the window.
///
/// Safe to call, and only correct from the thread that will pump its messages:
/// that is a threading precondition rather than a memory-safety one, which is
/// why it is a note and not an `unsafe fn`.
fn create(topmost: bool) -> Option<(HWND, ScreenRect)> {
    // SAFETY: a read-only query for the primary monitor's rectangle. Both calls
    // take a handle this call just produced and a struct whose `cbSize` is set,
    // which is the whole of what `GetMonitorInfoW` requires of a caller.
    let area = unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        info.rcMonitor
    };
    let width = area.right - area.left;
    let height = area.bottom - area.top;

    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        lpszClassName: w!("MoriLensFullScreenHarness"),
        // A visible surface rather than a hole, so the run is looking at a
        // window that actually covers what it claims to.
        hbrBackground: HBRUSH(std::ptr::null_mut()),
        ..Default::default()
    };
    // SAFETY: registering a class this module names and owns. Registering the
    // same name twice is an error this deliberately ignores, because a second
    // run in one process is the ordinary case and the first registration is
    // the one that counts.
    unsafe {
        RegisterClassW(&class);
    }

    let style = if topmost {
        WS_EX_TOPMOST
    } else {
        WINDOW_EX_STYLE(0)
    };
    // SAFETY: creating a window of the class registered immediately above, with
    // a rectangle taken from the monitor it is meant to cover. The handle is
    // checked before use.
    let window = unsafe {
        CreateWindowExW(
            style,
            w!("MoriLensFullScreenHarness"),
            w!("Mori Lens full-screen harness"),
            // No border, no caption, no resize frame. This is what an application
            // switches to when it goes full screen without taking the display.
            WS_POPUP | WS_VISIBLE,
            area.left,
            area.top,
            width,
            height,
            None,
            None,
            None,
            None,
        )
        .ok()?
    };

    Some((window, ScreenRect::new(area.left, area.top, width, height)))
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    const WM_DESTROY: u32 = 0x0002;
    // SAFETY: this is the window procedure Windows is calling, on the thread
    // that owns the window, with the arguments Windows passed. Ending the pump
    // on destruction and handing everything else to the default procedure is
    // the whole of what it does.
    unsafe {
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}
