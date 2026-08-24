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
use std::time::Duration;

use crate::glass::Glass;
use lens_core::card::{Card, CardBody};
use lens_core::geometry::ScreenPoint;
use lens_core::geometry::ScreenRect;
use lens_core::theme::Theme;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, GetDC, GetDeviceCaps,
    GetMonitorInfoW, InvalidateRect, MonitorFromPoint, ReleaseDC, SelectObject, SetBkMode,
    SetTextColor, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CALCRECT, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_SINGLELINE, DT_WORDBREAK, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, HBRUSH, HFONT,
    LOGPIXELSY, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
    HWND_TOPMOST, LWA_ALPHA, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE,
    SW_SHOWNOACTIVATE, WM_PAINT, WM_QUIT, WM_USER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// Card geometry, in physical pixels at 100%. Deliberately fixed for now: a
/// card that resizes to its content on every hover flickers, and the sizing
/// rules are a design question rather than a plumbing one.
/// The widest a card may be, in pixels at 100% scaling.
///
/// Scaled per monitor like everything else on the card. It was not, and the
/// result was a card whose text grew with the display while its box did not:
/// on a 150% screen the same sentence wrapped onto more lines inside the same
/// number of pixels, so the card was physically narrower and taller than the
/// one beside it on a 100% screen.
const CARD_WIDTH: i32 = 300;

/// Space under the original, before the answer.
///
/// **The largest gap on the card**, and it was the smaller of the two until
/// 2026-08-11. This is the one boundary a reader crosses: above it is what they
/// pointed at, below it is what this product made of it. The footer is a line
/// most readers never reach.
///
/// A hairline rule was drawn here first and taken out again. `glass.rs` already
/// records what happens when a card says one thing three ways, and space
/// separates as well as a line does while adding no ink.
///
/// Shared with [`measure`] rather than written twice. The two disagreed once
/// already, over a footer measured in points and summed with pixels.
const GAP_AFTER_ORIGINAL: i32 = 13;
/// Space under the answer, before the footer.
const GAP_BEFORE_FOOTER: i32 = 10;
/// Space between two rows of an analysis.
///
/// Much smaller than either of the others, because those separate kinds of
/// thing and this separates items of one kind. A list whose rows are as far
/// apart as its sections has stopped looking like a list.
const GAP_BETWEEN_ROWS: i32 = 3;
/// Space between an analysis label and the value beside it.
const LABEL_GUTTER: i32 = 7;
/// How much of the work area a card may take, at most.
///
/// **A share of the screen rather than a constant, and that took two goes.**
/// It was 420 physical pixels, then 420 scaled per monitor, and both are
/// numbers about a card rather than about the screen it is on.
///
/// Nothing clamps the drawing, so a card over the ceiling is not shortened, it
/// is **clipped**: the last lines of the answer vanish with nothing to say so.
/// Measured on a text with no sentence boundary in it, which is what the
/// segmenter returns for KakaoTalk's single region (issue #34): 182 pixels lost
/// at 100% and 273 at 150%.
///
/// Two thirds of the work area holds both of those whole on any ordinary
/// display, and still leaves a card that cannot cover the screen. A card this
/// tall means a reader hovered a text this long, and showing them most of it is
/// not better than showing them all of it.
const MOST_OF_THE_SCREEN: f32 = 0.66;

/// `DWMWCP_ROUND`. Windows 11 rounded corners.
const DWMWCP_ROUND: i32 = 2;

/// Posted to the overlay thread when the content has changed.
const WM_LENS_REFRESH: u32 = WM_USER + 1;

/// The card being drawn. One overlay, so one slot.
///
/// A mutex rather than a channel payload because `WM_PAINT` arrives on the
/// overlay thread at times nobody chose, and the paint handler needs whatever
/// is current then.
static CONTENT: Mutex<Option<Card>> = Mutex::new(None);

/// The theme the painter reads. Set before the window exists so the first
/// frame is already correct rather than a default that flashes.
static THEME: Mutex<Option<Theme>> = Mutex::new(None);

