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
//! **They take over the screen while they run.** Harness windows appear on
//! top, cards are drawn and hidden, a tray icon comes and goes. That is
//! unavoidable: these exist because the things they check cannot be seen any
//! other way. It does mean the suite is not something to run in a loop while
//! somebody is using the machine, and it was, and they noticed.
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
            cached: false,
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
            cached: false,
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
    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::translate::{EngineId, QuickTranslation};

    let Some(overlay) = crate::Overlay::start() else {
        panic!("could not create the overlay window");
    };

    // So a count left over from an earlier check cannot pass for this one.
    crate::overlay::forget_last_frame();

    let card = Card::translated(
        "The output files came back empty.",
        &QuickTranslation {
            text: "출력 파일이 비어 있었습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1090,
            cached: false,
        },
    );
    assert!(overlay.show(card, ScreenRect::new(500, 400, 400, 22)));
    std::thread::sleep(std::time::Duration::from_millis(800));

    let ink = crate::overlay::last_frame_ink();
    println!("opaque pixels in the presented frame: {ink}");

    // This used to sample the window's own device context. That stopped being
    // possible when the card became a layered window presented through
    // `UpdateLayeredWindow`: the content lives in the compositor and the
    // window reads back as `CLR_INVALID`, so the check was quietly measuring
    // one colour and would have gone on passing had the threshold been lower.
    //
    // Reading the screen instead would work and is refused. These checks
    // sample the product's own surface and never the user's desktop.
    assert!(
        ink > 200,
        "the presented frame carried {ink} opaque pixels, so the card is blank. \
         Text is not being painted.",
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
    use std::sync::atomic::Ordering;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;

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
            // SAFETY: a read-only query on a window this process owns.
            let visible = unsafe { IsWindowVisible(window) }.as_bool();
            if !visible {
                watched.hidden += 1;
                std::thread::sleep(std::time::Duration::from_millis(3));
                continue;
            }

            // What the card is made of, rather than what the window's device
            // context says. Since the card became a layered window presented
            // through `UpdateLayeredWindow`, its content lives in the
            // compositor and the window itself reads back as `CLR_INVALID`.
            // This watcher was sampling that and finding one colour every
            // time, which would have reported a permanent flicker and, with a
            // looser threshold, a permanent pass.
            //
            // Reading the screen would also work and is refused: these checks
            // sample the product's own surface and never the user's desktop.
            //
            // It measures a frame that was *presented* rather than a frame
            // that is *visible*, which is a real difference and a small one:
            // a layered window shows exactly the bitmap it was handed.
            let ink = crate::overlay::last_frame_ink();
            watched.thinnest = watched.thinnest.min(ink);
            if ink == 0 {
                watched.blank += 1;
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
    crate::overlay::forget_last_frame();
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
        "{} samples every {period_ms:.1}ms · {} hidden · {} blank · thinnest {} opaque pixels",
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
                cached: false,
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
            cached: false,
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

/// Wedge an application in **another process** and read it.
///
/// `AcquireError::Timeout` has been in the failure taxonomy since it was
/// written and nothing had ever produced one, because there was no timeout
/// anywhere in the Windows acquisition path: UI Automation's default is two
/// minutes. Issue #28 measured what that costs, one `acquire` blocking for 45
/// seconds with the pointer loop behind it.
///
/// **Another process on purpose.** The first version of this test wedged a
/// window on a second thread of this one, and the read blocked for the full
/// stall and came back `Empty`. That is not a bug in the guard, it is its
/// scope: `IUIAutomation2::SetTransactionTimeout` bounds the client's
/// cross-process calls and does nothing for a provider inside the client. The
/// negative is kept below, because a guard's edges are worth knowing and the
/// product never reads its own process anyway (`AppPolicy::deny_self`).
///
/// So this spawns the test binary again, has the child put a window up and stop
/// dispatching, and reads it from here.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_wedged_application_in_another_process_is_a_timeout() {
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
    use lens_core::geometry::ScreenPoint;
    use lens_core::hover::Generation;

    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    assert!(
        acquirer.calls_are_bounded(),
        "the per-call limit was refused, so there is no guard to test",
    );
    let context = AcquireContext::new(Generation(1));

    let mut child =
        std::process::Command::new(std::env::current_exe().expect("this test binary has a path"))
            .args(["--exact", WEDGE_TEST, "--ignored", "--nocapture"])
            .env(WEDGE_ENV, "1")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("could not spawn the wedge process");

    let lines = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut point = None;
    let mut stalling = false;
    for line in lines.map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix("WEDGE-READY ") {
            let mut parts = rest.split_whitespace();
            let x: i32 = parts.next().and_then(|v| v.parse().ok()).expect("x");
            let y: i32 = parts.next().and_then(|v| v.parse().ok()).expect("y");
            let window: i64 = parts.next().and_then(|v| v.parse().ok()).expect("window");
            point = Some(ScreenPoint::new(x, y));

            assert_eq!(
                crate::window::root_window_at(ScreenPoint::new(x, y)),
                Some(window),
                "another window is on top of the wedged one, so this would measure the occluder",
            );

            // While the child is still dispatching.
            let began = Instant::now();
            let healthy = acquirer.acquire(ScreenPoint::new(x, y), &context).err();
            println!(
                "answering:  {healthy:?} in {}ms",
                began.elapsed().as_millis(),
            );
            // Only that a dispatching window answers fast. Which error it
            // answers with is a separate question, and an open one: read from
            // another process this same control has come back `Empty` on one
            // run and `Protected(Declared)` on the next, where the in-process
            // test on it always answers `Empty`. Fail-safe in both directions
            // seen so far, and tracked separately. Asserting a particular one
            // here would make this test fail for a reason unrelated to
            // timeouts, which is all it is for.
            assert_ne!(
                healthy,
                Some(AcquireError::Timeout),
                "a healthy window timed out, so the limit is set too low",
            );
        }
        if line.starts_with("WEDGE-STALLING") {
            stalling = true;
            break;
        }
    }

    let point = point.expect("the child never reported its window");
    assert!(stalling, "the child never reached its stall");
    // Let the sleep actually begin inside the window procedure.
    std::thread::sleep(Duration::from_millis(200));

    let began = Instant::now();
    let wedged = acquirer.acquire(point, &context).err();
    let elapsed = began.elapsed();
    println!("wedged:     {wedged:?} in {}ms", elapsed.as_millis());

    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(
        wedged,
        Some(AcquireError::Timeout),
        "a wedged application must be recorded as slow rather than as \
         unsupported or protected: the compatibility matrix is built from \
         these names and would otherwise carry a verdict about an application \
         that never answered the question",
    );
    // The whole point. Without the guard this is the two-minute default.
    assert!(
        elapsed < Duration::from_secs(3),
        "the read took {}ms, which is not a bounded call",
        elapsed.as_millis(),
    );
}

