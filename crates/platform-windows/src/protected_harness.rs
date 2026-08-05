//! A live check that a password field is refused.
//!
//! `docs/privacy-model.md` §1 makes the strongest promise in this project:
//! **protected fields are refused before they are read**, and a field whose
//! protection cannot be proven is treated as protected. Until this file
//! existed, that path had never executed. A privacy guarantee nobody has run is
//! a comment.
//!
//! Testing it needs a real password field, which normally means a browser, a
//! login page, and a human. Instead this builds one: a Win32 window owning two
//! `EDIT` controls, identical except that the second carries `ES_PASSWORD`. The
//! acquirer is then pointed at each.
//!
//! It doubles as the Win32 `EDIT` case the compatibility matrix wants. Mori
//! measured that plain `EDIT` controls expose no text pattern at all, and the
//! matrix has had no genuine Win32 application to check that against.
//!
//! # Why the window lives on its own thread
//!
//! UI Automation queries a provider by sending messages to the thread that owns
//! the window. Creating the window and calling `ElementFromPoint` on the same
//! thread deadlocks: the call waits for a reply from a thread that is inside
//! the call. So the window gets a thread with a message pump, and the acquirer
//! runs on the test's thread.

use std::sync::mpsc;

use lens_core::geometry::{ScreenPoint, ScreenRect};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, PostMessageW, PostThreadMessageW, RegisterClassW, SetForegroundWindow,
    SetWindowPos, TranslateMessage, HWND_TOPMOST, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_QUIT, WNDCLASSW, WS_CHILD, WS_EX_TOPMOST,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

/// The message that makes the window's thread stop dispatching.
///
/// In the `WM_APP` range, which Windows reserves for exactly this: private
/// messages an application defines for itself.
const WM_STALL: u32 = WM_APP + 1;

/// What both `EDIT` controls contain.
///
/// Latin and unambiguous: a recogniser check asserts exact words, and this is
/// the text it asserts. Nothing about it is a user's.
pub const HARNESS_TEXT: PCWSTR = w!("The output files came back empty");

/// `ES_PASSWORD`. Not exposed as a constant by the binding, and the raw value
/// is stable since Windows 3.
const ES_PASSWORD: u32 = 0x0020;
/// `ES_AUTOHSCROLL`, so typing past the edge does not resize anything.
const ES_AUTOHSCROLL: u32 = 0x0080;

/// Where the harness put its two fields, in screen coordinates.
#[derive(Debug, Clone, Copy)]
pub struct HarnessFields {
    pub plain: ScreenRect,
    pub password: ScreenRect,
}

/// Whether the harness's own window still owns a point.
///
/// A test must be able to tell "the guarantee failed" from "another window was
/// on top of my field". Without this check the second reads exactly like the
/// first, and it produced a false privacy alarm twice while this harness was
/// being written.
pub fn harness_owns(window_id: i64, point: ScreenPoint) -> bool {
    crate::window::root_window_at(point) == Some(window_id)
}

/// Only one harness window at a time.
///
/// The desktop is shared state and `cargo test` runs tests in parallel by
/// default, so two harnesses cascade their windows and one covers the other's
/// fields. `ElementFromPoint` then answers with the window on top, and the read
/// comes back `ElementGone`, which reads exactly like the privacy guarantee
/// having failed. It cost a debugging round to find, so the lock is here rather
/// than a note asking for `--test-threads=1`.
static DESKTOP: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A window with a plain and a password `EDIT`, owned by its own thread.
pub struct ProtectedHarness {
    fields: HarnessFields,
    window_id: i64,
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
    /// Held for the harness's lifetime so no second window appears over it.
    _desktop: std::sync::MutexGuard<'static, ()>,
}

