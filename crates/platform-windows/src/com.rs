//! COM apartment handling for one scope.
//!
//! UI Automation is COM, and Lens's acquisition runs on a worker thread whose
//! apartment it does not own. The rule, and the reason it is a guard rather
//! than a pair of calls: an early `?` must never leave a thread in an apartment
//! this code created.
//!
//! Re-derived from `swarea/mori`'s `src-tauri/src/language/capture_windows.rs`.
//! Copied rather than shared, per `docs/decisions/ADR-0006-no-shared-package-yet.md`; the
//! origin is named here so a later extraction can find both sites.

use windows::Win32::Foundation::S_FALSE;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

/// Initialises COM for this scope and undoes it on drop, but only if this scope
/// is what initialised it.
#[derive(Debug)]
pub struct ComScope {
    owned: bool,
}

impl ComScope {
    pub fn enter() -> Self {
        // SAFETY: matched by `CoUninitialize` in `Drop`, and only when this call
        // is the one that initialised the apartment.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

        // Three outcomes worth distinguishing:
        //   S_OK              we initialised it, we uninitialise it
        //   S_FALSE           already initialised on this thread, and our
        //                     uninitialise is still required to balance it
        //   RPC_E_CHANGED_MODE  someone else owns a different apartment. Usable,
        //                     but not ours to tear down.
        Self {
            owned: result.is_ok() || result == S_FALSE,
        }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balances the successful `CoInitializeEx` above.
            unsafe { CoUninitialize() };
        }
    }
}
