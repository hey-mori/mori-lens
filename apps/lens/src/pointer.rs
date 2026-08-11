//! Pointer and input state, read by polling.
//!
//! Polling rather than a `WH_MOUSE_LL` hook, deliberately. A low-level hook
//! receives events sooner but must return almost immediately and needs a
//! message loop, and neither buys anything in a design that waits 320ms before
//! acting on a resting pointer
//! (`docs/research/hover-text-acquisition.md` §3).

use std::sync::atomic::{AtomicBool, Ordering};

use lens_core::geometry::ScreenPoint;
use lens_platform_windows::process_name_at as platform_process_name_at;
use windows::core::BOOL;
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_MENU, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Set by the console handler, read by the loop. A `static` because
/// `SetConsoleCtrlHandler` takes a bare function pointer with no user data to
/// carry a reference through.
static STOP: AtomicBool = AtomicBool::new(false);

/// Ask the console to tell us about Ctrl+C instead of killing the process, so
/// the per-application summary still prints. A probe run that loses its
/// summary on exit has measured nothing.
pub fn install_stop_handler() {
    // SAFETY: `handler` is a valid `extern "system"` function that lives for
    // the whole process.
    let _ = unsafe { SetConsoleCtrlHandler(Some(handler), true) };
}

/// Whether a stop was requested.
pub fn stopping() -> bool {
    STOP.load(Ordering::Relaxed)
}

unsafe extern "system" fn handler(event: u32) -> BOOL {
    if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
        STOP.store(true, Ordering::Relaxed);
        // Handled: do not fall through to the default handler, which
        // terminates the process before the summary is written.
        return BOOL(1);
    }
    BOOL(0)
}

/// The pointer's position in physical screen pixels, or `None` when the desktop
/// will not answer, which happens on a locked or switching session.
pub fn position() -> Option<ScreenPoint> {
    let mut point = POINT::default();
    // SAFETY: `GetCursorPos` only writes into the provided POINT.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some(ScreenPoint::new(point.x, point.y))
}

fn key_down(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    // SAFETY: a read-only query of the asynchronous key state.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    // The high bit means "down now". The low bit means "pressed since the last
    // call", which would report a click the user finished a second ago.
    (state as u16 & 0x8000) != 0
}

/// A mouse button is held: the user is clicking or dragging, not reading.
pub fn any_button_down() -> bool {
    key_down(VK_LBUTTON) || key_down(VK_RBUTTON) || key_down(VK_MBUTTON)
}

/// What to call the policy modifier when telling a reader to hold it.
///
/// One name, because two places tell them: the welcome window's heading and
/// the card shown when they hover without it. A product that called the same
/// key two things would be teaching one of them wrongly.
pub const MODIFIER_NAME: &str = "Alt";

/// The policy modifier. Alt, matching the modifier-tier design.
pub fn modifier_held() -> bool {
    key_down(VK_MENU)
}

/// The executable stem owning the window under a point.
pub fn process_name_at(point: ScreenPoint) -> Option<String> {
    platform_process_name_at(point)
}
