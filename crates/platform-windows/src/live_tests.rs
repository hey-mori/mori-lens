//! Live-desktop checks, ignored by default.
//!
//! These need a real desktop session with a window server, so they cannot run
//! in the normal suite and are not part of `scripts/check.sh`. Run them by
//! hand:
//!
//! ```text
//! cargo test -p lens-platform-windows -- --ignored --nocapture
//! ```
//!
//! They exist because two claims in this project could not be checked any other
//! way. The privacy model promises that a protected field is refused before it
//! is read, and the compatibility matrix wants to know whether a plain Win32
//! `EDIT` exposes text at all. Both need a real control, so the harness builds
//! one rather than waiting for a browser and a human.
//!
//! Every check confirms the harness still owns the point before drawing a
//! conclusion from it. A covered window reads exactly like a failed guarantee,
//! and that produced a false privacy alarm twice while this was being written.

use lens_core::acquire::{AcquireContext, AcquireError, ProtectedReason, TextAcquirer};
use lens_core::geometry::ScreenPoint;
use lens_core::hover::Generation;

use crate::protected_harness::{centre, harness_owns, ProtectedHarness};
use crate::UiaTextAcquirer;

/// Fail with the right diagnosis when the harness window is not on top.
fn require_ownership(harness: &ProtectedHarness, point: ScreenPoint, what: &str) {
    assert!(
        harness_owns(harness.window_id(), point),
        "another window covers the {what} at {point:?}, so this run measured \
         someone else's window rather than the harness. Not a product defect.",
    );
}

/// The single most important privacy guarantee in the project, executed.
///
/// A `Protected` result means the refusal fired. It is checked before any text
/// is materialised, so a pass here is also evidence that the field's contents
/// never became a Rust value.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_password_field_is_refused() {
    let harness = ProtectedHarness::start().expect("could not build the harness window");
    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));

    let point = centre(harness.fields().password);
    require_ownership(&harness, point, "password field");

    let result = acquirer.acquire(point, &context);
    println!("password field  → {result:?}");

    match result {
        Err(AcquireError::Protected(ProtectedReason::Declared)) => {}
        Err(AcquireError::Protected(ProtectedReason::Unprovable)) => {
            // Also a refusal, and also correct, but it means the provider did
            // not answer the property rather than answering "yes". Worth
            // distinguishing, because a build where every field is Unprovable
            // would refuse everything and look like it worked.
            println!("refused, but the IsPassword property could not be read");
        }
        other => panic!("a password field was NOT refused, which is a privacy defect: {other:?}"),
    }
}

/// The refusal is specific to the protected field, not a blanket failure.
///
/// Without this, a build that refused *everything* would pass the check above
/// and look like a working privacy guarantee while the product read nothing at
/// all. The plain field is the control: it must reach a different outcome.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_refusal_is_specific_to_the_protected_field() {
    let harness = ProtectedHarness::start().expect("could not build the harness window");
    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));

    let fields = harness.fields();
    let plain_point = centre(fields.plain);
    let password_point = centre(fields.password);
    require_ownership(&harness, plain_point, "plain field");
    require_ownership(&harness, password_point, "password field");

    let plain = acquirer.acquire(plain_point, &context);
    let password = acquirer.acquire(password_point, &context);

    println!(
        "plain    → {:?}\npassword → {:?}",
        plain.as_ref().err().map(|error| error.reason()),
        password.as_ref().err().map(|error| error.reason()),
    );

    assert!(
        matches!(password, Err(AcquireError::Protected(_))),
        "the password field was not refused",
    );
    assert!(
        !matches!(plain, Err(AcquireError::Protected(_))),
        "the plain field was also refused, so the refusal is not discriminating \
         and the guarantee proves nothing",
    );
}

/// What a plain Win32 `EDIT` returns, measured rather than asserted.
///
/// Mori found that plain `EDIT` controls expose no text pattern at all, which
/// is the reason the OCR fallback exists rather than being a defensive nicety.
/// The compatibility matrix has had no genuine Win32 application to check that
/// against, and the Notepad row that seemed to disprove it turned out to be
/// another window's numbers.
///
/// Deliberately not asserted: this records what the platform does. An assertion
/// would turn a finding about Windows into a failing test the day Windows
/// changes.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn what_a_plain_win32_edit_returns() {
    let harness = ProtectedHarness::start().expect("could not build the harness window");
    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));

    let point = centre(harness.fields().plain);
    require_ownership(&harness, point, "plain field");

    match acquirer.acquire(point, &context) {
        Ok(target) => println!(
            "plain EDIT      → acquired  control={:?}  unit={}  chars={}  rects={}",
            target.provider.control_id,
            target.provider.expanded_unit.as_str(),
            target.segmented.context().chars().count(),
            target.word_bounds.len(),
        ),
        Err(error) => println!("plain EDIT      → {}", error.reason()),
    }
}

