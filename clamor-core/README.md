# clamor-core

The library behind [`clamor`](https://github.com/kvnxiao/clamor): desktop
notifications and audio for Claude Code hooks.

It does the work: parsing the hook input (`input`), evaluating `--when` jq
conditions against the payload (`condition`, via embedded jaq), and deciding
what notification and sound to fire (`dispatch`). The actual command-line tool
lives in the `clamor` crate. See the [workspace README](../README.md) for usage.
