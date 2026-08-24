# Changelog

All notable changes are recorded here. Versions follow
[semantic versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-08-24

First release.

### Added

- Renders the Claude 5-hour and weekly subscription limits to the Aula F75 Max
  128×128 screen.
- Reads live usage from the `anthropic-ratelimit-unified-*` headers of a
  ~9-token Haiku ping, authenticated with the Claude Code OAuth token. Never
  touches API-key billing.
- Falls back through a local cache to Claude Code's `usage-exact.json`, and
  labels the source and age on screen so a stale frame cannot pass for a live one.
- `--watch` mode that survives suspend/resume by detecting wall-clock jumps.
- `--probe-wireless` and `--probe-write` for investigating the 2.4GHz dongle.
- `install.sh`, a udev rule, and a systemd user unit.

### Notes

- The screen is wired-only. `docs/wireless-re.md` documents why, with proof
  decompiled from the vendor software.
- Wireless RGB byte order is R, G, B at bytes 4, 5, 6 — both existing
  open-source drivers have red and blue swapped.

[Unreleased]: https://github.com/schiz0x00/keyboard-screen/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/schiz0x00/keyboard-screen/releases/tag/v0.1.0
