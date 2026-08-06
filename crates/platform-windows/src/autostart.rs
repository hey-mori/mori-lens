//! Starting with Windows, if the user asks.
//!
//! A tool that only helps while it is running, and that has to be started by
//! hand every morning, is a tool that gets used for a week. This is the
//! standard per-user run key, which needs no installer, no elevation, and no
//! service.
//!
//! # What it deliberately does not do
//!
//! It is **off unless the user turns it on**, from the tray menu, and there is
//! no flag or setting that enables it as a side effect of something else. A
//! screen-reading tool that arranged to start with the machine without being
//! asked would be doing the thing it exists to promise it does not.
//!
//! `HKEY_CURRENT_USER` rather than `HKEY_LOCAL_MACHINE`: this is one person's
//! choice on their own account, it needs no administrator, and it cannot affect
//! anyone else who uses the machine.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};

/// Where Windows looks for things to start when this user logs in.
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

/// The value name. Ours, and specific enough that nothing else will collide.
const VALUE: PCWSTR = w!("Mori Lens");

/// Whether Lens is set to start with Windows.
pub fn is_enabled() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    read(RUN_KEY, VALUE).is_some_and(|recorded| points_at(&recorded, &exe))
}

/// Whether a recorded run-key command starts this exact binary.
///
/// **Not whether the value exists**, which is what this used to ask. The run
/// key holds a path, and the README tells people to put `lens.exe` wherever
/// they keep such things, so moving it or unzipping a new release beside the
/// old one leaves an entry pointing at a binary that is not there. The tray
/// went on showing a tick, which is the product asserting something Windows
/// would disagree with at the next sign-in.
///
/// Compared case-insensitively, because Windows paths are, and with the quotes
/// stripped from both sides, because the value carries them and a path does
/// not.
///
/// Two copies of Lens in two folders is a real arrangement and this handles it
/// the only way one run-key value can: the one named is ticked, the other is
/// not, and ticking the other takes the entry.
fn points_at(recorded: &str, exe: &std::path::Path) -> bool {
    let unquoted = recorded.trim().trim_matches('"');
    unquoted.eq_ignore_ascii_case(&exe.display().to_string())
}

/// Start with Windows, using this executable's current location.
///
/// Returns whether it took. The path is recorded as it is now, so moving the
/// binary breaks it: that is how the run key works, and re-enabling it from the
/// new location is the fix.
pub fn enable() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    // Quoted, because a path with a space in it is otherwise read as a command
    // and its first argument. `C:\Program Files\...` is the common case, not
    // the exotic one.
    let command = format!("\"{}\"", exe.display());
    write(RUN_KEY, VALUE, &command)
}

/// Stop starting with Windows.
pub fn disable() -> bool {
    delete(RUN_KEY, VALUE)
}

/// Read a string value, or `None` when it is not there.
fn read(subkey: PCWSTR, value: PCWSTR) -> Option<String> {
    let mut size = 0u32;
    // SAFETY: `RegGetValueW` with a null buffer asks for the size only, which
    // is the documented way to find out how much to allocate.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    // SAFETY: the buffer is exactly the size the call above asked for.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    let wide: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&wide))
}

fn write(subkey: PCWSTR, value: PCWSTR, contents: &str) -> bool {
    let mut wide: Vec<u16> = contents.encode_utf16().collect();
    wide.push(0);
    // SAFETY: reinterpreting a `u16` slice this function owns as the bytes the
    // registry API takes. Alignment is looser, not tighter, and the length is
    // exact.
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };

    // SAFETY: documented registry calls; the key is closed on every path.
    unsafe {
        let mut key = HKEY::default();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE | KEY_READ,
            None,
            &mut key,
            None,
        );
        if status != ERROR_SUCCESS {
            return false;
        }
        let set = RegSetValueExW(key, value, None, REG_SZ, Some(bytes)) == ERROR_SUCCESS;
        let _ = RegCloseKey(key);
        set
    }
}

fn delete(subkey: PCWSTR, value: PCWSTR) -> bool {
    // SAFETY: documented registry calls; the key is closed on every path.
    unsafe {
        let mut key = HKEY::default();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE | KEY_READ,
            None,
            &mut key,
            None,
        );
        if status != ERROR_SUCCESS {
            return false;
        }
        let deleted = RegDeleteValueW(key, value) == ERROR_SUCCESS;
        let _ = RegCloseKey(key);
        deleted
    }
}

/// Exercise the read, write and delete against a key of our own.
///
/// **Not the run key.** A test that turned on autostart to prove it could would
/// leave a machine starting Lens at login because someone ran the suite, which
/// is precisely the thing the module above refuses to do without being asked.
/// This uses a scratch value under this application's own key and removes it.
///
/// Returns the three outcomes: written, read back correctly, deleted.
#[cfg(test)]
pub fn round_trip_against_a_scratch_key() -> (bool, bool, bool) {
    const SCRATCH: PCWSTR = w!("Software\\Mori Lens\\SuiteScratch");
    const NAME: PCWSTR = w!("round-trip");

    let written = write(SCRATCH, NAME, "\"C:\\somewhere\\lens.exe\"");
    let read_back = read(SCRATCH, NAME).as_deref() == Some("\"C:\\somewhere\\lens.exe\"");
    let deleted = delete(SCRATCH, NAME);
    (written, read_back, deleted)
}

#[cfg(test)]
pub fn scratch_is_gone() -> bool {
    read(w!("Software\\Mori Lens\\SuiteScratch"), w!("round-trip")).is_none()
}

#[cfg(test)]
mod points_at_tests {
    use super::points_at;
    use std::path::Path;

    #[test]
    fn the_quoted_path_this_binary_wrote_is_recognised() {
        let exe = Path::new(r"C:\Tools\Mori Lens\lens.exe");
        assert!(points_at(r#""C:\Tools\Mori Lens\lens.exe""#, exe));
    }

    /// Windows paths are case-insensitive and the registry keeps whatever was
    /// typed, so a comparison that is not would report a moved binary for one
    /// that never moved.
    #[test]
    fn case_does_not_matter() {
        let exe = Path::new(r"C:\Tools\lens.exe");
        assert!(points_at(r#""c:\tools\LENS.EXE""#, exe));
    }

    /// An entry written before this comparison existed, or by hand, without
    /// the quotes.
    #[test]
    fn an_unquoted_value_still_matches() {
        let exe = Path::new(r"C:\Tools\lens.exe");
        assert!(points_at(r"C:\Tools\lens.exe", exe));
    }

    /// The case this exists for: the binary moved, or a new release was
    /// unzipped beside the old one.
    #[test]
    fn a_different_location_is_not_this_binary() {
        let exe = Path::new(r"C:\Tools\lens.exe");
        assert!(!points_at(r#""C:\Downloads\lens.exe""#, exe));
        assert!(!points_at(r#""C:\Tools\old\lens.exe""#, exe));
    }

    #[test]
    fn nothing_recorded_is_not_this_binary() {
        assert!(!points_at("", Path::new(r"C:\Tools\lens.exe")));
        assert!(!points_at(r#""""#, Path::new(r"C:\Tools\lens.exe")));
    }
}
