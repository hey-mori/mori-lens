//! The binary must not open a console window.
//!
//! A tray application that opens a black terminal you then have to leave open
//! is not a tray application. `#![windows_subsystem = "windows"]` in
//! `main.rs` is what prevents it, and an attribute is exactly the kind of thing
//! that gets removed while chasing something else: nothing fails, the tests
//! stay green, and the product quietly goes back to looking unfinished.
//!
//! So this reads the field out of the executable Cargo just built.

#![cfg(windows)]

/// Offset of `e_lfanew` in the DOS header: where the PE header starts.
const E_LFANEW: usize = 0x3c;
/// Bytes from the PE signature to the start of the optional header: the
/// four-byte signature plus a twenty-byte COFF header.
const OPTIONAL_HEADER: usize = 24;
/// `Subsystem` sits at this offset inside the optional header, in both the
/// 32-bit and 64-bit layouts.
const SUBSYSTEM: usize = 68;

/// `IMAGE_SUBSYSTEM_WINDOWS_GUI`. No console is allocated at startup.
const WINDOWS_GUI: u16 = 2;
/// `IMAGE_SUBSYSTEM_WINDOWS_CUI`. Windows creates a console window for it.
const WINDOWS_CUI: u16 = 3;

#[test]
fn the_built_binary_does_not_ask_windows_for_a_console() {
    let path = env!("CARGO_BIN_EXE_lens");
    let image = std::fs::read(path).expect("cargo built this binary, so it can be read");

    let pe_at = u32::from_le_bytes(
        image[E_LFANEW..E_LFANEW + 4]
            .try_into()
            .expect("a DOS header"),
    ) as usize;
    assert_eq!(&image[pe_at..pe_at + 4], b"PE\0\0", "not a PE image");

    let at = pe_at + OPTIONAL_HEADER + SUBSYSTEM;
    let subsystem = u16::from_le_bytes(image[at..at + 2].try_into().expect("a subsystem field"));

    assert_ne!(
        subsystem, WINDOWS_CUI,
        "lens.exe is a console application, so double-clicking it opens a terminal window that has to stay open for the product to keep running. Restore #![windows_subsystem = \"windows\"] in main.rs.",
    );
    assert_eq!(
        subsystem, WINDOWS_GUI,
        "unexpected PE subsystem {subsystem}",
    );
}