/// What the guard does **not** cover, recorded rather than left to be
/// rediscovered: a provider inside this process.
///
/// `SetTransactionTimeout` bounds the client's cross-process calls. A window on
/// another thread of the same process is reached without one, so it blocks for
/// as long as it likes. Harmless here because Lens refuses to read itself
/// (`AppPolicy::deny_self`, and without it the card translates its own
/// translation), but it is the difference between a guard and a guarantee.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_guard_does_not_reach_a_provider_inside_this_process() {
    use std::time::{Duration, Instant};

    use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
    use lens_core::hover::Generation;

    use crate::protected_harness::{centre, ProtectedHarness};

    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));
    let harness = ProtectedHarness::start().expect("harness");
    let point = centre(harness.fields().plain);

    harness.stall_for(Duration::from_secs(3));
    std::thread::sleep(Duration::from_millis(150));

    let began = Instant::now();
    let wedged = acquirer.acquire(point, &context).err();
    let elapsed = began.elapsed();
    println!(
        "same process, wedged: {wedged:?} in {}ms",
        elapsed.as_millis(),
    );

    assert_ne!(
        wedged,
        Some(AcquireError::Timeout),
        "the in-process path is now bounded too. Good news, but this test \
         records the opposite; check what changed before deleting it.",
    );
    assert!(
        elapsed >= Duration::from_secs(2),
        "the call returned in {}ms, so it was not actually blocked and this \
         test is measuring nothing",
        elapsed.as_millis(),
    );
}

const WEDGE_ENV: &str = "MORI_LENS_WEDGE";
const WEDGE_TEST: &str = "live_tests::be_the_wedged_application";

/// Not a test. The other half of the cross-process timeout test above, which
/// re-runs this binary with [`WEDGE_ENV`] set so there is a second process to
/// wedge.
///
/// It lives here rather than in a separate helper binary so that the window it
/// puts up is the same `ProtectedHarness` every other live test uses, and so
/// that nothing has to be built before the suite can run.
#[test]
#[ignore = "not a test; the wedged process for the timeout test"]
fn be_the_wedged_application() {
    use std::io::Write;
    use std::time::Duration;

    use crate::protected_harness::{centre, ProtectedHarness};

    if std::env::var(WEDGE_ENV).is_err() {
        println!("not the wedge process; nothing to do");
        return;
    }

    let harness = ProtectedHarness::start().expect("harness");
    let point = centre(harness.fields().plain);
    // The window id too, so the parent can prove it is reading this window and
    // not whatever happens to be on top of it. Measuring the occluder instead
    // of the target is a mistake this repository has already made once.
    println!(
        "WEDGE-READY {} {} {}",
        point.x,
        point.y,
        harness.window_id()
    );
    let _ = std::io::stdout().flush();

    // Long enough for the parent to take its healthy reading.
    std::thread::sleep(Duration::from_secs(2));

    println!("WEDGE-STALLING");
    let _ = std::io::stdout().flush();
    harness.stall_for(Duration::from_secs(20));
    std::thread::sleep(Duration::from_secs(25));
}

