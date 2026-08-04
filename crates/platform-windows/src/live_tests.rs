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
