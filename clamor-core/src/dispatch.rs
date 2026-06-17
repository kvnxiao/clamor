//! Builds and fires a dispatch: an optional notification message and an audio
//! cue, each controlled independently.
//!
//! There is no configuration file and no event routing here: the caller (the
//! `clamor` binary) derives the toast and sound from command-line flags and the
//! body from the hook `message`, then hands a fully-specified [`Dispatch`] to
//! [`fire`].

use crate::Result;
use crate::audio;
use crate::notify;
use crate::notify::NativeSound;
use crate::notify::NotificationSpec;
use camino::Utf8PathBuf;
use std::borrow::Cow;

/// The sound to play with a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sound {
    /// The platform's native notification sound.
    Native,
    /// No sound.
    Silent,
    /// Custom audio files. Any toast shown alongside is silent; one file,
    /// chosen at random, is played.
    Files(Vec<Utf8PathBuf>),
}

impl Sound {
    /// Interprets the raw `--audio` values into a [`Sound`].
    ///
    /// - no values: [`Sound::Native`] (audio unspecified rides on the toast)
    /// - a sole `"native"` / `"none"`: [`Sound::Native`] / [`Sound::Silent`]
    /// - anything else: every value is treated as a file path
    ///   ([`Sound::Files`]); the keywords are honored only when given alone.
    ///
    /// File-path values have a leading `~` expanded to the home directory and
    /// `$VAR`/`${VAR}` references expanded from the environment; an undefined
    /// variable is left as written. Keywords are matched first, so
    /// `native`/`none` are never expanded.
    ///
    /// # Arguments
    ///
    /// * `values` - the raw `--audio` flag values, in the order given
    ///
    /// # Examples
    ///
    /// ```
    /// use clamor_core::Sound;
    ///
    /// assert_eq!(Sound::from_values(&[]), Sound::Native);
    /// assert_eq!(Sound::from_values(&["none".to_owned()]), Sound::Silent);
    /// ```
    #[must_use]
    pub fn from_values(values: &[String]) -> Self {
        match values {
            [] => Sound::Native,
            [only] if only.as_str() == "native" => Sound::Native,
            [only] if only.as_str() == "none" => Sound::Silent,
            paths => Sound::Files(paths.iter().map(|p| expand(p)).collect()),
        }
    }
}

/// Expands a leading `~` and `$VAR`/`${VAR}` references in an `--audio` file
/// path against the real home directory and process environment.
///
/// Infallible by construction: a non-UTF-8 home or an undefined variable leaves
/// the corresponding token literal rather than erroring, so a custom-file
/// request is never silently turned into the native chime.
fn expand(input: &str) -> Utf8PathBuf {
    let expanded = expand_with_context(
        input,
        || std::env::home_dir().and_then(|p| p.to_str().map(str::to_owned)),
        |name: &str| std::env::var(name).ok(),
    );
    Utf8PathBuf::from(expanded.as_ref())
}

/// Pure core of [`expand`]: leading-`~` and `$VAR`/`${VAR}` expansion against
/// the supplied `home` and `context` closures, leaving undefined references
/// literal. Split out so the substitution logic is testable with fake closures,
/// without touching the process environment.
fn expand_with_context<HomeFn, HomeValue, ContextFn, ContextValue>(
    input: &str,
    home: HomeFn,
    context: ContextFn,
) -> Cow<'_, str>
where
    HomeFn: FnOnce() -> Option<HomeValue>,
    HomeValue: AsRef<str>,
    ContextFn: FnMut(&str) -> Option<ContextValue>,
    ContextValue: AsRef<str>,
{
    shellexpand::full_with_context_no_errors(input, home, context)
}

/// The notification message: a desktop toast's summary and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Toast summary line.
    pub title: String,
    /// Toast body.
    pub body: String,
}

/// A dispatch: an optional notification message and an audio cue, each
/// controlled independently.
///
/// The two channels are orthogonal: a dispatch may show a toast, play a sound,
/// or both. The one interaction is that [`Sound::Native`] is the toast's own
/// system sound, so it is audible only when [`Dispatch::toast`] is `Some`;
/// without a toast there is nothing for it to ride on and it plays nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatch {
    /// The notification to show, or `None` to show no toast.
    pub toast: Option<Toast>,
    /// The audio cue to play.
    pub sound: Sound,
}

/// Shows the notification (when present) and plays any custom audio.
///
/// The toast, if any, plays the native system sound only for [`Sound::Native`];
/// every other case shows a silent toast, with custom audio (if any) played
/// separately afterwards. Custom audio plays whether or not a toast is shown.
///
/// # Arguments
///
/// * `dispatch` - the toast to show (or `None`) and the audio cue to play
///
/// # Errors
///
/// Returns an error if showing the toast fails, or if a custom audio file
/// cannot be opened, decoded, or played. Callers in hook mode should swallow
/// the error and exit zero so the notifier never blocks the agent loop.
///
/// # Examples
///
/// ```no_run
/// use clamor_core::{Dispatch, Sound, Toast};
///
/// clamor_core::fire(&Dispatch {
///     toast: Some(Toast {
///         title: "Task complete".to_owned(),
///         body: "Claude Code has finished responding.".to_owned(),
///     }),
///     sound: Sound::Native,
/// })?;
/// # Ok::<(), clamor_core::Error>(())
/// ```
pub fn fire(dispatch: &Dispatch) -> Result<()> {
    if let Some(toast) = &dispatch.toast {
        let sound = match dispatch.sound {
            Sound::Native => NativeSound::Default,
            Sound::Silent | Sound::Files(_) => NativeSound::Silent,
        };
        notify::show(&NotificationSpec {
            title: toast.title.clone(),
            body: toast.body.clone(),
            sound,
        })?;
    }
    if let Sound::Files(files) = &dispatch.sound
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
    fn expand_expands_leading_tilde() {
        assert_eq!(
            expand_with_context("~/x.wav", || Some("/home/u"), |_| None::<&str>).as_ref(),
            "/home/u/x.wav"
        );
    }

    #[test]
    fn expand_leaves_non_leading_tilde_literal() {
        // `~` is only special at the very start; mid-path it is just a char.
        assert_eq!(
            expand_with_context("/a/~/b", || Some("/home/u"), |_| None::<&str>).as_ref(),
            "/a/~/b"
        );
    }

    #[test]
    fn expand_expands_env_var() {
        assert_eq!(
            expand_with_context(
                "$SND/x.wav",
                || None::<&str>,
                |name| (name == "SND").then_some("/a")
            )
            .as_ref(),
            "/a/x.wav"
        );
    }

    #[test]
    fn expand_expands_braced_env_var() {
        assert_eq!(
            expand_with_context(
                "${SND}x.wav",
                || None::<&str>,
                |name| (name == "SND").then_some("/a")
            )
            .as_ref(),
            "/ax.wav"
        );
    }

    #[test]
    fn expand_leaves_undefined_var_literal() {
        // An undefined variable stays as written so the open fails (and is
        // swallowed) rather than collapsing into the native chime.
        assert_eq!(
            expand_with_context("$NOPE/x.wav", || None::<&str>, |_| None::<&str>).as_ref(),
            "$NOPE/x.wav"
        );
    }

    #[test]
    fn expand_leaves_token_free_input_unchanged() {
        assert_eq!(
            expand_with_context("/tmp/chime.wav", || None::<&str>, |_| None::<&str>).as_ref(),
            "/tmp/chime.wav"
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