/// Milestone 2's gate condition: Lens never reads its own overlay.
///
/// If it did, the loop would translate its own translation and the second card
/// would explain the first. Two mechanisms are supposed to stop it:
///
/// - `WS_EX_TRANSPARENT` on the overlay, so hit testing passes through it and
///   `ElementFromPoint` never reaches it. This is the one that runs.
/// - `AppPolicy::deny_self`, refusing by process name if anything ever does
///   reach it. The backstop, checked separately below.
///
/// The click-through test already proves **Win32** hit testing skips the card.
/// UI Automation does its own hit testing and is not obliged to agree, so this
/// asks it directly.
///
/// **It aims the card at text rather than hoping.** The first version put a
/// card over an empty patch of desktop, read nothing from any of nine samples,
/// and passed: proving a dead region stays dead proves nothing. The second
/// tried to cover the anchor point and could not, because the card is placed
/// *beside* the text it explains, on purpose. So this measures where placement
/// puts a card, then works backwards to an anchor that lands the card on a
/// point already known to read.
fn one_attempt_at_reading_through_a_card() -> Result<String, String> {
    use std::time::{Duration, Instant};

    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::translate::{EngineId, QuickTranslation};

    use crate::window::{root_window_at, top_level_windows};

    // This test's own strings, distinctive enough that finding either in a
    // result is unambiguous.
    const ORIGINAL: &str = "Zarvex qulmite brandophane, the seventh.";
    const TRANSLATED: &str = "자르벡스 퀄마이트 브랜도페인, 일곱 번째.";

    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));

    // Find a coordinate on this desktop that reads real text. Bounded, because
    // a screen with no prose on it would otherwise scan for minutes.
    // A budget for the whole search, and one per window inside it.
    //
    // Without the second, the scan spends everything on whichever window comes
    // first: measured, 2111 points across 3 windows in 15 seconds, which is
    // one full-screen window's grid and never reaches the other two. A desktop
    // with text on it was reported as having none.
    let searched = Instant::now();
    let deadline = searched + Duration::from_secs(20);
    const PER_WINDOW: Duration = Duration::from_secs(4);
    let mut anchored = None;
    let mut probed = 0usize;
    let mut windows = 0usize;
    'window: for window in top_level_windows() {
        windows += 1;
        let window_deadline = Instant::now() + PER_WINDOW;
        let bounds = window.client_bounds;
        let mut y = bounds.y + 12;
        while y < bounds.bottom() - 12 {
            let mut x = bounds.x + 12;
            while x < bounds.right() - 12 {
                let now = Instant::now();
                if now > deadline {
                    break 'window;
                }
                if now > window_deadline {
                    continue 'window;
                }
                let point = ScreenPoint::new(x, y);
                if root_window_at(point) == Some(window.id) {
                    probed += 1;
                    match acquirer.acquire(point, &context) {
                        Ok(target) => {
                            if target.segmented.context().trim().chars().count() >= 12 {
                                anchored = Some(point);
                                break 'window;
                            }
                        }
                        // Give up on this window entirely. Another check in
                        // this file deliberately wedges an application, and a
                        // point over it costs the full guard: at 800ms a
                        // sample the search spent its whole budget on one
                        // unresponsive window and reported that the desktop
                        // had no text on it.
                        Err(AcquireError::Timeout) => continue 'window,
                        Err(_) => {}
                    }
                }
                x += 32;
            }
            y += 32;
        }
    }

    let Some(point) = anchored else {
        return Err(format!(
            "no text found: {probed} points across {windows} windows in {}ms{}",
            searched.elapsed().as_millis(),
            if Instant::now() > deadline {
                ", stopped at the budget"
            } else {
                ""
            },
        ));
    };

    let overlay = crate::Overlay::start().expect("overlay");
    let make_card = || {
        Card::translated(
            ORIGINAL,
            &QuickTranslation {
                text: TRANSLATED.to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:3b".to_owned(),
                latency_ms: 900,
                cached: false,
            },
        )
    };

    // Learn where placement puts a card relative to its anchor, by showing one
    // and measuring. Placement is a pure function in `lens-core`, but its
    // output in physical pixels depends on the monitor and the DPI, so it is
    // measured here rather than assumed.
    let probe_anchor = ScreenRect::new(point.x, point.y, 200, 20);
    assert!(overlay.show(make_card(), probe_anchor));
    std::thread::sleep(Duration::from_millis(500));
    let first = card_rect(&overlay);
    assert!(overlay.hide());
    std::thread::sleep(Duration::from_millis(300));

    let offset = (first.left - probe_anchor.x, first.top - probe_anchor.y);
    let size = (first.right - first.left, first.bottom - first.top);

    // Now aim: put the card's centre on the text that just answered. Without
    // this the card lands wherever placement puts it, which on one run was
    // nine points of empty desktop, and a test that cannot see any text cannot
    // answer the question it exists to ask.
    let aimed = ScreenRect::new(
        point.x - offset.0 - size.0 / 2,
        point.y - offset.1 - size.1 / 2,
        200,
        20,
    );

    // What is at that point with no card in the way, twice.
    //
    // Twice because this test runs against a live desktop that nothing here
    // controls: a page can scroll, a caret can blink into the text, another
    // test's window can appear. When that happens the reading changes for a
    // reason that has nothing to do with the card, and the assertion below
    // would report it as the card altering what the accessibility layer sees,
    // which is a serious claim and would be a false one. It failed exactly
    // that way once.
    let read_point = || {
        acquirer
            .acquire(point, &context)
            .map(|target| target.segmented.context().to_owned())
    };
    let Ok(baseline) = read_point() else {
        return Err("the point stopped reading between the search and the test".to_owned());
    };
    if read_point().ok().as_deref() != Some(baseline.as_str()) {
        return Err(format!(
            "the text at ({}, {}) changed between two reads with no card involved",
            point.x, point.y,
        ));
    }

    assert!(overlay.show(make_card(), aimed));
    std::thread::sleep(Duration::from_millis(500));
    let placed = card_rect(&overlay);
    let covered = point.x >= placed.left
        && point.x < placed.right
        && point.y >= placed.top
        && point.y < placed.bottom;

    let through = acquirer.acquire(point, &context);
    let _ = overlay.hide();

    if !covered {
        // Placement clamps into the work area, which is correct. It means this
        // particular point cannot be used, not that anything is wrong.
        return Err(format!(
            "the card aimed at ({}, {}) landed at {:?} and did not cover it",
            point.x,
            point.y,
            (placed.left, placed.top, placed.right, placed.bottom),
        ));
    }

    let Ok(read) = through else {
        return Err("the point stopped reading while the card was up".to_owned());
    };
    let text = read.segmented.context().to_owned();

    // Compared, never printed. This is the product's own card, and the rule
    // against logging captured text does not carve out exceptions.
    assert!(
        !text.contains(ORIGINAL) && !text.contains(TRANSLATED),
        "the acquirer read the card's own text through UI Automation, so Lens \
         would translate its own translation",
    );
    assert_eq!(
        text, baseline,
        "the point read different text once a card was over it, so the card \
         changes what the accessibility layer sees even though it is never \
         itself read",
    );

    Ok(format!(
        "{} chars at ({}, {}), read identically with a card drawn over that \
         exact point, and none of it the card's",
        baseline.chars().count(),
        point.x,
        point.y,
    ))
}

