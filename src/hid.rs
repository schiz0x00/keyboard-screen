//! Direct Linux `hidraw` access for the Aula F75 Max.
//!
//! The keyboard exposes two vendor interfaces we care about:
//!   * usage page 0xff13 - 64-byte command channel, driven with HID feature reports
//!   * usage page 0xff68 - 4096-byte raw channel, driven with HID output reports
//!
//! Everything here is raw syscalls; there is no hidapi dependency.

use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

pub const VENDOR_ID: u16 = 0x0c45;
pub const PRODUCT_ID: u16 = 0x800a;

pub const COMMAND_USAGE_PAGE: u16 = 0xff13;
pub const RAW_USAGE_PAGE: u16 = 0xff68;

pub const COMMAND_LEN: usize = 64;
pub const CHUNK_LEN: usize = 4096;
pub const ACK_LEN: usize = 128;

/// `_IOC(_IOC_READ|_IOC_WRITE, 'H', 0x06, len)` - HIDIOCSFEATURE.
fn hidiocsfeature(len: usize) -> libc::c_ulong {
    const DIR_RW: u64 = 3; // _IOC_READ | _IOC_WRITE
    ((DIR_RW << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | 0x06) as libc::c_ulong
}

/// `_IOC(_IOC_READ|_IOC_WRITE, 'H', 0x07, len)` - HIDIOCGFEATURE.
fn hidiocgfeature(len: usize) -> libc::c_ulong {
    const DIR_RW: u64 = 3;
    ((DIR_RW << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | 0x07) as libc::c_ulong
}

pub struct HidDevice {
    file: fs::File,
    pub path: PathBuf,
}

impl HidDevice {
    fn open(path: PathBuf) -> std::io::Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)?;
        Ok(Self { file, path })
    }

    /// Send a HID feature report. `data` excludes the report ID, which is 0.
    pub fn send_feature(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(data.len() + 1);
        buf.push(0u8); // report ID
        buf.extend_from_slice(data);
        let rc = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                hidiocsfeature(buf.len()),
                buf.as_mut_ptr(),
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Read a HID feature report back. Failures are non-fatal for our flow.
    pub fn read_feature(&mut self, len: usize) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len + 1];
        let rc = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                hidiocgfeature(buf.len()),
                buf.as_mut_ptr(),
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        buf.remove(0);
        Ok(buf)
    }

    /// Send a HID output report (report ID 0 prepended).
    pub fn send_output(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(data.len() + 1);
        buf.push(0u8);
        buf.extend_from_slice(data);
        self.file.write_all(&buf)
    }

    /// Best-effort drain of one input report. The keyboard ACKs each chunk; we
    /// wait for it to keep from overrunning the device, but a timeout is not an
    /// error - the transfer still lands.
    pub fn await_ack(&mut self, len: usize, timeout_ms: i32) {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if rc > 0 {
            let mut buf = vec![0u8; len];
            let _ = self.file.read(&mut buf);
        }
    }
}

use std::os::unix::fs::OpenOptionsExt;

/// Find the hidraw node for this keyboard exposing `usage_page`.
pub fn find(usage_page: u16) -> Result<HidDevice, String> {
    let want = format!("HID_ID=0003:{:08X}:{:08X}", VENDOR_ID, PRODUCT_ID);
    let mut candidates = Vec::new();

    let entries = fs::read_dir("/sys/class/hidraw")
        .map_err(|e| format!("cannot list /sys/class/hidraw: {e}"))?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let sys = entry.path().join("device");

        let uevent = match fs::read_to_string(sys.join("uevent")) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !uevent.lines().any(|l| l.trim() == want) {
            continue;
        }

        let descriptor = match fs::read(sys.join("report_descriptor")) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if descriptor_usage_page(&descriptor) != Some(usage_page) {
            continue;
        }
        candidates.push(PathBuf::from(format!("/dev/{name}")));
    }

    let path = candidates.into_iter().next().ok_or_else(|| {
        format!(
            "no hidraw node for {:04x}:{:04x} with usage page 0x{:04x} \
             (is the keyboard plugged in over USB-C? the screen is wired-only)",
            VENDOR_ID, PRODUCT_ID, usage_page
        )
    })?;

    HidDevice::open(path.clone()).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!(
                "permission denied opening {}. Install the udev rule:\n  \
                 sudo cp udev/99-aula-f75max.rules /etc/udev/rules.d/ && \
                 sudo udevadm control --reload && sudo udevadm trigger",
                path.display()
            )
        } else {
            format!("cannot open {}: {e}", path.display())
        }
    })
}

/// Pull the top-level Usage Page out of a HID report descriptor.
/// We only need the 16-bit form (`0x06 lo hi`) that the vendor pages use.
fn descriptor_usage_page(descriptor: &[u8]) -> Option<u16> {
    let mut i = 0usize;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        // Usage Page (Global, tag 0x0) = prefix 0b0000_01_xx
        if prefix & 0xfc == 0x04 && size == 2 && i + 2 < descriptor.len() {
            return Some(u16::from_le_bytes([descriptor[i + 1], descriptor[i + 2]]));
        }
        i += 1 + size;
    }
    None
}
