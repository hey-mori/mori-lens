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
            part_was_removed: false,
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
            part_was_removed: false,
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
            part_was_removed: false,
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
    let anchor = ScreenRect::new(500, 400, 300, 22);
    let original = "The output files came back empty, so the full test suite produced nothing.";

    /// How dense the sampling has to be before a pass means anything.
    ///
    /// The bug hid the card for one 25ms loop tick, so a sampler slower than
    /// this could miss it and report nothing wrong.
    const FINE_ENOUGH_MS: f64 = 20.0;

    let mut attempts = Vec::new();
    let mut observed = None;
    // **Twice, and the retry is about the machine rather than the product.**
    // A sampler that did not get enough turns has not measured anything, and
    // reporting that as a card flickering would blame the product for a busy
    // afternoon. This first failed in a suite of forty checks, several of them
    // enlarging large bitmaps immediately before it, and passed alone and on
    // the next run.
    for attempt in 1..=2 {
        crate::overlay::forget_last_frame();
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
            "attempt {attempt}: {} samples every {period_ms:.1}ms · {} hidden · {} blank · thinnest {} opaque",
            seen.samples, seen.hidden, seen.blank, seen.thinnest,
        );
        attempts.push(format!("{} samples every {period_ms:.1}ms", seen.samples));
        if seen.samples > 60 && period_ms < FINE_ENOUGH_MS {
            observed = Some(seen);
            break;
        }
        println!("  too coarse to see a 25ms flicker; the machine was busy, trying again");
    }

    let Some(seen) = observed else {
        panic!(
            "the sampler never got enough turns to see a 25ms flicker, so nothing was              measured: {attempts:?}",
        );
    };
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
                part_was_removed: false,
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
            part_was_removed: false,
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
                part_was_removed: false,
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
/// - the window is **exactly the card**, plus the blank edge, and the
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

    // **Deliberately not the default theme.** The claim under test is that the
    // surface can be translucent while the text on it is not, and the default
    // surface is opaque, which makes every assertion below trivially true. The
    // guard at the end of this function says so; it is what caught this when
    // the default changed and eight fixtures were repointed at it together.
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Clear,
        TextScale::Normal,
        SystemPreferences::default(),
    );
    let overlay = crate::Overlay::start_with(theme).expect("overlay");
    let anchor = ScreenRect::new(500, 400, 400, 22);
    let card = Card::translated(
        "The output files came back empty, so the suite produced nothing at all.",
        &QuickTranslation {
            text: "출력 파일이 비어 있어서 테스트 스위트는 아무것도 만들지 않았습니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1090,
            cached: false,
            part_was_removed: false,
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
            part_was_removed: false,
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
    // **At the anchor's own scaling**, which this laid out at 1.0 while the
    // window was drawn at whatever the monitor is. On a 125% display that made
    // `laid` a fifth smaller than the window, and the width check below then
    // passed for the wrong reason.
    // SAFETY: read-only monitor and DPI queries.
    let scale = unsafe { crate::overlay::scale_for(anchor.center()) };
    let glass = crate::glass::Glass::new().expect("Direct2D");
    let laid = glass
        .lay_out(
            &second,
            &theme,
            (crate::overlay::max_card_width() as f32 * scale).round() as i32,
            scale,
        )
        .expect("layout");
    println!(
        "window {width}x{height}, card {}x{}",
        laid.width, laid.height
    );

    // The window is the card plus its blank edge and nothing else. This is what
    // says the Direct2D path drew the frame **on screen**, rather than an
    // offscreen bitmap that happens to look right: the first time, the window
    // had `SetLayeredWindowAttributes` called on it at creation, which makes
    // `UpdateLayeredWindow` fail, and every other overlay test stayed green
    // while nothing was drawn.
    //
    // It read `width > laid.width`, on the reasoning that the shadow needed
    // room. The shadow is gone and that edge is two pixels, so the comparison
    // stopped being evidence and started passing because `laid` was measured at
    // the wrong scaling. The fallback painter takes the full allowed width, so
    // being narrower than that is the thing a GDI frame cannot be.
    let fallback_width = (crate::overlay::max_card_width() as f32 * scale).round() as i32;
    assert!(
        width < fallback_width,
        "the window is {width}px wide, which is the whole {fallback_width}px the \
         GDI fallback would take, so the frame on screen was not the Direct2D one",
    );
    assert!(
        (width - laid.width).abs() <= 8,
        "the window is {width}px and the card laid out at {}px, so what is on \
         screen is not the frame this measured",
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
        Translucency::DEFAULT,
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
                part_was_removed: false,
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
        Translucency::DEFAULT,
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
                part_was_removed: false,
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
        Translucency::DEFAULT,
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
            part_was_removed: false,
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

    // Alternating rounds, and the winner counted rather than a ratio compared
    // against a threshold.
    //
    // **Two thresholds have now been wrong here.** An absolute microsecond
    // bound passed alone at 8.2us and failed inside the full suite. It was
    // replaced by `uncached > cached * 2.0`, on the reasoning that the shared
    // cost is in both figures so the ratio would hold where the absolute
    // number did not. That reasoning is wrong, and this machine showed it:
    // 33.5us against 53.8us is a working cache at **1.6x**. The shared cost
    // being in both is exactly what makes the ratio move, because it grows
    // with load while the part the cache removes does not.
    //
    // What the cache claims is not a factor, it is that it removes work. So
    // the two are run against each other several times and the cached side has
    // to win nearly every round. Load drifts across a run and lifts both
    // sides; it does not reverse which is cheaper.
    const SAMPLES: u32 = 2_000;
    const ROUNDS: usize = 5;
    let mut wrong = 0usize;
    let mut cached_wins = 0usize;
    let mut cached_each = Vec::new();
    let mut uncached_each = Vec::new();

    for _ in 0..ROUNDS {
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

        if cached < uncached {
            cached_wins += 1;
        }
        cached_each.push(cached);
        uncached_each.push(uncached);
    }

    let best = |mut times: Vec<f64>| -> f64 {
        times.sort_by(f64::total_cmp);
        times[0]
    };
    let cached = best(cached_each);
    let uncached = best(uncached_each);
    println!(
        "{ROUNDS} rounds of {SAMPLES} · best cached {cached:.1}us · best uncached \
         {uncached:.1}us · cached won {cached_wins} of {ROUNDS} · {wrong} wrong",
    );

    assert_eq!(wrong, 0, "the name changed under repeated lookups");
    assert!(
        cached_wins >= ROUNDS - 1,
        "the cached lookup was faster in only {cached_wins} of {ROUNDS} rounds, \
         so it is not saving the process handle the loop opens forty times a \
         second: best cached {cached:.1}us against best uncached {uncached:.1}us",
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
            Translucency::DEFAULT,
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
            part_was_removed: false,
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
        Translucency::DEFAULT,
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
            part_was_removed: false,
        },
    );

    let laid = glass
        .lay_out(&card, &theme, crate::overlay::max_card_width(), 1.0)
        .expect("layout");
    // SAFETY: read-only monitor queries.
    let work_area = unsafe { crate::overlay::work_area_at(ScreenPoint::new(400, 400)) };
    println!(
        "a {}-character paragraph lays out {}px tall; the limit on this screen is {}px",
        long.chars().count(),
        laid.height,
        crate::overlay::max_card_height(work_area),
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
                part_was_removed: false,
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
    // Deliberately not asserted in either direction, and the first version of
    // this asserted one of them. A capture of the card's rectangle is a
    // **mixture**: the surface is 92% opaque, so its own text usually wins,
    // and over a busy background the desktop's text comes through instead.
    // Both have been seen on this machine, minutes apart.
    //
    // Which one wins is not the finding. The finding is that neither is
    // trustworthy, so the milestone hides the card before capturing under it
    // rather than reasoning about what the composite will contain.
    let _ = flat.contains(&wanted);
}

/// What a card's frame actually contains, pixel by pixel.
///
/// **Draws no window.** It renders to a bitmap and reads it, which is the only
/// honest way to answer "why is there a box around it" without asking somebody
/// to stare at their screen again.
#[test]
#[ignore = "renders to a bitmap; no window, run manually"]
fn dump_the_frame_around_a_card() {
    use lens_core::card::Card;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::DEFAULT,
        TextScale::Normal,
        SystemPreferences::default(),
    );
    let card = Card::translated(
        "This reveals a genuine contradiction between the two documents.",
        &QuickTranslation {
            text: "이것은 두 문서 사이의 진짜 모순을 드러냅니다.".to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5-coder:7b".to_owned(),
            latency_ms: 3400,
            cached: false,
            part_was_removed: false,
        },
    );
    let laid = glass
        .lay_out(&card, &theme, crate::overlay::max_card_width(), 1.0)
        .expect("layout");
    // SAFETY: renders into its own bitmap and releases it.
    let (width, height, pixels) = unsafe { glass.pixels(&laid, &theme) }.expect("pixels");

    let at = |x: i32, y: i32| pixels[(y * width + x) as usize];
    let middle = height / 2;

    println!(
        "frame {width}x{height}, card {}x{}",
        laid.width, laid.height
    );
    println!("surface_alpha asked for: {}", theme.surface_alpha);

    // A horizontal slice from the bitmap's edge into the card.
    println!("left edge inwards (x: argb):");
    for x in 0..28.min(width) {
        let p = at(x, middle);
        println!("  {x:>3}  a={:>3}  rgb={:06x}", p >> 24, p & 0x00ff_ffff);
    }

    // And the top-left corner diagonally, which is where a box shows first.
    println!("corner diagonal (n: argb):");
    for n in 0..14 {
        let p = at(n, n);
        println!("  {n:>3}  a={:>3}  rgb={:06x}", p >> 24, p & 0x00ff_ffff);
    }
}

/// How much of the page a card actually takes.
///
/// A user called the card's presence too large, which is a judgement about
/// area rather than about any one number in it. This prints the area, so the
/// next change to padding, gaps or width is answerable with a figure instead of
/// another screenshot.
///
/// The card is the one from that screenshot: two lines of English original, two
/// of Korean answer, and the footer.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_how_much_of_the_page_a_card_covers() {
    use lens_core::card::Card;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::DEFAULT,
        TextScale::Normal,
        SystemPreferences::default(),
    );

    let make = |original: &str, translation: &str| {
        Card::translated(
            original,
            &QuickTranslation {
                text: translation.to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5-coder:7b".to_owned(),
                latency_ms: 1500,
                cached: false,
                part_was_removed: false,
            },
        )
    };
    let card = make(
        "So deploying is what makes yesterday's work real, and that's the safe, obvious \
         first step.",
        "어제의 일은 배포로 현실화되며, 이것이 안전하고 명백한 첫 단계입니다.",
    );

    // Four lines each side, from a screenshot whose footer sat on the card's
    // bottom edge with no padding under it.
    let tall = make(
        "The bigger structural question is understanding how the base and datasheet \
         starters relate to each other, whether one depends on the other or if they're \
         independent.",
        "기본 구조적 질문은 기본과 데이터시트 시작자가 서로 어떻게 관련되어 있는지 이해하는 \
         것입니다. 하나가 다른 것에 의존하는지 아니면 독립적인지.",
    );
    for scale in [1.0f32, 1.5] {
        let widest = (crate::overlay::max_card_width() as f32 * scale).round() as i32;
        let laid = glass.lay_out(&tall, &theme, widest, scale).expect("layout");
        // SAFETY: read-only monitor queries.
        let work_area = unsafe { crate::overlay::work_area_at(ScreenPoint::new(400, 400)) };
        let window = crate::overlay::card_height_for(laid.height, work_area);
        println!(
            "tall  scale {scale}: laid {}x{}, window height {window}, {}",
            laid.width,
            laid.height,
            if window < laid.height {
                "CLIPPED"
            } else {
                "fits"
            },
        );
    }

    // A sentence long enough to test the ceiling. One sentence, not a
    // paragraph: the segmenter hands the card a sentence, so this is what the
    // tallest realistic card is made of.
    let long = make(
        "The migration ran against the production database on the second attempt, after the          first was rolled back because a unique index on the accounts table rejected two rows          that the staging data had never contained, which nobody noticed until the deploy had          already been announced in the channel and three people were watching it.",
        "마이그레이션은 두 번째 시도에서 운영 데이터베이스에 적용됐습니다. 첫 번째는 계정          테이블의 고유 인덱스가 스테이징 데이터에 한 번도 없던 두 행을 거부해서 롤백됐고,          배포가 이미 채널에 공지되어 세 사람이 지켜보고 있을 때까지 아무도 눈치채지          못했습니다.",
    );
    // And a blob with no sentence boundary in it, which is what the segmenter
    // hands back when there is nothing to segment on. KakaoTalk exposes its
    // whole conversation as one region, so this is a real shape rather than a
    // pathological one.
    let blob = make(
        &"one more clause about the deploy and what it did to the accounts table ".repeat(12),
        &"배포와 그것이 계정 테이블에 한 일에 대한 절 하나 더 ".repeat(12),
    );
    for scale in [1.0f32, 1.5] {
        let widest = (crate::overlay::max_card_width() as f32 * scale).round() as i32;
        let laid = glass.lay_out(&blob, &theme, widest, scale).expect("layout");
        // SAFETY: read-only monitor queries.
        let work_area = unsafe { crate::overlay::work_area_at(ScreenPoint::new(400, 400)) };
        let window = crate::overlay::card_height_for(laid.height, work_area);
        println!(
            "blob  scale {scale}: laid {}x{}, window height {window}, {}",
            laid.width,
            laid.height,
            if window < laid.height {
                format!("CLIPPED, {} pixels lost", laid.height - window)
            } else {
                "fits".to_owned()
            },
        );
    }

    for scale in [1.0f32, 1.5] {
        let widest = (crate::overlay::max_card_width() as f32 * scale).round() as i32;
        let laid = glass.lay_out(&long, &theme, widest, scale).expect("layout");
        // SAFETY: read-only monitor queries.
        let work_area = unsafe { crate::overlay::work_area_at(ScreenPoint::new(400, 400)) };
        let window = crate::overlay::card_height_for(laid.height, work_area);
        println!(
            "long  scale {scale}: laid {}x{}, window height {window}, {}",
            laid.width,
            laid.height,
            if window < laid.height {
                format!("CLIPPED, {} pixels lost", laid.height - window)
            } else {
                "fits".to_owned()
            },
        );
    }

    for scale in [1.0f32, 1.5] {
        // Scaled, as `show_card` scales it. Passing the 100% figure at 150%
        // measures a card a third narrower than the product draws, and the
        // first run of this reported one: 350 wide and 251 tall, which is not a
        // card that exists.
        let widest = (crate::overlay::max_card_width() as f32 * scale).round() as i32;
        let laid = glass.lay_out(&card, &theme, widest, scale).expect("layout");
        println!(
            "scale {scale}: {}x{} = {} px",
            laid.width,
            laid.height,
            laid.width * laid.height,
        );
    }

    // No assertion on the figures. What is small enough is the user's call and
    // it changes; what this guarantees is that the answer is measured.
    let laid = glass
        .lay_out(&card, &theme, crate::overlay::max_card_width(), 1.0)
        .expect("layout");
    assert!(laid.width > 0 && laid.height > 0);
    assert!(
        laid.width <= crate::overlay::max_card_width(),
        "a card may never be wider than it was offered",
    );
}

