//! `clamor` command-line entry point: an argument-driven Claude Code hook.
//!
//! `clamor` is registered as a hook command in Claude Code's `settings.json`.
//! Its title and sound come from command-line flags on that hook entry; its
//! body comes from the hook `message` read from standard input (when piped).
//! It always exits zero and never panics, so it can never block the agent loop.

use clamor_core::HookInput;
use clamor_core::Notification;
use clamor_core::Sound;
use clap::Parser;
use std::io::IsTerminal;
use std::process::ExitCode;

/// Cross-platform desktop notifications and audio for Claude Code hooks.
#[derive(Debug, Parser)]
#[command(name = "clamor", version, about)]
struct Cli {
    /// Toast title (the summary line).
    #[arg(long, default_value = "Claude Code")]
    title: String,

    /// Toast body. Overrides the hook `message` read from standard input.
    #[arg(long)]
    body: Option<String>,

    /// Sound to play: `native`, `none`, or a path to an audio file. Repeat the
    /// flag to supply several files; one is chosen at random. Defaults to
    /// `native` when omitted.
    #[arg(long)]
    sound: Vec<String>,
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

/// Builds the notification from the flags plus the optional stdin `message` and
/// fires it. Any failure is logged (only when `CLAMOR_DEBUG` is set) and
/// swallowed.
fn run(cli: Cli) {
    let body = cli
        .body
        .unwrap_or_else(|| stdin_message().unwrap_or_default());
    let notification = Notification {
        title: cli.title,
        body,
        sound: Sound::from_values(&cli.sound),
    };
    if let Err(error) = clamor_core::fire(&notification) {
        debug_log(&format!("failed to fire notification: {error}"));
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
