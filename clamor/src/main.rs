//! `clamor` command-line entry point: an argument-driven Claude Code hook.
//!
//! `clamor` is registered as a hook command in Claude Code's `settings.json`.
//! The notification message and audio cue come from command-line flags on that
//! hook entry; the toast body falls back to the hook `message` read from
//! standard input (when piped). The two channels are independent: a hook can
//! show a notification, play an audio cue, or both. It always exits zero and
//! never panics, so it can never block the agent loop.

use clamor_core::Dispatch;
use clamor_core::HookInput;
use clamor_core::Sound;
use clamor_core::Toast;
use clamor_core::Volume;
use clamor_core::condition;
use clamor_core::condition::Verdict;
use clap::Parser;
use std::io::IsTerminal;
use std::io::Read;
use std::process::ExitCode;

/// Cross-platform desktop notifications and audio for Claude Code hooks.
#[derive(Debug, Parser)]
#[command(name = "clamor", version, about)]
struct Cli {
    /// Show a desktop notification (toast). Without this flag no toast is
    /// shown and `--title`/`--body` are ignored.
    #[arg(long)]
    notify: bool,

    /// Toast title (the summary line). Used only with `--notify`.
    #[arg(long, default_value = "Claude Code")]
    title: String,

    /// Toast body. Overrides the hook `message` read from standard input. Used
    /// only with `--notify`.
    #[arg(long)]
    body: Option<String>,

    /// Audio cue: `native`, `none`, or a path to an audio file. Repeat the flag
    /// to supply several files; one is chosen at random. With `--notify` and no
    /// `--audio`, the toast plays the native system sound; `native` is audible
    /// only alongside a notification. In a file path a leading `~` and
    /// `$VAR`/`${VAR}` references are expanded by clamor; an undefined variable
    /// is left as written.
    #[arg(long)]
    audio: Vec<String>,

    /// Playback volume for a custom `--audio` file, as a linear multiplier
    /// clamped to `0.0..=1.0`: `1.0` is the file's normal level and `0.0` is
    /// silent. Has no effect on `native`/`none` audio.
    #[arg(long, default_value_t = 1.0)]
    volume: f32,

    /// Fire only if the jq FILTER is truthy against the hook payload on stdin.
    /// Repeatable: every `--when` must pass (logical AND). A filter that
    /// evaluates to false gates the cue silently; a filter that cannot be
    /// evaluated (typo, runtime error, no payload) instead raises a default
    /// error notification so the breakage is never silent. Set `CLAMOR_DEBUG`
    /// to see the reason.
    #[arg(long)]
    when: Vec<String>,
}

fn main() -> ExitCode {
    match Cli::try_parse() {
        Ok(cli) => run(cli),
        Err(error) => handle_parse_error(&error),
    }
    // Always exit zero: a `Stop`/`SubagentStop` hook that exits non-zero blocks
    // Claude Code, so even a malformed `settings.json` flag must not fail hard.
    ExitCode::SUCCESS
}

/// Builds the dispatch from the flags plus the optional stdin `message` and
/// fires it. Any failure is logged (only when `CLAMOR_DEBUG` is set) and
/// swallowed.
///
/// The hook payload is read from standard input exactly once, up front: the
/// `--when` gate parses it as JSON for the jq filters, and the toast body
/// parses the same buffer as a typed [`HookInput`] for its `message` fallback.
fn run(cli: Cli) {
    let raw = stdin_bytes();

    // Gate before building anything: a suppressed cue should do no work, and the
    // error path overrides the configured cue entirely.
    if !cli.when.is_empty() {
        match decide(&cli.when, raw.as_deref()) {
            Gate::Fire => {}
            Gate::Suppress => return,
            Gate::Error(reason) => {
                debug_log(&reason);
                fire_error_toast(&reason);
                return;
            }
        }
    }

    let toast = if cli.notify {
        let body = cli
            .body
            .unwrap_or_else(|| message_from(raw.as_deref()).unwrap_or_default());
        Some(Toast {
            title: cli.title,
            body,
        })
    } else {
        None
    };
    let dispatch = Dispatch {
        toast,
        sound: resolve_sound(&cli.audio, cli.notify, Volume::new(cli.volume)),
    };
    if let Err(error) = clamor_core::fire(&dispatch) {
        debug_log(&format!("failed to fire notification: {error}"));
    }
}