const READABLE_ENV: &str = "MORI_LENS_READABLE";
const READABLE_TEST: &str = "live_tests::be_the_readable_application";

/// The other half of the cross-process pair: a window that answers normally.
///
/// Started by `a_plain_edit_in_another_process_answers_the_same_way_every_time`
/// and does nothing but exist. `be_the_wedged_application` cannot be reused,
/// because it stops dispatching after two seconds and every reading after that
/// would be a timeout rather than an answer.
#[test]
#[ignore = "spawned by another test; does nothing on its own"]
fn be_the_readable_application() {
    use std::io::Write;
    use std::time::Duration;

    use crate::protected_harness::{centre, ProtectedHarness};

    if std::env::var(READABLE_ENV).is_err() {
        println!("not the readable process; nothing to do");
        return;
    }

    let harness = ProtectedHarness::start().expect("harness");
    let fields = harness.fields();
    let plain = centre(fields.plain);
    let password = centre(fields.password);
    println!(
        "READABLE-READY {} {} {} {} {}",
        plain.x,
        plain.y,
        password.x,
        password.y,
        harness.window_id(),
    );
    println!(
        "READABLE-FIELDS plain=({},{} {}x{}) password=({},{} {}x{})",
        fields.plain.x,
        fields.plain.y,
        fields.plain.width,
        fields.plain.height,
        fields.password.x,
        fields.password.y,
        fields.password.width,
        fields.password.height,
    );
    let _ = std::io::stdout().flush();

    // Long enough for the parent to take a few dozen readings, and bounded so
    // an abandoned child cannot outlive the run.
    std::thread::sleep(Duration::from_secs(30));
}

