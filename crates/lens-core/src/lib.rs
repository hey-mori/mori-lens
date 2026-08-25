//! Mori Lens · platform-neutral core.
//!
//! Everything here runs identically on Windows and macOS because none of it
//! knows which one it is on. That is the whole point: the hard, untestable work
//! lives in an adapter crate behind [`acquire::TextAcquirer`], and everything
//! that decides *what to do* lives here where it can be tested without a
//! desktop.
//!
//! # The rule this crate enforces by construction
//!
//! No OS crates. Not `windows`, not `objc`, not `tauri`. If a type needs an
//! operating system concept, the concept gets a plain Rust representation here
//! and the adapter converts at the boundary. See
//! `docs/decisions/ADR-0003-rust-core-platform-adapters.md`.
//!
//! # Reading order
//!
//! - [`hover`] decides that the user is reading, before anything is spent.
//! - [`acquire`] is the contract an operating system adapter implements.
//! - [`text`] splits what was read into word, sentence, and paragraph.
//! - [`policy`] decides where the pointer is allowed to look at all.
//! - [`session`] assembles those into the loop a binary drives.
//! - [`geometry`] is the one coordinate space, physical screen pixels.

pub mod acquire;
pub mod analyse;
pub mod barren;
pub mod cache;
pub mod card;
pub mod detail;
pub mod geometry;
pub mod hover;
pub mod icon;
pub mod keep;
pub mod language;
pub mod policy;
pub mod reach;
pub mod session;
pub mod settings;
pub mod text;
pub mod theme;
pub mod translate;
pub mod welcome;

pub use acquire::{
    AcquireContext, AcquireError, AcquisitionSource, AppIdentity, ProtectedReason, ProviderInfo,
    TextAcquirer, TextTarget,
};
pub use geometry::{ScreenPoint, ScreenRect};
pub use hover::{
    Generation, HoverConfig, HoverEvent, HoverMachine, PointerSample, Presentation,
    TargetFingerprint, Timestamp,
};
pub use policy::{AppPolicy, AppTier};
pub use session::{HoverSession, InputState, OcrMode, Resolution};
pub use text::{segment, SegmentedText, TextSpan};

#[cfg(test)]
mod contract_tests;