/// What the `--when` filters collectively decide for the configured cue.
#[derive(Debug)]
enum Gate {
    /// Every filter passed: fire the configured dispatch.
    Fire,
    /// At least one filter cleanly evaluated to false (and none was
    /// unevaluable): suppress the cue *silently*.
    Suppress,
    /// At least one filter could not be evaluated: suppress the configured cue
    /// and raise the fallback error notification carrying this reason.
    Error(String),
}

/// Combines every `--when` filter with logical AND, resolving the tri-state
/// precedence: any unevaluable filter wins as [`Gate::Error`] (a broken filter
/// must surface even past a sibling's clean false); otherwise any clean false
/// is a silent [`Gate::Suppress`]; all-pass is [`Gate::Fire`]. A missing
/// payload is itself an error, since nothing could be evaluated against it.
fn decide(filters: &[String], payload: Option<&[u8]>) -> Gate {
    let Some(payload) = payload else {
        return Gate::Error("no payload on stdin to evaluate --when against".to_owned());
    };
    let mut any_fail = false;
    for filter in filters {
        match condition::evaluate(filter, payload) {
            Verdict::Pass => {}
            Verdict::Fail => any_fail = true,
            Verdict::Unevaluable(reason) => return Gate::Error(reason),
        }
    }
    if any_fail { Gate::Suppress } else { Gate::Fire }
}

/// Fires the fallback notification when a `--when` filter could not be
/// evaluated. A broken gate must never be silent, so this deliberately ignores
/// `--notify`/`--audio`/`--volume`: it always shows a default toast with the
/// native system sound. It reuses [`clamor_core::fire`], so its own failures
/// are swallowed and the never-block invariant holds.
fn fire_error_toast(reason: &str) {
    let dispatch = Dispatch {
        toast: Some(Toast {
            title: "clamor: --when could not be evaluated".to_owned(),
            body: reason.to_owned(),
        }),
        sound: Sound::Native,
    };
    if let Err(error) = clamor_core::fire(&dispatch) {
        debug_log(&format!("failed to fire error notification: {error}"));
    }
}

/// Resolves the `--audio` values into a [`Sound`], carrying `volume` onto a
/// custom-file cue. With no values the default depends on whether a toast is
/// shown: a notification rides the native system sound, while a toast-less
/// dispatch stays silent (there is nothing for the native sound to play on).
fn resolve_sound(audio: &[String], notify: bool, volume: Volume) -> Sound {
    if audio.is_empty() && !notify {
        Sound::Silent
    } else {
        Sound::from_values(audio, volume)
    }
}

/// Handles a clap parse failure without ever exiting non-zero: `--help` and
/// `--version` are printed normally; a genuine error is logged (only when
/// `CLAMOR_DEBUG` is set) and otherwise swallowed.
fn handle_parse_error(error: &clap::Error) {
    use clap::error::ErrorKind;
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        if let Err(print_error) = error.print() {
            debug_log(&format!("failed to print help/version: {print_error}"));
        }
    } else {
        debug_log(&format!("failed to parse arguments: {error}"));
    }
}

/// Reads the hook payload from standard input as raw bytes. Returns `None` when
/// stdin is a terminal (so interactive runs do not block waiting for input) or
/// when the payload cannot be read. The raw bytes feed both the `--when` jq
/// gate and the typed [`HookInput`] message fallback, so stdin is consumed only
/// once.
fn stdin_bytes() -> Option<Vec<u8>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut buf = Vec::new();
    match stdin.read_to_end(&mut buf) {
        Ok(_) => Some(buf),
        Err(error) => {
            debug_log(&format!("failed to read stdin: {error}"));
            None
        }
    }
}

