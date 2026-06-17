//! Windows `AppUserModelID` (AUMID) registration.
//!
//! Windows toasts need a registered AUMID, or `notify-rust` falls back to a
//! generic PowerShell app id and the toast is mislabelled. The notification
//! path registers the AUMID lazily on the first toast, so there is no separate
//! setup step.

use crate::Error;
use crate::Result;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// The stable `AppUserModelID` used for `clamor`'s Windows toasts.
pub(crate) const WINDOWS_APP_ID: &str = "Clamor.ClaudeCode";

/// The `HKCU` sub-path under which the AUMID is registered. The write
/// (`register_app_id`) and the read-back probe (`ensure_registered`) must
/// agree on this exact path, so it lives in one place.
fn app_id_subkey() -> String {
    format!("Software\\Classes\\AppUserModelId\\{WINDOWS_APP_ID}")
}

/// Registers the `AppUserModelID` under
/// `HKCU\Software\Classes\AppUserModelId\<app id>` with the given display
/// name, so Windows toasts are branded correctly.
///
/// # Errors
///
/// Returns [`Error::RegisterAppId`] if the registry key cannot be created or
/// its `DisplayName` value cannot be written.
pub(crate) fn register_app_id(display_name: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(app_id_subkey())
        .map_err(Error::RegisterAppId)?;
    key.set_value("DisplayName", &display_name)
        .map_err(Error::RegisterAppId)?;
    Ok(())
}

/// Registers the `AppUserModelID` only if it is not already present.
///
/// Windows silently drops a toast whose AUMID is unregistered, so the
/// notification path calls this on every toast to guarantee one can be shown.
/// If the key already exists it is left alone, so the first toast owns the
/// canonical `DisplayName`.
///
/// # Errors
///
/// Returns [`Error::RegisterAppId`] if the key is absent and cannot be created.
pub(crate) fn ensure_registered(display_name: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if hkcu.open_subkey(app_id_subkey()).is_ok() {
        return Ok(());
    }
    register_app_id(display_name)
}