/// Milestone 2's gate condition, retried because the desktop is not ours.
///
/// The attempt above searches a live desktop for real text. A page that
/// scrolls, a caret that blinks into the line, or another check's window
/// appearing all change the answer for reasons that have nothing to do with the
/// card. Measured: it passed three runs out of three on its own and failed
/// about one full suite in three.
///
/// Retrying rather than loosening the assertions is the whole point. The claim
/// this makes, that a card does not change what the accessibility layer sees,
/// would fail on **every** attempt if it were false. A desktop that moved fails
/// one attempt and passes the next.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_point_that_reads_text_still_reads_it_through_a_card_on_top() {
    let mut reasons = Vec::new();
    for attempt in 1..=4 {
        match one_attempt_at_reading_through_a_card() {
            Ok(found) => {
                println!("{found} (attempt {attempt})");
                return;
            }
            Err(reason) => {
                println!("attempt {attempt}: {reason}");
                reasons.push(reason);
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        }
    }
    panic!(
        "four attempts and none could be run, so nothing was tested:\n  {}",
        reasons.join("\n  "),
    );
}

/// Where a shown card actually is, in physical screen pixels.
fn card_rect(overlay: &crate::Overlay) -> windows::Win32::Foundation::RECT {
    let mut rect = windows::Win32::Foundation::RECT::default();
    // SAFETY: a read-only query for the overlay's own rectangle.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowRect(
            windows::Win32::Foundation::HWND(overlay.window_id() as *mut _),
            &mut rect,
        )
        .expect("window rect");
    }
    rect
}

/// The backstop, in case the overlay ever stops being click-through.
///
/// `deny_self` is what refuses by name, and it is the mechanism that would
/// have to work if `WS_EX_TRANSPARENT` were ever dropped. Cheap to check, and
/// it is checked here rather than only in `lens-core` because the name it has
/// to match is the one this platform reports.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_running_process_name_is_the_one_the_policy_denies() {
    use lens_core::policy::{AppPolicy, AppTier};

    let overlay = crate::Overlay::start().expect("overlay");
    let point = {
        let mut rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: a read-only query for the window's own rectangle.
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(
                windows::Win32::Foundation::HWND(overlay.window_id() as *mut _),
                &mut rect,
            )
            .expect("window rect");
        }
        ScreenPoint::new((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2)
    };

    // Whatever Windows says is at that point, which for a click-through card
    // is the window underneath it.
    let underneath = crate::window::identity(crate::window::window_at(point));
    println!("under the card: {:?}", underneath.process_name);

    // And what this process is called, which is the string the product passes
    // to `deny_self`.
    let own = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .expect("this process has a name");
    println!("this process: {own}");

    let mut policy = AppPolicy::new(AppTier::Allow);
    policy.deny_self(&own);
    assert_eq!(
        policy.tier_for(Some(&own)),
        AppTier::Deny,
        "deny_self did not deny the name this process actually runs under, so \
         the backstop would not fire",
    );

    assert!(overlay.hide());
}