/// Extracts the hook `message` from an already-read payload buffer, if present.
/// Returns `None` when there is no payload, the bytes are not UTF-8, or the
/// JSON cannot be parsed; a parse failure is logged under `CLAMOR_DEBUG`.
fn message_from(payload: Option<&[u8]>) -> Option<String> {
    let text = std::str::from_utf8(payload?).ok()?;
    match HookInput::from_json(text) {
        Ok(input) => input.message,
        Err(error) => {
            debug_log(&format!("failed to parse hook input: {error}"));
            None
        }
    }
}

/// Logs a diagnostic to stderr, but only when `CLAMOR_DEBUG` is set.
fn debug_log(message: &str) {
    if std::env::var_os("CLAMOR_DEBUG").is_some() {
        eprintln!("clamor: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_audio_with_notify_rides_native_sound() {
        assert_eq!(resolve_sound(&[], true, Volume::default()), Sound::Native);
    }

    #[test]
    fn empty_audio_without_notify_is_silent() {
        // No cue requested and no toast to carry the native sound: silent.
        assert_eq!(resolve_sound(&[], false, Volume::default()), Sound::Silent);
    }

    #[test]
    fn explicit_keyword_ignores_notify_flag() {
        assert_eq!(
            resolve_sound(&["none".to_owned()], true, Volume::default()),
            Sound::Silent
        );
        assert_eq!(
            resolve_sound(&["native".to_owned()], false, Volume::default()),
            Sound::Native
        );
    }

    #[test]
    fn explicit_file_path_becomes_files_with_volume() {
        // resolve_sound must forward the volume onto the custom-file cue; the
        // path-parsing itself is covered by clamor-core's from_values tests.
        assert!(matches!(
            resolve_sound(&["/tmp/chime.wav".to_owned()], false, Volume::new(0.3)),
            Sound::Files { volume, .. } if volume == Volume::new(0.3)
        ));
    }

    /// One filter, owned, for the table below.
    fn one(filter: &str) -> Vec<String> {
        vec![filter.to_owned()]
    }

    #[test]
    fn decide_all_pass_fires() {
        let payload = br#"{"background_tasks":[]}"#;
        assert!(matches!(
            decide(&one(".background_tasks | length == 0"), Some(payload)),
            Gate::Fire
        ));
    }

    #[test]
    fn decide_clean_false_suppresses() {
        let payload = br#"{"background_tasks":[{"status":"running"}]}"#;
        assert!(matches!(
            decide(&one(".background_tasks | length == 0"), Some(payload)),
            Gate::Suppress
        ));
    }

    #[test]
    fn decide_multiple_filters_are_anded() {
        // Both pass -> Fire; flip either to false -> Suppress.
        let payload = br#"{"a":true,"b":true}"#;
        let both = vec![".a".to_owned(), ".b".to_owned()];
        assert!(matches!(decide(&both, Some(payload)), Gate::Fire));

        let payload = br#"{"a":true,"b":false}"#;
        assert!(matches!(decide(&both, Some(payload)), Gate::Suppress));
    }

    #[test]
    fn decide_unevaluable_wins_over_clean_false() {
        // A broken filter must surface even when a sibling cleanly gates: the
        // clean false comes first, the typo second, yet Error wins.
        let payload = br#"{"a":false}"#;
        let filters = vec![".a".to_owned(), "this is | not valid".to_owned()];
        assert!(matches!(decide(&filters, Some(payload)), Gate::Error(_)));
    }

    #[test]
    fn decide_missing_payload_is_error() {
        // `--when` with no stdin (e.g. a terminal) cannot be evaluated, so it is
        // a loud Error rather than a silent pass: the fallback toast fires.
        assert!(matches!(
            decide(&one(".background_tasks | length == 0"), None),
            Gate::Error(_)
        ));
    }

    #[test]
    fn message_from_extracts_message_else_none() {
        assert_eq!(
            message_from(Some(br#"{"message":"Bash(npm test)"}"#)),
            Some("Bash(npm test)".to_owned())
        );
        assert_eq!(message_from(Some(br"{}")), None, "no message field");
        assert_eq!(message_from(None), None, "no payload at all");
        assert_eq!(message_from(Some(b"not json")), None, "unparseable payload");
    }
}