impl ProtectedHarness {
    /// Build the window and wait until it is on screen.
    pub fn start() -> Option<Self> {
        // A poisoned lock means an earlier harness panicked. The window is
        // gone either way, so the desktop is usable and the guard is taken.
        let desktop = DESKTOP
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Before any window exists and before any coordinate is computed.
        //
        // Without it this process is DPI-unaware, so Windows virtualises its
        // coordinates: `ClientToScreen` answers in logical units while UI
        // Automation expects physical ones. On a 125% display the two disagree
        // by a quarter, the point lands outside the control, and the read comes
        // back `ElementGone`. That is exactly what the first run of this
        // harness did, and it looked like the privacy guarantee had failed.
        crate::window::declare_dpi_awareness();

        let (ready, ready_rx) = mpsc::channel::<Option<(HarnessFields, i64, u32)>>();

        let join = std::thread::spawn(move || {
            // SAFETY: every call below is a documented window API on handles
            // this thread owns, and the window is destroyed before returning.
            unsafe {
                let Some((window, fields)) = create_window() else {
                    let _ = ready.send(None);
                    return;
                };
                // Bring it up before anyone reads it. The window is this
                // harness's own, so raising it rearranges nothing that belongs
                // to the user, and a covered window is read as whatever sits
                // on top of it.
                let _ = SetForegroundWindow(window);
                let _ = BringWindowToTop(window);
                let _ = ready.send(Some((fields, window.0 as i64, GetCurrentThreadId())));

                // Pump until someone posts WM_QUIT to this thread. Without a
                // pump the provider never answers and every read times out.
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = DestroyWindow(window);
            }
        });

        let (fields, window_id, thread_id) = ready_rx.recv().ok()??;
        // The window is created but the provider may not have settled. This is
        // a harness, so a fixed pause is honest and cheaper than polling for a
        // condition UI Automation does not expose.
        std::thread::sleep(std::time::Duration::from_millis(250));

        // And it may not be on top. The desktop is shared with whatever else is
        // running, and a window that appears in the meantime makes every check
        // built on this harness read as a failed guarantee: about one full
        // suite in five, `a_password_field_is_refused` reported that a password
        // field had not been refused when it had never been reached.
        //
        // Raising it is not a workaround for a product defect. The harness owns
        // this window and putting it back on top is what `start` promised.
        raise_until_owned(window_id, fields);

        Some(Self {
            fields,
            window_id,
            thread_id,
            join: Some(join),
            _desktop: desktop,
        })
    }

    pub fn fields(&self) -> HarnessFields {
        self.fields
    }

    pub fn window_id(&self) -> i64 {
        self.window_id
    }

    /// Stop answering for a while, the way a wedged application does.
    ///
    /// The pump is what UI Automation talks to: a provider is queried by
    /// sending messages to the thread that owns the window, so a thread that
    /// is not dispatching is a provider that never replies. This posts a
    /// message whose handler sleeps, which blocks the pump for the duration
    /// without touching anything else about the window.
    ///
    /// Returns immediately. The stall runs on the window's thread.
    ///
    /// Nothing in the product does this. It exists because
    /// `AcquireError::Timeout` was in the taxonomy from the start and no code
    /// path had ever produced one, so the guard against a 45-second `acquire`
    /// (issue #28) could not be shown to work.
    pub fn stall_for(&self, duration: std::time::Duration) {
        let millis = usize::try_from(duration.as_millis()).unwrap_or(usize::MAX);
        // SAFETY: posting a documented message to a window this harness owns.
        // Posted rather than sent, so this thread does not wait on the sleep
        // it is asking for.
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.window_id as *mut _)),
                WM_STALL,
                WPARAM(millis),
                LPARAM(0),
            );
        }
    }
}