/// Does the same plain `EDIT` answer the same way, read from another process?
///
/// **Issue #40, as an instrument.** Read from inside its own process this
/// control answers `Empty` every time. Read from another process it has
/// answered `Empty` on one run and `Protected(Declared)` on another, which
/// means `CurrentIsPassword` returned true for a control with no `ES_PASSWORD`.
///
/// One reading either way is an anecdote. This takes forty, on one control, in
/// one child process, and prints the distribution. When a reading refuses, it
/// also describes the element that refused: its control type, its rectangle,
/// and whether that rectangle contains the point that produced it. If the
/// refusals come from an element that does not contain the point, the answer is
/// that `ElementFromPoint` is returning a neighbour and the protection check is
/// about the wrong control.
///
/// The password field is read too, and it must refuse **every** time. That is
/// the assertion; the plain field is measured rather than asserted, because
/// what it does is the open question.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_plain_edit_in_another_process_answers_the_same_way_every_time() {
    use std::io::{BufRead, BufReader};
    use std::time::Duration;

    use lens_core::acquire::{AcquireContext, AcquireError, ProtectedReason, TextAcquirer};
    use lens_core::geometry::ScreenPoint;
    use lens_core::hover::Generation;

    let acquirer = UiaTextAcquirer::new().expect("UI Automation is unreachable");
    let context = AcquireContext::new(Generation(1));

    let mut child =
        std::process::Command::new(std::env::current_exe().expect("this test binary has a path"))
            .args(["--exact", READABLE_TEST, "--ignored", "--nocapture"])
            .env(READABLE_ENV, "1")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("could not spawn the readable process");

    let mut lines = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut ready = String::new();
    for line in lines.by_ref().map_while(Result::ok) {
        if line.starts_with("READABLE-FIELDS ") {
            println!("child says: {line}");
            break;
        }
        if line.starts_with("READABLE-READY ") {
            ready = line;
        }
    }
    assert!(!ready.is_empty(), "the child never reported a window");
    let numbers: Vec<i32> = ready
        .trim_start_matches("READABLE-READY ")
        .split_whitespace()
        .filter_map(|value| value.parse::<i64>().ok().map(|value| value as i32))
        .collect();
    let plain = ScreenPoint::new(numbers[0], numbers[1]);
    let password = ScreenPoint::new(numbers[2], numbers[3]);
    let window = numbers[4] as i64;

    // Measuring the occluder instead of the target is a mistake this repository
    // has already made once.
    assert_eq!(
        crate::window::root_window_at(plain),
        Some(window),
        "another window is on top of the harness, so this would measure it",
    );

    const ROUNDS: usize = 40;
    let mut answers: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut off_element = 0usize;
    let mut refusals_described = Vec::new();

    for _ in 0..ROUNDS {
        let outcome = acquirer.acquire(plain, &context);
        let name = match &outcome {
            Ok(_) => "acquired".to_owned(),
            Err(error) => error.reason().to_owned(),
        };
        *answers.entry(name).or_default() += 1;

        if matches!(
            outcome,
            Err(AcquireError::Protected(ProtectedReason::Declared))
        ) {
            if let Some(described) = acquirer.describe_element_at(plain) {
                if !described.contains_point {
                    off_element += 1;
                }
                if refusals_described.len() < 4 {
                    refusals_described.push(described);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    println!("\nplain EDIT, {ROUNDS} readings from another process:");
    for (name, count) in &answers {
        println!("  {name:<22} {count}");
    }
    if let Some(described) = acquirer.describe_element_at(plain) {
        println!("  element now            {described:?}");
    }
    for described in &refusals_described {
        println!("  refused as             {described:?}");
    }
    if !refusals_described.is_empty() {
        println!(
            "  of the refusals, {off_element} came from an element whose rectangle does \
             not contain the point",
        );
    }

    // Now that it is explained, both are asserted. The plain control must
    // never be refused as protected, which is the whole of issue #40, and the
    // password control must be refused every time, which is the direction that
    // would matter if this were ever wrong.
    assert_eq!(
        answers.get("protected").copied().unwrap_or(0),
        0,
        "a plain EDIT was refused as protected from another process: {answers:?}",
    );
    let plain_element = acquirer
        .describe_element_at(plain)
        .expect("no element under the plain field");
    assert_eq!(
        plain_element.is_password,
        Some(false),
        "the element under the plain field declares itself a password field",
    );
    assert!(
        plain_element.contains_point,
        "the element under the plain field does not contain the point that found          it, which is the two coordinate spaces disagreeing again",
    );

    let mut password_answers: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for _ in 0..8 {
        let name = match acquirer.acquire(password, &context) {
            Ok(_) => "acquired".to_owned(),
            Err(error) => error.reason().to_owned(),
        };
        *password_answers.entry(name).or_default() += 1;
    }
    println!("\npassword EDIT, 8 readings from another process:");
    for (name, count) in &password_answers {
        println!("  {name:<22} {count}");
    }
    let password_element = acquirer
        .describe_element_at(password)
        .expect("no element under the password field");
    println!("  element                {password_element:?}");

    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(
        password_answers.get("protected").copied().unwrap_or(0),
        8,
        "a declared password field was not refused every time: {password_answers:?}",
    );
    assert_eq!(password_element.is_password, Some(true));
}

/// How much better the recogniser reads small text when it is enlarged first.
///
/// **Scored against text this repository drew**, which is the one way to score
/// a recogniser without printing anybody's screen. The renderer lays out a
/// known sentence at a known point size, `Glass::pixels` hands back the bitmap,
/// and the recogniser is asked to read it back. What it should say is known
/// before it is asked.
///
/// The case this is for: a card in VS Code rendered `do은5` for `does` and
/// `5h0l word_banner` for `show_word`, and Lens translated both without
/// noticing. `Windows.Media.Ocr` is trained on text at document sizes and a
/// code editor is well below them.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_what_enlarging_a_capture_does_for_small_text() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenPoint;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let known = "The test correctly refuses to pass vacuously";
    let card = Card::translated(
        known,
        &QuickTranslation {
            text: "테스트가 의미 없이 통과하는 것을 올바르게 거부한다".to_owned(),
            engine: EngineId::LocalOllama,
            model: "fixture".to_owned(),
            latency_ms: 0,
            cached: false,
            part_was_removed: false,
        },
    );

    // Small, and at 100%: the point of this is text at the size a code editor
    // renders, not text at the size a card usually is. Light and dark, because
    // every card a reader photographed with garbage in it was light text on a
    // dark background, and the readable ones were the other way round.
    for (scale, dark) in [(0.75f32, false), (0.75, true), (1.0, true)] {
        let theme = Theme::resolve(
            Preset::System,
            Translucency::Solid,
            TextScale::Small,
            SystemPreferences {
                dark,
                ..SystemPreferences::default()
            },
        );
        let laid = glass.lay_out(&card, &theme, 420, scale).expect("layout");
        // SAFETY: the renderer creates and releases its own objects.
        let (width, height, argb) = unsafe { glass.pixels(&laid, &theme) }.expect("pixels");
        let mut bgra = Vec::with_capacity(argb.len() * 4);
        for pixel in &argb {
            bgra.extend_from_slice(&pixel.to_le_bytes());
        }
        let capture =
            crate::capture::Capture::from_bgra(width, height, bgra).expect("a well-formed buffer");

        println!(
            "\ncard {width}x{height} at scale {scale}, {}pt, {}",
            theme.small_pt,
            if dark {
                "light on dark"
            } else {
                "dark on light"
            },
        );
        println!("  brightness   {}", capture.brightness());
        for factor in [1u32, 2, 3] {
            let enlarged;
            let subject = if factor == 1 {
                &capture
            } else {
                enlarged = capture.scaled(factor).expect("enlarged");
                &enlarged
            };
            let began = std::time::Instant::now();
            let read = crate::ocr::read(subject, ScreenPoint::new(0, 0));
            let took = began.elapsed();
            if factor == 1 {
                // What the product will now do with this same picture, so the
                // table below has the shipping answer in it rather than only
                // the ingredients.
                let began = std::time::Instant::now();
                let best = crate::ocr::read_best(subject, ScreenPoint::new(0, 0));
                println!(
                    "  best {}ms  ·  median word {:?}px",
                    began.elapsed().as_millis(),
                    read.as_ref()
                        .and_then(crate::ocr::Recognised::median_word_height),
                );
                println!(
                    "      read: {}",
                    best.map(|read| read.text())
                        .unwrap_or_default()
                        .replace('\n', " ⏎ "),
                );
            }
            let text = read.map(|read| read.text()).unwrap_or_default();

            // Scored by how many of the known words came back exactly. The
            // sentence is this repository's own, so printing what was read is
            // allowed here and is the only way to see *how* it failed.
            let got: Vec<String> = text
                .split_whitespace()
                .map(|word| {
                    word.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .collect();
            let hits = known
                .split_whitespace()
                .filter(|word| got.iter().any(|read| read == &word.to_lowercase()))
                .count();
            let expected = known.split_whitespace().count();
            println!(
                "  {factor}x  {hits}/{expected} words exact  ·  {}ms  ·  {}x{}",
                took.as_millis(),
                subject.width,
                subject.height,
            );
            println!("      read: {}", text.replace('\n', " ⏎ "));

            // The same picture inverted, which is what the dark rows are
            // really asking about.
            if factor == 3 {
                let flipped = subject.inverted();
                println!(
                    "  inverted brightness {} from {}",
                    flipped.brightness(),
                    subject.brightness(),
                );
                let read = crate::ocr::read(&flipped, ScreenPoint::new(0, 0));
                let text = read.map(|read| read.text()).unwrap_or_default();
                let got: Vec<String> = text
                    .split_whitespace()
                    .map(|word| {
                        word.trim_matches(|c: char| !c.is_alphanumeric())
                            .to_lowercase()
                    })
                    .collect();
                let hits = known
                    .split_whitespace()
                    .filter(|word| got.iter().any(|read| read == &word.to_lowercase()))
                    .count();
                println!("  3x inverted  {hits}/{expected} words exact");
                println!("      read: {}", text.replace('\n', " ⏎ "));
            }
        }
    }
}

/// How much contrast the recogniser needs, at the size a code editor uses.
///
/// **The measurement before this one had a hole in it.** It compared the card's
/// light theme against its dark one and found 6 of 7 words against 3 of 7,
/// which says those two themes differ and says nothing about why. They differ
/// in polarity, which was ruled out, and in their actual colours, which was
/// not. A reader's problem is a code editor's dark theme, not this card's, and
/// nothing in that run could speak to it.
///
/// So this fixes the background, fixes the size, and varies **one thing**: how
/// far the ink is from the paper. What comes out is a curve rather than a
/// comparison, and a curve says what a surface needs rather than which of two
/// surfaces was better.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_how_much_contrast_the_recogniser_needs() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenPoint;
    use lens_core::theme::{Rgb, Theme};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let known = "The test correctly refuses to pass vacuously";
    let card = Card::translated(
        known,
        &QuickTranslation {
            text: "테스트가 의미 없이 통과하는 것을 올바르게 거부한다".to_owned(),
            engine: EngineId::LocalOllama,
            model: "fixture".to_owned(),
            latency_ms: 0,
            cached: false,
            part_was_removed: false,
        },
    );

    // A dark editor's background, and inks walking away from it. VS Code's
    // default dark theme is #1e1e1e paper with #d4d4d4 comment ink, which is
    // the row this exists to place.
    let paper = Rgb::new(0x1e, 0x1e, 0x1e);
    let inks = [
        Rgb::new(0x3a, 0x3a, 0x3a),
        Rgb::new(0x55, 0x55, 0x55),
        Rgb::new(0x70, 0x70, 0x70),
        Rgb::new(0x8b, 0x8b, 0x8b),
        Rgb::new(0xa6, 0xa6, 0xa6),
        Rgb::new(0xd4, 0xd4, 0xd4),
        Rgb::new(0xff, 0xff, 0xff),
    ];

    println!("\npaper #1e1e1e, text at 8pt\n");
    println!("  ink        contrast    3x    4x    5x    6x   (of 7 words)");
    for ink in inks {
        let mut theme = Theme::resolve(
            lens_core::theme::Preset::System,
            lens_core::theme::Translucency::Solid,
            lens_core::theme::TextScale::Small,
            lens_core::theme::SystemPreferences {
                dark: true,
                ..lens_core::theme::SystemPreferences::default()
            },
        );
        theme.surface = paper;
        theme.body = ink;
        theme.original = ink;
        theme.footer = ink;
        theme.border = paper;

        let laid = glass.lay_out(&card, &theme, 420, 0.75).expect("layout");
        // SAFETY: the renderer creates and releases its own objects.
        let (width, height, argb) = unsafe { glass.pixels(&laid, &theme) }.expect("pixels");
        let mut bgra = Vec::with_capacity(argb.len() * 4);
        for pixel in &argb {
            bgra.extend_from_slice(&pixel.to_le_bytes());
        }
        let native =
            crate::capture::Capture::from_bgra(width, height, bgra).expect("a well-formed buffer");
        let score = |capture: &crate::capture::Capture| -> usize {
            let text = crate::ocr::read(capture, ScreenPoint::new(0, 0))
                .map(|read| read.text())
                .unwrap_or_default();
            let got: Vec<String> = text
                .split_whitespace()
                .map(|word| {
                    word.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .collect();
            known
                .split_whitespace()
                .filter(|word| got.iter().any(|read| read == &word.to_lowercase()))
                .count()
        };
        // Past the 3x this crate stops at, because the contrast columns say
        // contrast is not the lever and size is the only one left.
        let at = |factor: u32| -> String {
            native.scaled(factor).map_or_else(
                || "  -".to_owned(),
                |capture| format!("{:>3}", score(&capture)),
            )
        };
        println!(
            "  #{:02x}{:02x}{:02x}   {:>6.1}:1  {}   {}   {}   {}",
            ink.r,
            ink.g,
            ink.b,
            ink.contrast_with(paper),
            at(3),
            at(4),
            at(5),
            at(6),
        );
    }
}

/// What one automatic fall-through costs, on text and on a margin.
///
/// Issue #87. `ocr = automatic` makes a hover the accessibility path could not
/// serve fall through to pixels, and in VS Code that path fails at 96% of
/// sampled points. So in an editor nearly every settled hover pays this, and
/// nobody had measured what "this" is.
///
/// Two cases, because they are not the same cost and the common one is the
/// cheap one. A band with text in it gets two recognition passes, since
/// `read_best` enlarges when the median word is small. A band with nothing in
/// it gets one: there is no median word to be small.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_what_an_automatic_fall_through_costs() {
    use std::time::Instant;

    use lens_core::acquire::{AcquireContext, TextAcquirer};
    use lens_core::geometry::ScreenPoint;
    use lens_core::hover::Generation;

    // No card is shown by this test, so the hook has nothing to hide and the
    // 60ms compositor wait is skipped. That wait is a real cost in the product
    // and it is added back in the arithmetic below rather than measured here,
    // because measuring it would mean drawing a card to hide.
    let acquirer = crate::OcrTextAcquirer::new(
        Box::new(|_band| {}),
        std::sync::Arc::new(crate::CaptureCost::default()),
    );
    let context = AcquireContext::new(Generation(1));

    // A point with text on it, found the way the OCR probe finds one, and a
    // point that is almost certainly empty. Off-screen would be refused by the
    // capture, so the margin case is a corner of the desktop.
    let mut cases: Vec<(&str, ScreenPoint)> = vec![("a margin", ScreenPoint::new(4, 4))];
    if let Ok(window) = crate::foreground_window() {
        cases.push(("the foreground window", window.client_bounds.center()));
    }

    for (what, point) in cases {
        let mut samples = Vec::new();
        let mut found = 0usize;
        for _ in 0..8 {
            let began = Instant::now();
            let outcome = acquirer.acquire(point, &context);
            samples.push(began.elapsed().as_millis());
            if outcome.is_ok() {
                found += 1;
            }
        }
        samples.sort_unstable();
        let median = samples[samples.len() / 2];
        println!(
            "\n{what} at ({}, {}): median {median}ms, {found} of 8 resolved a word",
            point.x, point.y,
        );
        println!("  samples {samples:?}");
        // What a reader would pay, at a rate a reader can actually produce. The
        // dwell is 320ms, so two settled hovers a second is a brisk upper
        // bound rather than a typical one.
        let with_hide = median + 60;
        println!(
            "  with the 60ms compositor wait: {with_hide}ms, which at two settled\n  \
             hovers a second is {:.0}% of one core",
            (with_hide as f64 * 2.0) / 10.0,
        );

        // The same eight readings with the memory left alone, which is what a
        // reader who is not scrolling or typing actually gets. The first is a
        // capture; any after it that land on no line should be refused before
        // one happens. `known-empty` is that refusal, and it is counted rather
        // than inferred from the timings.
        let quiet = AcquireContext {
            view_may_have_changed: false,
            ..AcquireContext::new(Generation(1))
        };
        let mut remembered = Vec::new();
        let mut skipped = 0usize;
        for _ in 0..8 {
            let began = Instant::now();
            let outcome = acquirer.acquire(point, &quiet);
            remembered.push(began.elapsed().as_millis());
            if matches!(outcome, Err(lens_core::AcquireError::KnownEmpty)) {
                skipped += 1;
            }
        }
        println!(
            "  with the reading remembered: {remembered:?}, {skipped} of 8 refused
               without a capture",
        );
    }
}

/// The focused pass finds the same word the whole-picture pass does.
///
/// Cropping before enlarging took a fall-through over a margin from 887ms to
/// 331ms (issue #87). That is only a fix if the crop still contains the
/// answer, and the way to know is to ask both about a point whose word is
/// known before either is asked.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn cropping_before_enlarging_still_finds_the_word() {
    use lens_core::card::Card;
    use lens_core::geometry::ScreenPoint;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let card = Card::translated(
        "The test correctly refuses to pass vacuously",
        &QuickTranslation {
            text: "테스트가 의미 없이 통과하는 것을 올바르게 거부한다".to_owned(),
            engine: EngineId::LocalOllama,
            model: "fixture".to_owned(),
            latency_ms: 0,
            cached: false,
            part_was_removed: false,
        },
    );
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Solid,
        TextScale::Small,
        SystemPreferences::default(),
    );
    let laid = glass.lay_out(&card, &theme, 420, 0.75).expect("layout");
    // SAFETY: the renderer creates and releases its own objects.
    let (width, height, argb) = unsafe { glass.pixels(&laid, &theme) }.expect("pixels");
    let mut bgra = Vec::with_capacity(argb.len() * 4);
    for pixel in &argb {
        bgra.extend_from_slice(&pixel.to_le_bytes());
    }
    let capture =
        crate::capture::Capture::from_bgra(width, height, bgra).expect("a well-formed buffer");
    let origin = ScreenPoint::new(0, 0);

    let whole = crate::ocr::read_best_around(&capture, origin, None).expect("whole");
    // A point inside the first line of the card, which is the English original.
    let Some((_, word)) = whole
        .lines
        .iter()
        .flat_map(|line| &line.words)
        .find(|word| word.text.chars().any(char::is_alphabetic))
        .map(|word| (word, word.clone()))
    else {
        println!("nothing was recognised at all, so there is nothing to compare");
        return;
    };
    let at = word.bounds.center();

    let focused = crate::ocr::read_best_around(&capture, origin, Some(at)).expect("focused");
    let from_whole = whole.word_at(at).map(|(_, word)| word.text.clone());
    let from_focused = focused.word_at(at).map(|(_, word)| word.text.clone());

    println!("point ({}, {})", at.x, at.y);
    println!("  whole picture enlarged: {from_whole:?}");
    println!("  crop enlarged:          {from_focused:?}");
    assert_eq!(
        from_whole, from_focused,
        "cropping before enlarging changed the answer at the point that was asked about",
    );
}

/// What automatic OCR costs a machine, sustained, rather than per hover.
///
/// The other half of issue #87. `perf: crop before enlarging` made each
/// fall-through cheaper and did not stop one happening on every hover the
/// accessibility path could not serve, which in an editor is 96% of them.
///
/// `2026-08-05-idle-cost.md` measures a Lens that captures nothing, so there is
/// no figure at all for one that does. This produces one: CPU time consumed by
/// this process across a run of fall-throughs at a rate a reader can actually
/// produce.
///
/// **Process CPU time, not wall clock.** A fall-through spends 60ms of its
/// wall clock asleep waiting for the compositor and more of it waiting on the
/// recogniser, and a laptop's battery does not care about sleeping.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_what_automatic_ocr_costs_a_battery() {
    use std::time::{Duration, Instant};

    use lens_core::acquire::{AcquireContext, TextAcquirer};
    use lens_core::geometry::ScreenPoint;
    use lens_core::hover::Generation;
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    /// CPU time this process has used, kernel and user together.
    fn cpu_used() -> Duration {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: a read-only query about this process, writing only into the
        // four structures above.
        let ok = unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        if ok.is_err() {
            return Duration::ZERO;
        }
        let hundred_ns = |time: FILETIME| -> u64 {
            (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
        };
        // FILETIME is in 100ns units.
        Duration::from_nanos((hundred_ns(kernel) + hundred_ns(user)) * 100)
    }

    let acquirer = crate::OcrTextAcquirer::new(
        Box::new(|_band| {}),
        std::sync::Arc::new(crate::CaptureCost::default()),
    );
    let context = AcquireContext::new(Generation(1));
    // A rate a reader can produce. The dwell is 320ms, so one settled hover
    // every 500ms is brisk reading rather than a stress test.
    const EVERY: Duration = Duration::from_millis(500);
    const ROUNDS: u32 = 20;

    // One discarded, so the figure is a warm recogniser rather than its first
    // load.
    let _ = acquirer.acquire(ScreenPoint::new(400, 400), &context);

    let before = cpu_used();
    let began = Instant::now();
    for round in 0..ROUNDS {
        let started = Instant::now();
        // Walked, so successive readings are not the same picture and the
        // recogniser cannot be caching anything.
        let at = ScreenPoint::new(300 + (round as i32 % 5) * 90, 300 + (round as i32 % 3) * 70);
        let _ = acquirer.acquire(at, &context);
        if let Some(rest) = EVERY.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let spent = cpu_used().saturating_sub(before);
    let over = began.elapsed();

    println!(
        "\n{ROUNDS} fall-throughs, one every {}ms",
        EVERY.as_millis()
    );
    println!("  wall clock   {:.1}s", over.as_secs_f64());
    println!("  CPU used     {:.0}ms", spent.as_secs_f64() * 1000.0);
    println!(
        "  per reading  {:.0}ms of CPU",
        spent.as_secs_f64() * 1000.0 / f64::from(ROUNDS),
    );
    println!(
        "  which is     {:.1}% of one core, sustained, while somebody reads",
        spent.as_secs_f64() * 100.0 / over.as_secs_f64(),
    );
    println!(
        "\n  against 0.16% of a core idle, measured 2026-08-05 on a Lens that\n  \
         captured nothing."
    );
}

/// A card too tall for its screen is cut and says so, rather than clipped.
///
/// A text with no sentence boundary in it, which is what the segmenter returns
/// for KakaoTalk's single region (issue #34), laid out 903 pixels tall at 150%
/// against a ceiling of 634. What vanished was the end of the answer, with
/// nothing on the card to say anything had.
///
/// This is the fitting loop `show_card` runs, exercised through the same
/// renderer, so the number it converges to is the one the product would use.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_card_too_tall_for_the_screen_is_cut_rather_than_clipped() {
    use lens_core::card::{Card, CardBody};
    use lens_core::geometry::ScreenPoint;
    use lens_core::theme::{Preset, SystemPreferences, TextScale, Theme, Translucency};
    use lens_core::translate::{EngineId, QuickTranslation};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::DEFAULT,
        TextScale::Normal,
        SystemPreferences::default(),
    );
    let mut card = Card::translated(
        "one more clause about the deploy and what it did to the accounts table ".repeat(12),
        &QuickTranslation {
            text: "배포와 그것이 계정 테이블에 한 일에 대한 절 하나 더 ".repeat(12),
            engine: EngineId::LocalOllama,
            model: "fixture".to_owned(),
            latency_ms: 900,
            cached: false,
            part_was_removed: false,
        },
    );

    let scale = 1.5f32;
    let widest = (crate::overlay::max_card_width() as f32 * scale).round() as i32;
    // SAFETY: read-only monitor queries.
    let work_area = unsafe { crate::overlay::work_area_at(ScreenPoint::new(400, 400)) };
    let ceiling = crate::overlay::max_card_height(work_area);
    let original = card.original.clone();

    // The same loop `show_card` runs, so the outcome is the product's.
    let mut heights = Vec::new();
    let mut refused = false;
    for _ in 0..4 {
        let laid = glass.lay_out(&card, &theme, widest, scale).expect("layout");
        heights.push(laid.height);
        if laid.height <= ceiling {
            break;
        }
        let answer = match &card.body {
            CardBody::Translation(text) => text.chars().count(),
            _ => break,
        };
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
            card = Card::too_long_to_show();
            refused = true;
            continue;
        }
        card = shorter;
    }

    println!(
        "
ceiling {ceiling}px, heights {heights:?}"
    );
    println!("footer  {}", card.footer);
    println!("refused {refused}");
    assert!(
        heights.last().is_some_and(|height| *height <= ceiling),
        "neither cutting nor refusing brought it inside the ceiling: {heights:?}",
    );
    if refused {
        // The original is too long for an answer to fit under it, so the card
        // says so rather than showing a source with the answer clipped off.
        assert!(card.original.is_empty(), "a refusal keeps no original");
        assert!(card.footer.contains("나누기"), "{}", card.footer);
    } else {
        assert!(
            card.footer.contains(Card::CUT),
            "it fitted without saying it was cut: {}",
            card.footer,
        );
        assert_eq!(
            card.footer.matches(Card::CUT).count(),
            1,
            "it said it was cut more than once: {}",
            card.footer,
        );
        assert_eq!(
            card.original, original,
            "the original was cut, and it is the only way to check the answer",
        );
    }
}

