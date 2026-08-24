# Changelog

All notable changes are recorded here. Versions follow
[semantic versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-08-24

### Added

- `--watch` now refreshes the screen the moment usage climbs, instead of only on
  its interval. Between pings it reads Claude Code's `usage-exact.json` - a free
  local read - every two seconds, and pushes a new frame within seconds of the
  numbers rising, at most once every 15 seconds. The API ping still owns the
  interval, since each one spends quota, and a quiet stretch stays on the plain
  60s cadence.
- The README now documents every option, and both `--watch` forms: bare (60s
  default) and `--watch SECS`.

## [0.1.2] - 2026-08-24

### Fixed

- Release notes now describe the actual changes. The release job cloned
  shallowly, so it could not see previous tags and labelled every release
  "First release". Notes are now taken from this changelog, falling back to the
  commit log.

## [0.1.1] - 2026-08-24

### Fixed

- `install.sh` now verifies the download against the release `SHA256SUMS` and
  refuses to install on a mismatch.
- A failed `sudo` no longer aborts the installer. The binary is already useful
  without the udev rule, so the script warns, prints the exact command to run by
  hand, and finishes normally. This affected any non-interactive run, where sudo
  has no terminal to prompt on.

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

[Unreleased]: https://github.com/schiz0x00/keyboard-screen/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/schiz0x00/keyboard-screen/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/schiz0x00/keyboard-screen/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/schiz0x00/keyboard-screen/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/schiz0x00/keyboard-screen/releases/tag/v0.1.0