/// The card must not take focus. Everything else about the overlay is
/// subordinate to this.
///
/// A card that activates itself while the user is typing eats a keystroke and
/// moves the caret, which is worse than showing no card at all. Three flags buy
/// it and any one of them silently regressing would be invisible in a
/// screenshot, so it is asserted here instead: record the foreground window,
/// show a card, and require that it did not move.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn showing_a_card_does_not_move_the_foreground_window() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::translate::{EngineId, QuickTranslation};

    let Some(overlay) = crate::Overlay::start() else {
        panic!("could not create the overlay window");
    };

    // SAFETY: a read-only query of which window has focus.
    let before = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };

    let card = Card::translated(
        "The output files came back empty, so the full test suite didn't produce anything.",
        &QuickTranslation {
            text:
                "출력 파일이 비어 있었기 때문에 전체 테스트 스위트는 아무것도 생성하지 않았습니다."
                    .to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1090,
        },
    );
    assert!(overlay.show(card, ScreenRect::new(400, 300, 520, 22)));

    // Long enough for the window to be created, positioned, and painted.
    std::thread::sleep(std::time::Duration::from_millis(700));

    // SAFETY: read-only.
    let after = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    println!("foreground before {before:?} after {after:?}");

    assert_eq!(
        before.0 as isize, after.0 as isize,
        "the overlay stole focus, which makes it unusable while typing",
    );

    assert!(overlay.hide());
    std::thread::sleep(std::time::Duration::from_millis(200));
}

/// A translation with no original beside it is refused at the overlay too.
///
/// The rule lives in `Card::is_showable`, and this is the second place it is
/// enforced. Belt and braces on purpose: it is the only mitigation for a model
/// that was talked into emitting text a page chose
/// (`docs/privacy-model.md` §1), so it should not rest on one call site.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_overlay_refuses_a_translation_with_no_original() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::translate::{EngineId, QuickTranslation};

    let Some(overlay) = crate::Overlay::start() else {
        panic!("could not create the overlay window");
    };

    let orphan = Card::translated(
        "",
        &QuickTranslation {
            text: "무엇인가".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 10,
        },
    );

    assert!(
        !overlay.show(orphan, ScreenRect::new(400, 300, 200, 20)),
        "a translation was shown with no original to check it against",
    );
}

/// The card actually paints text, proven by reading its own pixels back.
///
/// Every other check here would pass on a blank card: focus is not stolen by a
/// window that draws nothing, and placement is arithmetic. This is the one that
/// fails if `WM_PAINT` never runs, the font is missing, or the text is drawn in
/// the background colour.
///
/// It copies **this process's own window** into memory and never touches the
/// disk, so it does not conflict with the rule that no screen capture is ever
/// written (`docs/privacy-model.md` §1). Nothing of the user's screen is read.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_shown_card_has_text_painted_on_it() {
    use std::collections::HashSet;

    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::translate::{EngineId, QuickTranslation};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetPixel, ReleaseDC, SelectObject, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    let Some(overlay) = crate::Overlay::start() else {
        panic!("could not create the overlay window");
    };

    let card = Card::translated(
        "The output files came back empty.",
        &QuickTranslation {
            text: "출력 파일이 비어 있었습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1090,
        },
    );
    assert!(overlay.show(card, ScreenRect::new(500, 400, 400, 22)));
    std::thread::sleep(std::time::Duration::from_millis(800));

    let window = HWND(overlay.window_id() as *mut _);
    // SAFETY: every GDI object below is released or deleted before returning,
    // and the window is this process's own.
    let colours: HashSet<u32> = unsafe {
        let mut rect = windows::Win32::Foundation::RECT::default();
        GetClientRect(window, &mut rect).expect("client rect");
        let (width, height) = (rect.right, rect.bottom);
        assert!(width > 100 && height > 60, "card is {width}x{height}");

        // The overlay's own device context, so nothing of the user's screen
        // is read even incidentally.
        let source = GetDC(Some(window));
        let memory = CreateCompatibleDC(Some(source));
        let bitmap = CreateCompatibleBitmap(source, width, height);
        let previous = SelectObject(memory, bitmap.into());

        let copied = BitBlt(memory, 0, 0, width, height, Some(source), 0, 0, SRCCOPY).is_ok();

        let mut seen = HashSet::new();
        if copied {
            for y in (0..height).step_by(3) {
                for x in (0..width).step_by(3) {
                    seen.insert(GetPixel(memory, x, y).0);
                }
            }
        }

        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(Some(window), source);
        seen
    };

    println!("distinct colours sampled on the card: {}", colours.len());
    assert!(
        colours.len() > 4,
        "the card looks blank: {} distinct colours. Text is not being painted.",
        colours.len(),
    );

    assert!(overlay.hide());
}

