# keyboard-screen

Your Claude **5-hour** and **weekly** usage limits, live on the 128×128 screen of
an Epomaker × Aula F75 Max.

![the rendered frame](docs/frame.png)

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/OWNER/keyboard-screen/main/install.sh | sh
```

Grabs the latest release binary (or builds from source if there isn't one),
installs it to `~/.local/bin`, and adds the udev rule so the screen is writable
without root. Replug the keyboard afterwards.

## Use

```sh
keyboard-screen                # refresh once and push to the screen
keyboard-screen --watch 60     # keep it updated (60s minimum)
keyboard-screen --dry-run -o /tmp/f.ppm   # render without a keyboard attached
keyboard-screen --no-refresh   # draw from cache; spend nothing
```

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

## Build

```sh
cargo build --release
```

Dependencies are `libc` (raw `ioctl`) and `ureq` + `rustls` for the HTTPS ping.
The JSON parser, 5×7 font, drawing primitives and HID transport are all in-tree.
