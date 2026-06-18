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
(usually `~/.claude/settings.json`) invokes `clamor` with flags that decide,
per event, whether it shows a notification, plays an audio cue, or both. The two
channels are independent. An event you don't wire up simply does nothing;
there's nothing to disable.

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
          "args": ["--notify", "--title", "Permission needed"], "timeout": 10 }] },
      { "matcher": "idle_prompt",
        "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--notify", "--title", "Waiting for you"], "timeout": 10 }] }
    ],
    "Stop": [
      { "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--notify", "--title", "Task complete",
                   "--body", "Claude Code has finished responding."], "timeout": 10 }] }
    ],
    "SubagentStop": [
      { "matcher": "Explore",
        "hooks": [{ "type": "command", "command": "clamor",
          "args": ["--notify", "--title", "Subagent done", "--body", "Explore finished.",
                   "--audio", "none"], "timeout": 10 }] }
    ]
  }
}
```

A bare `--notify` plays the native system sound; pass `--audio none` for a
silent toast, or `--audio <path>` for a custom cue. Drop `--notify` entirely to
play an audio cue with no toast at all.

Keep the timeout short. A custom audio file has to be a brief chime, or it gets
cut off when the hook times out.

Test it without waiting for a real event by running `clamor` directly:

```sh
clamor --notify --title "Task complete"
echo '{"message":"Bash(npm test)"}' | clamor --notify --title "Permission needed"
clamor --audio /path/to/chime.wav   # audio cue only, no toast
```

## Configuration

There is no configuration file. The dispatch is built entirely from flags plus
the hook payload on standard input. The notification and the audio cue are
independent:

| Flag | Meaning |
|---|---|
| `--notify` | Show a desktop notification (toast). Without it, no toast; `--title`/`--body` are ignored. |
| `--title <STR>` | Toast summary line. Defaults to `Claude Code`. Used only with `--notify`. |
| `--body <STR>` | Toast body. Overrides the hook `message` from standard input. Used only with `--notify`. |
| `--audio <VAL>` | `native`, `none`, or a path to an audio file. Repeat for several files. |
| `--volume <MULT>` | Volume for a custom `--audio` file, a `0.0..=1.0` multiplier. Defaults to `1.0`. No effect on `native`/`none`. |

`--audio` is one of:

- `native`: the OS's default notification sound. It rides on the toast, so it is
  audible only with `--notify`, and it is the default when `--notify` is set and
  `--audio` is omitted.
- `none`: silent.
- a file path (`--audio /path/to/chime.wav`): play your own WAV, OGG, MP3, or
  FLAC. Any toast shown alongside is silent. A leading `~` and `$VAR`/`${VAR}`
  references are expanded by clamor (see [below](#portability-of-custom-audio));
  an undefined variable is left as written, so the file just fails to open.
- several file paths (`--audio /a.wav --audio /b.wav`): pick one at random each
  time.

`--volume` scales a custom file's playback level by a `0.0..=1.0` multiplier
(`1.0` is the file's normal level, `0.0` silent); values outside the range are
clamped. It applies to whichever file the random pick lands on and has no effect
on `native`/`none` (the system chime's volume is the OS's to control).

The result is four combinations:

| Want | Flags |
|---|---|
| Notification with the native system sound | `--notify` |
| Notification, silent | `--notify --audio none` |
| Notification plus a custom audio cue | `--notify --audio /path/to/chime.wav` |
| Audio cue only, no toast | `--audio /path/to/chime.wav` |

When `--body` is omitted, the body is the hook `message` (e.g. the permission
request text). `Stop`/`SubagentStop` carry no message, so give those a `--body`.

### Routing with matchers

Claude Code's hook `matcher` does the per-event routing `clamor` used to do
internally:

| Hook event | Matches on | Example matcher |
|---|---|---|
| `Notification` | notification type | `permission_prompt`, `idle_prompt`, `auth_success` |
| `SubagentStop` | agent type | `Explore`, `Plan`, custom agent names |
| `Stop` | (none, always fires) | |

So the same event can play different cues per matcher: a `permission_prompt`
toast and an `idle_prompt` toast are two `Notification` matcher groups with
different `--title`/`--audio`.

### Portability of custom audio

`native` and `none` are portable, so a `settings.json` shared across machines
(e.g. symlinked) keeps working everywhere. Custom audio file paths are
machine-specific, but a home-relative one is the exception: clamor expands a
leading `~` and `$VAR`/`${VAR}` references in `--audio` itself, the same way on
Windows, macOS, and Linux. An undefined variable is left as written (the file
then just fails to open). So `~`/`$HOME` work alongside Claude Code's
`${CLAUDE_PROJECT_DIR}` / `${CLAUDE_PLUGIN_ROOT}` placeholders for keeping one
`settings.json` portable.

For clamor to do the expanding, the string has to reach it unexpanded. Use the
exec form: a hook entry with an `args` array is spawned directly with no shell,
so `~`/`$VAR` pass through verbatim:

```json
{
  "type": "command",
  "command": "clamor",
  "args": ["--notify", "--audio", "~/sounds/notify.wav"]
}
```

clamor then expands the path itself, so one entry works on all three platforms.

Avoid the shell form (a single `command` string, no `args`) for paths that need
expanding: it routes through a per-OS shell (`sh -c` on macOS/Linux, PowerShell
on Windows when Git Bash is absent) that may pre-expand the string before clamor
sees it, and `~`/`$VAR`/`%VAR%` quoting has no single portable spelling across
those shells.

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