/// Where the card is on screen, or `None` when nothing is shown.
///
/// Readable from any thread, which is the point: the OCR acquirer runs on the
/// acquisition thread and has to know whether the band it is about to
/// photograph has the card in it. Without that it hides the card before every
/// capture, and with automatic OCR that is every hover the accessibility path
/// could not serve. A reader nudging the pointer twenty pixels off a sentence
/// would watch the answer they were reading disappear, which is exactly what
/// `KEEP_CARD_WITHIN_PX` exists to prevent.
static PLACED: Mutex<Option<ScreenRect>> = Mutex::new(None);

/// Where the card is on screen right now, if it is.
///
/// The hover loop needs it so a pointer moved onto a card to read it does not
/// count as having left the text the card explains.
pub fn card_rect() -> Option<ScreenRect> {
    PLACED.lock().ok().and_then(|slot| *slot)
}

/// Remember, or forget, where the card is.
fn remember_placement(placed: Option<ScreenRect>) {
    if let Ok(mut slot) = PLACED.lock() {
        *slot = placed;
    }
}

/// The theme to paint with, or a readable fallback.
///
/// A fallback rather than a panic: a card that renders in the wrong colours is
/// a cosmetic fault, and a translation that never appears because a lock was
/// poisoned is a product one.
fn current_theme() -> Theme {
    THEME.lock().ok().and_then(|slot| *slot).unwrap_or_else(|| {
        Theme::resolve(
            lens_core::theme::Preset::System,
            lens_core::theme::Translucency::DEFAULT,
            lens_core::theme::TextScale::Normal,
            lens_core::theme::SystemPreferences::default(),
        )
    })
}

/// Pack an `Rgb` into the `COLORREF` byte order, which is BGR rather than RGB.
///
/// Named rather than inlined because getting it backwards produces colours
/// that look deliberate: a warm sage theme renders as cold blue and reads as a
/// design choice somebody made.
pub(crate) fn colorref(colour: lens_core::theme::Rgb) -> COLORREF {
    COLORREF(u32::from(colour.r) | (u32::from(colour.g) << 8) | (u32::from(colour.b) << 16))
}

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
        Self::start_with(current_theme())
    }

    /// Create the window with a chosen theme.
    pub fn start_with(theme: Theme) -> Option<Self> {
        if let Ok(mut slot) = THEME.lock() {
            *slot = Some(theme);
        }
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

    /// Paint with a different theme from now on.
    ///
    /// The card resolves its theme when it draws, so this takes effect on the
    /// next one. Without it the theme was decided at startup, and switching
    /// Windows to dark mode left a pale card sitting over dark windows until
    /// Lens was restarted.
    pub fn set_theme(&self, theme: Theme) {
        if let Ok(mut slot) = THEME.lock() {
            *slot = Some(theme);
        }
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

    /// A way to hide the card from somewhere that cannot hold this.
    ///
    /// The OCR acquirer has to take the card down before it photographs the
    /// screen, or Lens reads its own translation. It lives on the acquisition
    /// thread and this lives on the hover loop's, so what crosses is a request
    /// rather than a handle: a `Sender` and a thread id, both of which are
    /// `Send`, and neither of which is a window.
    pub fn hider(&self) -> impl Fn(ScreenRect) + Send + 'static {
        let commands = self.commands.clone();
        let thread_id = self.thread_id;
        move |band: ScreenRect| {
            // **Only when the card is in the way.** Hiding it unconditionally
            // costs nothing when nothing is shown and costs the reader their
            // answer when something is: with automatic OCR this runs on every
            // hover accessibility could not serve, which over a margin is most
            // of them.
            let in_the_way = PLACED
                .lock()
                .ok()
                .and_then(|slot| *slot)
                .is_some_and(|card| card.overlaps(band));
            if !in_the_way {
                return;
            }
            if commands.send(Command::Hide).is_ok() {
                // SAFETY: posting a documented message to a thread the overlay
                // owns, exactly as `wake` does.
                unsafe {
                    let _ = PostThreadMessageW(thread_id, WM_LENS_REFRESH, WPARAM(0), LPARAM(0));
                }
            }
        }
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
            //   NOACTIVATE   the card never takes focus from what is being typed
            //   TOOLWINDOW   it is not a task-switcher entry
            //   TOPMOST      it sits above the application it is explaining
            //   LAYERED      it can be drawn with alpha
            //   TRANSPARENT  clicks reach the application underneath
            //
            // TRANSPARENT is what ADR-0004 calls the passive tooltip ignoring
            // mouse events, and it was missing: the card silently swallowed
            // every click on the text it was explaining. It is correct only
            // while the card is passive. The interactive card that ADR-0004
            // also describes needs its own window without this flag, because a
            // button inside a click-through window cannot be clicked.
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT,
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

        // Note what is *not* here: `SetLayeredWindowAttributes`. It and
        // `UpdateLayeredWindow` are two mutually exclusive ways of using
        // `WS_EX_LAYERED`, and a window that has had the first called on it
        // makes the second fail with an invalid parameter. Setting it here
        // "just in case" is what made the Direct2D path silently fall back to
        // GDI on its first run, with every other overlay test still green
        // because they cannot tell the two apart.
        //
        // The fallback sets it in `pump`, once, and only when there is no
        // Direct2D.

        // Only for the fallback. The Direct2D card draws its own rounded
        // outline into the alpha channel, which is sharper than the system's
        // and is the same shape on every build.
        //
        // Failure is ignored on purpose: this is Windows 11 build 22000 and
        // later, and a square card on an older build is not worth a branch.
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner).cast(),
            std::mem::size_of::<i32>() as u32,
        );

        Some(window)
    }
}

