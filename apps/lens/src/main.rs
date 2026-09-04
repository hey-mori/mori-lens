//! Mori Lens. Rest the pointer on a sentence; read it in your language.
//!
//! Cross-platform entry point. The platform-specific product loop lives in
//! `win.rs` (Windows) and `macos.rs` (macOS). Each is compiled only on its
//! own platform; the other is invisible to the compiler.

// Windows: the full product loop, in win.rs.
#[cfg(windows)]
#[path = "win.rs"]
mod win;

// macOS: M6 acquisition wired in; overlay and OCR arrive in M7.
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;

// Re-export the platform main so the binary has a crate-level entry point.
#[cfg(windows)]
fn main() -> std::process::ExitCode {
    win::main()
}

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    macos::main()
}
