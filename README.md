# clamor

Desktop notifications and sound for Claude Code hooks, on Windows, macOS, and
Linux.

Claude Code fires hook events (permission prompts, idle waits, finished tasks)
but never surfaces them with a toast or a sound. `clamor` does: register it as
your hook command, and each event becomes a desktop notification with either the
native system sound or an audio file you pick. It is a single binary that
resolves on `PATH` everywhere, so one `settings.json` works unchanged across all
three platforms.

## Install

```sh
cargo install --git https://github.com/kvnxiao/clamor --locked clamor
```

`--locked` pins the exact versions from the committed `Cargo.lock`.

## Setup

`clamor` has no config file. Each hook entry in your Claude Code `settings.json`
(usually `~/.claude/settings.json`) invokes `clamor` with flags that set the
title and sound for that event. An event you don't wire up simply makes no
sound; there's nothing to disable.

On Windows the toast app ID registers itself on the first notification, so there
is no separate setup step.

Add hook entries like this. Using the `args` array (rather than a single
`command` string) avoids shell-quoting differences, so the same `settings.json`
works unchanged on Windows, macOS, and Linux:

```json
{
  "hooks": {
    "Notification": [
      { "matcher": "permission_prompt",
        "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--title", "Permission needed", "--sound", "native"], "timeout": 10 }] },
      { "matcher": "idle_prompt",
        "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--title", "Waiting for you", "--sound", "native"], "timeout": 10 }] }
    ],
    "Stop": [
      { "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--title", "Task complete",
                   "--body", "Claude Code has finished responding.",
                   "--sound", "native"], "timeout": 10 }] }
    ],
    "SubagentStop": [
      { "matcher": "Explore",
        "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--title", "Subagent done", "--body", "Explore finished.",
                   "--sound", "none"], "timeout": 10 }] }
    ]
  }
}
```

Keep the timeout short. A custom audio file has to be a brief chime, or it gets
cut off when the hook times out.

Test it without waiting for a real event by running `clamor` directly:

```sh
clamor --title "Task complete" --sound native
echo '{"message":"Bash(npm test)"}' | clamor --title "Permission needed" --sound native
```

## Configuration

There is no configuration file. The notification is built entirely from flags
plus the hook payload on standard input:

| Flag | Meaning |
|---|---|
| `--title <STR>` | Toast summary line. Defaults to `Claude Code`. |
| `--body <STR>` | Toast body. Overrides the hook `message` from standard input. |
| `--sound <VAL>` | `native`, `none`, or a path to an audio file. Repeat for several files. Defaults to `native`. |

`--sound` is one of:

- `native`: the OS's default notification sound.
- `none`: silent.
- a file path (`--sound /path/to/chime.wav`): play your own WAV, OGG, MP3, or
  FLAC after a silent notification.
- several file paths (`--sound /a.wav --sound /b.wav`): pick one at random each
  time, then play it after a silent notification.

When `--body` is omitted, the body is the hook `message` (e.g. the permission
request text). `Stop`/`SubagentStop` carry no message, so give those a `--body`.

### Routing with matchers

Claude Code's hook `matcher` does the per-event routing `clamor` used to do
internally:

| Hook event | Matches on | Example matcher |
|---|---|---|
| `Notification` | notification type | `permission_prompt`, `idle_prompt`, `auth_success` |
| `SubagentStop` | agent type | `Explore`, `Plan`, custom agent names |
| `Stop` | (none — always fires) | |

So the same event can play different cues per matcher: a `permission_prompt`
toast and an `idle_prompt` toast are two `Notification` matcher groups with
different `--title`/`--sound`.

### Portability of custom audio

`native` and `none` are portable, so a `settings.json` shared across machines
(e.g. symlinked) keeps working everywhere. Custom audio file paths are
machine-specific; make them portable with Claude Code's
`${CLAUDE_PROJECT_DIR}` / `${CLAUDE_PLUGIN_ROOT}` placeholders, or accept that
they only resolve on the machine they point at.

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
