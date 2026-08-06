//! Which window is under the pointer, and who owns it.
//!
//! Read-only queries only. Nothing here changes another application's state,
//! and nothing is invented: a window with no title yields `None` rather than a
//! placeholder, because the probe prints this as the source and an invented
//! source is exactly what a measurement instrument must not produce.

use lens_core::acquire::AppIdentity;
use lens_core::geometry::{ScreenPoint, ScreenRect};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, MAX_PATH, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClientRect, GetForegroundWindow, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SendMessageTimeoutW, WindowFromPoint,
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
///
/// # What it cost to leave to the caller
///
/// This was documented and left to whoever built the process, and the product
/// does call it. Nothing else did, and issue #40 is what that looked like: a
/// plain `EDIT` control read as a declared password field from another process
/// and as ordinary text from its own, intermittently, for two days.
///
/// The explanation is entirely here. `ProtectedHarness::start` calls this, so a
/// test that builds the harness **and** reads it is aware on both sides and
/// agrees with itself. A test that spawns a child to build the harness is aware
/// only in the child: the parent asks UI Automation for rectangles and gets
/// them divided by the scaling, so a point computed from the child's numbers
/// lands on the control underneath the intended one. On a 125% display the
/// plain field's centre landed inside the password field, which refused, 40
/// times out of 40. Nothing was wrong with the refusal.
///
/// The intermittency was test ordering: whether some earlier test in the same
/// binary had already built a harness and declared this on the parent's behalf.
///
/// So `UiaTextAcquirer::new` now calls this itself. Not as a courtesy: the
/// acquirer's whole contract is rectangles in physical screen pixels, and it
/// cannot honour that in a virtualised process. It is a precondition of the
/// type existing, not a side effect of constructing it.
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
    // SAFETY: reads the owning process id, which is a cheap lookup in the
    // window manager and opens nothing.
    let pid = unsafe { process_id_of(hwnd) }?;
    // SAFETY: `pid` names a process that owned a window a moment ago.
    unsafe { executable_stem_of(pid) }
}