/// Put the harness window back on top until it owns its own fields.
///
/// Gives up after a few attempts rather than looping: if something else is
/// determinedly topmost, a check that says so is worth more than one that
/// waits forever.
fn raise_until_owned(window_id: i64, fields: HarnessFields) {
    for _ in 0..6 {
        if harness_owns(window_id, centre(fields.plain))
            && harness_owns(window_id, centre(fields.password))
        {
            return;
        }
        // SAFETY: a documented z-order change on a window this harness owns.
        // NOACTIVATE, so raising it does not take focus from the user.
        unsafe {
            let _ = SetWindowPos(
                HWND(window_id as *mut _),
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

impl Drop for ProtectedHarness {
    fn drop(&mut self) {
        // SAFETY: posting a documented message to a thread this harness owns.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// # Safety
/// Must run on the thread that will pump messages for the returned window.
unsafe fn create_window() -> Option<(HWND, HarnessFields)> {
    // SAFETY: a documented class registration; a duplicate registration fails
    // harmlessly and the subsequent create still succeeds.
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            lpszClassName: w!("MoriLensProtectedHarness"),
            ..Default::default()
        };
        RegisterClassW(&class);

        let window = CreateWindowExW(
            // Topmost, not foreground. Windows refuses to let a background
            // console process steal activation, so SetForegroundWindow is
            // ignored and the harness lands under whatever was already there.
            // Topmost is a z-order property and needs no activation, so the
            // fields are readable without taking focus from the user.
            WS_EX_TOPMOST,
            w!("MoriLensProtectedHarness"),
            w!("Mori Lens protected-field harness"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            // A fixed position rather than CW_USEDEFAULT: the default cascades
            // each new window a little further down, so successive runs
            // measured different parts of the desktop and were not comparable.
            80,
            80,
            420,
            200,
            None,
            None,
            None,
            None,
        )
        .ok()?;

        let plain = edit_control(window, 20, 30, 0)?;
        let password = edit_control(window, 20, 90, ES_PASSWORD)?;

        Some((
            window,
            HarnessFields {
                plain: to_screen(window, plain),
                password: to_screen(window, password),
            },
        ))
    }
}

/// One `EDIT` control at a client-relative position, returning its client rect.
///
/// # Safety
/// `parent` must be a valid window owned by the calling thread.
unsafe fn edit_control(parent: HWND, x: i32, y: i32, extra: u32) -> Option<ScreenRect> {
    const WIDTH: i32 = 360;
    const HEIGHT: i32 = 40;
    // SAFETY: a documented control creation on a window this thread owns.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            // Text, so this window is something a recogniser can be pointed
            // at. It is opaque and at a fixed position, which the card is not:
            // a translucent card captured with the desktop showing through it
            // reads as a mixture of both, which is what the first attempt at
            // an OCR check found.
            //
            // A password field shows dots whatever it is given, which is
            // itself worth having on screen.
            HARNESS_TEXT,
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | ES_AUTOHSCROLL | extra),
            x,
            y,
            WIDTH,
            HEIGHT,
            Some(parent),
            None,
            None,
            None,
        )
        .ok()?;
    }
    Some(ScreenRect::new(x, y, WIDTH, HEIGHT))
}

/// Map a client-relative rectangle onto the screen.
///
/// # Safety
/// `parent` must be a valid window.
unsafe fn to_screen(parent: HWND, rect: ScreenRect) -> ScreenRect {
    let mut origin = windows::Win32::Foundation::POINT {
        x: rect.x,
        y: rect.y,
    };
    // SAFETY: maps one point in place.
    unsafe {
        let _ = ClientToScreen(parent, &mut origin);
    }
    ScreenRect::new(origin.x, origin.y, rect.width, rect.height)
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_STALL {
        // Sleeping inside a window procedure is exactly the thing a real
        // application must never do, which is why it models one that has.
        std::thread::sleep(std::time::Duration::from_millis(wparam.0 as u64));
        return LRESULT(0);
    }
    // SAFETY: the documented default handler for every message not handled here.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// A point in the middle of a rectangle, which is where a user would point.
pub fn centre(rect: ScreenRect) -> ScreenPoint {
    rect.center()
}
