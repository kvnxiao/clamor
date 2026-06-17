# clamor

Desktop notifications and audio for Claude Code hooks.

`clamor` is one binary you register as the hook command for Claude Code's
`Notification`, `Stop`, and `SubagentStop` events. It reads the hook JSON on
stdin and shows a notification with the system sound or an audio file you
choose. The same `settings.json` works on Windows, macOS, and Linux.

See the [workspace README](../README.md) for install and config.
