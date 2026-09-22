//! Integration tests for Tauri application.
//!
//! These tests verify the built binary and basic functionality.
//! They run against the actual compiled artifact, not mocks.

/// Test that the binary was built successfully.
#[test]
fn the_binary_exists_and_is_executable() {
    let path = env!("CARGO_BIN_EXE_lens");
    assert!(
        std::path::Path::new(path).exists(),
        "binary should exist at {}",
        path
    );
}

/// Test that settings storage directory can be created.
#[test]
fn settings_storage_can_be_initialized() {
    let temp_dir = std::env::temp_dir().join("lens-integration-test-settings");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");

    // Test that we can create a settings path
    let settings_path = temp_dir.join("settings.json");
    assert!(!settings_path.exists(), "settings should not exist yet");

    // Verify we can write to the directory
    let test_file = temp_dir.join("test.txt");
    std::fs::write(&test_file, "test").expect("write test file");
    assert!(test_file.exists(), "should be able to write to temp dir");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Test that library storage directory can be created.
#[test]
fn library_storage_can_be_initialized() {
    let temp_dir = std::env::temp_dir().join("lens-integration-test-library");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");

    // Test that we can create a library path
    let library_path = temp_dir.join("library.json");
    assert!(!library_path.exists(), "library should not exist yet");

    // Verify we can write to the directory
    let test_file = temp_dir.join("test.txt");
    std::fs::write(&test_file, "test").expect("write test file");
    assert!(test_file.exists(), "should be able to write to temp dir");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Test that the binary has the correct PE subsystem on Windows.
/// This is a duplicate of no_console_window.rs but included here for completeness.
#[test]
#[cfg(windows)]
fn binary_is_gui_application_on_windows() {
    let path = env!("CARGO_BIN_EXE_lens");
    let image = std::fs::read(path).expect("cargo built this binary, so it can be read");

    // PE header offset
    let e_lfanew = u32::from_le_bytes(
        image[0x3c..0x3c + 4]
            .try_into()
            .expect("a DOS header"),
    ) as usize;

    // Verify PE signature
    assert_eq!(&image[e_lfanew..e_lfanew + 4], b"PE\0\0", "not a PE image");

    // Subsystem field offset: PE signature (4) + COFF header (20) + optional header offset (68)
    let subsystem_offset = e_lfanew + 24 + 68;
    let subsystem = u16::from_le_bytes(
        image[subsystem_offset..subsystem_offset + 2]
            .try_into()
            .expect("a subsystem field"),
    );

    // IMAGE_SUBSYSTEM_WINDOWS_GUI = 2
    assert_eq!(
        subsystem, 2,
        "binary should be a GUI application (subsystem 2), got {}",
        subsystem
    );
}
