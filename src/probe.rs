//! Read-only investigation of the 2.4GHz dongle.
//!
//! Purpose: find out whether we can talk to the keyboard over the dongle at all,
//! WITHOUT writing any opcode whose meaning is unknown.
//!
//! The wireless protocol is not the wired one on a narrower pipe - it is a
//! different vocabulary. Wired display commands are 64-byte `04 xx` packets with
//! no checksum; wireless commands are 32-byte packets with opcodes 05/07/0f/20,
//! an `aa 55` marker and a checksum at byte 31. Sending a wired opcode here
//! would be firing an undefined instruction at the firmware, and its wired
//! neighbours (`04 19`, `04 15`, `04 11`) are the factory-reset sequence. So the
//! only thing this writes is the battery query, which is documented by the
//! existing open-source drivers.

use crate::hid;

/// The one known-safe wireless write: "report your battery level".
fn battery_query(len: usize, include_report_id: bool) -> Vec<u8> {
    let mut p = vec![0u8; len];
    if include_report_id {
        // Byte 0 is the HID report ID; the opcode follows it.
        if len > 1 {
            p[1] = 0x20;
        }
        if len > 2 {
            p[2] = 0x01;
        }
        checksum(&mut p, 32);
    } else {
        p[0] = 0x20;
        if len > 1 {
            p[1] = 0x01;
        }
        checksum(&mut p, 31);
    }
    p
}

