# Brief: finding the wireless display opcode

## The one open question

The keyboard's 128x128 screen can be driven over USB. It cannot yet be driven
over the 2.4GHz dongle, because we do not know the opcode. **Everything else is
solved.**

## Established on hardware (do not re-test)

* The wired path works: commands `04 18` / `04 72` / `04 02` on usage page
  `0xff13` (64B feature reports), pixels as 4096-byte output reports on
  `0xff68`, RGB565 little-endian, 256-byte header, commit required.
* **Wireless writes reach the firmware.** Driving RGB over `0xff60` with the
  documented packets visibly changes the lighting.
* **The wireless envelope is therefore correct**: 32 bytes, `aa 55` marker at
  offset 17, checksum (sum of all bytes & 0xff) at byte 31.
* Known wireless opcodes: `05` RGB, `07` key settings, `0f` commit, `20` battery.
* Wireless RGB byte order is **R, G, B at bytes 4, 5, 6** — the existing
  open-source drivers have red and blue swapped.
* Bluetooth is a dead end; the vendor software refuses it outright.

## Framings already tried and rejected

All four were accepted by the kernel and silently discarded by the firmware.

| Commands | Pixel data | Result |
|---|---|---|
| `0xff59` 64B, no checksum | `0xff60` 32B | no change |
| `0xff60` 32B + checksum | `0xff60` 32B | no change |
| `0xff60` 32B + checksum | `0xff59` 64B | no change |
| `0xff59` 64B + checksum | `0xff59` 64B | no change |

Since the envelope is proven good, the conclusion is that `0x04` is simply not a
defined opcode on the wireless side. **Do not try more framings** — find the
opcode.

## The binary

`re/vendor-app/DeviceDriver.exe` (gitignored; re-extract with
`innoextract` from the vendor installer). PE32, i386, MFC/C++, ~1.8MB.
`md5 d0b63a2b7ff29092c0884b26685b6484`.

Useful facts already recovered:

* `config.xml` declares two modes: USB (`0C45:800A`, `MI_00`) and
  **2.4G TYPE-A** (`05AC:024F`, `MI_03`) — so the vendor app itself operates
  over the dongle.
* `hid.dll` is resolved via `GetProcAddress`, so HID calls are indirect and do
  not appear in the import table. `WriteFile` is imported normally.
* The HID write wrapper is at **`0x0045127d`**: it takes the transfer length
  from a 16-bit field in the device struct (`movzx ebx, word ptr [edi + 8]`),
  i.e. the interface's `OutputReportByteLength` at runtime — one code path
  serves both 4096-byte wired and 32-byte wireless transfers.
* Wireless packet builders are locatable by the `aa 55` marker, written as
  `mov word ptr [ebp - X], 0x55aa`. There are **18 such sites**; the packet
  base is `X + 17` bytes below `ebp`. Confirmed example: the site at
  `0x00418e6f` writes brightness/speed/direction at packet offsets 12/13/14,
  matching the documented RGB layout.
* Opcode bytes are *not* written as immediates near those sites — they come
  from registers or a template, which is why static scanning stalled and
  proper decompilation is needed.

## The hunt

1. Decompile the 18 `0x55aa` sites; identify which builds an LCD/screen packet.
2. Read its opcode at packet offset 0 and the field layout.
3. Find how the pixel stream is chunked in that path and which interface it
   opens.

UI strings that may help anchor the search (from `language/1033.lan`):
`58=TFT Screen`, `801=LED screen preview:`, `821=Upload to keyboard`,
`861=Animation Upload`, `835=Delete current LCD data?`.

## Testing a candidate

`src/display.rs` has `WIRELESS_VARIANTS`; add a framing there. Then:

```sh
cargo build --release
./target/release/keyboard-screen --wireless --variant <name> --test-pattern
```

Use `--test-pattern`, never the dashboard: if the numbers have not moved, a
successful push and a dropped one look identical. A working push turns the
screen orange with `2.4G / TEST` and three colour bars.
