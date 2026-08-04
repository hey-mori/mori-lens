//! What Windows says the interface should look like.
//!
//! Read rather than offered as settings. A user who has already told Windows
//! they want less transparency has answered the question, and asking a second
//! time in Lens is how the two answers end up disagreeing
//! (`docs/decisions/ADR-0011-glass-needs-direct2d.md`).
//!
//! Every reader here fails soft. An unreadable preference yields the
//! conservative answer, which for transparency and motion means *less* of
//! them: guessing wrong towards a plainer card is a cosmetic loss, and
//! guessing wrong towards a translucent one can make a translation
//! unreadable for the person who asked for it not to be.

use lens_core::theme::SystemPreferences;
use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Accessibility::HIGHCONTRASTW;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// `HCF_HIGHCONTRASTON`. Taken as a literal rather than from the binding,
/// which places it inconsistently between versions.
const HCF_HIGHCONTRASTON: u32 = 0x0000_0001;

/// Read every appearance preference the theme cares about.
pub fn system_preferences() -> SystemPreferences {
    SystemPreferences {
        // Absent key means light, which is the Windows default.
        dark: personalize_dword("AppsUseLightTheme")
            .map(|v| v == 0)
            .unwrap_or(false),
        // Absent key means transparency is on, so reduced is false.
        reduced_transparency: personalize_dword("EnableTransparency")
            .map(|v| v == 0)
            .unwrap_or(false),
        high_contrast: high_contrast(),
        reduced_motion: reduced_motion(),
    }
}

/// One DWORD from the Personalize key, or `None` if it cannot be read.
fn personalize_dword(name: &str) -> Option<u32> {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();

    // SAFETY: reads a DWORD into `value`, with `size` its true capacity. The
    // path is a static string and the key is read-only.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            windows::core::PCWSTR(wide.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::addr_of_mut!(value).cast()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(value)
}

fn high_contrast() -> bool {
    let mut info = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: writes into `info`, whose `cbSize` states its true size.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            Some(std::ptr::addr_of_mut!(info).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    // Unreadable means assume high contrast is on: the plainer card.
    ok.is_err() || (info.dwFlags.0 & HCF_HIGHCONTRASTON) != 0
}

fn reduced_motion() -> bool {
    let mut animations = windows::core::BOOL(1);
    // SAFETY: writes one BOOL.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::addr_of_mut!(animations).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    // Unreadable means assume motion should be reduced.
    ok.is_err() || !animations.as_bool()
}