fn checksum(bytes: &mut [u8], index: usize) {
    if index >= bytes.len() {
        return;
    }
    bytes[index] = 0;
    let sum = bytes.iter().fold(0u32, |a, b| a + *b as u32) & 0xff;
    bytes[index] = sum as u8;
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Documented wireless RGB packets, from the existing open-source drivers.
/// Used to answer one question: does the keyboard ACT on a wireless write?
fn rgb_commit() -> Vec<u8> {
    let mut p = vec![0u8; 32];
    p[0] = 0x0f;
    checksum(&mut p, 31);
    p
}

fn rgb_set(mode: u8, brightness: u8, color: u32) -> Vec<u8> {
    let mut p = vec![0u8; 32];
    p[0] = 0x05;
    p[1] = 0x10;
    p[3] = mode;
    // Byte order is R, G, B - determined empirically on real hardware. The
    // existing open-source drivers write blue into byte 4 and red into byte 6,
    // which comes out with red and blue swapped; green is unaffected either way,
    // which is why the mistake is easy to miss.
    p[4] = ((color >> 16) & 0xff) as u8;
    p[5] = ((color >> 8) & 0xff) as u8;
    p[6] = (color & 0xff) as u8;
    p[11] = 0; // not "colorful" - use the fixed colour we passed
    p[12] = brightness;
    p[13] = 3; // speed
    p[14] = 0; // direction
    p[17] = 0xaa;
    p[18] = 0x55;
    checksum(&mut p, 31);
    p
}

/// Drive the RGB over 2.4GHz with the documented packets. If the lighting
/// changes, wireless writes reach the firmware and only the display opcode is
/// wrong; if nothing happens, the dongle does not accept host commands at all
/// and no amount of guessing at display framing will help.
pub fn write_test() -> Result<(), String> {
    let mut dev = hid::find_on(
        hid::DONGLE_VENDOR_ID,
        hid::DONGLE_PRODUCT_ID,
        hid::WIRELESS_RAW_USAGE_PAGE,
    )?;
    println!(
        "Driving RGB over 2.4GHz on {} using documented opcodes.",
        dev.path.display()
    );
    println!("Watch the keyboard lighting.\n");

    for (name, color) in [
        ("RED", 0xff0000u32),
        ("GREEN", 0x00ff00),
        ("BLUE", 0x0000ff),
    ] {
        println!("  -> solid {name}");
        dev.send_output(&rgb_commit())
            .map_err(|e| format!("commit: {e}"))?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        // mode 1 is the first non-off effect; brightness 5 is max.
        dev.send_output(&rgb_set(1, 5, color))
            .map_err(|e| format!("set: {e}"))?;
        std::thread::sleep(std::time::Duration::from_millis(1200));
    }
    println!("\nDid the lighting change? That answers whether wireless writes land.");
    println!("Restore your normal lighting from the vendor software or the keyboard shortcuts.");
    Ok(())
}

pub fn run(listen_secs: u64) -> Result<(), String> {
    println!("== Aula F75 Max 2.4GHz probe (read-only) ==\n");

    // 1. Is the dongle even present, and what does it expose?
    let mut found_any = false;
    for (page, name) in [
        (hid::WIRELESS_COMMAND_USAGE_PAGE, "command"),
        (hid::WIRELESS_RAW_USAGE_PAGE, "raw"),
    ] {
        match hid::find_on(hid::DONGLE_VENDOR_ID, hid::DONGLE_PRODUCT_ID, page) {
            Ok(d) => {
                found_any = true;
                println!("  0x{page:04x} ({name:<7}) -> {}", d.path.display());
            }
            Err(_) => println!("  0x{page:04x} ({name:<7}) -> NOT FOUND"),
        }
    }
    if !found_any {
        return Err("the 2.4GHz dongle is not plugged in".into());
    }

    // 2. Ask for the battery level. A reply proves the dongle is actually
    //    bridging to a live keyboard, not just enumerating its own endpoints.
    println!("\n-- battery query on 0xff60 (the only write this makes) --");
    let mut raw = hid::find_on(
        hid::DONGLE_VENDOR_ID,
        hid::DONGLE_PRODUCT_ID,
        hid::WIRELESS_RAW_USAGE_PAGE,
    )?;

    let mut answered = false;
    // The reference driver tries several framings; the keyboard answers to one.
    for len in [64usize, 33, 32] {
        for with_id in [false, true] {
            let q = battery_query(len, with_id);
            if raw.send_output_raw(&q).is_err() {
                continue;
            }
            if let Some(reply) = raw.read_input(512, 250) {
                println!(
                    "  len={len:<3} report_id={with_id:<5} <- {}",
                    hex(&reply[..reply.len().min(16)])
                );
                // Documented shape: 20 01 xx <percent>.
                let body = if reply[0] == 0x00 {
                    &reply[1..]
                } else {
                    &reply[..]
                };
                if body.len() >= 4 && body[0] == 0x20 && body[1] == 0x01 {
                    println!(
                        "\n  BATTERY: {}% - the dongle is talking to the keyboard.",
                        body[3]
                    );
                    answered = true;
                }
            }
        }
        if answered {
            break;
        }
    }
    if !answered {
        println!("  no recognisable battery reply.");
        println!("  If the keyboard is not switched to 2.4G mode, that alone explains it.");
    }

    // 3. Listen. Pure read - whatever the keyboard volunteers on either page.
    println!("\n-- listening {listen_secs}s on both wireless pages (press keys, toggle things) --");
    let mut chans = Vec::new();
    for (page, name) in [
        (hid::WIRELESS_COMMAND_USAGE_PAGE, "0xff59"),
        (hid::WIRELESS_RAW_USAGE_PAGE, "0xff60"),
    ] {
        if let Ok(d) = hid::find_on(hid::DONGLE_VENDOR_ID, hid::DONGLE_PRODUCT_ID, page) {
            chans.push((name, d));
        }
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(listen_secs);
    let mut seen = 0usize;
    while std::time::Instant::now() < deadline {
        for (name, dev) in chans.iter_mut() {
            if let Some(r) = dev.read_input(512, 100) {
                seen += 1;
                println!("  {name} [{:3}B] {}", r.len(), hex(&r[..r.len().min(24)]));
            }
        }
    }
    if seen == 0 {
        println!("  (silence)");
    }

    println!("\n-- conclusion --");
    println!("  The wireless raw pipe carries 32-byte reports; the wired one carries 4096.");
    println!("  A 128x128 frame is 9 wired chunks, but would be ~1032 wireless packets,");
    println!("  so the wired framing cannot be what the firmware expects here. No public");
    println!("  driver implements image upload over 2.4GHz - they all gate it on wired.");
    println!("  Next step is a USB capture of the vendor software pushing an image while");
    println!("  wireless; if its own app cannot do that, the firmware has no such path.");
    Ok(())
}