/// The tray icon reaches the notification area, and a click on it is heard.
///
/// `Tray::start` returning `Some` already means the shell accepted `NIM_ADD`,
/// which is most of what can be asserted about an icon without a camera. What
/// it does not cover is the half this module actually got wrong the first time
/// it was written: decoding the callback. The shell packs the mouse message
/// into the **low word of `lParam`**, and reading `wParam` instead, or the high
/// word, produces a tray icon that appears perfectly and does nothing.
///
/// So the click is replayed rather than performed: the same message with the
/// same packing the shell sends, posted to the same window.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_click_on_the_tray_icon_becomes_a_request() {
    use std::time::Duration;

    use lens_core::icon::Glyph;
    use lens_core::theme::Rgb;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_LBUTTONUP};

    use crate::tray::{Tray, TrayEvent, WM_TRAY};

    let ink = Rgb {
        r: 120,
        g: 170,
        b: 255,
    };
    let Some(tray) = Tray::start(ink, "Mori Lens · 켜짐") else {
        panic!("the shell refused the icon, so Lens would run invisibly");
    };
    println!("icon accepted by the shell");

    // Nothing has been clicked.
    assert!(
        tray.poll().is_empty(),
        "the tray reported an event nobody caused",
    );

    // SAFETY: posting a documented message to a window this test owns, with
    // the payload the shell uses.
    unsafe {
        let _ = PostMessageW(
            Some(windows::Win32::Foundation::HWND(tray.window_id() as *mut _)),
            WM_TRAY,
            WPARAM(0),
            LPARAM(WM_LBUTTONUP as isize),
        );
    }

    // The pump is on its own thread, so the answer arrives when it arrives.
    let mut seen = Vec::new();
    for _ in 0..40 {
        seen.extend(tray.poll());
        if !seen.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    println!("after a left click: {seen:?}");
    assert_eq!(
        seen,
        vec![TrayEvent::TogglePause],
        "a left click on the icon did not become a pause request, so the icon \
         is decoration",
    );

    // And the state can change, which is what makes the icon honest about
    // whether anything is being read.
    assert!(
        tray.set_state(Glyph::Paused, "Mori Lens · 일시정지", ink),
        "the icon could not be updated, so it would keep claiming to listen",
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(tray.set_state(Glyph::Listening, "Mori Lens · 켜짐", ink));
    std::thread::sleep(Duration::from_millis(300));
    println!("state changed to paused and back");

    // Dropping it must take the icon out of the tray. A leftover icon that
    // does nothing when clicked is the most annoying way to fail.
    drop(tray);
    println!("icon removed on drop");
}

/// The card is drawn with real per-pixel alpha, not a uniform window opacity.
///
/// Every other overlay test passes whether Direct2D ran or the GDI fallback
/// did, which makes them useless for the one thing this change is about. A
/// renderer that silently fell back would look identical to every assertion in
/// this file.
///
/// So this asserts the two things only the alpha path can produce:
///
/// - the window is **larger than the card**, by the shadow margin, and the
///   extra pixels are almost entirely transparent
/// - the window's corner and its centre are **different**, which for a GDI card
///   filling its whole client area with one brush they cannot be
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_card_is_painted_with_per_pixel_alpha_and_not_a_flat_rectangle() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    assert!(
        crate::glass::Glass::new().is_some(),
        "Direct2D could not start on this machine, so the card is the GDI \
         fallback and this test has nothing to measure. That is a supported \
         state for the product and not for this assertion.",
    );

    let overlay = crate::Overlay::start().expect("overlay");
    let anchor = ScreenRect::new(500, 400, 400, 22);
    let card = Card::translated(
        "The output files came back empty, so the suite produced nothing at all.",
        &QuickTranslation {
            text: "출력 파일이 비어 있어서 테스트 스위트는 아무것도 만들지 않았습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1090,
            cached: false,
        },
    );
    let second = Card::translated(
        "The output files came back empty, so the suite produced nothing at all.",
        &QuickTranslation {
            text: "출력 파일이 비어 있어서 테스트 스위트는 아무것도 만들지 않았습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1090,
            cached: false,
        },
    );
    assert!(overlay.show(card, anchor));
    std::thread::sleep(std::time::Duration::from_millis(800));

    // SAFETY: read-only query against this process's own window.
    let (width, height) = unsafe {
        let mut rect = RECT::default();
        GetWindowRect(HWND(overlay.window_id() as *mut _), &mut rect).expect("window rect");
        (rect.right - rect.left, rect.bottom - rect.top)
    };

    // What the renderer says this card should be, so the comparison is against
    // the card that was drawn rather than against a fixed number. The card
    // fits its contents now, so a constant would only be right by accident.
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Balanced,
        TextScale::Normal,
        SystemPreferences::default(),
    );
    let glass = crate::glass::Glass::new().expect("Direct2D");
    let laid = glass
        .lay_out(&second, &theme, crate::overlay::max_card_width(), 1.0)
        .expect("layout");
    println!(
        "window {width}x{height}, card {}x{}",
        laid.width, laid.height
    );

    // The shadow needs room, so the window has to be wider than the card. A
    // window exactly the card's width means the Direct2D path did not draw
    // this frame, which is what happened the first time: the window had
    // `SetLayeredWindowAttributes` called on it at creation, which makes
    // `UpdateLayeredWindow` fail, and every other overlay test stayed green.
    assert!(
        width > laid.width,
        "the window is {width}px wide and the card is {}px, so there is no \
         shadow margin and this is the GDI fallback",
        laid.width,
    );
    assert!(overlay.hide());

    // And the pixels themselves. Not read back from the window: once
    // `UpdateLayeredWindow` has the bitmap the content lives in the
    // compositor and `GetPixel` on the window answers CLR_INVALID.
    // SAFETY: the renderer creates and releases its own objects.
    let (w, h, pixels) = unsafe { glass.pixels(&laid, &theme) }.expect("pixels");
    let alpha = |x: i32, y: i32| pixels[(y * w + x) as usize] >> 24;

    let corner = alpha(0, 0);
    let middle = alpha(w / 2, h / 2);
    let opaque = pixels.iter().filter(|pixel| (*pixel >> 24) == 255).count();
    println!(
        "corner alpha {corner} · card alpha {middle} · surface_alpha {} · {opaque} fully opaque pixels",
        theme.surface_alpha,
    );

    // Outside the rounded outline and outside the shadow's reach.
    assert_eq!(
        corner, 0,
        "the window's corner is not transparent, so the card is a rectangle          with a visible box around it",
    );

    // The surface is translucent, at the theme's own figure. Exactly, which
    // is the point: the first version filled its shadow rings rather than
    // stroking them, so fourteen of them stacked under the card and it
    // measured 250 where the theme asked for 236. A theme's opacity that the
    // renderer quietly adjusts is not a setting.
    assert_eq!(
        middle,
        u32::from(theme.surface_alpha),
        "the surface is not being drawn at the theme's opacity",
    );

    // The headline claim of this whole renderer: the text is opaque while the
    // surface under it is not. With `SetLayeredWindowAttributes` this number
    // is zero, because one opacity applies to every pixel in the window.
    assert!(
        opaque > 200,
        "only {opaque} fully opaque pixels, so the text is being faded along          with the surface and nothing was gained",
    );
    assert!(
        u32::from(theme.surface_alpha) < 255,
        "this theme is opaque, so the test above proves nothing; pick a          translucent one",
    );
    assert!(overlay.hide());
}

/// Layout comes from the renderer that is going to draw it.
///
/// Measuring with `DrawTextW` and drawing with DirectWrite is how a card ends
/// up with its last line clipped: the two engines break lines differently, and
/// the difference only shows on the paragraph lengths nobody tested.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_longer_card_is_taller_and_the_renderer_is_the_one_that_says_so() {
    use lens_core::card::Card;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Balanced,
        TextScale::Normal,
        SystemPreferences::default(),
    );

    let make = |translation: &str| {
        Card::translated(
            "A sentence.",
            &QuickTranslation {
                text: translation.to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:3b".to_owned(),
                latency_ms: 900,
                cached: false,
            },
        )
    };

    let short = glass
        .lay_out(&make("짧은 문장."), &theme, 360, 1.0)
        .expect("layout");
    let long = glass
        .lay_out(
            &make(
                "훨씬 더 긴 문장입니다. 이 문장은 카드 너비를 넘어가서 여러 줄로 \
                 나뉘어야 하고, 그만큼 카드도 높아져야 합니다. 그렇지 않으면 \
                 마지막 줄이 잘립니다.",
            ),
            &theme,
            360,
            1.0,
        )
        .expect("layout");

    println!("short {}px · long {}px", short.height, long.height);
    assert!(
        long.height > short.height + 20,
        "a paragraph that must wrap did not make the card taller: {} vs {}",
        long.height,
        short.height,
    );

    // And scaling is real, not decorative: the same card on a 150% display is
    // half again as tall.
    let scaled = glass
        .lay_out(&make("짧은 문장."), &theme, 360, 1.5)
        .expect("layout");
    println!("the same short card at 150%: {}px", scaled.height);
    assert!(
        scaled.height > short.height,
        "the card ignored the display scaling",
    );
}

