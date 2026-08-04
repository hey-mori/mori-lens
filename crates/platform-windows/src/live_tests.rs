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
