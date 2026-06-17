# clamor

Cross-platform desktop notifications and audio for Claude Code hooks.

`clamor` is a single binary registered as the hook command for Claude Code's
`Notification`, `Stop`, and `SubagentStop` events. It reads the hook JSON on
stdin and fires a desktop notification with either the native system sound or a
user-supplied audio file. The same `settings.json` works unchanged on Windows,
macOS, and Linux.

See the [workspace README](../README.md) for installation and configuration.