/// A string put on the clipboard is the string that comes back.
///
/// Live because there is nothing to assert without a real clipboard: the whole
/// function is four Win32 calls and the ownership rule between them.
///
/// **It replaces whatever the user had copied**, which is why it is `#[ignore]`
/// like the rest and why it puts the previous contents back afterwards. A test
/// that quietly emptied somebody's clipboard would be the kind of thing this
/// repository writes rules about.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn text_put_on_the_clipboard_comes_back_unchanged() {
    use crate::clipboard;

    let theirs = clipboard::read_text();

    // Two lines, Korean and Latin, which is the shape a card produces.
    let written =
        "The pointer must rest before anything is read.\n포인터가 멈춘 뒤에야 무엇이든 읽습니다.";
    assert!(clipboard::put_text(written), "the clipboard would not open");

    let read = clipboard::read_text().expect("something on the clipboard");
    assert_eq!(read, written);

    // A second write replaces rather than appends, which is what `EmptyClipboard`
    // is there for and the failure a reader would notice first.
    assert!(clipboard::put_text("second"));
    assert_eq!(clipboard::read_text().as_deref(), Some("second"));

    // Nothing is written for nothing, so an empty card cannot wipe a clipboard.
    assert!(!clipboard::put_text(""));
    assert_eq!(clipboard::read_text().as_deref(), Some("second"));

    match theirs {
        Some(theirs) => {
            let _ = clipboard::put_text(&theirs);
            println!("clipboard restored, {} chars", theirs.chars().count());
        }
        None => println!("the clipboard held no text before this ran"),
    }
}

