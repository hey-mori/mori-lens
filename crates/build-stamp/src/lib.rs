//! Bake the commit a binary was built from into it.
//!
//! Two problems, one fix, and the second is the expensive one.
//!
//! The compatibility matrix requires a build sha with every row, and an
//! operator who has to look it up by hand will eventually record the wrong one,
//! usually after a rebuild they forgot they did.
//!
//! Worse, a **stale binary produces output that looks completely ordinary**.
//! `cargo test` does not rebuild a binary target, so running `scripts/check.sh`
//! and then the binary can measure code that is minutes out of date, silently.
//! That happened during the probe's first sweep and cost a full run.
//!
//! It happened again on 2026-08-06, to the product rather than the probe, and
//! that is why this is a crate rather than one app's build script. A reader
//! pasted a session log and neither of us could tell which build it came from,
//! so a defect that had been fixed an hour earlier was re-read as still
//! present. The probe printed its sha and `lens` did not.
//!
//! One copy, because two copies of "which commit is this" can disagree, and
//! the whole value of the answer is that it cannot.

use std::process::Command;

/// Emit `LENS_BUILD_SHA` for the crate being built.
///
/// Call from a `build.rs`. The value is the short sha, with `-dirty` appended
/// when the working tree has uncommitted changes, or `unknown` outside a git
/// checkout.
pub fn emit() {
    // Re-run when the checked-out commit changes. `.git/index` is included so
    // staging something usually refreshes the dirty flag too, though that flag
    // is best-effort by nature: cargo cannot watch every source file for it.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");

    let sha = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty = git(&["status", "--porcelain"]).is_some_and(|output| !output.is_empty());

    println!(
        "cargo:rustc-env=LENS_BUILD_SHA={sha}{}",
        if dirty { "-dirty" } else { "" }
    );
}

/// Run a git command, or `None` if git is unavailable or the command failed.
///
/// A build outside a git checkout is a normal thing rather than an error: it
/// yields `unknown`, which is honest and still tells a reader the sha is not to
/// be trusted.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
