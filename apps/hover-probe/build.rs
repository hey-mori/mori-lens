//! Bake the commit this probe was built from into the binary.
//!
//! Two problems, one fix.
//!
//! The compatibility matrix requires a build sha with every row, and an
//! operator who has to look it up by hand will eventually record the wrong
//! one, usually after a rebuild they forgot they did.
//!
//! Worse, a *stale* binary produces measurements that look completely
//! ordinary. `cargo test` does not rebuild the binary target, so running
//! `scripts/check.sh` and then `./target/debug/hover-probe` can measure code
//! that is minutes out of date, silently. That happened during this crate's
//! first sweep and cost a full run.
//!
//! A sha printed in the output makes both visible: compare it to
//! `git rev-parse --short HEAD` and a stale binary announces itself.

use std::process::Command;

fn main() {
    // Re-run when the checked-out commit changes. `.git/index` is included so
    // staging something usually refreshes the dirty flag too, though that flag
    // is best-effort by nature: cargo cannot watch every source file for this.
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
/// A build outside a git checkout is a normal thing, not an error: it yields
/// `unknown`, which is honest and still tells a reader the sha is not to be
/// trusted.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
