//! Core library for `clamor`: cross-platform desktop notifications and audio
//! for Claude Code hooks.
//!
//! Claude Code fires hook events (permission prompts, idle waiting, task and
//! subagent completion) but has no built-in desktop notification or sound.
//! `clamor` is registered as the hook command; it reads the hook JSON on
//! standard input, resolves a per-event configuration, and fires a desktop
//! notification with either the native system sound or a user-supplied audio
//! file.
//!
//! The entry point is [`dispatch`], which takes a parsed [`HookInput`] and
//! performs the notification side-effect. Parse the hook payload with
//! [`HookInput::from_json`].
//!
//! # Examples
//!
//! ```no_run
//! use clamor_core::HookInput;
//!
//! let payload = r#"{"hook_event_name":"Stop"}"#;
//! let input = HookInput::from_json(payload)?;
//! clamor_core::dispatch(&input)?;
//! # Ok::<(), clamor_core::Error>(())
//! ```

mod audio;
mod config;
mod dispatch;
mod event;
mod notify;

#[cfg(windows)]
mod windows;

pub use crate::config::Config;
pub use crate::config::DEFAULT_CONFIG_TOML;
pub use crate::config::EventConfig;
pub use crate::config::Notifications;
pub use crate::config::SoundConfig;
pub use crate::config::SoundKeyword;
pub use crate::config::user_config_path;
pub use crate::dispatch::TestOutcome;
pub use crate::dispatch::dispatch;
pub use crate::dispatch::dispatch_test;
pub use crate::event::HookInput;
#[cfg(windows)]
pub use crate::windows::WINDOWS_APP_ID;
#[cfg(windows)]
pub use crate::windows::register_app_id;
use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors produced while loading configuration, parsing hook input, or firing
/// a notification.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The hook payload on standard input was not valid JSON.
    #[error("failed to parse hook input as JSON")]
    ParseInput(#[source] serde_json::Error),

    /// A filesystem operation failed. Path context is carried by `fs-err`.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The configuration file existed but was not valid TOML.
    #[error("failed to parse config file `{path}`")]
    ParseConfig {
        /// Path to the offending config file.
        path: Utf8PathBuf,
        /// The underlying TOML parse error.
        #[source]
        source: toml::de::Error,
    },

    /// Showing the desktop notification failed.
    #[error("failed to show desktop notification")]
    Notify(#[source] notify_rust::error::Error),

    /// Opening the default audio output device failed.
    #[error("failed to open the default audio output device")]
    AudioDevice(#[source] rodio::DeviceSinkError),

    /// Decoding or playing the custom audio file failed.
    #[error("failed to play audio file `{path}`")]
    AudioPlay {
        /// Path to the audio file that could not be played.
        path: Utf8PathBuf,
        /// The underlying audio decode/playback error.
        #[source]
        source: rodio::PlayError,
    },

    /// Writing to the Windows registry to register the app id failed.
    #[cfg(windows)]
    #[error("failed to register the Windows AppUserModelID")]
    RegisterAppId(#[source] std::io::Error),
}

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;
