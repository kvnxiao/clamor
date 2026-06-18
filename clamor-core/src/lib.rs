//! Core library for `clamor`: cross-platform desktop notifications and audio
//! for Claude Code hooks.
//!
//! Claude Code fires hook events (permission prompts, idle waiting, task and
//! subagent completion) but has no built-in desktop notification or sound.
//! `clamor` is registered as the hook command; the title and sound come from
//! command-line flags set in the hook's `settings.json` entry, and the body
//! from the hook `message` on standard input.
//!
//! The entry point is [`fire`], which takes a fully-specified [`Dispatch`]
//! and performs the notification side-effect. Parse the hook payload (for its
//! `message`) with [`HookInput::from_json`].
//!
//! # Examples
//!
//! ```no_run
//! use clamor_core::{Dispatch, Sound, Toast};
//!
//! clamor_core::fire(&Dispatch {
//!     toast: Some(Toast {
//!         title: "Task complete".to_owned(),
//!         body: "Claude Code has finished responding.".to_owned(),
//!     }),
//!     sound: Sound::Native,
//! })?;
//! # Ok::<(), clamor_core::Error>(())
//! ```

mod audio;
mod dispatch;
mod input;
mod notify;

#[cfg(windows)]
mod windows;

pub use crate::dispatch::Dispatch;
pub use crate::dispatch::Sound;
pub use crate::dispatch::Toast;
pub use crate::dispatch::Volume;
pub use crate::dispatch::fire;
pub use crate::input::HookInput;
use camino::Utf8PathBuf;
use thiserror::Error;

/// The application name shown on the toast and used as the Windows AUMID
/// display label.
pub(crate) const APP_NAME: &str = "Claude Code";

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
