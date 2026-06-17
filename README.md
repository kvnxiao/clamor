# clamor

Cross-platform desktop notifications and audio for [Claude
Code](https://code.claude.com) hooks.

Claude Code fires hook events (permission prompts, idle waiting, task and
subagent completion) but has no built-in desktop notification or sound.
`clamor` is a small Rust binary registered as the hook command: it reads the
hook JSON on stdin, looks up a per-event configuration, and fires a desktop
notification with either the native system sound or a user-supplied audio file.

Because Claude Code resolves `clamor` on `PATH` on every OS (`.exe` is
auto-appended on Windows), **a single `settings.json` works unchanged on
Windows, macOS, and Linux** — handy when the file is symlinked across machines.

## Install

```sh
cargo install --git https://github.com/kvnxiao/clamor --locked clamor
```

`--locked` installs the exact dependency versions from the committed
`Cargo.lock`.

## Setup

Run `clamor init` to scaffold a default config, print the `settings.json`
snippet to paste, and (on Windows) register the toast `AppUserModelID`:

```sh
clamor init
```

Then add the printed snippet to your Claude Code `settings.json`
(`~/.claude/settings.json`):

```json
{
  "hooks": {
    "Notification":  [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }],
    "Stop":          [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }],
    "SubagentStop":  [{ "hooks": [{ "type": "command", "command": "clamor", "timeout": 10 }] }]
  }
}
```

The short `timeout` matters: a custom audio file must be a short chime, or it
will be cut off when the hook times out.

Verify it works without waiting for a real Claude Code event:

```sh
clamor test stop
clamor test permission
```

## Configuration

Configuration is TOML. The location is resolved in this order, **first found
wins** (no merging):

1. `$CLAMOR_CONFIG` (explicit path)
2. `$CLAUDE_PROJECT_DIR/.clamor.toml` (per-project)
3. the user config directory, as resolved by the
   [`directories`](https://docs.rs/directories) crate:
   - Linux: `~/.config/clamor/config.toml`
   - macOS: `~/Library/Application Support/clamor/config.toml`
   - Windows: `%APPDATA%\clamor\config.toml`
4. built-in defaults (used when no file is found)

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

Every field is optional; omitted fields fall back to the built-in default for
that event. The `sound` value is one of:

- `"native"` — the platform's default notification sound (delivered through the
  notification itself).
- `"none"` — silent.
- `{ file = "/path/to/chime.wav" }` — play a custom audio file (WAV, OGG, MP3,
  or FLAC) after showing a silent notification.

### Events

| Hook event | `notification_type` | Config key | Default |
|---|---|---|---|
| `Notification` | `permission_prompt` | `permission` | enabled, native |
| `Notification` | `idle_prompt` | `idle` | enabled, native |
| `Stop` | — | `stop` | enabled, native |
| `SubagentStop` | — | `subagent_stop` | disabled, silent |
| `Notification` | other (`auth_success`, `elicitation_*`, …) | key of the same name | disabled |

## Behaviour

`Stop` and `SubagentStop` hooks can block Claude Code if they exit non-zero.
`clamor` in hook mode **always exits zero and never panics** — any error is
logged to stderr (only when `CLAMOR_DEBUG` is set) and swallowed, so the
notifier can never stall the agent loop.

## Development

This is a Cargo workspace with two crates: `clamor-core` (library) and `clamor`
(binary). Formatting requires nightly rustfmt; lint and test on stable:

```sh
rustup toolchain install nightly --component rustfmt
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

On Linux, building requires ALSA development headers
(`sudo apt-get install -y libasound2-dev`).

## License

MIT