/// # Safety
/// `window` must be owned by this thread.
unsafe fn pump(window: HWND, commands: &mpsc::Receiver<Command>) {
    // Created here rather than passed in: Direct2D's single-threaded factory
    // belongs to the thread that made it, and that is this one.
    //
    // `None` is not an error state. A machine where Direct2D cannot start keeps
    // the GDI painter, because a plain card is a product and no card is not.
    let glass = Glass::new();
    let mut shown = Shown::default();
    if glass.is_none() {
        // SAFETY: a documented call on this thread's own window. Only on this
        // path: see `create_window`.
        unsafe {
            let theme = current_theme();
            let _ = SetLayeredWindowAttributes(window, COLORREF(0), theme.surface_alpha, LWA_ALPHA);
        }
    }
    let mut message = MSG::default();
    // SAFETY: the standard message loop for a window owned by this thread.
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            // Commands are drained on every wake rather than handled as
            // messages, so a burst of pointer movement collapses into the last
            // instruction instead of queueing a card per sample.
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Show(card, anchor) => {
                        show_card(window, *card, anchor, glass.as_ref(), &mut shown);
                    }
                    Command::Hide => {
                        let _ = ShowWindow(window, SW_HIDE);
                        // The next card is a new hover and places itself
                        // afresh, wherever the reader has moved to.
                        shown = Shown::default();
                        remember_placement(None);
                    }
                }
            }
            DispatchMessageW(&message);
        }
    }
}

/// # Safety
/// `window` must be owned by this thread.
/// The card currently on screen, so a streamed answer does not move it.
///
/// A translation arrives a few words at a time. Each partial is a whole card,
/// and a card fitted to each of those changes width several times a second;
/// placement derives its x from the width, so it slides sideways as well. The
/// first screenshot of a real hover showed the same card in three places.
///
/// So the geometry is decided once per hover and held: the width is pinned and
/// the height only grows. A card that gets taller as an answer arrives reads as
/// the answer arriving. One that moves reads as a fault.
#[derive(Debug, Default)]
struct Shown {
    anchor: Option<ScreenRect>,
    placed: Option<ScreenRect>,
}

