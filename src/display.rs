//! The Aula F75 Max display upload sequence.
//!
//! Wire format of the payload stream:
//!   byte 0            frame count
//!   bytes 1..=count   per-frame delay byte
//!   bytes 0..256      header (zero padded)
//!   then              frame_count * 128*128*2 bytes of little-endian RGB565
//! The whole stream is zero-padded to a multiple of 4096 and pushed as HID
//! output reports on the 0xff68 interface, bracketed by commands on 0xff13.

use crate::canvas::Canvas;
use crate::hid::{self, HidDevice, ACK_LEN, CHUNK_LEN, COMMAND_LEN};

const HEADER_LEN: usize = 256;
const FRAME_BYTES: usize = crate::canvas::W * crate::canvas::H * 2;

fn command(a: u8, b: u8) -> [u8; COMMAND_LEN] {
    let mut p = [0u8; COMMAND_LEN];
    p[0] = a;
    p[1] = b;
    p
}

/// Build the padded payload stream for a single static frame.
pub fn encode(canvas: &Canvas) -> Vec<u8> {
    let payload_len = HEADER_LEN + FRAME_BYTES;
    let chunks = payload_len.div_ceil(CHUNK_LEN);
    let mut stream = vec![0u8; chunks * CHUNK_LEN];

    stream[0] = 1; // one frame
    stream[1] = 100; // delay byte; irrelevant for a still image

    canvas.to_rgb565(&mut stream[HEADER_LEN..HEADER_LEN + FRAME_BYTES]);
    stream
}

/// Push an encoded stream to `slot` (1-255) and trigger a refresh.
pub fn upload(stream: &[u8], slot: u8, verbose: bool) -> Result<(), String> {
    if slot == 0 {
        return Err("slot must be 1-255".into());
    }
    let chunk_count = stream.len() / CHUNK_LEN;

    let mut cmd = hid::find(hid::COMMAND_USAGE_PAGE)?;
    let mut raw = hid::find(hid::RAW_USAGE_PAGE)?;
    if verbose {
        eprintln!("command: {}", cmd.path.display());
        eprintln!("raw:     {}", raw.path.display());
        eprintln!("sending {chunk_count} chunks to slot {slot}");
    }

    // Begin display transaction.
    exchange(&mut cmd, &command(0x04, 0x18))?;

    // Metadata: target slot and chunk count (little-endian u16).
    let mut meta = command(0x04, 0x72);
    meta[2] = slot;
    meta[8] = (chunk_count & 0xff) as u8;
    meta[9] = ((chunk_count >> 8) & 0xff) as u8;
    exchange(&mut cmd, &meta)?;
    raw.await_ack(ACK_LEN, 150);

    for (i, chunk) in stream.chunks(CHUNK_LEN).enumerate() {
        raw.send_output(chunk)
            .map_err(|e| format!("chunk {}/{}: {e}", i + 1, chunk_count))?;
        raw.await_ack(ACK_LEN, 350);
        if verbose {
            eprint!("\r  {}/{}", i + 1, chunk_count);
        }
    }
    if verbose {
        eprintln!();
    }

    // Commit - the screen does not change until this lands.
    exchange(&mut cmd, &command(0x04, 0x02))?;
    Ok(())
}

fn exchange(dev: &mut HidDevice, packet: &[u8]) -> Result<(), String> {
    dev.send_feature(packet)
        .map_err(|e| format!("command {:#04x} {:#04x} failed: {e}", packet[0], packet[1]))?;
    // The reply is an ack we do not need to interpret; a failure here is benign.
    let _ = dev.read_feature(COMMAND_LEN);
    Ok(())
}

/// Wireless transport variants.
///
/// The firmware demonstrably accepts a 32-byte packet on `0xff60` carrying a
/// checksum at byte 31 - that is how the RGB commands land. What is not known is
/// which channel and framing the *pixel* stream uses, so each variant pairs a
/// command channel with a data channel and the upload is tried against it.
#[derive(Clone, Copy)]
pub struct WirelessVariant {
    pub cmd_page: u16,
    pub cmd_len: usize,
    /// Byte index of the checksum in a command packet, if the channel wants one.
    pub cmd_checksum: Option<usize>,
    pub data_page: u16,
    pub data_len: usize,
}