/// Can a window that is covered be read at all, and by which path?
///
/// A reader asked whether OCR means the window has to be in front. The answer
/// has three parts and only measurement separates them, so this asks each
/// window on the desktop to render itself with `PrintWindow` and reports
/// whether anything came back.
///
/// **Shape only, never content** (`docs/privacy-model.md`). What is printed is
/// the size, whether the bitmap is one flat colour, and the share of pixels
/// that differ from the corner. Nothing is written anywhere.
///
/// This is an experiment, not a product path. Lens captures a band around the
/// pointer and the pointer is on the topmost window by definition.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_whether_a_covered_window_can_render_itself() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};

    /// `PW_RENDERFULLCONTENT`, which is what makes this worth trying at all:
    /// without it a composited window renders as a blank rectangle.
    const FULL_CONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(0x0000_0002);

    println!("\nwindow                 size        rendered   ink");

    for window in crate::top_level_windows() {
        let bounds = window.client_bounds;
        let (width, height) = (bounds.width.min(1600), bounds.height.min(1000));
        if width <= 0 || height <= 0 {
            continue;
        }
        let name = window.process_name.clone().unwrap_or_else(|| "?".into());

        let header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        // SAFETY: every handle is released on every path, and the pixels are
        // read before the bitmap is deleted.
        let measured = unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                continue;
            }
            let memory = CreateCompatibleDC(Some(screen));
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let made = CreateDIBSection(Some(memory), &header, DIB_RGB_COLORS, &mut bits, None, 0);
            let outcome = match made {
                Ok(bitmap) => {
                    let previous = SelectObject(memory, bitmap.into());
                    let ok = PrintWindow(HWND(window.id as *mut _), memory, FULL_CONTENT).as_bool();
                    let ink = if ok && !bits.is_null() {
                        let count = (width * height) as usize * 4;
                        let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), count);
                        let corner = &pixels[..3];
                        let different = pixels
                            .chunks_exact(4)
                            .filter(|pixel| pixel[..3] != *corner)
                            .count();
                        different as f64 * 100.0 / (width * height) as f64
                    } else {
                        0.0
                    };
                    SelectObject(memory, previous);
                    let _ = DeleteObject(bitmap.into());
                    Some((ok, ink))
                }
                Err(_) => None,
            };
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            outcome
        };

        if let Some((ok, ink)) = measured {
            println!(
                "{name:<22} {width:>5}x{height:<5} {:<10} {ink:>5.1}%",
                if ok { "yes" } else { "NO" },
            );
        }
    }

    println!(
        "\nink is the share of pixels differing from the corner one. A window that\n\
         rendered but came back at 0.0% answered the call and drew nothing, which\n\
         is the failure this was run to find."
    );
}

