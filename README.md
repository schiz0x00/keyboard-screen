# keyboard-screen

Puts the Claude **5-hour** and **weekly** subscription limits on the 128x128
screen of an Epomaker x Aula F75 Max.

![the rendered frame](docs/frame.png)

## How it gets the numbers

The figures come off the `anthropic-ratelimit-unified-*` response headers of a
tiny `/v1/messages` ping — about 9 tokens of Haiku 4.5 — authenticated with the
OAuth token Claude Code already stores in `~/.claude/.credentials.json`.

**This spends subscription quota, never API credits.** The only credential the
program will send is a Claude Code OAuth access token (`sk-ant-oat…`); it never
reads `ANTHROPIC_API_KEY`, never sets an `x-api-key` header, and refuses to send
anything it does not recognise rather than guessing which account pays. It
refreshes the OAuth token when it expires and writes it back atomically at
`0600`, preserving every other key in the file.

Two consequences worth knowing:

* The headers are **account-wide**, so the screen also reflects usage from
  claude.ai or another machine — which is the point.
* A ping **opens a 5-hour window** if none is running, so watching around the
  clock keeps one open.

Claude Code's own `~/.claude/usage-exact.json` is only used as a last-resort
fallback: it is refreshed when Claude Code feels like it, not when you ask, so
it goes stale. The footer always says which source the numbers came from and how
old they are — `LIVE`, `CACHED 4M`, or `CC FILE 2H30M` — so a frozen screen
cannot pass for a live one.

## Build

```sh
cargo build --release
install -Dm755 target/release/keyboard-screen ~/.local/bin/keyboard-screen
```

## Permissions

```sh
sudo cp udev/99-aula-f75max.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

Replug the keyboard afterwards.

## Use

```sh
keyboard-screen                  # refresh once and push to the screen
keyboard-screen --watch 60       # keep it updated (60s minimum)
keyboard-screen --dry-run -o /tmp/f.ppm   # render without a keyboard attached
keyboard-screen --no-refresh     # draw from cache; spend nothing
keyboard-screen --slot 2         # write a different display slot (1-255)
```

Run it continuously with the bundled unit:

```sh
install -Dm644 systemd/keyboard-screen.service ~/.config/systemd/user/
systemctl --user enable --now keyboard-screen
```

## The screen protocol

The keyboard (`0c45:800a`, SONiX) exposes two vendor HID interfaces. The screen
is **wired-only** — over the 2.4GHz dongle the usage pages differ.

| Usage page | Report | Role |
|---|---|---|
| `0xff13` | 64-byte feature | command channel |
| `0xff68` | 4096-byte output | raw pixel channel |

Upload sequence:

1. `04 18` on the command channel — begin.
2. `04 72` with the slot at byte 2 and the chunk count as a little-endian
   `u16` at bytes 8–9.
3. The payload as 4096-byte output reports, each followed by a 128-byte ACK.
4. `04 02` — commit. **The screen does not change until this arrives.**

The payload is a 256-byte header (frame count, then one delay byte per frame)
followed by `frames × 128 × 128 × 2` bytes of **little-endian RGB565**, zero
padded to a multiple of 4096. A still image is one frame — 9 chunks.

Protocol credit: reverse-engineered by the
[Aula-F75-Max-Driver](https://github.com/VitalyArt/Aula-F75-Max-Driver) and
[Aula-F75-Max-OSX](https://github.com/RoseWaveStudio/Aula-F75-Max-OSX) projects.

## Layout

Dependencies are `libc` (raw `ioctl`/`getrandom`) and `ureq`+`rustls` for the
HTTPS ping. The JSON parser, the 5×7 font, the drawing primitives and the HID
transport are all in-tree.

| File | Role |
|---|---|
| `src/hid.rs` | hidraw discovery and raw feature/output reports |
| `src/display.rs` | the upload sequence and payload encoding |
| `src/canvas.rs` | 128×128 canvas, drawing, RGB565 packing |
| `src/font.rs` | 5×7 bitmap font |
| `src/render.rs` | dashboard layout |
| `src/api.rs` | OAuth token handling and the usage ping |
| `src/state.rs` | snapshot provenance and the fallback chain |
| `src/json.rs` | minimal JSON parser/serializer |

## Wireless

**The screen is wired-only, and this is settled.** Decompiling the vendor
software shows it enumerates HID interfaces by report length and assigns its
bulk pixel handle *only* to an interface reporting 4097 bytes (4096 + report
ID). The 2.4GHz dongle exposes only 33-byte pipes, so it never receives that
handle and the vendor's own image upload is a no-op in 2.4G mode. There is no
wireless display opcode to find, because there is no wireless bulk endpoint.

Bluetooth is excluded by the vendor too: *"keyboard settings are not supported
in Bluetooth mode for now."*

Full evidence, including the decompiled classifier and the confirmed wired
command sequence, is in `docs/wireless-re.md`.

The wireless channel does work for *settings*. `--probe-write` drives the RGB
over 2.4GHz with documented opcodes, and established that the byte order is
**R, G, B at bytes 4, 5, 6** — the existing open-source drivers swap red and
blue.

`--wireless` and its `--variant` framings are kept only as a record of what was
tried; none of them work, and none of them can.
