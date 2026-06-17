//! Windows `AppUserModelID` (AUMID) registration.
//!
//! Windows toasts need a registered AUMID, or `notify-rust` falls back to a
//! generic PowerShell app id and the toast is mislabelled. `clamor init`
//! registers the AUMID once; the notification path then references it.

use crate::Error;
use crate::Result;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// The stable `AppUserModelID` used for `clamor`'s Windows toasts.
pub const WINDOWS_APP_ID: &str = "Clamor.ClaudeCode";

/// Registers the `AppUserModelID` under
/// `HKCU\Software\Classes\AppUserModelId\<app id>` with the given display
/// name, so Windows toasts are branded correctly.
///
/// # Errors
///
/// Returns [`Error::RegisterAppId`] if the registry key cannot be created or
/// its `DisplayName` value cannot be written.
pub fn register_app_id(display_name: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!("Software\\Classes\\AppUserModelId\\{WINDOWS_APP_ID}");
    let (key, _) = hkcu.create_subkey(&path).map_err(Error::RegisterAppId)?;
    key.set_value("DisplayName", &display_name.to_owned())
        .map_err(Error::RegisterAppId)?;
    Ok(())
}
