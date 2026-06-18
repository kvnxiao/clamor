//! Desktop notification delivery.
//!
//! Everywhere except macOS this goes through `notify-rust`, which abstracts
//! over D-Bus (Linux/BSD) and `tauri-winrt-notification` (Windows); the native
//! sound rides on the notification under a per-platform sound name selected
//! with a `cfg`'d constant.
//!
//! macOS is the exception. `notify-rust`'s default backend is the deprecated
//! `NSUserNotification` API, which silently delivers nothing for an unbundled
//! CLI binary on modern macOS (the call still reports success, so the failure
//! is invisible). clamor ships as a bare binary on `PATH`, not a signed `.app`,
//! and every non-deprecated macOS API (`UNUserNotificationCenter`) requires
//! such a bundle. So macOS shells out to `osascript`'s `display notification`,
//! which runs inside a system app context and actually displays. The cost is
//! that the toast is attributed to "Script Editor" (the bundle `osascript`
//! borrows) and the notifying app name cannot be overridden.

use crate::Error;
use crate::Result;

/// Which sound the OS should play with the notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSound {
    /// Request the platform's default notification sound.
    Default,
    /// Suppress notification sound. Used for `none`, and when a custom audio
    /// file plays the sound instead.
    Silent,
}

/// A fully-resolved notification ready to display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotificationSpec {
    /// Notification title (summary line).
    pub title: String,
    /// Notification body.
    pub body: String,
    /// Which sound the OS should play.
    pub sound: NativeSound,
}

/// The native notification sound name, which differs per platform. An invalid
/// name is silently treated as no sound by the backends, so these must match
/// each platform's expected identifier. macOS does not use this (its
/// `osascript` backend names a concrete system sound instead).
#[cfg(not(target_os = "macos"))]
mod native_sound {
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(super) const SOUND_NAME: &str = "message-new-instant";

    #[cfg(target_os = "windows")]
    pub(super) const SOUND_NAME: &str = "Default";
}

/// Shows a desktop notification through `notify-rust`.
///
/// # Errors
///
/// Returns [`Error::Notify`] if the platform notification backend fails to
/// display the toast.
#[cfg(not(target_os = "macos"))]
pub(crate) fn show(spec: &NotificationSpec) -> Result<()> {
    use crate::notify::native_sound::SOUND_NAME;
    use notify_rust::Notification;

    let mut notification = Notification::new();
    notification
        .appname(crate::APP_NAME)
        .summary(&spec.title)
        .body(&spec.body);

    match spec.sound {
        NativeSound::Default => {
            notification.sound_name(SOUND_NAME);
        }
        NativeSound::Silent => {
            // Linux/BSD play a sound by default; suppress it explicitly. Windows
            // is silent when no sound name is set.
            #[cfg(all(unix, not(target_os = "macos")))]
            notification.hint(notify_rust::Hint::SuppressSound(true));
        }
    }

    // Windows silently drops a toast whose AppUserModelID is not registered, so
    // register ours (idempotent) and only brand the toast with it when that
    // succeeds. Otherwise fall back to notify-rust's default app id so the
    // toast still appears. On Windows, branding comes from this AUMID rather
    // than `appname`, which is a no-op there.
    #[cfg(windows)]
    if crate::windows::ensure_registered(crate::APP_NAME).is_ok() {
        notification.app_id(crate::windows::WINDOWS_APP_ID);
    }

    notification.show().map_err(Error::Notify)?;
    Ok(())
}

/// Shows a desktop notification by shelling out to macOS `osascript`.
///
/// `notify-rust`'s default macOS backend (the deprecated `NSUserNotification`
/// API) silently shows nothing for an unbundled CLI binary, and every modern
/// macOS API needs a signed `.app` bundle clamor does not have. `osascript`
/// runs inside a system app context, so its `display notification` actually
/// appears. Title and body are passed as `argv`, never interpolated into the
/// script source, so arbitrary text — including the hook `message` — cannot
/// inject `AppleScript`.
///
/// # Errors
///
/// Returns [`Error::Notify`] if `osascript` cannot be spawned or exits
/// non-zero.
#[cfg(target_os = "macos")]
pub(crate) fn show(spec: &NotificationSpec) -> Result<()> {
    use std::process::Command;

    // A system sound file in `/System/Library/Sounds`, played for
    // `NativeSound::Default`. `display notification` has no token for the user's
    // configured default alert sound, so a concrete name stands in.
    const NATIVE_SOUND_NAME: &str = "Ping";

    // The script source is a fixed literal; the title, body, and sound name
    // arrive as `argv` items, so user text is data rather than code. The only
    // structural difference is whether a `sound name` clause is present.
    let script = match spec.sound {
        NativeSound::Default => {
            "on run argv\n\
             display notification (item 2 of argv) with title (item 1 of argv) sound name (item 3 of argv)\n\
             end run"
        }
        NativeSound::Silent => {
            "on run argv\n\
             display notification (item 2 of argv) with title (item 1 of argv)\n\
             end run"
        }
    };

    let mut command = Command::new("osascript");
    command
        .arg("-e")
        .arg(script)
        .arg(&spec.title)
        .arg(&spec.body);
    if matches!(spec.sound, NativeSound::Default) {
        command.arg(NATIVE_SOUND_NAME);
    }

    let output = command.output().map_err(Error::Notify)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Notify(std::io::Error::other(format!(
            "osascript exited with {}: {}",
            output.status,
            stderr.trim()
        ))));
    }
    Ok(())
}