/// Autostart writes, reads back, and removes cleanly.
///
/// **Against a scratch key, never the run key.** A test that switched on
/// autostart to prove it could would leave a machine starting Lens at login
/// because somebody ran the suite, which is exactly what
/// `crate::autostart` refuses to do without being asked.
///
/// What it covers is the part that is actually easy to get wrong: the size
/// query, the second call with the allocated buffer, and the UTF-16 that comes
/// back with its terminator still on it.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn autostart_writes_reads_and_removes_its_own_value() {
    let (written, read_back, deleted) = crate::autostart::round_trip_against_a_scratch_key();
    println!("written {written} · read back {read_back} · deleted {deleted}");

    assert!(written, "the value could not be written");
    assert!(
        read_back,
        "the value came back different from what was written, which for a \
         command line means Windows would run something else at login",
    );
    assert!(deleted, "the value could not be removed");
    assert!(
        crate::autostart::scratch_is_gone(),
        "the scratch value is still there after being deleted",
    );

    // And the real setting is untouched by all of that.
    println!("autostart is currently {}", crate::autostart::is_enabled());
}

/// The card is the size of what is in it.
///
/// A fixed box holding one short line is mostly empty, and empty space where an
/// answer should be reads as something that failed to load. This is the check
/// that the width is chosen rather than assumed.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_short_answer_gets_a_narrow_card_and_a_long_one_gets_a_wide_card() {
    use lens_core::card::Card;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Balanced,
        TextScale::Normal,
        SystemPreferences::default(),
    );
    let widest = crate::overlay::max_card_width();

    let card = |original: &str, translated: &str| {
        Card::translated(
            original,
            &QuickTranslation {
                text: translated.to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:3b".to_owned(),
                latency_ms: 900,
                cached: false,
            },
        )
    };

    let short = glass
        .lay_out(&card("Sign in", "로그인"), &theme, widest, 1.0)
        .expect("layout");
    let long = glass
        .lay_out(
            &card(
                "The output files came back empty, so the full test suite produced nothing at all.",
                "출력 파일이 비어 있었기 때문에 전체 테스트 스위트는 아무것도 생성하지 않았습니다.",
            ),
            &theme,
            widest,
            1.0,
        )
        .expect("layout");

    println!(
        "short {}x{} · long {}x{} · limit {widest}",
        short.width, short.height, long.width, long.height,
    );

    assert!(
        short.width < long.width,
        "both cards came out the same width, so the width is not being fitted",
    );
    assert!(
        long.width <= widest,
        "a long answer was given {}px, past the {widest}px limit",
        long.width,
    );
    // And not so narrow that the footer wraps under it.
    assert!(
        short.width >= 200,
        "a short answer shrank to {}px, which is a tooltip",
        short.width,
    );
}

/// The card scales with the display, not just the text on it.
///
/// The width was a fixed pixel count while the fonts and padding were scaled,
/// so on a 150% monitor the same sentence wrapped into more lines inside the
/// same box: a card that was physically narrower and taller than the one on the
/// 100% monitor beside it.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_same_card_is_the_same_physical_size_on_a_scaled_display() {
    use lens_core::card::Card;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Balanced,
        TextScale::Normal,
        SystemPreferences::default(),
    );
    let card = Card::translated(
        "The output files came back empty, so the full test suite produced nothing at all.",
        &QuickTranslation {
            text:
                "출력 파일이 비어 있었기 때문에 전체 테스트 스위트는 아무것도 생성하지 않았습니다."
                    .to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 900,
            cached: false,
        },
    );

    let widest = crate::overlay::max_card_width() as f32;
    let plain = glass
        .lay_out(&card, &theme, widest as i32, 1.0)
        .expect("layout");
    let scaled = glass
        .lay_out(&card, &theme, (widest * 1.5).round() as i32, 1.5)
        .expect("layout");

    println!(
        "100%: {}x{} · 150%: {}x{}",
        plain.width, plain.height, scaled.width, scaled.height,
    );

    // Half again as wide, within a pixel or two of rounding.
    let expected = plain.width as f32 * 1.5;
    assert!(
        (scaled.width as f32 - expected).abs() <= 4.0,
        "at 150% the card is {}px where {expected:.0}px would be the same \
         physical width",
        scaled.width,
    );
    // And therefore not dramatically taller: the same words fit the same way.
    assert!(
        (scaled.height as f32) < plain.height as f32 * 1.8,
        "at 150% the card is {}px tall against {}px, so the text is wrapping \
         differently and the box is not scaling with it",
        scaled.height,
        plain.height,
    );
}