/// What a sampler saw while watching the card window.
#[derive(Debug)]
struct Watched {
    samples: usize,
    hidden: usize,
    blank: usize,
    thinnest: usize,
}

/// Sample the card window until told to stop.
///
/// Coarse spatially and fine temporally on purpose: the flicker being looked
/// for lasts a single 25ms loop tick, so time resolution matters and a finer
/// pixel grid would cost more per sample than the event lasts.
fn watch_card(
    window_id: i64,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<Watched> {
    use std::collections::HashSet;
    use std::sync::atomic::Ordering;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetPixel, ReleaseDC, SelectObject, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsWindowVisible};

    std::thread::spawn(move || {
        let window = HWND(window_id as *mut _);
        let mut watched = Watched {
            samples: 0,
            hidden: 0,
            blank: 0,
            thinnest: usize::MAX,
        };

        while !stop.load(Ordering::Relaxed) {
            watched.samples += 1;
            // SAFETY: read-only queries; every GDI object is released below.
            unsafe {
                if !IsWindowVisible(window).as_bool() {
                    watched.hidden += 1;
                    std::thread::sleep(std::time::Duration::from_millis(3));
                    continue;
                }
                let mut rect = windows::Win32::Foundation::RECT::default();
                if GetClientRect(window, &mut rect).is_err() {
                    continue;
                }
                let (w, h) = (rect.right, rect.bottom);
                let source = GetDC(Some(window));
                let memory = CreateCompatibleDC(Some(source));
                let bitmap = CreateCompatibleBitmap(source, w, h);
                let previous = SelectObject(memory, bitmap.into());
                let copied = BitBlt(memory, 0, 0, w, h, Some(source), 0, 0, SRCCOPY).is_ok();

                let mut seen = HashSet::new();
                if copied {
                    for y in (0..h).step_by(9) {
                        for x in (0..w).step_by(9) {
                            seen.insert(GetPixel(memory, x, y).0);
                        }
                    }
                }
                SelectObject(memory, previous);
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(memory);
                ReleaseDC(Some(window), source);

                if copied {
                    watched.thinnest = watched.thinnest.min(seen.len());
                    // Background plus border is two or three colours. At or
                    // below that is a card with no text on it.
                    if seen.len() <= 4 {
                        watched.blank += 1;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        watched
    })
}

/// Sample the card's own window while a streamed answer redraws it.
///
/// This is the check I said could not be made mechanically, and I was wrong:
/// flicker has two observable causes and both are measurable from inside the
/// process.
///
/// - **The card disappearing between redraws.** Sampling `IsWindowVisible`
///   while partials arrive catches a show/hide cycle directly. That was the
///   real bug: a card retired 25ms after appearing, re-shown by the next
///   partial, retired again.
/// - **A blank frame mid-paint.** Without double buffering the window shows a
///   filled background with no text on it for the length of the layout.
///   Counting distinct colours in a sampled frame catches that: a painted card
///   has dozens, a blank one has two or three.
///
/// What it still cannot see is a flash shorter than the sampling period, or
/// anything about how it *feels*. It proves the two known causes are gone, not
/// that a human sees a steady card.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_streamed_answer_does_not_blank_or_disappear_between_redraws() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;

    let overlay = crate::Overlay::start().expect("overlay");
    let anchor = ScreenRect::new(500, 400, 300, 22);
    let original = "The output files came back empty, so the full test suite produced nothing.";

    assert!(overlay.show(Card::partial(original, "출력"), anchor));
    std::thread::sleep(std::time::Duration::from_millis(400));

    let stop = Arc::new(AtomicBool::new(false));
    let watcher = watch_card(overlay.window_id(), Arc::clone(&stop));

    // What a streamed answer does: the same card, growing, on the throttle.
    for piece in [
        "출력 파일이",
        "출력 파일이 비어 있었기",
        "출력 파일이 비어 있었기 때문에 전체",
        "출력 파일이 비어 있었기 때문에 전체 테스트 스위트는",
        "출력 파일이 비어 있었기 때문에 전체 테스트 스위트는 아무것도 생성하지 않았습니다.",
    ] {
        assert!(overlay.show(Card::partial(original, piece), anchor));
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    std::thread::sleep(std::time::Duration::from_millis(200));

    stop.store(true, Ordering::Relaxed);
    let seen = watcher.join().expect("watcher");
    let period_ms = 800.0 / seen.samples as f64;
    println!(
        "{} samples every {period_ms:.1}ms · {} hidden · {} blank · thinnest {} colours",
        seen.samples, seen.hidden, seen.blank, seen.thinnest,
    );

    // The bug hid the card for one 25ms loop tick, so the sampler has to be
    // well inside that or a pass means nothing.
    assert!(
        seen.samples > 60 && period_ms < 20.0,
        "{} samples, one every {period_ms:.1}ms: too coarse to see a 25ms flicker",
        seen.samples,
    );
    assert_eq!(
        seen.hidden, 0,
        "the card disappeared while a streamed answer was arriving",
    );
    assert_eq!(
        seen.blank, 0,
        "{} frames showed a card with no text on it, which is the paint flickering",
        seen.blank,
    );

    assert!(overlay.hide());
}

/// The detector's own control: it must fail on the behaviour it exists to
/// catch.
///
/// A flicker detector that passes on both the broken and the fixed code is
/// worthless, and there is no way to tell which one it is without making it
/// fail on purpose. This reproduces the old bug, a card retired between
/// partials, and asserts the sampler notices.
///
/// If this ever stops seeing hidden frames, the detector above has gone blind
/// and its passes mean nothing.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_flicker_detector_catches_a_card_that_disappears() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;

    let overlay = crate::Overlay::start().expect("overlay");
    let anchor = ScreenRect::new(500, 400, 300, 22);

    assert!(overlay.show(Card::partial("original text", "번역"), anchor));
    std::thread::sleep(std::time::Duration::from_millis(300));

    let stop = Arc::new(AtomicBool::new(false));
    let watcher = watch_card(overlay.window_id(), Arc::clone(&stop));

    // Exactly what the old dismissal rule did: hide almost immediately after
    // each partial, then let the next one show it again.
    for piece in ["번역이", "번역이 도착", "번역이 도착하는 중"] {
        assert!(overlay.show(Card::partial("original text", piece), anchor));
        std::thread::sleep(std::time::Duration::from_millis(40));
        assert!(overlay.hide());
        std::thread::sleep(std::time::Duration::from_millis(60));
    }

    stop.store(true, Ordering::Relaxed);
    let seen = watcher.join().expect("watcher");
    println!(
        "control: {} samples · {} hidden · {} blank",
        seen.samples, seen.hidden, seen.blank,
    );

    assert!(
        seen.hidden > 0,
        "the detector did not notice a card that was hidden three times;          it is blind and the test above proves nothing",
    );
}

/// Does this machine actually give a Win32 window a glass backdrop?
///
/// Measured rather than read off a documentation page, because the answer
/// decides how the card is painted. The documented minimum is Windows 11 build
/// 22621 and this machine is 26200, but "the API exists" and "the API does
/// something to a window shaped like ours" are different claims.
///
/// Three questions, in order of how much they cost to be wrong about:
///
/// 1. Do the calls succeed at all on this build?
/// 2. Do they succeed on a window that is `WS_EX_LAYERED` with a uniform
///    alpha, which is what the card is today?
/// 3. Does the attribute read back as the value that was set, or does DWM
///    quietly ignore it?
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn what_this_machine_does_with_a_glass_backdrop() {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DwmSetWindowAttribute,
        DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWINDOWATTRIBUTE,
    };
    use windows::Win32::UI::Controls::MARGINS;

    /// `DWMSBT_TRANSIENTWINDOW`: Desktop Acrylic, the transient-window
    /// material, which is what a card that appears for a few seconds wants.
    const DWMSBT_TRANSIENTWINDOW: i32 = 3;
    /// `DWMWCP_ROUND`.
    const DWMWCP_ROUND: i32 = 2;

    let overlay = crate::Overlay::start().expect("overlay");
    let window = HWND(overlay.window_id() as *mut _);

    // SAFETY: documented DWM attribute calls on this process's own window.
    unsafe {
        let backdrop = DWMSBT_TRANSIENTWINDOW;
        let set_backdrop = DwmSetWindowAttribute(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::addr_of!(backdrop).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        println!("DWMWA_SYSTEMBACKDROP_TYPE = TRANSIENTWINDOW  -> {set_backdrop:?}");

        let mut read_back = 0i32;
        let get_backdrop = DwmGetWindowAttribute(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::addr_of_mut!(read_back).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        println!("  reads back as {read_back} ({get_backdrop:?})");

        let corner = DWMWCP_ROUND;
        let set_corner = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        println!("DWMWA_WINDOW_CORNER_PREFERENCE = ROUND      -> {set_corner:?}");

        // Extending the frame into the whole client area is what lets the
        // backdrop show through a client area at all.
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let extended = DwmExtendFrameIntoClientArea(window, &margins);
        println!("DwmExtendFrameIntoClientArea(-1)            -> {extended:?}");

        // Whether the composition attributes survive alongside the uniform
        // alpha the card sets today.
        let layered = windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
            window,
            COLORREF(0),
            244,
            windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
        );
        println!("SetLayeredWindowAttributes(alpha 244)       -> {layered:?}");

        let mut after = 0i32;
        let _ = DwmGetWindowAttribute(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::addr_of_mut!(after).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        println!("  backdrop after layered alpha: {after}");
    }

    println!(
        "\nDWMWINDOWATTRIBUTE ids in this binding: backdrop={:?} corner={:?}",
        DWMWA_SYSTEMBACKDROP_TYPE.0, DWMWA_WINDOW_CORNER_PREFERENCE.0,
    );
    let _ = DWMWINDOWATTRIBUTE::default();
}

/// A card anchored on a second monitor must be placed on that monitor.
///
/// `place_card` has unit tests for a monitor with a negative origin and they
/// pass, because they are handed a correct work area. The runtime used to hand
/// them `SPI_GETWORKAREA`, which answers for the **primary** monitor only, and
/// `place_card` clamps into whatever it is given. Measured before the fix on a
/// two-monitor desk: an anchor at x=2500 was placed at x=1116, about 1,400px
/// away on the wrong display.
///
/// Green unit tests, wrong product. This one drives the real window so the two
/// cannot drift apart again.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_card_anchored_on_a_second_monitor_stays_on_it() {
    use lens_core::card::Card;
    use lens_core::geometry::{ScreenPoint, ScreenRect};
    use lens_core::translate::{EngineId, QuickTranslation};
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
        MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    // Every monitor on this desk, so the check adapts rather than assuming a
    // layout.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::core::BOOL {
        // SAFETY: the pointer was supplied by the call below and outlives it.
        let found = unsafe { &mut *(lparam.0 as *mut Vec<ScreenRect>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: writes into `info`, whose `cbSize` states its size.
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            found.push(ScreenRect::from_ltrb(
                info.rcWork.left,
                info.rcWork.top,
                info.rcWork.right,
                info.rcWork.bottom,
            ));
        }
        windows::core::BOOL(1)
    }

    let mut monitors: Vec<ScreenRect> = Vec::new();
    // SAFETY: synchronous enumeration; `monitors` outlives it.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            windows::Win32::Foundation::LPARAM(&mut monitors as *mut Vec<ScreenRect> as isize),
        );
    }
    println!("monitors: {monitors:?}");

    if monitors.len() < 2 {
        println!("SKIPPED: one monitor, so there is nothing for a card to be dragged off");
        return;
    }

    let overlay = crate::Overlay::start().expect("overlay");
    let mut wrong = 0usize;

    for area in &monitors {
        // A line of text near the middle of this monitor.
        let anchor = ScreenRect::new(area.x + area.width / 2, area.y + area.height / 2, 240, 22);
        let card = Card::translated(
            "The output files came back empty.",
            &QuickTranslation {
                text: "출력 파일이 비어 있었습니다.".to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:3b".to_owned(),
                latency_ms: 900,
            },
        );
        assert!(overlay.show(card, anchor));
        std::thread::sleep(std::time::Duration::from_millis(350));

        let window = HWND(overlay.window_id() as *mut _);
        // SAFETY: read-only queries.
        let (placed, landed_on) = unsafe {
            let mut rect = RECT::default();
            GetWindowRect(window, &mut rect).expect("window rect");
            let centre =
                ScreenPoint::new((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
            let monitor = MonitorFromPoint(
                POINT {
                    x: centre.x,
                    y: centre.y,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(monitor, &mut info);
            (
                ScreenRect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom),
                ScreenRect::from_ltrb(
                    info.rcWork.left,
                    info.rcWork.top,
                    info.rcWork.right,
                    info.rcWork.bottom,
                ),
            )
        };

        let correct = landed_on == *area;
        if !correct {
            wrong += 1;
        }
        println!(
            "anchor on {:?}
    card placed {:?} -> monitor {:?}   {}",
            area,
            placed,
            landed_on,
            if correct {
                "same monitor"
            } else {
                "WRONG MONITOR"
            },
        );
    }

    assert!(overlay.hide());
    assert_eq!(
        wrong, 0,
        "{wrong} card(s) were placed on a different monitor from the text they explain",
    );
}

/// A click on the card must reach the application underneath.
///
/// ADR-0004 says the passive tooltip ignores mouse events. The card never did:
/// it had no `WS_EX_TRANSPARENT` and no hit-test handling, so it silently
/// swallowed every click on the text it was explaining.
///
/// Measured without injecting any input. `WindowFromPoint` performs the same
/// hit test the mouse does, so if it returns anything other than the card for
/// a point inside the card, a click at that point goes elsewhere.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_click_on_the_card_passes_through_to_what_is_underneath() {
    use lens_core::card::Card;
    use lens_core::geometry::{ScreenPoint, ScreenRect};
    use lens_core::translate::{EngineId, QuickTranslation};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, WindowFromPoint};

    let overlay = crate::Overlay::start().expect("overlay");
    let card = Card::translated(
        "The output files came back empty.",
        &QuickTranslation {
            text: "출력 파일이 비어 있었습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 900,
        },
    );
    assert!(overlay.show(card, ScreenRect::new(500, 400, 300, 22)));
    std::thread::sleep(std::time::Duration::from_millis(600));

    let window = windows::Win32::Foundation::HWND(overlay.window_id() as *mut _);
    // SAFETY: read-only queries.
    let (centre, hit) = unsafe {
        let mut rect = windows::Win32::Foundation::RECT::default();
        GetWindowRect(window, &mut rect).expect("window rect");
        let centre = ScreenPoint::new((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
        let hit = WindowFromPoint(POINT {
            x: centre.x,
            y: centre.y,
        });
        (centre, hit)
    };

    println!(
        "card {:?} · hit test at its centre returns {:?} (card is {:?})",
        centre, hit.0, window.0,
    );

    assert_ne!(
        hit.0 as isize, window.0 as isize,
        "the card is hit-testable, so it swallows clicks meant for the text \
         underneath, which ADR-0004 forbids",
    );

    assert!(overlay.hide());
}

/// Can a window be measured without being brought to the front?
///
/// The matrix has been gated on the operator raising each application in turn,
/// because `ElementFromPoint` answers with whatever is topmost. That is a
/// property of the *entry point*, not of UI Automation: `ElementFromHandle`
/// takes a window handle and no coordinate at all.
///
/// If that works on an occluded window, the compatibility matrix stops needing
/// a human to arrange the desktop, and it gains something better than the grid
/// sweep: **ground truth**. The grid guesses where text might be. A tree walk
/// knows exactly which elements hold text and where each one is, so a
/// resolution can be checked against the element it should have hit.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn what_a_window_exposes_without_raising_it() {
    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
        TreeScope_Descendants, UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
    };

    let _com = crate::com::ComScope::enter();
    // SAFETY: creating the documented automation co-class in-process.
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .expect("automation");

    let mut any = false;
    for window in crate::top_level_windows() {
        let name = window.process_name.clone().unwrap_or_else(|| "?".into());
        // SAFETY: read-only automation queries on a handle just enumerated.
        let report = unsafe {
            let hwnd = HWND(window.id as *mut _);
            let Ok(element) = automation.ElementFromHandle(hwnd) else {
                continue;
            };
            // Every descendant that offers text, regardless of what covers it.
            let Ok(condition) = automation.CreatePropertyCondition(
                UIA_IsTextPatternAvailablePropertyId,
                &VARIANT::from(true),
            ) else {
                continue;
            };
            let Ok(found) = element.FindAll(TreeScope_Descendants, &condition) else {
                continue;
            };
            let count = found.Length().unwrap_or(0);
            if count == 0 {
                continue;
            }

            // How much text, and how localised. A window whose text lives in
            // one enormous element is the blob case; many small ones is what
            // a hover can aim at.
            let mut with_text = 0usize;
            let mut chars = 0usize;
            let mut smallest = i32::MAX;
            for index in 0..count.min(200) {
                let Ok(child): Result<IUIAutomationElement, _> = found.GetElement(index) else {
                    continue;
                };
                let Ok(pattern) = child.GetCurrentPattern(UIA_TextPatternId) else {
                    continue;
                };
                let Ok(pattern) = pattern.cast::<IUIAutomationTextPattern>() else {
                    continue;
                };
                let Ok(range) = pattern.DocumentRange() else {
                    continue;
                };
                let text = range
                    .GetText(400)
                    .map(|t| t.to_string())
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                with_text += 1;
                chars += text.chars().count();
                if let Ok(rect) = child.CurrentBoundingRectangle() {
                    let height = rect.bottom - rect.top;
                    if height > 0 {
                        smallest = smallest.min(height);
                    }
                }
            }
            (count, with_text, chars, smallest)
        };

        let (elements, with_text, chars, smallest) = report;
        if with_text == 0 {
            continue;
        }
        any = true;
        println!(
            "{name:<18} {elements:>4} text elements, {with_text:>3} with content, \
             {chars:>6} chars, smallest {}px",
            if smallest == i32::MAX {
                "?".to_owned()
            } else {
                smallest.to_string()
            },
        );
    }

    assert!(
        any,
        "no window exposed text through ElementFromHandle, so this route is \
         not an alternative to raising windows",
    );
}

/// A window that closes under the pointer must not take Lens with it.
///
/// Milestone 1's other untested gate: "no crash from an element belonging to a
/// closed window". `docs/architecture.md` calls this a normal outcome rather
/// than an exception, and `AcquireError::ElementGone` exists for it, but
/// nothing had ever destroyed a window mid-flight to find out.
///
/// It is not a hypothetical. A menu closes, a tooltip disappears, a tab is
/// shut: the UI under a resting pointer changes constantly, and
/// `ElementFromPoint` can hand back an element whose process has already let
/// go of it.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn reading_where_a_window_just_closed_is_an_error_not_a_crash() {
    use lens_core::acquire::{AcquireContext, TextAcquirer};
    use lens_core::hover::Generation;

    use crate::protected_harness::{centre, ProtectedHarness};

    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));

    // A window with a real control in it, then the same coordinate after the
    // window is gone.
    let point = {
        let harness = ProtectedHarness::start().expect("harness");
        let point = centre(harness.fields().plain);
        let before = acquirer.acquire(point, &context);
        println!("while the window existed: {:?}", before.map(|_| "read"));
        point
        // Dropped here: the window is destroyed and its thread joined.
    };

    // The window is gone. Whatever is underneath now answers, or nothing does,
    // and either is fine. What must not happen is a panic or a hang.
    let started = std::time::Instant::now();
    let after = acquirer.acquire(point, &context);
    let elapsed = started.elapsed();

    println!(
        "after it closed: {:?} in {}ms",
        after
            .as_ref()
            .map(|_| "read")
            .map_err(|error| error.reason()),
        elapsed.as_millis(),
    );

    // Reaching this line at all is most of the test: a crash would have taken
    // the process. The rest guards against the other failure, an adapter that
    // blocks forever on an element nobody owns.
    //
    // What this does **not** cover: the transition from a successful read to a
    // closed window. The harness's plain EDIT exposes no text pattern, so the
    // "before" read is already `Unsupported`. Covering that properly needs a
    // harness control that reads, and the gate this satisfies is about not
    // crashing rather than about the transition.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "reading a closed window's coordinate took {}ms, which is a hang rather \
         than an error",
        elapsed.as_millis(),
    );

    // Repeated, because a stale COM interface tends to fail differently the
    // second time, and the adapter caches nothing precisely so that it cannot.
    for _ in 0..20 {
        let _ = acquirer.acquire(point, &context);
    }
    println!("twenty further reads at the same coordinate, still alive");
}
