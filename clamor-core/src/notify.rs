//! Desktop notification delivery via `notify-rust`.
//!
//! `notify-rust` abstracts over D-Bus (Linux/BSD), `mac-notification-sys`
//! (macOS), and `tauri-winrt-notification` (Windows). The native sound is
//! delivered through the notification itself; the per-platform sound name
//! differs, so it is selected with a `cfg`'d constant.

use crate::Error;
use crate::Result;
use crate::notify::native_sound::SOUND_NAME;
use notify_rust::Notification;

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
/// each platform's expected identifier.
mod native_sound {
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(super) const SOUND_NAME: &str = "message-new-instant";

    #[cfg(target_os = "macos")]
    pub(super) const SOUND_NAME: &str = "NSUserNotificationDefaultSoundName";

    #[cfg(target_os = "windows")]
    pub(super) const SOUND_NAME: &str = "Default";
}

/// Shows a desktop notification.
///
/// # Errors
///
/// Returns [`Error::Notify`] if the platform notification backend fails to
/// display the toast.
pub(crate) fn show(spec: &NotificationSpec) -> Result<()> {
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
            // Linux/BSD play a sound by default; suppress it explicitly. macOS
            // and Windows are silent when no sound name is set.
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