/// The per-sample process lookup is cached, and still correct.
///
/// `process_name_at` runs on every pointer sample, forty times a second, and
/// it decides whether the policy permits a read at all. It used to open a
/// process handle every time: measured on 2026-08-05, the product spent
/// **1.56% of a core doing nothing**, which on a laptop is a background tool
/// that gets uninstalled.
///
/// Two things have to hold together. It has to be fast, and it has to be the
/// **right name**, because a wrong one hands an application the wrong tier.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_process_under_the_pointer_is_cached_without_going_stale() {
    use std::time::Instant;

    use crate::protected_harness::{centre, ProtectedHarness};

    let harness = ProtectedHarness::start().expect("harness");
    let point = centre(harness.fields().plain);

    let own = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .expect("this process has a name");

    let first = crate::window::process_name_at(point);
    println!("under the pointer: {first:?}, this process is {own}");
    assert_eq!(
        first.as_deref(),
        Some(own.as_str()),
        "the window under the point belongs to this test, so this is the name \
         the policy would be given",
    );

    // Two thousand of each, back to back, so both pay the same machine.
    //
    // An absolute microsecond threshold was tried first and was flaky: it
    // passed alone at 8.2us against a 10us bound and failed inside the full
    // suite. Most of a cached call is `WindowFromPoint` and
    // `GetWindowThreadProcessId`, which happen either way, so the figure moves
    // with how busy the box is. A ratio does not.
    const SAMPLES: u32 = 2_000;
    let mut wrong = 0usize;

    let began = Instant::now();
    for _ in 0..SAMPLES {
        if crate::window::process_name_at(point).as_deref() != Some(own.as_str()) {
            wrong += 1;
        }
    }
    let cached = began.elapsed().as_secs_f64() * 1e6 / f64::from(SAMPLES);

    let began = Instant::now();
    for _ in 0..SAMPLES {
        if crate::window::uncached_process_name_at(point).as_deref() != Some(own.as_str()) {
            wrong += 1;
        }
    }
    let uncached = began.elapsed().as_secs_f64() * 1e6 / f64::from(SAMPLES);

    println!(
        "{SAMPLES} each · cached {cached:.1}us · uncached {uncached:.1}us · {:.1}x · {wrong} wrong",
        uncached / cached.max(0.001),
    );

    assert_eq!(wrong, 0, "the name changed under repeated lookups");
    assert!(
        uncached > cached * 2.0,
        "cached {cached:.1}us against uncached {uncached:.1}us, so the cache is \
         not saving the process handle the loop opens forty times a second",
    );
}

/// A theme set while the overlay is running reaches the next card.
///
/// `Overlay::set_theme` writing into a static is easy to believe and easy to
/// get wrong: the painter resolves its theme when it draws, and if it read a
/// copy taken at startup instead, this would store the new theme and go on
/// painting the old one with nothing failing.
///
/// The signal is the card's **size**. A larger text scale is a taller card, so
/// the window measured after the change says whether the painter saw it.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_theme_set_at_runtime_changes_the_next_card() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenRect;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let theme_at = |scale| {
        Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            scale,
            SystemPreferences::default(),
        )
    };

    let overlay = crate::Overlay::start().expect("overlay");
    let card = Card::translated(
        "The output files came back empty, so the suite produced nothing at all.",
        &QuickTranslation {
            text: "출력 파일이 비어 있어서 테스트 스위트는 아무것도 만들지 않았습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 900,
            cached: false,
        },
    );
    let anchor = ScreenRect::new(500, 300, 300, 22);

    let measure = || {
        // SAFETY: a read-only query on this process's own window.
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(HWND(overlay.window_id() as *mut _), &mut rect).expect("window rect");
            rect.bottom - rect.top
        }
    };

    overlay.set_theme(theme_at(TextScale::Normal));
    assert!(overlay.show(card.clone(), anchor));
    std::thread::sleep(std::time::Duration::from_millis(500));
    let normal = measure();

    overlay.set_theme(theme_at(TextScale::Larger));
    assert!(overlay.show(card, anchor));
    std::thread::sleep(std::time::Duration::from_millis(500));
    let larger = measure();

    println!("normal {normal}px tall · larger {larger}px tall");
    assert!(
        larger > normal,
        "the same card is {larger}px at the larger text scale and {normal}px at \
         the normal one, so the painter never saw the new theme",
    );

    assert!(overlay.hide());
}

/// What a very long sentence does to the card.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_what_a_long_paragraph_does_to_the_card() {
    use lens_core::card::Card;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Balanced,
        TextScale::Normal,
        SystemPreferences::default(),
    );

    let long = "The accessibility layer is the first path Mori Lens tries, because it \
                returns the characters an application already knows it is drawing rather \
                than a guess recovered from pixels, and that difference is the whole \
                reason the OCR fallback is a fallback rather than the design. A second \
                sentence exists so that expansion to an enclosing unit has somewhere to \
                stop, and a third so the paragraph is genuinely longer than any card \
                could hold at a readable size.";
    let card = Card::translated(
        long,
        &QuickTranslation {
            text: "접근성 계층은 Mori Lens가 가장 먼저 시도하는 경로입니다. 픽셀에서 \
                   복원한 추측이 아니라 애플리케이션이 이미 그리고 있다고 알고 있는 문자를 \
                   돌려주기 때문이고, 그 차이가 OCR 대체 경로를 설계가 아니라 대체 경로로 \
                   만드는 이유입니다. 두 번째 문장은 둘러싸는 단위로의 확장이 멈출 곳을 \
                   갖도록 존재하고, 세 번째 문장은 문단이 어떤 카드보다도 길어지도록 \
                   존재합니다."
                .to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 900,
            cached: false,
        },
    );

    let laid = glass
        .lay_out(&card, &theme, crate::overlay::max_card_width(), 1.0)
        .expect("layout");
    println!(
        "a {}-character paragraph lays out {}px tall; the card's limit is {}px",
        long.chars().count(),
        laid.height,
        crate::overlay::max_card_height(),
    );
}

/// Windows' own OCR, from a binary built the way `lens.exe` is.
///
/// [#62](https://github.com/swarea/mori-lens/issues/62) settled the question
/// that blocked this milestone: `Windows.Media.Ocr` documents a package
/// identity requirement for desktop applications, and Lens ships one loose
/// exe. A cargo test binary has no package identity either.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn windows_ocr_is_available_and_speaks_both_languages() {
    println!("languages: {:?}", crate::ocr::available_languages());
    println!("korean:  {}", crate::ocr::can_recognise("ko"));
    println!("english: {}", crate::ocr::can_recognise("en-US"));

    // Measured rather than asserted: which recognition packs are installed is
    // a property of the machine, and a suite that fails on somebody's laptop
    // for that reason teaches them to ignore it.
    assert!(
        !crate::ocr::available_languages().is_empty(),
        "no recognisers at all, so this backend cannot serve anyone here",
    );
}