/// Would walking up from the point find text where the point itself has none?
///
/// The acquirer gives up the moment `ElementFromPoint`'s element has no text
/// pattern, and answers `Unsupported`. In a Chromium or Electron window a point
/// often lands on an overlay or a container, with the text-bearing element one
/// or two steps above it.
///
/// # It samples the desktop, not a window
///
/// The first version measured the foreground window, and could not run at all
/// when nothing was in the foreground, which is how a measurement this one
/// contradicts came to stand unchecked for an afternoon.
///
/// `ElementFromPoint` answers about whatever is topmost, which is exactly what
/// a pointer would hit. So there is nothing to arrange: sample points across
/// the windows that are visible, attribute each to whichever process owns it,
/// and the numbers are about what a reader's pointer actually meets.
///
/// # What the third column is, and why the first version of it was wrong
///
/// `RangeFromPoint` returns a **degenerate** range, a caret-shaped sliver
/// whose rectangle may contain no sampled point at all. Testing containment on
/// that reported everything off target for a mechanical reason. It is expanded
/// to a line first, which is what the product does next.
///
/// Counts only, and no text is read: this asks whether the pattern exists and
/// where its line is, not what it holds (`docs/privacy-model.md`).
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_whether_an_ancestor_holds_the_text_a_point_does_not() {
    use std::collections::BTreeMap;
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, TextUnit_Line, UIA_TextPatternId,
    };

    /// How far up to look. Two or three is the depth a wrapper adds; a walk to
    /// the window root would find the document element, whose range covers
    /// everything and which ADR-0009's distance gate exists to reject.
    const STEPS: usize = 3;

    #[derive(Default)]
    struct Tally {
        points: usize,
        at_the_point: usize,
        at_an_ancestor: usize,
        on_target: usize,
    }

    let _com = crate::com::ComScope::enter();
    // SAFETY: creating the documented automation co-class in-process.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        println!("no automation");
        return;
    };

    let windows_on_screen = crate::top_level_windows();
    if windows_on_screen.is_empty() {
        println!("no windows");
        return;
    }
    let mut by_process: BTreeMap<String, Tally> = BTreeMap::new();

    // SAFETY: read-only automation queries; a window that changes mid-walk
    // yields an error rather than a crash.
    unsafe {
        let Ok(walker) = automation.RawViewWalker() else {
            println!("no tree walker");
            return;
        };
        let has_text = |element: &windows::Win32::UI::Accessibility::IUIAutomationElement| {
            element.GetCurrentPattern(UIA_TextPatternId).ok().is_some()
        };

        for window in &windows_on_screen {
            let bounds = window.client_bounds;
            if bounds.width <= 0 || bounds.height <= 0 {
                continue;
            }
            let owner = window.process_name.clone().unwrap_or_else(|| "?".into());
            let step = 48;
            let mut y = bounds.y + 8;
            while y < bounds.bottom() - 8 {
                let mut x = bounds.x + 8;
                while x < bounds.right() - 8 {
                    let here = lens_core::geometry::ScreenPoint::new(x, y);
                    x += step;
                    // Only where this window is the one a pointer would hit,
                    // so a covered window contributes nothing rather than
                    // somebody else's numbers.
                    if crate::root_window_at(here) != Some(window.id) {
                        continue;
                    }
                    let point = windows::Win32::Foundation::POINT {
                        x: here.x,
                        y: here.y,
                    };
                    let Ok(element) = automation.ElementFromPoint(point) else {
                        continue;
                    };
                    let tally = by_process.entry(owner.clone()).or_default();
                    tally.points += 1;

                    if has_text(&element) {
                        tally.at_the_point += 1;
                        continue;
                    }
                    let mut current = element;
                    for _ in 1..=STEPS {
                        let Ok(parent) = walker.GetParentElement(&current) else {
                            break;
                        };
                        if has_text(&parent) {
                            tally.at_an_ancestor += 1;
                            let usable = parent
                                .GetCurrentPattern(UIA_TextPatternId)
                                .ok()
                                .and_then(|pattern| pattern.cast::<IUIAutomationTextPattern>().ok())
                                .and_then(|pattern| pattern.RangeFromPoint(point).ok())
                                .and_then(|range| {
                                    range
                                        .ExpandToEnclosingUnit(TextUnit_Line)
                                        .ok()
                                        .map(|()| range)
                                })
                                .map(|range| {
                                    crate::uia::bounding_rectangles(&range)
                                        .iter()
                                        .any(|rect| rect.contains(here))
                                })
                                .unwrap_or(false);
                            if usable {
                                tally.on_target += 1;
                            }
                            break;
                        }
                        current = parent;
                    }
                }
                y += step;
            }
        }
    }

    println!("\nprocess              points  at the point   within 3 up   and on target");
    let mut totals = Tally::default();
    for (owner, tally) in &by_process {
        if tally.points < 20 {
            continue;
        }
        let share = |count: usize| count as f64 * 100.0 / tally.points as f64;
        println!(
            "{owner:<20} {:>6}  {:>5} ({:>4.1}%)  {:>4} ({:>4.1}%)  {:>4} ({:>4.1}%)",
            tally.points,
            tally.at_the_point,
            share(tally.at_the_point),
            tally.at_an_ancestor,
            share(tally.at_an_ancestor),
            tally.on_target,
            share(tally.on_target),
        );
        totals.points += tally.points;
        totals.at_the_point += tally.at_the_point;
        totals.at_an_ancestor += tally.at_an_ancestor;
        totals.on_target += tally.on_target;
    }
    if totals.points > 0 {
        let share = |count: usize| count as f64 * 100.0 / totals.points as f64;
        println!(
            "{:<20} {:>6}  {:>5} ({:>4.1}%)  {:>4} ({:>4.1}%)  {:>4} ({:>4.1}%)",
            "all",
            totals.points,
            totals.at_the_point,
            share(totals.at_the_point),
            totals.at_an_ancestor,
            share(totals.at_an_ancestor),
            totals.on_target,
            share(totals.on_target),
        );
    }

    for line in [
        "",
        "at the point     what the acquirer uses today. Anything else is",
        "                 Unsupported and the reader sees nothing.",
        "within 3 up      how many more have text one to three steps above.",
        "and on target    of those, how many answer with a line containing",
        "                 the point. This is the column that decides whether",
        "                 walking would help or would answer confidently",
        "                 about the wrong sentence.",
    ] {
        println!("{line}");
    }
}

