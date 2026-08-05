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

use lens_core::theme::{Rgb, SystemColours, SystemPreferences};
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
    let hc = high_contrast();
    SystemPreferences {
        // Absent means light, the Windows default. An unreadable key also
        // yields light, which is the safe direction here: the wrong light card
        // is ugly, the wrong dark one is what a reader chose against.
        dark: matches!(personalize_dword("AppsUseLightTheme"), Read::Value(0)),
        reduced_transparency: match personalize_dword("EnableTransparency") {
            // The user turned transparency off.
            Read::Value(0) => true,
            Read::Value(_) => false,
            // Absent is the documented Windows default: transparency on.
            Read::Absent => false,
            // An actual failure. Fail towards the solid card, because guessing
            // wrong that way costs an effect and guessing wrong the other way
            // can leave a translation unreadable for the person who asked for
            // it not to be.
            Read::Failed => true,
        },
        high_contrast: hc,
        reduced_motion: reduced_motion(),
        // Only when high contrast is on. Outside it these are the ordinary
        // system colours, and a preset chosen for reading is the better
        // answer than the desktop's window colours.
        high_contrast_colours: hc.then(high_contrast_colours).flatten(),
    }
}

/// The three outcomes of reading a preference, which are not two.
///
/// An absent key and a failed read mean different things and deserve different
/// answers: absent is a documented default, failed is ignorance. Collapsing
/// them into `Option` is exactly what made this file's comment and its
/// behaviour disagree, the comment promising a conservative fallback that the
/// code did not perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Read {
    Value(u32),
    Absent,
    Failed,
}

/// One DWORD from the Personalize key.
fn personalize_dword(name: &str) -> Read {
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
    // `RegGetValueW` returns a WIN32_ERROR directly rather than a Result.
    // ERROR_FILE_NOT_FOUND is the documented answer for a key or value that
    // is simply not there, which is the Windows default rather than a fault.
    const ERROR_SUCCESS: u32 = 0;
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    match status.0 {
        ERROR_SUCCESS => Read::Value(value),
        ERROR_FILE_NOT_FOUND => Read::Absent,
        _ => Read::Failed,
    }
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

/// The user's chosen high-contrast colours.
///
/// Read only when high contrast is on, because outside it these are the
/// ordinary system colours and a preset is the better answer.
fn high_contrast_colours() -> Option<SystemColours> {
    use windows::Win32::Graphics::Gdi::{
        GetSysColor, COLOR_GRAYTEXT, COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    };

    // `GetSysColor` returns a COLORREF, which is 0x00BBGGRR.
    fn rgb(colorref: u32) -> Rgb {
        Rgb::new(
            (colorref & 0xff) as u8,
            ((colorref >> 8) & 0xff) as u8,
            ((colorref >> 16) & 0xff) as u8,
        )
    }

    // SAFETY: read-only lookups of documented system colour indices.
    unsafe {
        Some(SystemColours {
            window: rgb(GetSysColor(COLOR_WINDOW)),
            window_text: rgb(GetSysColor(COLOR_WINDOWTEXT)),
            gray_text: rgb(GetSysColor(COLOR_GRAYTEXT)),
            hot_light: rgb(GetSysColor(COLOR_HOTLIGHT)),
        })
    }
}