/// The process that owns a window. Cheap: no handle is opened.
///
/// # Safety
/// `hwnd` must have been valid when obtained.
unsafe fn process_id_of(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: writes only into `pid`.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

/// The executable stem of a process id.
///
/// The expensive half: it opens a process handle. Everything above it exists to
/// avoid calling this forty times a second.
///
/// # Safety
/// Opens and closes a process handle.
unsafe fn executable_stem_of(pid: u32) -> Option<String> {
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
    let pid = unsafe { process_id_of(hwnd) }?;

    // Keyed on the process id, and that is what makes this a cache rather than
    // a gamble. A process's executable cannot change while it runs, so a hit
    // is not merely probably right, it is right.
    //
    // Keying on the **window handle** would have been the obvious choice and
    // would be wrong: handles are recycled, and this answer decides whether the
    // policy permits a read. A stale name on a reused handle could hand a
    // password manager the tier that belonged to a browser.
    LAST_PROCESS.with(|last| {
        if let Some((cached, name)) = last.borrow().as_ref() {
            if *cached == pid {
                return name.clone();
            }
        }
        // SAFETY: `pid` owned a window a moment ago.
        let name = unsafe { executable_stem_of(pid) };
        *last.borrow_mut() = Some((pid, name.clone()));
        name
    })
}

/// The same answer with the cache deliberately bypassed.
///
/// Only for the live check that compares the two. An absolute microsecond
/// threshold was tried first and was flaky: most of a cached call is
/// `WindowFromPoint` and `GetWindowThreadProcessId`, which happen either way,
/// so the number moves with how busy the machine is. A ratio measured under
/// the same conditions does not.
#[cfg(test)]
pub fn uncached_process_name_at(point: ScreenPoint) -> Option<String> {
    let hwnd = window_at(point)?;
    // SAFETY: `hwnd` came from `WindowFromPoint` and was checked for validity.
    unsafe { executable_stem(hwnd) }
}

thread_local! {
    /// The last process id looked up, and what it was called.
    ///
    /// One entry, because a pointer sits inside one window for long stretches
    /// and crossing between two of them costs one lookup either way. Measured
    /// on 2026-08-05: the loop resolves a name forty times a second and spent
    /// 1.56% of a core doing nothing, which on a laptop is a background tool
    /// that gets uninstalled.
    static LAST_PROCESS: std::cell::RefCell<Option<(u32, Option<String>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The scaling of the monitor a point is on, as a multiplier on 96 DPI.
///
/// Per monitor, because a desk with a 150% display beside a 100% one would
/// otherwise crop a physically different amount of screen on each.
pub fn monitor_scale_at(point: ScreenPoint) -> f32 {
    // SAFETY: read-only monitor and DPI queries; NEAREST always yields one.
    unsafe {
        let monitor = windows::Win32::Graphics::Gdi::MonitorFromPoint(
            windows::Win32::Foundation::POINT {
                x: point.x,
                y: point.y,
            },
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        );
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        if windows::Win32::UI::HiDpi::GetDpiForMonitor(
            monitor,
            windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI,
            &mut dpi_x,
            &mut dpi_y,
        )
        .is_err()
        {
            return 1.0;
        }
        dpi_x as f32 / 96.0
    }
}

/// The id of the top-level window that actually owns a screen point.
///
/// `WindowFromPoint` answers with whatever is topmost, which is the only
/// honest answer: a point over a window that another window covers belongs to
/// the one on top. Anything sampling a specific window has to check this, or
/// it silently measures the occluder and files the result under the occluded.
pub fn root_window_at(point: ScreenPoint) -> Option<i64> {
    let hwnd = window_at(point)?;
    // SAFETY: `hwnd` came from `WindowFromPoint` and was checked for validity.
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let owner = if root.is_invalid() { hwnd } else { root };
    Some(owner.0 as i64)
}

/// A visible top-level window, and where its content is.
///
/// `id` is the window handle as a plain integer. It is deliberately opaque:
/// the probe passes it back to ask for a wake and never interprets it, so
/// nothing outside this crate learns what an `HWND` is.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: i64,
    pub title: Option<String>,
    pub process_name: Option<String>,
    /// The client area in physical screen pixels: the content, without the
    /// title bar or the resize border. Sampling the whole window rectangle
    /// would spend a third of a sweep on chrome that holds no text.
    pub client_bounds: ScreenRect,
}

/// The window the user is actually looking at, if it has a client area.
///
/// Needed by the OCR probe and by nothing else so far. A capture reads pixels,
/// and the pixels inside a covered window's rectangle belong to whatever is
/// covering it, so an OCR measurement of a window that is not in front measures
/// the wrong application and reports it under the right name. Rather than
/// raising windows, which would rearrange the desktop being measured, the probe
/// measures whatever the operator has put in front.
pub fn foreground_window() -> Result<WindowInfo, ForegroundProblem> {
    // SAFETY: a read-only query for a handle, which is then described by the
    // same code that describes every other window.
    let handle = unsafe { GetForegroundWindow() };
    if handle.is_invalid() {
        // Not a rare state. Windows answers with nothing while activation is
        // moving between windows, and while a lock screen or a secure desktop
        // is up.
        return Err(ForegroundProblem::NoForegroundWindow);
    }
    // The **root**. A click can leave focus on a child, and a child is not what
    // `EnumWindows` reports, so matching the raw handle against that list finds
    // nothing and blames the window for not existing.
    // SAFETY: a read-only walk to the top of a handle the OS just gave us.
    let root = unsafe { GetAncestor(handle, GA_ROOT) };
    let wanted = if root.is_invalid() { handle } else { root };

    let windows = top_level_windows();
    if let Some(found) = windows.iter().find(|window| window.id == wanted.0 as i64) {
        return Ok(found.clone());
    }
    // It is a window and it is in front, so the reason it is not in the list is
    // one of the enumeration's own filters. Say which, because "no foreground
    // window" is a dead end and this is a measurement tool.
    // SAFETY: read-only queries on a handle obtained a moment ago.
    let bounds = unsafe { client_bounds(wanted) };
    Err(match bounds {
        Some(rect) if rect.width < 120 || rect.height < 80 => ForegroundProblem::TooSmall(rect),
        Some(_) => ForegroundProblem::NotEnumerated {
            // SAFETY: as above.
            process: unsafe { executable_stem(wanted) },
        },
        None => ForegroundProblem::NoClientArea,
    })
}

/// Why the window in front could not be described.
///
/// A typed reason rather than `None`, because this is the first thing a probe
/// run hits when it hits anything, and a tool that answers a measurement
/// request with "no" has told the operator nothing they can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForegroundProblem {
    /// Windows reported no foreground window at all.
    NoForegroundWindow,
    /// It has no client rectangle, which a full-screen exclusive surface and a
    /// few shell windows do not.
    NoClientArea,
    /// Smaller than the enumeration's floor, so it holds no readable layout.
    TooSmall(ScreenRect),
    /// It has a client area of a usable size and still is not in the list,
    /// which means it is invisible or minimised by the shell's reckoning even
    /// though it is in front. A cloaked virtual-desktop window does this.
    NotEnumerated { process: Option<String> },
}

impl ForegroundProblem {
    pub fn explain(&self) -> String {
        match self {
            Self::NoForegroundWindow => "Windows reports no foreground window. That happens \
                 while activation is moving, and while a lock screen is up. Click the window \
                 and run this again."
                .to_owned(),
            Self::NoClientArea => {
                "the window in front has no client area, which a full-screen exclusive \
                 surface does not"
                    .to_owned()
            }
            Self::TooSmall(rect) => format!(
                "the window in front is {}x{}, under the 120x80 floor for a window worth \
                 measuring",
                rect.width, rect.height,
            ),
            Self::NotEnumerated { process } => format!(
                "the window in front ({}) has a usable client area but is not in the \
                 enumeration, so the shell considers it hidden or minimised",
                process.as_deref().unwrap_or("unknown"),
            ),
        }
    }
}

/// Every visible, non-minimised top-level window with a client area.
///
/// Read-only enumeration. Nothing here activates, moves, or resizes a window:
/// a measurement tool that rearranged the desktop it was measuring would
/// change the thing it is trying to observe.
pub fn top_level_windows() -> Vec<WindowInfo> {
    let mut found: Vec<WindowInfo> = Vec::new();
    // SAFETY: `collect_window` is a valid callback for the lifetime of this
    // call, and `found` outlives the enumeration because `EnumWindows` is
    // synchronous.
    unsafe {
        let _ = EnumWindows(
            Some(collect_window),
            LPARAM(&mut found as *mut Vec<WindowInfo> as isize),
        );
    }
    found
}

/// # Safety
/// `lparam` must carry a `*mut Vec<WindowInfo>` that outlives the enumeration.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: the pointer was handed to `EnumWindows` by `top_level_windows`,
    // which owns the vector for the duration of the synchronous call.
    let found = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };

    // SAFETY: read-only queries on a handle the OS just gave us.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }
        let Some(bounds) = client_bounds(hwnd) else {
            return BOOL(1);
        };
        // A window smaller than this holds no readable layout, and sweeping it
        // would add noise rather than a row.
        if bounds.width < 120 || bounds.height < 80 {
            return BOOL(1);
        }
        found.push(WindowInfo {
            id: hwnd.0 as i64,
            title: window_title(hwnd),
            process_name: executable_stem(hwnd),
            client_bounds: bounds,
        });
    }
    BOOL(1)
}

/// The client area in screen coordinates.
///
/// # Safety
/// `hwnd` must be a window handle that was valid when obtained.
unsafe fn client_bounds(hwnd: HWND) -> Option<ScreenRect> {
    let mut rect = RECT::default();
    // SAFETY: writes only into `rect`.
    unsafe { GetClientRect(hwnd, &mut rect) }.ok()?;

    // GetClientRect answers in client coordinates, so the origin is always
    // (0, 0) and the rectangle says nothing about where the window is. Two
    // corners have to be mapped to screen space before the numbers mean
    // anything, and forgetting that yields a plausible rectangle at the top
    // left of the primary monitor.
    let mut corners = [
        POINT {
            x: rect.left,
            y: rect.top,
        },
        POINT {
            x: rect.right,
            y: rect.bottom,
        },
    ];
    for corner in &mut corners {
        // SAFETY: maps one point in place; fails only on an invalid window.
        if unsafe { ClientToScreen(hwnd, corner) }.as_bool() {
            continue;
        }
        return None;
    }

    Some(ScreenRect::from_ltrb(
        corners[0].x,
        corners[0].y,
        corners[1].x,
        corners[1].y,
    ))
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