/// Does a big text element hand back its lines as separate rectangles?
///
/// The open problem after 2026-08-07: a window's text is exposed in elements
/// too large to aim inside, so a point tells you the document and not the
/// sentence. That is #14 and #34 in one sentence.
///
/// There is one more thing the provider already knows. `GetBoundingRectangles`
/// on a range returns **one rectangle per rendered line**, which is exactly the
/// shape `lens_core::geometry::line_under` was extracted to work on for the OCR
/// path. If a container's visible range comes back as fifty line rectangles,
/// then a point can be resolved to a line without `RangeFromPoint` being asked
/// anything.
///
/// This measures whether the rectangles arrive at all and what shape they are.
/// Counts and sizes, never text.
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn measure_whether_a_container_gives_up_its_lines() {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, TreeScope_Descendants,
        UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
    };

    let _com = crate::com::ComScope::enter();
    // SAFETY: creating the documented automation co-class in-process.
    let Ok(automation) = (unsafe {
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }) else {
        println!("no automation");
        return;
    };
    let Ok(front) = crate::foreground_window() else {
        println!("no foreground window");
        return;
    };
    println!(
        "\n{} · {}x{}",
        front.process_name.clone().unwrap_or_else(|| "?".into()),
        front.client_bounds.width,
        front.client_bounds.height,
    );
    println!("  element        ranges  rects  median height  tallest  points on a line   asked at its centre");

    // SAFETY: read-only automation queries on a handle just enumerated.
    unsafe {
        let Ok(element) =
            automation.ElementFromHandle(windows::Win32::Foundation::HWND(front.id as *mut _))
        else {
            return;
        };
        let Ok(condition) = automation
            .CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))
        else {
            return;
        };
        let Ok(found) = element.FindAll(TreeScope_Descendants, &condition) else {
            return;
        };
        let count = found.Length().unwrap_or(0);

        // The biggest few, because the question is about containers. A small
        // element is already a line and does not need taking apart.
        let mut candidates = Vec::new();
        for index in 0..count.min(200) {
            let Ok(child) = found.GetElement(index) else {
                continue;
            };
            let Ok(rect) = child.CurrentBoundingRectangle() else {
                continue;
            };
            let area = (rect.right - rect.left) as i64 * (rect.bottom - rect.top) as i64;
            candidates.push((area, rect.bottom - rect.top, child));
        }
        candidates.sort_by_key(|(area, _, _)| std::cmp::Reverse(*area));

        for (index, (_, height, child)) in candidates.into_iter().take(5).enumerate() {
            let Ok(pattern) = child.GetCurrentPattern(UIA_TextPatternId) else {
                continue;
            };
            let Ok(pattern) = pattern.cast::<IUIAutomationTextPattern>() else {
                continue;
            };
            // The visible part, not the document: a scrolled document's
            // rectangles are mostly empty and a hover only ever points at what
            // is on screen.
            let ranges = pattern.GetVisibleRanges().ok();
            // **The number that decides the cost.** If a visible range is
            // already one line, a point resolves in two calls: find the
            // rectangle, take that range's text. If one range carries all the
            // lines, the text of the Nth needs walking to it, one call a line.
            let range_count = ranges
                .as_ref()
                .and_then(|array| array.Length().ok())
                .unwrap_or(0);
            let rects: Vec<lens_core::geometry::ScreenRect> = match &ranges {
                Some(array) => (0..array.Length().unwrap_or(0))
                    .filter_map(|at| array.GetElement(at).ok())
                    .flat_map(|range| crate::uia::bounding_rectangles(&range))
                    .collect(),
                None => Vec::new(),
            };
            let mut heights: Vec<i32> = rects.iter().map(|rect| rect.height).collect();
            heights.sort_unstable();
            // **Does asking at the line's own centre answer with that line?**
            //
            // Walking a range line by line to reach the Nth costs a call per
            // line. Asking `RangeFromPoint` at a point guaranteed to be on a
            // glyph does not. The measurement that said `RangeFromPoint` is
            // useless here asked it at grid points, most of which are margin
            // and whitespace; this asks it where the text provably is.
            let mut asked = 0usize;
            let mut answered_with_itself = 0usize;
            for rect in rects.iter().filter(|rect| rect.height <= 40) {
                // Just inside the left edge, vertically centred: on the first
                // glyph of the line rather than in the gap before it.
                let probe = windows::Win32::Foundation::POINT {
                    x: rect.x + 2,
                    y: rect.y + rect.height / 2,
                };
                asked += 1;
                let answered = pattern
                    .RangeFromPoint(probe)
                    .ok()
                    .and_then(|range| {
                        range
                            .ExpandToEnclosingUnit(windows::Win32::UI::Accessibility::TextUnit_Line)
                            .ok()
                            .map(|()| range)
                    })
                    .map(|range| {
                        crate::uia::bounding_rectangles(&range)
                            .iter()
                            .any(|got| got.y == rect.y && got.height == rect.height)
                    })
                    .unwrap_or(false);
                if answered {
                    answered_with_itself += 1;
                }
            }
            let hit = if asked == 0 {
                0.0
            } else {
                answered_with_itself as f64 * 100.0 / asked as f64
            };

            // The number that decides whether this is worth building: over the
            // element's own area, how many grid points land inside one of its
            // line rectangles. `line_under` is the rule that would turn that
            // into an answer, and it is the one the OCR path already uses.
            let lines_only: Vec<lens_core::geometry::ScreenRect> = rects
                .iter()
                .copied()
                .filter(|rect| rect.height <= 40)
                .collect();
            let area = child
                .CurrentBoundingRectangle()
                .map(|rect| {
                    lens_core::geometry::ScreenRect::new(
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                    )
                })
                .unwrap_or(lens_core::geometry::ScreenRect::new(0, 0, 0, 0));
            let (mut points, mut on_a_line) = (0usize, 0usize);
            let mut y = area.y + 4;
            while y < area.bottom() - 4 {
                let mut x = area.x + 4;
                while x < area.right() - 4 {
                    points += 1;
                    if lens_core::geometry::line_under(
                        &lines_only,
                        lens_core::geometry::ScreenPoint::new(x, y),
                    )
                    .is_some()
                    {
                        on_a_line += 1;
                    }
                    x += 24;
                }
                y += 24;
            }
            let share = if points == 0 {
                0.0
            } else {
                on_a_line as f64 * 100.0 / points as f64
            };
            println!(
                "  {:<14} {:>6}  {:>5}  {:>13}  {:>7}  {on_a_line:>5}/{points:<5} ({share:>3.0}%)  {answered_with_itself:>3}/{asked:<3} ({hit:>3.0}%)",
                format!("#{index} h={height}"),
                range_count,
                rects.len(),
                heights.get(heights.len() / 2).copied().unwrap_or(0),
                heights.last().copied().unwrap_or(0),
            );
        }
    }

    println!(
        "\n  A container answering with many rectangles of a line's height is\n  \
         the whole idea: `line_under` already turns those into an answer, and\n  \
         it is the rule the OCR path uses. One rectangle the height of the\n  \
         element means the provider will not take it apart, and the pointer\n  \
         has nothing finer to aim at than the document."
    );
}

/// Does the welcome window put its lines where it says it did?
///
/// The whole first-run lesson rests on one claim: the rectangles handed to the
/// acquirer are the ones a pointer would hit. Everything else about it is
/// ordinary product code that `cargo test` already covers, and this is the part
/// that cannot be asserted without a desktop, a device context, and a font.
///
/// It is also the check that would have caught the two ways this can go wrong
/// silently. A window measured on one monitor's density and shown on another
/// reports rectangles a pointer misses by a quarter, which is the failure
/// `protected_harness` was built to recognise. And a layout measured before the
/// window is resized reports rectangles that are correct and off-screen.
///
/// Printing the lesson is allowed here, and only because this repository wrote
/// it (`docs/privacy-model.md`). Nothing read from a screen appears.
///
/// ```
/// cargo test -p lens-platform-windows -- --ignored --nocapture --test-threads=1 \
///   the_welcome_puts_its_lines
/// ```
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn the_welcome_puts_its_lines_where_it_says_it_did() {
    use lens_core::theme::{Preset, TextScale, Theme, Translucency};

    let Some(lesson) = lens_core::welcome::lesson_for("Korean") else {
        println!("no lesson for a Korean reader, which is itself a failure");
        panic!("lesson_for returned nothing");
    };
    let theme = Theme::resolve(
        Preset::System,
        Translucency::Balanced,
        TextScale::Normal,
        crate::appearance::system_preferences(),
    );
    // The shipped default tier, so this measures the window a new reader
    // actually gets rather than a variant of it.
    let heading = lens_core::welcome::heading_for(lens_core::policy::AppTier::Modifier, "Alt")
        .expect("the modifier tier has a heading");
    let Some(welcome) = crate::welcome::Welcome::open(lesson, theme, heading) else {
        println!("the welcome window could not be created");
        panic!("Welcome::open returned nothing");
    };
    let placement = welcome.placement();

    println!(
        "\nlesson     {} ({} lines)",
        lesson.language,
        placement.lines.len()
    );
    assert_eq!(placement.lines.len(), 2, "a lesson is two lines");

    let mut owned = 0;
    let mut resolved = 0;
    for (rect, line) in &placement.lines {
        let centre = lens_core::geometry::ScreenPoint::new(
            rect.x + rect.width / 2,
            rect.y + rect.height / 2,
        );
        let mine = crate::window::root_window_at(centre) == Some(placement.window_id);
        let answer = placement.line_under(centre);
        owned += usize::from(mine);
        resolved += usize::from(answer == Some((*rect, *line)));
        println!(
            "  {:>4}x{:<4} at {:>5},{:<5}  owned {}  resolves {}",
            rect.width,
            rect.height,
            rect.x,
            rect.y,
            if mine { "yes" } else { "NO" },
            if answer.is_some() { "yes" } else { "NO" },
        );
    }

    // Far outside any window this test created. A lesson that answered here
    // would be answering for the reader's own screen, which is the one thing
    // it must never do.
    let outside = lens_core::geometry::ScreenPoint::new(2, 2);
    let elsewhere = placement.line_under(outside);
    println!(
        "  a point at 2,2 resolves to {}",
        if elsewhere.is_some() {
            "A LESSON LINE, which is wrong"
        } else {
            "nothing, as it must"
        },
    );

    // **Is anything actually drawn on it?**
    //
    // Everything above passed while the window was blank. `ShowWindow` paints,
    // and what to paint was stored afterwards, so the first paint drew nothing
    // and nothing invalidated the window again. A reader got a correctly
    // sized, correctly placed, perfectly reachable white rectangle, and every
    // assertion here was green.
    //
    // Sampled from the screen at the same points the assertions above just
    // proved this window owns, so it asks what an eye would meet there rather
    // than what the window believes it drew. A count is reported and nothing
    // is captured, written, or printed (`docs/privacy-model.md`).
    let surface = crate::overlay::colorref(theme.surface);
    let mut ink = 0;
    let mut sampled = 0;
    // SAFETY: a screen device context, released below. `GetPixel` is read-only.
    unsafe {
        let dc = windows::Win32::Graphics::Gdi::GetDC(None);
        assert!(!dc.is_invalid(), "no screen device context");
        for (rect, _) in &placement.lines {
            let y = rect.y + rect.height / 2;
            let mut x = rect.x;
            while x < rect.x + rect.width {
                sampled += 1;
                if windows::Win32::Graphics::Gdi::GetPixel(dc, x, y) != surface {
                    ink += 1;
                }
                x += 2;
            }
        }
        windows::Win32::Graphics::Gdi::ReleaseDC(None, dc);
    }
    println!("  ink        {ink} of {sampled} sampled pixels differ from the surface");

    assert_eq!(owned, 2, "the window does not own the points it reported");
    assert_eq!(resolved, 2, "a line's own centre does not resolve to it");
    // **A band, not "more than none".**
    //
    // The window that shipped blank was *white*, and this theme's surface is
    // not, so every sampled pixel differed from it and `ink > 0` would have
    // passed the very failure it was written for. A painted line of text
    // covers some of its row and not all of it; a solid fill of any colour is
    // either all or nothing.
    let share = ink as f64 / sampled as f64;
    assert!(
        (0.05..0.95).contains(&share),
        "{ink} of {sampled} pixels differ from the surface, which is a solid fill rather than text",
    );
    assert!(
        elsewhere.is_none(),
        "the lesson answered off its own window"
    );
    assert!(welcome.is_open(), "the window closed on its own");
}

