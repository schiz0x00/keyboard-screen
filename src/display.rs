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