/// Capture a rectangle, read it, and get words back where they actually are.
///
/// **The subject is the harness window**, which is this repository's own,
/// opaque, at a fixed position, and now carries a known sentence. The obvious
/// subject was one of Lens's own cards, and it was wrong for a reason worth
/// writing down: the card is **translucent**, so capturing its rectangle
/// captures the desktop showing through it. The first run of this check read
/// back `계속`, a word from whatever was behind it.
///
/// Reading the product's own card is also the thing the milestone gate forbids.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_captured_rectangle_comes_back_as_words_in_the_right_places() {
    use lens_core::geometry::{ScreenPoint, ScreenRect};

    use crate::protected_harness::ProtectedHarness;

    let harness = ProtectedHarness::start().expect("harness");
    // Settled, drawn, and on top: the harness raises itself, and a capture
    // taken before it has painted reads whatever it is covering.
    std::thread::sleep(std::time::Duration::from_millis(400));

    let field = harness.fields().plain;
    // A little wider than the control, so a word at the edge is not clipped in
    // half and reported as two.
    let region = ScreenRect::new(
        field.x - 6,
        field.y - 6,
        field.width + 12,
        field.height + 12,
    );

    let capture = crate::capture::rectangle(region).expect("the region could not be captured");
    let read = crate::ocr::read(&capture, ScreenPoint::new(region.x, region.y))
        .expect("the recogniser returned nothing");

    let found = read.text();
    println!(
        "captured {}x{} · read {found:?}",
        region.width, region.height
    );

    let flat: String = found.chars().filter(|c| !c.is_whitespace()).collect();
    let wanted: String = "The output files came back empty"
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(
        flat.contains(&wanted),
        "the harness's own sentence did not come back out of the recogniser: \
         {found:?}",
    );

    // The half that matters more than the text: a word knows where it is.
    // Every rectangle must land inside the region that was captured, or the
    // translation from bitmap coordinates to screen coordinates is wrong and
    // the pointer would resolve to the wrong word.
    let words: Vec<_> = read.lines.iter().flat_map(|line| &line.words).collect();
    assert!(!words.is_empty(), "text but no words");
    for word in &words {
        assert!(
            word.bounds.x >= region.x
                && word.bounds.y >= region.y
                && word.bounds.right() <= region.right() + 2
                && word.bounds.bottom() <= region.bottom() + 2,
            "{:?} at {:?} is outside the captured region {region:?}",
            word.text,
            word.bounds,
        );
    }

    // And pointing at the middle of a word resolves to that word, which is the
    // whole reason the rectangles are carried at all.
    let target = words
        .iter()
        .find(|word| word.text.chars().count() >= 5)
        .expect("a word worth pointing at");
    let middle = ScreenPoint::new(
        target.bounds.x + target.bounds.width / 2,
        target.bounds.y + target.bounds.height / 2,
    );
    let (_, resolved) = read.word_at(middle).expect("a word under the pointer");
    println!(
        "pointing at the middle of {:?} resolved {:?}",
        target.text, resolved.text,
    );
    assert_eq!(
        resolved.text, target.text,
        "a point in the middle of a word resolved to a different one",
    );
}

/// What a capture of a translucent card actually contains.
///
/// Recorded rather than fixed, because it is not a defect: a card drawn with
/// per-pixel alpha lets the desktop through, and `BitBlt` from the screen sees
/// the composited result. It matters for the milestone, where the card has to
/// be **hidden** before a region under it is captured, and it is the kind of
/// thing that is obvious once seen and expensive to discover later.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_what_a_capture_sees_through_the_card() {
    use lens_core::card::Card;
    use lens_core::geometry::{ScreenPoint, ScreenRect};
    use lens_core::translate::{EngineId, QuickTranslation};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    const KOREAN: &str = "출력 파일이 비어 있었습니다.";

    let overlay = crate::Overlay::start().expect("overlay");
    assert!(overlay.show(
        Card::translated(
            "The output files came back empty.",
            &QuickTranslation {
                text: KOREAN.to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:3b".to_owned(),
                latency_ms: 900,
                cached: false,
            },
        ),
        ScreenRect::new(400, 300, 300, 22),
    ));
    std::thread::sleep(std::time::Duration::from_millis(700));

    // SAFETY: a read-only query on this process's own window.
    let rect = unsafe {
        let mut rect = RECT::default();
        GetWindowRect(HWND(overlay.window_id() as *mut _), &mut rect).expect("window rect");
        rect
    };
    let region = ScreenRect::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    );
    let capture = crate::capture::rectangle(region).expect("capture");
    let read = crate::ocr::read(&capture, ScreenPoint::new(region.x, region.y)).expect("read");
    let _ = overlay.hide();

    let flat: String = read.text().chars().filter(|c| !c.is_whitespace()).collect();
    let wanted: String = KOREAN.chars().filter(|c| !c.is_whitespace()).collect();
    // Shape only. The first version of this printed what it read, and what it
    // read was the desktop behind the card: an editor, a conversation. A check
    // is not exempt from `docs/privacy-model.md` §3 for being a check, and
    // this is the single most likely place in the repository to break it.
    println!(
        "through the card: {} lines, {} words, {} characters · the card's own \
         text recovered: {}",
        read.lines.len(),
        read.lines
            .iter()
            .map(|line| line.words.len())
            .sum::<usize>(),
        flat.chars().count(),
        flat.contains(&wanted),
    );
    assert!(
        !flat.contains(&wanted),
        "the card's own text came back, so it is opaque after all and the \
         milestone would not need to hide it before capturing underneath",
    );
}
