# clamor

Desktop notifications and sound for Claude Code hooks, on Windows, macOS, and
Linux.

Claude Code emits hook events when it wants your attention, whether that is a
permission prompt, an idle wait, or a finished task, but it does not surface any
of them on its own; there is no built-in toast and no sound. `clamor` is what
fills that gap. You register it as the hook command, and on each event it reads
the hook payload from `stdin`, resolves the configuration you have set for that
event, and fires a desktop notification with either the native system sound or
an audio file of your choosing.

The reason this is a separate binary rather than a shell one-liner is
portability. A raw `osascript` or `PowerShell` command is bound to a single
platform, which becomes a problem the moment you keep one `settings.json`
symlinked across machines (which I do). Since Claude Code resolves `clamor` on
`PATH` on every operating system, appending the `.exe` for you on Windows, the
exact same hook configuration works unchanged everywhere; you install the binary
once per machine, and the configuration travels with it.

## Install

```sh
cargo install --git https://github.com/kvnxiao/clamor --locked clamor
```

`--locked` pins the exact versions from the committed `Cargo.lock`.

## Setup

`clamor init` writes a default config, prints the snippet to paste, and on
Windows registers the toast app ID:

```sh
clamor init
```

Add the printed snippet to your Claude Code `settings.json` (usually
`~/.claude/settings.json`):

```json
{
  "hooks": {
    "Notification":  [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }],
    "Stop":          [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }],
    "SubagentStop":  [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }]
  }
}
```

Keep the timeout short. A custom audio file has to be a brief chime, or it gets
cut off when the hook times out.

Test it without waiting for a real event:

```sh
clamor test stop
clamor test permission
```

## Configuration

Config is TOML. `clamor` uses the first of these it finds, with no merging:

1. `$CLAMOR_CONFIG`
2. `$CLAUDE_PROJECT_DIR/.clamor.toml`
3. the user config dir, via the [`directories`](https://docs.rs/directories)
   crate: `~/.config/clamor/config.toml` on Linux,
   `~/Library/Application Support/clamor/config.toml` on macOS,
   `%APPDATA%\clamor\config.toml` on Windows
4. built-in defaults, if there's no file at all

### Schema

```toml
[notifications]
enabled  = true            # master switch
app_name = "Claude Code"   # toast app name / Windows AUMID display label

[events.permission]
enabled = true
title   = "Permission needed"   # body defaults to the hook `message`
sound   = "native"              # "native" | "none" | { file = "/path/chime.wav" }

[events.idle]
enabled = true
title   = "Waiting for you"
sound   = "native"

[events.stop]
enabled = true
title   = "Task complete"
sound   = "native"

[events.subagent_stop]
enabled = false
title   = "Subagent done"
sound   = "none"
```

Leave out any field and it falls back to that event's default. `sound` is one
of:

- `"native"`: the OS's default notification sound.
- `"none"`: silent.
- `{ file = "/path/to/chime.wav" }`: play your own WAV, OGG, MP3, or FLAC after
  a silent notification.

### Events

| Hook event | `notification_type` | Config key | Default |
|---|---|---|---|
| `Notification` | `permission_prompt` | `permission` | on, native |
| `Notification` | `idle_prompt` | `idle` | on, native |
| `Stop` | | `stop` | on, native |
| `SubagentStop` | | `subagent_stop` | off, silent |
| `Notification` | anything else | the matching key | off |

## Reliability

The `Stop` and `SubagentStop` hooks block Claude Code if they exit non-zero, so
in hook mode `clamor` always exits 0 and never panics. If something fails it
goes to stderr (only when `CLAMOR_DEBUG` is set) and is otherwise ignored. The
notifier can't stall your session.

## Development

Two crates: `clamor-core` (the library) and `clamor` (the binary). Formatting
needs nightly rustfmt; everything else runs on stable.

```sh
rustup toolchain install nightly --component rustfmt
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Building on Linux needs ALSA headers: `sudo apt-get install -y libasound2-dev`.

## License

MIT
