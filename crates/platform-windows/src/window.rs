//! Which window is under the pointer, and who owns it.
//!
//! Read-only queries only. Nothing here changes another application's state,
//! and nothing is invented: a window with no title yields `None` rather than a
//! placeholder, because the probe prints this as the source and an invented
//! source is exactly what a measurement instrument must not produce.

use lens_core::acquire::AppIdentity;
use lens_core::geometry::ScreenPoint;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, MAX_PATH, POINT, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetWindowTextW, GetWindowThreadProcessId, SendMessageTimeoutW, WindowFromPoint,
    GA_ROOT, OBJID_CLIENT, SMTO_ABORTIFHUNG, WM_GETOBJECT,
};

/// Opt into per-monitor DPI awareness v2.
///
/// Called once, as early as possible. Without it Windows virtualises
/// coordinates for the process and every rectangle UI Automation returns is
/// silently wrong on any monitor that is not at 100 percent scaling. The
/// numbers stay plausible, which is what makes it expensive to find later.
///
/// Failure is not fatal: the context can already be set by a manifest, and on
/// a single 100 percent display nothing changes either way.
pub fn declare_dpi_awareness() -> bool {
    // SAFETY: a process-wide, idempotent opt-in with no pointer arguments.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() }
}

/// The window under a screen point, or `None` when the point is over nothing.
pub fn window_at(point: ScreenPoint) -> Option<HWND> {
    // SAFETY: a read-only hit test that returns a null handle for no window.
    let hwnd = unsafe {
        WindowFromPoint(POINT {
            x: point.x,
            y: point.y,
        })
    };
    (!hwnd.is_invalid()).then_some(hwnd)
}

/// The owning executable and the top-level window title.
pub fn identity(hwnd: Option<HWND>) -> AppIdentity {
    let Some(hwnd) = hwnd else {
        return AppIdentity::default();
    };

    // SAFETY: `hwnd` came from `WindowFromPoint` and was checked for validity.
    // A window can close between that call and this one, in which case each of
    // these fails and yields `None`, which is the correct answer.
    unsafe {
        // The title lives on the top-level window; the point is usually over a
        // child control that has no title of its own.
        let root = GetAncestor(hwnd, GA_ROOT);
        let titled = if root.is_invalid() { hwnd } else { root };

        AppIdentity {
            process_name: executable_stem(hwnd),
            window_title: window_title(titled),
        }
    }
}

/// # Safety
/// `hwnd` must be a window handle that was valid when obtained.
unsafe fn window_title(hwnd: HWND) -> Option<String> {
    let mut buffer = [0u16; 512];
    // SAFETY: `GetWindowTextW` writes at most `buffer.len()` UTF-16 units.
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if length <= 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer[..length as usize]);
    let trimmed = text.trim().to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// The owning executable's file stem, for example `chrome`.
///
/// # Safety
/// `hwnd` must be a window handle that was valid when obtained.
unsafe fn executable_stem(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: writes only into `pid`.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    // LIMITED_INFORMATION is the least privilege that still resolves a path, so
    // Lens never opens another process with more rights than it needs.
    // SAFETY: the handle is closed on every path below.
    let handle: HANDLE =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buffer = [0u16; MAX_PATH as usize];
    let mut size = buffer.len() as u32;
    // SAFETY: `size` is the true capacity of `buffer` and is updated in place.
    let query = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    // SAFETY: `handle` came from a successful `OpenProcess` and is not used again.
    let _ = unsafe { CloseHandle(handle) };
    query.ok()?;

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    std::path::Path::new(&path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
}

/// Just the executable stem owning the window under a point.
///
/// Separate from [`identity`] because this runs on every pointer sample, tens
/// of times a second, to decide whether the policy even permits a look. The
/// window title is not read here: it is the expensive half and nothing needs it
/// until a hover has settled.
pub fn process_name_at(point: ScreenPoint) -> Option<String> {
    let hwnd = window_at(point)?;
    // SAFETY: `hwnd` came from `WindowFromPoint` and was checked for validity.
    unsafe { executable_stem(hwnd) }
}

/// How long to wait for a foreign window to answer the wake message.
const WAKE_TIMEOUT_MS: u32 = 200;

/// Ask a Chromium-family window to build its accessibility tree.
///
/// Chromium, and therefore Electron, every Chromium browser, VS Code, and Slack,
/// constructs no accessibility tree until it believes an assistive client is
/// present. A cold read returns a stub with no text pattern. That is not a
/// failed read: the tree does not exist yet. The documented trigger is a
/// `WM_GETOBJECT` for `OBJID_CLIENT`
/// (`docs/research/inherited-from-mori.md` §2).
///
/// Sent with a timeout and `SMTO_ABORTIFHUNG` because this is a synchronous
/// message into a foreign, possibly wedged, UI thread. Lens must not stall
/// because the application under the pointer is busy.
///
/// The caller is responsible for sending this **once per window**, not once per
/// hover: Mori pays this cost per keypress, and Lens would pay it per glance.
pub fn wake_accessibility(hwnd: HWND) {
    if hwnd.is_invalid() {
        return;
    }
    // SAFETY: a documented message send to a valid window, bounded by a timeout.
    unsafe {
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_GETOBJECT,
            WPARAM(0),
            LPARAM(OBJID_CLIENT.0 as isize),
            SMTO_ABORTIFHUNG,
            WAKE_TIMEOUT_MS,
            None,
        );
    }
}

/// A stable key for "have we already woken this window".
pub fn window_key(hwnd: HWND) -> isize {
    hwnd.0 as isize
}
