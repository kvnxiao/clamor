# clamor

Desktop notifications and a sound when Claude Code needs you, on Windows,
macOS, and Linux.

Claude Code can signal that it wants attention (a permission prompt, going
idle, a finished task) through hook events, but it won't show a notification or
make a sound on its own. That's what `clamor` is for. You register it as the
hook command; it reads the hook JSON on stdin and shows a notification with
either the system sound or an audio file you point it at.

It's one binary, and Claude Code finds it on `PATH` (Windows adds the `.exe`),
so the same `settings.json` works on all three OSes. That helps if you symlink
that file across machines.

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
