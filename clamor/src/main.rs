//! `clamor` command-line entry point.
//!
//! With no subcommand, `clamor` runs in hook mode: it reads a Claude Code hook
//! payload from standard input and fires the configured notification. Hook
//! mode always exits zero and never panics, so it can never block the agent
//! loop. The `init` and `test` subcommands are interactive helpers.

#[cfg(windows)]
use clamor_core::Config;
use clamor_core::HookInput;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use std::process::ExitCode;

/// The `settings.json` hook snippet printed by `clamor init`. Identical on
/// every OS because Claude Code resolves `clamor` on `PATH`.
const SETTINGS_SNIPPET: &str = r#"{
  "hooks": {
    "Notification":  [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }],
    "Stop":          [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }],
    "SubagentStop":  [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }]
  }
}"#;

/// Cross-platform desktop notifications and audio for Claude Code hooks.
#[derive(Debug, Parser)]
#[command(name = "clamor", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// The `clamor` subcommands. Absent in hook mode.
#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold a default config (if absent) and print the settings.json hook
    /// snippet. On Windows, also registers the `AppUserModelID`.
    Init,
    /// Fire a synthetic notification for an event to verify toast and sound,
    /// without triggering a real Claude Code event.
    Test {
        /// Which event to simulate.
        #[arg(value_enum)]
        event: TestEvent,
    },
}

/// The events that `clamor test` can simulate.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum TestEvent {
    /// A permission prompt notification.
    Permission,
    /// An idle waiting notification.
    Idle,
    /// Task completion.
    Stop,
    /// Subagent completion.
    SubagentStop,
}

impl TestEvent {
    /// Builds a representative hook payload for this event.
    fn synthesize(self) -> HookInput {
        let owned = |value: &str| Some(value.to_owned());
        match self {
            TestEvent::Permission => HookInput {
                hook_event_name: "Notification".to_owned(),
                notification_type: owned("permission_prompt"),
                message: owned("clamor test: permission prompt"),
                agent_type: None,
            },
            TestEvent::Idle => HookInput {
                hook_event_name: "Notification".to_owned(),
                notification_type: owned("idle_prompt"),
                message: owned("clamor test: idle"),
                agent_type: None,
            },
            TestEvent::Stop => HookInput {
                hook_event_name: "Stop".to_owned(),
                notification_type: None,
                message: None,
                agent_type: None,
            },
            TestEvent::SubagentStop => HookInput {
                hook_event_name: "SubagentStop".to_owned(),
                notification_type: None,
                message: None,
                agent_type: owned("Explore"),
            },
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        None => {
            run_hook();
            ExitCode::SUCCESS
        }
        Some(Command::Init) => run_init(),
        Some(Command::Test { event }) => run_test(event),
    }
}

/// Hook mode: read stdin, parse, dispatch. Never returns an error; any failure
/// is logged (only when `CLAMOR_DEBUG` is set) and swallowed so the agent loop
/// is never blocked.
fn run_hook() {
    let raw = match std::io::read_to_string(std::io::stdin()) {
        Ok(raw) => raw,
        Err(error) => {
            debug_log(&format!("failed to read stdin: {error}"));
            return;
        }
    };
    let input = match HookInput::from_json(&raw) {
        Ok(input) => input,
        Err(error) => {
            debug_log(&format!("failed to parse hook input: {error}"));
            return;
        }
    };
    if let Err(error) = clamor_core::dispatch(&input) {
        debug_log(&format!("dispatch failed: {error}"));
    }
}

/// `clamor init`: scaffold config, register the Windows AUMID, print snippet.
fn run_init() -> ExitCode {
    if let Err(error) = init() {
        eprintln!("clamor init failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// `clamor test <event>`: synthesize a hook payload and dispatch it.
fn run_test(event: TestEvent) -> ExitCode {
    match clamor_core::dispatch(&event.synthesize()) {
        Ok(()) => {
            println!("Fired '{event:?}' test notification (subject to your config).");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("clamor test failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Scaffolds the config, registers the AUMID on Windows, and prints the hook
/// snippet.
fn init() -> clamor_core::Result<()> {
    scaffold_config()?;
    #[cfg(windows)]
    register_windows()?;
    println!("\nAdd the following to your Claude Code settings.json:\n");
    println!("{SETTINGS_SNIPPET}");
    Ok(())
}

/// Writes the default config to the user config path if it does not exist.
fn scaffold_config() -> clamor_core::Result<()> {
    let Some(path) = clamor_core::user_config_path() else {
        eprintln!("Could not determine a user config directory; skipping config scaffold.");
        return Ok(());
    };
    if path.exists() {
        println!("Config already exists at {path}");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    fs_err::write(&path, clamor_core::DEFAULT_CONFIG_TOML)?;
    println!("Wrote default config to {path}");
    Ok(())
}

/// Registers the Windows `AppUserModelID` using the configured app name.
#[cfg(windows)]
fn register_windows() -> clamor_core::Result<()> {
    let config = Config::load()?;
    clamor_core::register_app_id(&config.notifications.app_name)?;
    println!(
        "Registered Windows AppUserModelID '{}' as '{}'.",
        clamor_core::WINDOWS_APP_ID,
        config.notifications.app_name
    );
    Ok(())
}

/// Logs a diagnostic to stderr, but only when `CLAMOR_DEBUG` is set.
fn debug_log(message: &str) {
    if std::env::var_os("CLAMOR_DEBUG").is_some() {
        eprintln!("clamor: {message}");
    }
}
