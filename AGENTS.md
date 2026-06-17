# clamor

Cross-platform desktop notifications and audio for Claude Code hooks. One Rust
binary on `PATH`, so a single `settings.json` works unchanged on Windows,
macOS, and Linux.

## Layout

Cargo workspace, two root-level crates:

- `clamor-core` (lib): `input` parses the hook `message` off stdin; `dispatch`
  holds `Sound`/`Notification`/`fire`; `notify` shows the toast (`notify-rust`);
  `audio` plays a custom file (`rodio`); `windows` registers the AUMID.
- `clamor` (bin): clap flags build a `Notification` and call `fire`. Hook mode
  only.

## Model

There is no config file. Each `settings.json` hook entry runs `clamor` with
`--title`/`--body`/`--sound`, and Claude Code's hook matchers do the routing
(`Notification` matches `notification_type`, `SubagentStop` matches agent type).
A hook entry that exists notifies; one that does not is silent. `--sound` is
`native`, `none`, or a file path (repeat the flag for a random pick). The body
falls back to the hook `message` unless `--body` is given.

## The invariant that matters

Hook mode never blocks the agent: it always exits 0 and never panics. `Stop` and
`SubagentStop` block Claude on a non-zero exit, so every failure is swallowed,
including dispatch errors, clap parse errors, and an unreadable stdin. Failures
go to stderr only when `CLAMOR_DEBUG` is set. Do not add a path that can exit
non-zero or panic in hook mode; that is the one rule the whole design protects.

## Conventions

Rust style and lints live in `.claude/rules/rust/`; GitHub Actions pinning in
`.claude/rules/github-actions/`. Formatting needs nightly rustfmt. The
build/lint/test gate is in the README:

```
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Custom audio paths are machine-specific, so a shared `settings.json` should keep
to `native`/`none` and leave file paths to per-machine entries.