/// # Safety
/// `window` must be owned by this thread.
unsafe fn show_card(
    window: HWND,
    card: Card,
    anchor: ScreenRect,
    glass: Option<&Glass>,
    shown: &mut Shown,
) {
    let theme = current_theme();

    // Laid out once, by whichever renderer is going to draw it. Measuring with
    // one engine and drawing with another is how a card ends up with its last
    // line clipped.
    // SAFETY: read-only monitor query.
    let scale = unsafe { scale_for(anchor.center()) };
    let widest = (CARD_WIDTH as f32 * scale).round() as i32;
    // Still the same hover, so the card keeps the width it already has.
    let holding = (shown.anchor == Some(anchor))
        .then_some(shown.placed)
        .flatten();
    let range = match holding {
        Some(previous) => (previous.width, previous.width),
        None => (0, widest),
    };
    // SAFETY: read-only monitor queries.
    let ceiling = card_height_ceiling(unsafe { work_area_for(anchor.center()) });

    // **Laid out, and cut to fit if it does not.** A card taller than its
    // ceiling is clipped by the window, and what disappears is the end of the
    // answer with nothing to say so. Cutting the answer and marking it is the
    // same loss with the reader told about it, and the original stays whole
    // because that is how they check what they are being told.
    //
    // Three times at most. Halving reaches an answer not worth showing in
    // three rounds, and a loop that measured a layout on a hover path more
    // often than that would be paying for a rare case on every ordinary one.
    let mut card = card;
    let mut laid = glass.and_then(|glass| glass.lay_out_between(&card, &theme, range, scale));
    for _ in 0..3 {
        if laid.as_ref().is_none_or(|laid| laid.height <= ceiling) {
            break;
        }
        let answer = match &card.body {
            CardBody::Translation(text) => text.chars().count(),
            _ => break,
        };
        // The share that fitted, less a little, because the cut adds a marker
        // and the lines do not divide evenly.
        // **Halved, not estimated.** The first version scaled the answer by
        // the share of the ceiling the card overflowed, which assumes the
        // height is proportional to the answer's length. It is not: the
        // original is a large fixed part of it, so that estimate converged at
        // 903, 765, 709 against a ceiling of 634 and would have taken a dozen
        // rounds. Halving reaches the floor in three, and the floor is a
        // refusal rather than a clip.
        let keep = answer / 2;
        let shorter = card.clone().with_the_answer_cut_to(keep);
        if shorter.body == card.body {
            // The answer is as short as it usefully gets and the card still
            // does not fit, which means the **original** is what does not fit.
            // Clipping now would take the answer off the bottom and leave the
            // reader their own source text, and cutting the original is the
            // one thing `card.rs` forbids. So neither.
            card = Card::too_long_to_show();
            laid = glass.and_then(|glass| glass.lay_out_between(&card, &theme, range, scale));
            break;
        }
        card = shorter;
        laid = glass.and_then(|glass| glass.lay_out_between(&card, &theme, range, scale));
    }

    // The card is as wide and as tall as its contents need, up to the limit. A
    // fixed box holding one short line is mostly empty, which reads as
    // something that failed to load rather than as an answer.
    let (width, height) = match &laid {
        // **No floor.** There was one, at 80 physical pixels, and a card that
        // laid out shorter than that got a window taller than its contents:
        // empty space along the bottom edge of exactly the cards that had least
        // in them. The renderer measures what it drew, and that measurement is
        // the answer.
        Some(laid) => (laid.width, laid.height.min(ceiling)),
        // The fallback painter has no way to measure a natural width, so it
        // keeps the full one. A machine without Direct2D gets the old card, and
        // the floor with it: `measure` is a GDI estimate rather than a layout,
        // and it is the one caller that can come back with too small a number.
        // SAFETY: measuring against the window's own device context.
        None => (widest, unsafe { measure(window, &card) }.clamp(80, ceiling)),
    };

    if let Ok(mut slot) = CONTENT.lock() {
        *slot = Some(card);
    }

    let mut placed = lens_core::card::place_card(
        anchor,
        anchor.center(),
        (width, height),
        // The monitor the text is on, not the primary one.
        // SAFETY: read-only monitor queries.
        unsafe { work_area_for(anchor.center()) },
    );

    // A card arriving, rather than one growing. A streamed answer calls this
    // several times a second and only the first of them is an entrance: the
    // rest would be a card that flashes once per word.
    let arriving = holding.is_none();

    // Same hover: keep where it already is, and only let it grow downwards.
    // Re-placing on every partial is what put the same card in three places.
    if let Some(previous) = holding {
        placed = ScreenRect::new(
            previous.x,
            previous.y,
            previous.width,
            placed.height.max(previous.height),
        );
    }
    shown.anchor = Some(anchor);
    shown.placed = Some(placed);
    remember_placement(Some(placed));

    // The Direct2D path positions, sizes and paints in one call, so none of
    // the GDI dance below applies to it.
    if let (Some(glass), Some(laid)) = (glass, laid) {
        let drawn = if arriving && theme.animate {
            // SAFETY: the window is this thread's own and is WS_EX_LAYERED.
            unsafe { rise(glass, window, &laid, &theme, placed) }
        } else {
            // SAFETY: as above.
            unsafe { glass.present(window, &laid, &theme, placed) }
        };
        if drawn {
            // SAFETY: NOACTIVATE is what keeps focus where it was.
            unsafe {
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    window,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                );
            }
            return;
        }
        // Fell through: the frame was not drawn, so the GDI painter below
        // still has a card to show rather than the user seeing nothing.
    }

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

