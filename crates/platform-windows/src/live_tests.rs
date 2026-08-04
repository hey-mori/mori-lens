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
