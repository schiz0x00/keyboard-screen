# The Aula F75 Max screen protocol

The keyboard is `0c45:800a` (SONiX). It exposes two vendor HID interfaces, and
the screen is **wired-only** — see `wireless-re.md` for why the 2.4GHz dongle
cannot carry pixels.

| Usage page | Report | Role |
|---|---|---|
| `0xff13` | 64-byte feature | command channel |
| `0xff68` | 4096-byte output | raw pixel channel |

## Upload sequence

1. `04 18` on the command channel — begin.
2. `04 72` with the slot at byte 2 and the chunk count as a little-endian `u16`
   at bytes 8–9.
3. The payload as 4096-byte output reports, each followed by a 128-byte ACK.
4. `04 02` — commit. **The screen does not change until this arrives.**

## Payload layout

A 256-byte header — frame count at byte 0, then one delay byte per frame —
followed by `frames × 128 × 128 × 2` bytes of **little-endian RGB565**, zero
padded to a multiple of 4096. A still image is one frame: 9 chunks.

## Provenance

The sequence was reverse-engineered by the
[Aula-F75-Max-Driver](https://github.com/VitalyArt/Aula-F75-Max-Driver) and
[Aula-F75-Max-OSX](https://github.com/RoseWaveStudio/Aula-F75-Max-OSX) projects,
and independently confirmed here by decompiling the vendor software: its upload
routine builds `0x1804`, then `0x7204` with the slot at payload byte 2 and the
chunk count at bytes 8–9, loops 4096-byte chunks, and finishes with `0x204`.

## Wireless settings

The 32-byte wireless channel (`0xff60` on the dongle) does work for settings —
RGB, key response, battery. Packets are 32 bytes with an `aa 55` marker at
offset 17 and a checksum (sum of all bytes, masked to 8 bits) at byte 31.

Note that **wireless RGB byte order is R, G, B at bytes 4, 5, 6**. Both existing
open-source drivers write blue into byte 4 and red into byte 6, which renders red
and blue swapped; green sits in the middle either way, which is what makes the
mistake easy to miss.