/// The monitor's scaling at a point, as a multiplier on 96 DPI.
///
/// Per monitor, for the same reason the work area is: a desk with a 150%
/// display beside a 100% one would otherwise get one physical card size on one
/// screen and a different one on the other.
///
/// # Safety
/// Read-only monitor and DPI queries.
pub(crate) unsafe fn scale_for(point: ScreenPoint) -> f32 {
    // SAFETY: `MONITOR_DEFAULTTONEAREST` always yields a monitor.
    unsafe {
        let monitor = MonitorFromPoint(
            windows::Win32::Foundation::POINT {
                x: point.x,
                y: point.y,
            },
            MONITOR_DEFAULTTONEAREST,
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

/// The work area of the monitor a point is on, excluding the taskbar.
///
/// **Per monitor, not `SPI_GETWORKAREA`.** That call answers for the primary
/// monitor only, and `place_card` clamps into whatever it is given, so a card
/// anchored on a second screen was pulled onto the first. Measured on a
/// two-monitor desk: an anchor at x=2500 was placed at x=1116, about 1,400px
/// away on the wrong display. The `place_card` unit tests never caught it
/// because they are handed a correct work area; only the runtime was wrong.
///
/// # Safety
/// Writes only into local structures.
unsafe fn work_area_for(point: ScreenPoint) -> ScreenRect {
    // SAFETY: read-only monitor queries; `MONITOR_DEFAULTTONEAREST` always
    // yields a monitor, so the handle is valid.
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: point.x,
                y: point.y,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            return ScreenRect::from_ltrb(
                info.rcWork.left,
                info.rcWork.top,
                info.rcWork.right,
                info.rcWork.bottom,
            );
        }
    }
    // Nothing plausible to invent here, so give the card room rather than
    // clamping it somewhere confidently wrong.
    ScreenRect::new(point.x - 960, point.y - 540, 1920, 1080)
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
        let theme = current_theme();
        let inner = CARD_WIDTH - theme.padding * 2;
        let body = match &card.body {
            CardBody::Translation(text) | CardBody::Problem(text) => text,
            CardBody::Waiting(status) => status.as_str(),
            // Measured by the function that draws it, below.
            CardBody::Analysis { .. } => "",
        };
        let original = text_height(dc, &card.original, inner, theme.small_pt);
        // **Only when there is one to separate from.** `paint` skips the gap
        // for a card with no original and this added it anyway, so every
        // refusal card was measured taller than it drew and carried a strip of
        // empty surface along its bottom edge.
        let after_original = if card.original.trim().is_empty() {
            0
        } else {
            GAP_AFTER_ORIGINAL
        };
        // **The same function that draws it, asked not to draw.** `measure` and
        // `paint` each had their own arithmetic once and disagreed, so the one
        // body with a layout of its own gets one implementation rather than a
        // matching pair.
        let translated = match &card.body {
            CardBody::Analysis {
                translation, rows, ..
            } => analysis(dc, translation.as_deref(), rows, 0, 0, inner, &theme, false),
            _ => text_height(dc, body, inner, theme.body_pt),
        };
        // Measured like the others rather than guessed from the point size.
        // The previous `small_pt + 4` mixed points into a sum of pixels, so
        // the card was the wrong height by a different amount on each monitor.
        let footer = text_height(dc, &card.footer, inner, theme.small_pt);
        ReleaseDC(Some(window), dc);
        theme.padding * 2 + original + after_original + translated + GAP_BEFORE_FOOTER + footer
    }
}

