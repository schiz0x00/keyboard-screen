# Wireless screen upload: settled, and it is not possible

**Conclusion: the 128x128 screen cannot be driven over the 2.4GHz dongle or over
Bluetooth. This is a hardware/firmware limit, not a missing opcode.** Do not
spend more time on it.

## The proof, from the vendor software

`DeviceDriver.exe` enumerates HID interfaces in `FUN_0044fa50` and classifies
each one by its **report length**, never by usage page (the binary contains zero
references to `0xff68`, `0xff13`, `0xff59` or `0xff60`):

```c
if      (report_len == 0x41)             ->  command device  (type 1)
else if (in == 0x21 && out == 0x21)      ->  wireless device (type 2)   // 33 = 32+ID
else if (report_len == 0x1001)           ->  this->bulk = dev           // 4097 = 4096+ID
else                                     ->  CloseHandle(...)           // discarded
```

The bulk handle lands at `this + 0x2c`. The pixel sender `FUN_0044f2b0` — the
only caller path for image data — begins with a null check on that handle and
returns immediately if it is unset:

```c
puVar1 = *(undefined4 **)((int)this + 0x2c);
if (puVar1 == NULL) return;
memset(buf, 0, 0x1001);
memcpy(buf + 1, data, 0x1000);       // 4096-byte chunk
FUN_00451210(puVar1, buf, 0x1001);   // WriteFile
```

The dongle exposes only 32-byte pipes (`0x21` with the report ID), so it is
classified as type 2 and **never assigned a bulk handle**. In 2.4G mode the
vendor's own image upload is therefore a no-op. There is no wireless display
opcode to discover, because there is no wireless bulk endpoint to carry pixels.

Bluetooth is excluded even earlier — the vendor UI string reads *"keyboard
settings are not supported in Bluetooth mode for now."*

## What the wired path is (confirmed at source level)

`FUN_00422930` is the upload routine. Ghidra confirms every byte of our
implementation:

* `local_1018 = 0x1804` -> `04 18` begin
* `local_1018 = 0x7204`, slot at payload byte 2, chunk count little-endian at
  payload bytes 8-9 -> `04 72` metadata
* loop of `memcpy(buf, p, 0x1000)` + `FUN_0044f2b0` -> 4096-byte pixel chunks
* `local_1018 = 0x204` -> `04 02` commit

Commands go out as 65-byte writes (`0x41` = 64 + report ID) via `FUN_0044eeb0`.

## What wireless *can* do

The 32-byte wireless channel is fully functional for settings — RGB, key
response, game mode, battery. Verified on hardware. If this project ever wants
to change lighting over 2.4GHz, `src/probe.rs` already has working packet
builders, and the byte order is **R, G, B at bytes 4, 5, 6** (the existing
open-source drivers have red and blue swapped).

## Practical consequence

Push a frame while wired; the screen retains it. `keyboard-screen` exits cleanly
when the keyboard is not on USB and labels the stale frame's age in the footer.
