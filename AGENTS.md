# clamor

Cross-platform desktop notifications and audio for Claude Code hooks. One Rust
binary on `PATH`, so a single `settings.json` works unchanged on Windows,
macOS, and Linux.

## Layout

Cargo workspace, two root-level crates:

- `clamor-core` (lib): `input` parses the hook `message` off stdin; `dispatch`
  holds `Sound`/`Toast`/`Dispatch`/`fire`; `notify` shows the toast
  (`notify-rust` on Windows/Linux, `osascript` on macOS — see "macOS
  notifications"); `audio` plays a custom file (`rodio`); `windows` registers
  the AUMID.
- `clamor` (bin): clap flags build a `Dispatch` (an optional `Toast` plus a
  `Sound`) and call `fire`. Hook mode only.

## Model

There is no config file. Each `settings.json` hook entry runs `clamor` with
`--notify`/`--title`/`--body`/`--audio`/`--volume`, and Claude Code's hook
matchers do the routing (`Notification` matches `notification_type`,
`SubagentStop` matches agent type). A hook entry that exists notifies; one that
does not is silent.

The notification and the audio cue are independent channels. `--notify` shows
the toast (without it, `--title`/`--body` are ignored and no toast appears).
`--audio` is `native`, `none`, or a file path (repeat the flag for a random
pick). `native` is the toast's own system sound, so it is audible only with
`--notify` and is the default when `--notify` is set and `--audio` is omitted; a
custom file always shows any toast silently and plays on its own when there is
no toast. The body falls back to the hook `message` unless `--body` is given.

`--volume` is a single `0.0..=1.0` multiplier (`Volume`, clamped on construction
with non-finite falling back to `1.0`) carried onto `Sound::Files` and applied to
the picked file via `rodio`'s `Player::set_volume`. It is global to the `--audio`
array (not per file) and a no-op for `native`/`none`, where there is no rodio
playback to scale. Volume lives on the `Files` variant so it is unrepresentable
where it has no meaning, which is why `Sound`/`Dispatch` drop the `Eq` derive
(an `f32` is not `Eq`).

clamor expands a leading `~` and `$VAR`/`${VAR}` in `--audio` paths
(`dispatch::expand`, via `shellexpand`'s context variant), so one home-relative
path is portable across OSes. Wire it with the hook **exec form** (`args` array,
no shell) so the string reaches clamor unexpanded. The expansion is infallible
by construction (undefined variable left literal, non-UTF-8 home left literal),
so it adds no `Error` variant and the never-fail invariant is intact.

## macOS notifications

macOS does not go through `notify-rust`. Its default `notify-rust` backend is the
deprecated `NSUserNotification` API, which silently delivers nothing for an
unbundled CLI binary on modern macOS (the call still returns `Ok`, so the failure
is invisible — the symptom was a missing toast while custom audio still played).
Every non-deprecated macOS API (`UNUserNotificationCenter`) requires a signed
`.app` bundle, which clamor's "one binary on `PATH`" model deliberately avoids.

So `notify::show` is `cfg`-split: Windows/Linux keep `notify-rust`, while macOS
shells out to `osascript`'s `display notification` (which runs inside a system
app context and actually displays). `notify-rust` is therefore a
`cfg(not(target_os = "macos"))` dependency, and `Error::Notify` wraps a
`std::io::Error` there instead of a `notify_rust` error. Consequences:

- The toast is attributed to "Script Editor" (the bundle `osascript` borrows);
  `appname` cannot override it, so `APP_NAME` is unused on macOS.
- `NativeSound::Default` maps to a concrete system sound (`Ping`) because
  `display notification` has no token for the user's configured default alert.
- Title and body are passed as `argv` items to an `on run argv` script, never
  interpolated into the script source, so arbitrary text (including the hook
  `message`) cannot inject AppleScript.

## The invariant that matters

Hook mode never blocks the agent: it always exits 0 and never panics. `Stop` and
`SubagentStop` block Claude on a non-zero exit, so every failure is swallowed,
including dispatch errors, clap parse errors, and an unreadable stdin. Failures
go to stderr only when `CLAMOR_DEBUG` is set. Do not add a path that can exit
non-zero or panic in hook mode; that is the one rule the whole design protects.

## Conventions

Rust style and lints live in `.claude/rules/rust/`; GitHub Actions pinning in
`.claude/rules/github-actions/`. Formatting needs nightly rustfmt. Common tasks
run through the `justfile` (`just` lists recipes); the full build/lint/test gate
is:

```
just check
```

That runs `fmt-check` + `clippy` + `test`; `just fmt` reformats in place. The
raw `cargo` invocations behind each recipe are in the README.

Custom audio paths are machine-specific, so a shared `settings.json` should keep
to `native`/`none` and leave file paths to per-machine entries.
