//! macOS Accessibility permission state.
//!
//! The AX subsystem requires the calling process to be trusted before any
//! element can be read. This module wraps the trust check in a typed state
//! so that missing permission flows through the domain as
//! [`AcquireError::Unavailable`] rather than a panic or silent failure.

use crate::ffi;

/// Whether the process is trusted by the Accessibility subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxPermission {
    /// The process has been granted access.
    Trusted,
    /// The process is not trusted. The user must grant access in System
    /// Settings.
    NotTrusted,
}

impl AxPermission {
    /// Check the current trust state without prompting.
    pub fn check() -> Self {
        // SAFETY: `AXIsProcessTrusted` is a pure query with no side effects.
        // It returns a Boolean (u8): 1 for trusted, 0 for not.
        let trusted = unsafe { ffi::AXIsProcessTrusted() };
        if trusted != 0 {
            Self::Trusted
        } else {
            Self::NotTrusted
        }
    }

    /// Check trust, optionally prompting the user.
    ///
    /// When `prompt` is true, the system may show a dialog asking the user to
    /// grant Accessibility access. The result reflects the state *after* the
    /// prompt: if the user grants, `Trusted`; otherwise `NotTrusted`.
    pub fn check_with_prompt(prompt: bool) -> Self {
        if !prompt {
            return Self::check();
        }
        // SAFETY: We build a CFDictionary with the prompt key and pass it to
        // AXIsProcessTrustedWithOptions. The dictionary is released after the
        // call. The function returns a Boolean.
        unsafe {
            let key = ffi::ax_trusted_check_option_prompt();
            let value =
                &ffi::K_CF_BOOLEAN_TRUE as *const ffi::__CFData as *const std::os::raw::c_void;
            let keys = [key];
            let values = [value];
            let dict = ffi::CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                std::ptr::null(),
                std::ptr::null(),
            );
            let trusted = ffi::AXIsProcessTrustedWithOptions(dict);
            if !dict.is_null() {
                ffi::CFRelease(dict);
            }
            if trusted != 0 {
                Self::Trusted
            } else {
                Self::NotTrusted
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_variants_are_distinct() {
        assert_ne!(AxPermission::Trusted, AxPermission::NotTrusted);
    }

    #[test]
    fn check_returns_a_valid_state() {
        // On CI without AX permission, calling AX APIs can cause SIGTRAP
        // during process exit (macOS AX framework cleanup bug). Skip this
        // test when not trusted to avoid the cleanup crash.
        // The AX API contract is verified on developer machines.
        if std::env::var("CI").is_ok() {
            // On CI, just verify the enum variants exist.
            let _ = AxPermission::Trusted;
            let _ = AxPermission::NotTrusted;
            return;
        }
        let state = AxPermission::check();
        assert!(
            state == AxPermission::Trusted || state == AxPermission::NotTrusted,
            "check must return a known variant"
        );
    }
}
