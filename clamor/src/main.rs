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
use clap::Parser;
use std::io::IsTerminal;
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
    /// only alongside a notification.
    #[arg(long)]
    audio: Vec<String>,
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
fn run(cli: Cli) {
    let toast = if cli.notify {
        let body = cli
            .body
            .unwrap_or_else(|| stdin_message().unwrap_or_default());
        Some(Toast {
            title: cli.title,
            body,
        })
    } else {
        None
    };
    let dispatch = Dispatch {
        toast,
        sound: resolve_sound(&cli.audio, cli.notify),
    };
    if let Err(error) = clamor_core::fire(&dispatch) {
        debug_log(&format!("failed to fire notification: {error}"));
    }
}

/// Resolves the `--audio` values into a [`Sound`]. With no values the default
/// depends on whether a toast is shown: a notification rides the native system
/// sound, while a toast-less dispatch stays silent (there is nothing for the
/// native sound to play on).
fn resolve_sound(audio: &[String], notify: bool) -> Sound {
    if audio.is_empty() && !notify {
        Sound::Silent
    } else {
        Sound::from_values(audio)
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

/// Reads the hook payload from standard input and returns its `message`, if
/// any. Returns `None` when stdin is a terminal (so interactive runs do not
/// block waiting for input) or when the payload cannot be read or parsed.
fn stdin_message() -> Option<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let raw = match std::io::read_to_string(stdin) {
        Ok(raw) => raw,
        Err(error) => {
            debug_log(&format!("failed to read stdin: {error}"));
            return None;
        }
    };
    match HookInput::from_json(&raw) {
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
        assert_eq!(resolve_sound(&[], true), Sound::Native);
    }

    #[test]
    fn empty_audio_without_notify_is_silent() {
        // No cue requested and no toast to carry the native sound: silent.
        assert_eq!(resolve_sound(&[], false), Sound::Silent);
    }

    #[test]
    fn explicit_keyword_ignores_notify_flag() {
        assert_eq!(resolve_sound(&["none".to_owned()], true), Sound::Silent);
        assert_eq!(resolve_sound(&["native".to_owned()], false), Sound::Native);
    }

    #[test]
    fn explicit_file_path_becomes_files() {
        assert!(matches!(
            resolve_sound(&["/tmp/chime.wav".to_owned()], false),
            Sound::Files(_)
        ));
    }
}