/// Does an analysis card put its values in a column?
///
/// The claim the row layout makes is that a label and its value share a top
/// edge and every value starts at the same x, so a reader finds that x once
/// instead of five times. Nothing about that is visible to `cargo test`: the
/// layout is DirectWrite's, the widths are the font's, and a card that draws
/// five sentences instead of five rows is green all the way down.
///
/// This asks the real layout where every run landed.
///
/// ```
/// cargo test -p lens-platform-windows -- --ignored --nocapture --test-threads=1 \
///   an_analysis_card_puts_its_values_in_a_column
/// ```
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn an_analysis_card_puts_its_values_in_a_column() {
    use std::collections::BTreeMap;

    use lens_core::card::Card;
    use lens_core::theme::{Preset, TextScale, Theme, Translucency};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::DEFAULT,
        TextScale::Normal,
        crate::appearance::system_preferences(),
    );
    let card = Card::analysed(
        "The provider returns one rectangle per rendered line.",
        &lens_core::analyse::DeepAnalysis {
            lemma: "rectangle".to_owned(),
            part_of_speech: "명사".to_owned(),
            sense: "화면에서 글자가 차지하는 사각 영역".to_owned(),
            // The answer stays on the card now, so the layout has to hold it.
            translation: Some("제공자는 각 렌더링된 줄당 하나의 직사각형을 반환합니다.".to_owned()),
            ..lens_core::analyse::DeepAnalysis::default()
        },
    );
    let laid = glass
        .lay_out_between(&card, &theme, (240, 420), 1.0)
        .expect("the card lays out");

    // Group by the top edge. A row is everything that shares one.
    let mut rows: BTreeMap<i64, Vec<f32>> = BTreeMap::new();
    for (x, y) in laid.placements() {
        rows.entry((y * 100.0).round() as i64).or_default().push(x);
    }

    println!("\n  row   runs  x offsets");
    for (top, xs) in &rows {
        println!(
            "  {:>5.1} {:>5}   {}",
            *top as f32 / 100.0,
            xs.len(),
            xs.iter()
                .map(|x| format!("{x:.0}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    // The original, three analysis rows, and the footer. The sense, structure
    // and note fields left empty are dropped rather than shown blank, which is
    // why three and not five.
    let paired: Vec<&Vec<f32>> = rows.values().filter(|xs| xs.len() == 2).collect();
    assert_eq!(
        paired.len(),
        3,
        "expected three rows carrying a label and a value, got {}",
        paired.len()
    );

    // Every label at the card's left edge, every value at one column.
    let column = paired[0][1];
    for xs in &paired {
        assert_eq!(xs[0], 0.0, "a label is not at the left edge: {xs:?}");
        assert!(xs[1] > 0.0, "a value is not indented: {xs:?}");
        assert!(
            (xs[1] - column).abs() < 0.5,
            "values start at different x: {} and {column}",
            xs[1],
        );
    }
    // The original, the translation it explains, and the footer. Each is a
    // paragraph and takes a row to itself.
    let alone = rows.values().filter(|xs| xs.len() == 1).count();
    assert_eq!(
        alone, 3,
        "expected the original, the translation and the footer to stand alone, got {alone}",
    );
    println!("\n  three rows, labels at 0, values all at {column:.0}");
}

/// Does the mark on a pair actually reach the screen?
///
/// The card carries byte offsets and the renderer turns them into a
/// DirectWrite range. Neither half is visible to `cargo test`: the offsets are
/// pure code and pass their own tests, the conversion to UTF-16 is arithmetic
/// nobody can see, and a range DirectWrite rejects leaves a card that is
/// correct in every other way.
///
/// So this draws the same card twice and counts the ink. **An underline is
/// ink.**
///
/// # The two cards differ by one underline and nothing else
///
/// `DeepAnalysis::word` appears in no row on the card. It is not `lemma`, which
/// is the dictionary form and does get a row; it is the surface word, and the
/// only thing it produces is `word_at`. So misspelling it removes exactly one
/// underline and changes not a glyph anywhere else.
///
/// The first version of this compared a card carrying a counterpart against one
/// carrying none, measured `+5700` pixels, and proved nothing: the marked card
/// also had a whole extra row on it.
///
/// ```
/// cargo test -p lens-platform-windows -- --ignored --nocapture --test-threads=1 \
///   a_marked_pair_puts_more_ink_on_the_card
/// ```
#[test]
#[ignore = "needs a live desktop session; run manually"]
fn a_marked_pair_puts_more_ink_on_the_card() {
    use lens_core::analyse::DeepAnalysis;
    use lens_core::card::Card;
    use lens_core::theme::{Preset, TextScale, Theme, Translucency};

    let glass = crate::glass::Glass::new().expect("Direct2D");
    let theme = Theme::resolve(
        Preset::System,
        Translucency::DEFAULT,
        TextScale::Normal,
        crate::appearance::system_preferences(),
    );

    let sentence = "The provider returns one rectangle per rendered line.";
    let korean = "제공자는 각 렌더링된 줄당 하나의 직사각형을 반환합니다.";
    let card_with = |word: &str, counterpart: Option<&str>| {
        Card::analysed(
            sentence,
            &DeepAnalysis {
                lemma: "rectangle".to_owned(),
                part_of_speech: "명사".to_owned(),
                word: Some(word.to_owned()),
                translation: Some(korean.to_owned()),
                counterpart: counterpart.map(str::to_owned),
                ..DeepAnalysis::default()
            },
        )
    };
    // **Pixels that are not the surface**, which is what ink is.
    //
    // The first version counted pixels with any alpha at all and got the card's
    // *area*: the surface is translucent, so every pixel inside the card
    // passes that test and two cards of the same size score identically
    // whatever is written on them. It reported `+0` and the fault was in the
    // question.
    //
    // The surface is by far the commonest value in the bitmap, so it is found
    // rather than assumed, which also survives a theme change.
    let ink = |card: Card| {
        let laid = glass
            .lay_out_between(&card, &theme, (300, 300), 1.0)
            .expect("the card lays out");
        // SAFETY: rendering to a bitmap this call owns, for verification.
        let (_, _, argb) = unsafe { glass.pixels(&laid, &theme) }.expect("pixels");
        let mut seen: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for pixel in &argb {
            *seen.entry(*pixel).or_default() += 1;
        }
        let surface = seen
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(pixel, _)| *pixel)
            .unwrap_or(0);
        argb.iter()
            .filter(|pixel| **pixel != surface && **pixel >> 24 > 0)
            .count()
    };

    // Identical cards. One knows where its word is and the other does not.
    let unmarked = ink(card_with("rectangel", Some("직사각형")));
    let marked = ink(card_with("rectangle", Some("직사각형")));

    println!("\n  word misspelt   {unmarked} opaque pixels");
    println!(
        "  word underlined {marked} opaque pixels  ({:+})",
        marked as i64 - unmarked as i64
    );

    assert!(
        marked > unmarked,
        "underlining the word drew no more ink than not underlining it",
    );

    // And a counterpart that is not in its translation marks nothing, so a
    // model that answered with a paraphrase cannot underline a guess.
    let invented = ink(card_with("rectangle", Some("삼각형")));
    println!("  counterpart absent from the translation: {invented} opaque pixels");
    assert!(
        invented < marked,
        "a counterpart that is not in the translation was marked anyway",
    );
}