pub const WIRELESS_VARIANTS: &[(&str, WirelessVariant)] = &[
    // Commands in the proven RGB envelope; pixels down the same 32-byte pipe.
    ("a", WirelessVariant {
        cmd_page: hid::WIRELESS_RAW_USAGE_PAGE, cmd_len: 32, cmd_checksum: Some(31),
        data_page: hid::WIRELESS_RAW_USAGE_PAGE, data_len: 32,
    }),
    // Commands in the proven envelope; pixels down the wider 64-byte channel.
    ("b", WirelessVariant {
        cmd_page: hid::WIRELESS_RAW_USAGE_PAGE, cmd_len: 32, cmd_checksum: Some(31),
        data_page: hid::WIRELESS_COMMAND_USAGE_PAGE, data_len: 64,
    }),
    // Both on the 64-byte channel, checksummed like the wireless packets.
    ("c", WirelessVariant {
        cmd_page: hid::WIRELESS_COMMAND_USAGE_PAGE, cmd_len: 64, cmd_checksum: Some(31),
        data_page: hid::WIRELESS_COMMAND_USAGE_PAGE, data_len: 64,
    }),
];

fn wireless_command(v: &WirelessVariant, a: u8, b: u8) -> Vec<u8> {
    let mut p = vec![0u8; v.cmd_len];
    p[0] = a;
    p[1] = b;
    if let Some(idx) = v.cmd_checksum {
        if idx < p.len() {
            p[idx] = 0;
            let sum = p.iter().fold(0u32, |acc, x| acc + *x as u32) & 0xff;
            p[idx] = sum as u8;
        }
    }
    p
}

/// Chunk size on the 2.4GHz raw pipe. The dongle's `0xff60` interface declares
/// 32-byte reports where the wired `0xff68` declares 4096.
const WIRELESS_CHUNK_LEN: usize = 32;

/// Push a frame over the 2.4GHz dongle.
///
/// UNPROVEN. The wired opcodes are reused on the wireless channels because the
/// vendor software treats 2.4GHz as a full mode (`config.xml` mode 2) and its
/// HID write takes the transfer length from the open interface's
/// `OutputReportByteLength` at runtime rather than a hardcoded 4096 - so one
/// upload path serves both transports, differing only in chunk width. That is
/// an inference from the binary, not a confirmed packet capture.
///
/// Two differences from the wired path are certain, both read off the
/// descriptors: the wireless command interface declares no Feature report, so
/// commands go out as Output reports; and the raw pipe is 32 bytes, so the
/// chunk count in the metadata packet is computed against 32, not 4096.
pub fn upload_wireless(
    stream: &[u8],
    slot: u8,
    variant: &WirelessVariant,
    verbose: bool,
) -> Result<(), String> {
    if slot == 0 {
        return Err("slot must be 1-255".into());
    }
    let chunk_count = stream.len().div_ceil(variant.data_len);
    if chunk_count > 0xffff {
        return Err(format!("{chunk_count} chunks exceeds the u16 metadata field"));
    }

    let mut cmd = hid::find_on(hid::DONGLE_VENDOR_ID, hid::DONGLE_PRODUCT_ID, variant.cmd_page)?;
    let mut data = hid::find_on(hid::DONGLE_VENDOR_ID, hid::DONGLE_PRODUCT_ID, variant.data_page)?;
    if verbose {
        eprintln!(
            "cmd 0x{:04x} ({}B) on {} | data 0x{:04x} ({}B) on {} | {} chunks",
            variant.cmd_page, variant.cmd_len, cmd.path.display(),
            variant.data_page, variant.data_len, data.path.display(), chunk_count
        );
    }

    cmd.send_output(&wireless_command(variant, 0x04, 0x18))
        .map_err(|e| format!("wireless begin: {e}"))?;

    let mut meta = wireless_command(variant, 0x04, 0x72);
    meta[2] = slot;
    meta[8] = (chunk_count & 0xff) as u8;
    meta[9] = ((chunk_count >> 8) & 0xff) as u8;
    if let Some(idx) = variant.cmd_checksum {
        meta[idx] = 0;
        let sum = meta.iter().fold(0u32, |a, x| a + *x as u32) & 0xff;
        meta[idx] = sum as u8;
    }
    cmd.send_output(&meta).map_err(|e| format!("wireless metadata: {e}"))?;
    data.await_ack(variant.data_len, 150);

    for (i, chunk) in stream.chunks(variant.data_len).enumerate() {
        data.send_output(chunk)
            .map_err(|e| format!("chunk {}/{}: {e}", i + 1, chunk_count))?;
        data.await_ack(variant.data_len, 3);
        if verbose && i % 128 == 0 {
            eprint!("\r  {}/{}", i + 1, chunk_count);
        }
    }
    if verbose {
        eprintln!("\r  {chunk_count}/{chunk_count}");
    }

    cmd.send_output(&wireless_command(variant, 0x04, 0x02))
        .map_err(|e| format!("wireless commit: {e}"))?;
    Ok(())
}