/// # Safety
/// `dc` must be a valid device context.
unsafe fn text_height(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &str,
    width: i32,
    point_size: i32,
) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: creates a font this function owns and deletes below.
    let font = unsafe { font(logical_height(dc, point_size), false) };
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

/// Convert a point size to the logical height `CreateFontW` expects.
///
/// `CreateFontW`'s height is in **logical units**, so passing a point size
/// straight in produces text whose size depends on the display's density.
/// Microsoft's own formula is `-MulDiv(points, LOGPIXELSY, 72)`, and the DC
/// here belongs to the card's window, so on a per-monitor-aware process it
/// carries the DPI of the monitor the card is on.
///
/// Before this, a `19` labelled as points was 19 logical units, which on this
/// desk meant different text on the 125% monitor and the 100% one.
///
/// # Safety
/// `dc` must be a valid device context.
pub(crate) unsafe fn logical_height(dc: windows::Win32::Graphics::Gdi::HDC, points: i32) -> i32 {
    // SAFETY: a read-only capability query.
    let dpi = unsafe { GetDeviceCaps(Some(dc), LOGPIXELSY) };
    let dpi = if dpi > 0 { dpi } else { 96 };
    -((points * dpi) / 72)
}

/// # Safety
/// The returned font must be deleted by the caller.
pub(crate) unsafe fn font(height: i32, semibold: bool) -> HFONT {
    // Malgun Gothic carries Hangul; Segoe UI does not. A card whose target
    // language is Korean rendered in a Latin-only face shows boxes.
    // SAFETY: a documented font creation with a static face name.
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            if semibold {
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

        let theme = current_theme();
        let background: HBRUSH = CreateSolidBrush(colorref(theme.surface));
        // Blended towards the surface rather than drawn at full strength, so
        // the two renderers agree on how firm the card's edge is. GDI has no
        // alpha here: mixing the colour is the same arithmetic Direct2D does
        // with `border_alpha`, done ahead of time.
        let border: HBRUSH = CreateSolidBrush(colorref(
            theme
                .surface
                .mix(theme.border, f64::from(theme.border_alpha)),
        ));
        FillRect(target, &rect, background);
        FrameRect(target, &rect, border);
        let _ = DeleteObject(background.into());
        let _ = DeleteObject(border.into());

        SetBkMode(target, TRANSPARENT);

        if let Some(card) = CONTENT.lock().ok().and_then(|slot| slot.clone()) {
            let padding = theme.padding;
            let inner = width - padding * 2;
            let mut y = padding;

            // The original first and dimmer. It is above the answer because
            // the reader's eye is already on it, and it is present at all
            // because it is the only way to notice an answer that is not a
            // translation (docs/privacy-model.md §1).
            if !card.original.trim().is_empty() {
                y += draw(
                    target,
                    &card.original,
                    padding,
                    y,
                    inner,
                    theme.small_pt,
                    false,
                    colorref(theme.original),
                ) + GAP_AFTER_ORIGINAL;
            }

            y += match &card.body {
                // A list, drawn as one. Every value starts at the same x so a
                // reader finds that x once instead of five times.
                CardBody::Analysis {
                    translation, rows, ..
                } => analysis(
                    target,
                    translation.as_deref(),
                    rows,
                    padding,
                    y,
                    inner,
                    &theme,
                    true,
                ),
                body => {
                    let (text, colour) = match body {
                        CardBody::Translation(text) => (text.as_str(), theme.body),
                        CardBody::Problem(text) => (text.as_str(), theme.problem),
                        // A status rather than an answer, so it takes the
                        // footer's colour and must not be mistaken for one.
                        CardBody::Waiting(status) => (status.as_str(), theme.footer),
                        // Unreachable: the arm above takes it.
                        CardBody::Analysis { .. } => ("", theme.body),
                    };
                    draw(
                        target,
                        text,
                        padding,
                        y,
                        inner,
                        theme.body_pt,
                        true,
                        colorref(colour),
                    )
                }
            } + GAP_BEFORE_FOOTER;

            draw(
                target,
                &card.footer,
                padding,
                y,
                inner,
                theme.small_pt,
                false,
                colorref(theme.footer),
            );
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

/// Lay out an analysis as label and value columns, and return its height.
///
/// **Measuring and drawing are the same code.** `measure` and `paint` each had
/// their own arithmetic once and disagreed about a footer, so the one body with
/// a layout of its own is laid out in one place and `draw_it` decides whether
/// any ink lands. A pair that disagrees is a card with its last row clipped.
///
/// The labels decide a column and every value starts at it, so a reader finds
/// that x once instead of five times. Both halves are at the original's size:
/// an analysis is a list beside a sentence, and giving the list the answer's
/// size makes a card that is mostly explanation shout.
///
/// # Safety
/// `dc` must be a valid device context.
#[allow(clippy::too_many_arguments)]
unsafe fn analysis(
    dc: windows::Win32::Graphics::Gdi::HDC,
    translation: Option<&str>,
    rows: &[(String, String)],
    x: i32,
    top: i32,
    width: i32,
    theme: &lens_core::theme::Theme,
    draw_it: bool,
) -> i32 {
    if rows.is_empty() {
        return 0;
    }
    let mut before = top;
    // **The answer stays on the card.** `Ctrl+Alt+D` replaced the translation
    // with an account of it, so a reader who pressed it to understand an answer
    // lost the answer. At the answer's size and colour: on this card it is
    // still the answer, and the rows below it are notes about it.
    if let Some(text) = translation.map(str::trim).filter(|text| !text.is_empty()) {
        // SAFETY: on the caller's context, which the caller keeps valid.
        before += unsafe {
            if draw_it {
                draw(
                    dc,
                    text,
                    x,
                    before,
                    width,
                    theme.body_pt,
                    false,
                    colorref(theme.body),
                )
            } else {
                text_height(dc, text, width, theme.body_pt)
            }
        } + GAP_AFTER_ORIGINAL;
    }
    let top = before;
    // SAFETY: measuring only; each call owns and restores its own font.
    let column = rows
        .iter()
        .map(|(label, _)| unsafe { text_width(dc, label, theme.small_pt) })
        .max()
        .unwrap_or(0)
        + LABEL_GUTTER;
    // A column wider than the card leaves no room for a value. Nothing here
    // produces one, and a label a model invented could.
    let column = column.min(width / 2);

    let mut y = top;
    for (index, (label, value)) in rows.iter().enumerate() {
        // SAFETY: both calls are on the caller's context and restore its state.
        let (label_height, value_height) = unsafe {
            if draw_it {
                (
                    draw(
                        dc,
                        label,
                        x,
                        y,
                        column,
                        theme.small_pt,
                        false,
                        colorref(theme.footer),
                    ),
                    draw(
                        dc,
                        value,
                        x + column,
                        y,
                        width - column,
                        theme.small_pt,
                        false,
                        colorref(theme.body),
                    ),
                )
            } else {
                (
                    text_height(dc, label, column, theme.small_pt),
                    text_height(dc, value, width - column, theme.small_pt),
                )
            }
        };
        y += label_height.max(value_height);
        if index + 1 < rows.len() {
            y += GAP_BETWEEN_ROWS;
        }
    }
    y - top
}

/// How wide one line of text wants to be, unwrapped.
///
/// # Safety
/// `dc` must be a valid device context.
unsafe fn text_width(dc: windows::Win32::Graphics::Gdi::HDC, text: &str, point_size: i32) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: creates a font it owns, measures, restores and deletes it.
    unsafe {
        let font = font(logical_height(dc, point_size), false);
        let previous = SelectObject(dc, font.into());
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut wide,
            &mut rect,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX | DT_LEFT,
        );
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        rect.right - rect.left
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
    point_size: i32,
    semibold: bool,
    colour: COLORREF,
) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    // SAFETY: the font is selected, used, restored, and deleted.
    unsafe {
        let font = font(logical_height(dc, point_size), semibold);
        let previous = SelectObject(dc, font.into());
        SetTextColor(dc, colour);

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

/// Opaque pixels in the last card frame handed to the compositor.
///
/// The live checks' only honest view of what was drawn, now that the card is a
/// layered window whose content the window itself no longer holds. See
/// `glass::last_frame_ink`.
pub fn last_frame_ink() -> usize {
    crate::glass::last_frame_ink()
}

/// Discard that count, so a check measures the frame it is waiting for.
pub fn forget_last_frame() {
    crate::glass::forget_last_frame();
}

/// The work area a point is on, for a check that needs the same answer the
/// card placement uses.
///
/// # Safety
/// Read-only monitor queries.
pub unsafe fn work_area_at(point: ScreenPoint) -> ScreenRect {
    // SAFETY: the caller's guarantee, passed straight through.
    unsafe { work_area_for(point) }
}

/// The tallest a card may be on this work area, for a check measuring it.
pub fn max_card_height(work_area: ScreenRect) -> i32 {
    card_height_ceiling(work_area)
}

/// How long a card takes to arrive.
///
/// Short. This is a reading tool and the card is an answer to a question the
/// reader already asked, so the entrance is there to stop it appearing from
/// nowhere, not to be watched. Anything longer reads as latency.
const ENTRANCE: Duration = Duration::from_millis(110);

/// How many frames that takes. Seven at 110ms is about 16ms apart, which is a
/// display's own refresh and as smooth as this can usefully be.
const ENTRANCE_FRAMES: u32 = 7;

/// How far the card rises as it arrives, in pixels at 100% scaling.
///
/// Small on purpose. A card that flies in from anywhere draws the eye to the
/// movement rather than to the words, and the words are the product.
const ENTRANCE_RISE: f32 = 7.0;

/// Fade a card in while it settles the last few pixels upwards.
///
/// # Safety
/// `window` must be a live `WS_EX_LAYERED` window owned by this thread.
unsafe fn rise(
    glass: &Glass,
    window: HWND,
    laid: &crate::glass::Laid,
    theme: &Theme,
    placed: ScreenRect,
) -> bool {
    let frame = ENTRANCE / ENTRANCE_FRAMES;
    let rise = ENTRANCE_RISE * laid.scale();

    for step in 1..=ENTRANCE_FRAMES {
        let progress = f32::from(step as u16) / ENTRANCE_FRAMES as f32;
        // Ease out cubic: fastest at the start, settling rather than stopping.
        // Linear movement over a distance this small reads as a jump.
        let eased = 1.0 - (1.0 - progress).powi(3);
        let offset = (rise * (1.0 - eased)).round() as i32;
        let at = ScreenRect::new(placed.x, placed.y + offset, placed.width, placed.height);

        // SAFETY: the caller's guarantee, and the fade is the window's own
        // constant alpha rather than the bitmap's, so the drawn frame stays
        // fully inked and the flicker checks keep meaning what they mean.
        let drawn =
            unsafe { glass.present_faded(window, laid, theme, at, (eased * 255.0).round() as u8) };
        if !drawn {
            return false;
        }
        if step == 1 {
            // Shown only once the first frame exists. Showing an empty window
            // first is a flash of nothing, which is the thing being avoided.
            // SAFETY: NOACTIVATE is what keeps focus where it was.
            unsafe {
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            }
        }
        if step < ENTRANCE_FRAMES {
            std::thread::sleep(frame);
        }
    }
    true
}

/// The widest a card may be at 100% scaling, before the shadow around it.
///
/// Exposed for the live checks, which lay a card out themselves and compare.
pub fn max_card_width() -> i32 {
    CARD_WIDTH
}

/// The window height a card of this laid-out height gets, on this work area.
///
/// Exposed so a diagnostic can ask whether a card fits the window it is drawn
/// into, which is a question no screenshot answers reliably.
pub fn card_height_for(laid_height: i32, work_area: ScreenRect) -> i32 {
    laid_height.min(card_height_ceiling(work_area))
}

/// The tallest a card may be on this work area.
fn card_height_ceiling(work_area: ScreenRect) -> i32 {
    // A floor as well, for a work area small enough that two thirds of it
    // holds nothing: a card that cannot show a line is worse than one that
    // covers a small screen.
    ((work_area.height as f32 * MOST_OF_THE_SCREEN).round() as i32).max(200)
}

/// Whether the overlay class is registered with the styles it must have.
///
/// Exposed for the live check, which asserts the focus properties rather than
/// trusting the flags above to stay put.
pub fn required_ex_styles() -> u32 {
    (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED).0
}
