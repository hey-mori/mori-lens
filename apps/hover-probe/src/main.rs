//! Mori Lens · hover probe.
//!
//! Milestone 1's deliverable, and it is an instrument rather than a feature.
//! It follows the pointer, resolves what is under it, and records what
//! happened: how far the chain got, how long each stage took, and how the
//! failures are distributed per application
//! (`docs/decisions/ADR-0008-probe-before-product.md`).
//!
//! It translates nothing and draws nothing. That is on purpose: the gate to
//! building an overlay is a filled-in compatibility matrix, and this is what
//! fills it in.

#[cfg(not(windows))]
fn main() {
    eprintln!("The hover probe is Windows-only today. macOS acquisition is Milestone 6.");
    std::process::exit(2);
}

#[cfg(windows)]
mod probe;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    probe::run()
}
