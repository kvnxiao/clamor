//! Builds and fires a notification from an explicit title, body, and sound.
//!
//! There is no configuration file and no event routing here: the caller (the
//! `clamor` binary) derives the title and sound from command-line flags and the
//! body from the hook `message`, then hands a fully-specified [`Notification`]
//! to [`fire`].

use crate::Result;
use crate::audio;
use crate::notify;
use crate::notify::NativeSound;
use crate::notify::NotificationSpec;
use camino::Utf8PathBuf;

/// The sound to play with a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sound {
    /// The platform's native notification sound.
    Native,
    /// No sound.
    Silent,
    /// Custom audio files. The notification is shown silently and one file,
    /// chosen at random, is played after it.
    Files(Vec<Utf8PathBuf>),
}

impl Sound {
    /// Interprets the raw `--sound` values into a [`Sound`].
    ///
    /// - no values: [`Sound::Native`] (a registered hook implies "notify me")
    /// - a sole `"native"` / `"none"`: [`Sound::Native`] / [`Sound::Silent`]
    /// - anything else: every value is treated as a file path
    ///   ([`Sound::Files`]); the keywords are honored only when given alone.
    #[must_use]
    pub fn from_values(values: &[String]) -> Self {
        match values {
            [] => Sound::Native,
            [only] if only.as_str() == "native" => Sound::Native,
            [only] if only.as_str() == "none" => Sound::Silent,
            paths => Sound::Files(paths.iter().map(Utf8PathBuf::from).collect()),
        }
    }
}

/// A fully-specified notification: what to show and how it should sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Toast summary line.
    pub title: String,
    /// Toast body.
    pub body: String,
    /// The sound to play.
    pub sound: Sound,
}

/// Shows the notification and plays any custom audio.
///
/// # Errors
///
/// Returns an error if showing the toast fails, or if a custom audio file
/// cannot be opened, decoded, or played. Callers in hook mode should swallow
/// the error and exit zero so the notifier never blocks the agent loop.
pub fn fire(notification: &Notification) -> Result<()> {
    // The native sound plays for `Sound::Native`; every other variant shows a
    // silent toast, with custom audio (if any) played separately afterwards.
    let sound = match notification.sound {
        Sound::Native => NativeSound::Default,
        Sound::Silent | Sound::Files(_) => NativeSound::Silent,
    };
    notify::show(&NotificationSpec {
        title: notification.title.clone(),
        body: notification.body.clone(),
        sound,
    })?;
    if let Sound::Files(files) = &notification.sound
        && let Some(path) = pick_audio(files)
    {
        audio::play_file(path)?;
    }
    Ok(())
}

/// Picks the custom audio file to play: `None` for an empty list, the sole
/// entry for one, or a uniformly random entry when several are configured.
fn pick_audio(candidates: &[Utf8PathBuf]) -> Option<&Utf8PathBuf> {
    fastrand::choice(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_values_defaults_to_native_when_empty() {
        assert_eq!(Sound::from_values(&[]), Sound::Native);
    }

    #[test]
    fn from_values_parses_keywords_alone() {
        assert_eq!(Sound::from_values(&["native".to_owned()]), Sound::Native);
        assert_eq!(Sound::from_values(&["none".to_owned()]), Sound::Silent);
    }

    #[test]
    fn from_values_parses_single_file() {
        assert_eq!(
            Sound::from_values(&["/tmp/chime.wav".to_owned()]),
            Sound::Files(vec![Utf8PathBuf::from("/tmp/chime.wav")])
        );
    }

    #[test]
    fn from_values_parses_multiple_files() {
        assert_eq!(
            Sound::from_values(&["/a.wav".to_owned(), "/b.wav".to_owned()]),
            Sound::Files(vec![
                Utf8PathBuf::from("/a.wav"),
                Utf8PathBuf::from("/b.wav")
            ])
        );
    }

    #[test]
    fn from_values_treats_keyword_with_files_as_paths() {
        // A keyword is only honored when it is the sole value; mixed with other
        // values it is just another (probably bogus) path candidate.
        assert_eq!(
            Sound::from_values(&["native".to_owned(), "/a.wav".to_owned()]),
            Sound::Files(vec![
                Utf8PathBuf::from("native"),
                Utf8PathBuf::from("/a.wav")
            ])
        );
    }

    #[test]
    fn pick_audio_handles_empty_single_and_multi() {
        assert_eq!(pick_audio(&[]), None, "empty list -> no audio");

        let single = [Utf8PathBuf::from("/only.wav")];
        assert_eq!(
            pick_audio(&single),
            single.first(),
            "single entry is always chosen"
        );

        let many = vec![
            Utf8PathBuf::from("/a.wav"),
            Utf8PathBuf::from("/b.wav"),
            Utf8PathBuf::from("/c.wav"),
        ];
        for _ in 0..50 {
            let picked = pick_audio(&many).expect("non-empty list yields a pick");
            assert!(many.contains(picked), "pick is one of the candidates");
        }
    }
}
