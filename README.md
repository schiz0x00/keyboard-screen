# keyboard-screen

[![CI](https://github.com/schiz0x00/keyboard-screen/actions/workflows/ci.yml/badge.svg)](https://github.com/schiz0x00/keyboard-screen/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/schiz0x00/keyboard-screen?sort=semver)](https://github.com/schiz0x00/keyboard-screen/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Your Claude **5-hour** and **weekly** usage limits, live on the 128×128 screen of
an Epomaker × Aula F75 Max.

![the rendered frame](docs/frame.png)

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/schiz0x00/keyboard-screen/main/install.sh | sh
```

Grabs the latest release binary, verifies it against the release checksums (or
builds from source if there's no release for your platform), installs it to
`~/.local/bin`, and adds the udev rule so the screen is writable without root.
Replug the keyboard afterwards.

To override anything, put the variable after the pipe — before it, the shell
would set it on `curl` instead:

```sh
curl -fsSL .../install.sh | KEYBOARD_SCREEN_BIN_DIR=~/bin KEYBOARD_SCREEN_SKIP_UDEV=1 sh
```

## Use

```sh
keyboard-screen                # refresh once and push to the screen
keyboard-screen --watch        # keep it updated, default 60s interval
keyboard-screen --watch 300    # ...or a slower one (60s minimum)
keyboard-screen --dry-run -o /tmp/f.ppm   # render without a keyboard attached
keyboard-screen --no-refresh   # draw from cache; spend nothing
```

| Option | What it does |
| --- | --- |
| `-w, --watch [SECS]` | Keep refreshing. The interval is optional and defaults to 60s; anything below that is raised to 60s, because each refresh spends quota. |
| `-s, --slot N` | Display slot to write, 1–255 (default 1). |
| `-n, --no-refresh` | Never ping the API; draw from the cache or Claude Code's file. |
| `-o, --preview FILE` | Also write the frame as a binary PPM for inspection. |
| `--dry-run` | Render but don't touch the keyboard. |
| `--test-pattern` | Push an unmistakable test frame instead of the dashboard. |
| `-v, --verbose` | Log what it is doing, including why a refresh fired. |
| `-h, --help`, `-V, --version` | Help and version. |

(`--wireless`, `--probe-wireless` and `--probe-write` exist only for the
reverse-engineering write-up; the screen is wired-only. See
[`docs/wireless-re.md`](docs/wireless-re.md).)

In `--watch` mode the API is pinged on the interval, but the screen does not
wait for it: between pings the loop reads Claude Code's own
`~/.claude/usage-exact.json` every two seconds - a free local read - and pushes
a fresh frame within seconds of the numbers climbing, at most once every 15
seconds. When nothing is being spent, it stays on the plain interval.

Run it continuously:

```sh
install -Dm644 systemd/keyboard-screen.service ~/.config/systemd/user/
systemctl --user enable --now keyboard-screen
```

## How it gets the numbers

From the `anthropic-ratelimit-unified-*` headers of a ~9-token Haiku ping,
authenticated with the OAuth token Claude Code already stores in
`~/.claude/.credentials.json`.

**This spends subscription quota, never API credits.** The only credential it
will send is a Claude Code OAuth token (`sk-ant-oat…`); it never reads
`ANTHROPIC_API_KEY`, never sets an `x-api-key` header, and refuses anything it
doesn't recognise rather than guessing which account pays.

Two things worth knowing: the headers are **account-wide**, so the screen also
reflects usage from claude.ai or another machine — which is the point; and a ping
**opens a 5-hour window** if none is running, so watching around the clock keeps
one open.

Claude Code's own `usage-exact.json` is a last-resort fallback only — it
refreshes when Claude Code feels like it, not when you ask. The footer always
says which source the numbers came from and how old they are (`LIVE`,
`CACHED 4M`, `CC FILE 2H30M`), so a frozen screen can't pass for a live one.

## Notes

The screen is **wired-only**. Over 2.4GHz or Bluetooth the tool exits cleanly and
the screen keeps its last frame, correctly labelled as stale — so you can leave
the service enabled full time.

- [`docs/protocol.md`](docs/protocol.md) — the screen protocol
- [`docs/wireless-re.md`](docs/wireless-re.md) — why wireless can't work, with
  the decompiled proof
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to build, test and release

## Build

```sh
cargo build --release
```

Dependencies are `libc` (raw `ioctl`) and `ureq` + `rustls` for the HTTPS ping.
The JSON parser, 5×7 font, drawing primitives and HID transport are all in-tree.

## License

MIT — see [LICENSE](LICENSE).

The screen protocol was reverse-engineered by the
[Aula-F75-Max-Driver](https://github.com/VitalyArt/Aula-F75-Max-Driver) and
[Aula-F75-Max-OSX](https://github.com/RoseWaveStudio/Aula-F75-Max-OSX) projects;
this implementation confirms it independently against the vendor software.
