//! USB transport: discovery, claim, bulk I/O, re-enumeration handling.
//!
//! The device has two faces (DRAFT §1.2):
//! - **Bootloader** `0471:2600` — single vendor interface 0, bulk EP
//!   0x02/0x82, maxpacket 64. All I/O here is 64-byte framed.
//! - **Runtime** `0471:2500` — USB composite; interfaces 0-3 are UVC/UAC
//!   (standard drivers, not ours), interface 4 is the vendor bulk
//!   interface. Runtime reads use 4096-byte chunks (xeyez.sys
//!   `MaximumTransferSize`), accumulated to 16 KiB per burst (EasyUSB).
//!
//! Read-only by construction: only [`Transport::write_frame`] ever talks
//! to the OUT endpoint, and the only callers are the (safety-gated) boot
//! uploader and the documented runtime replies.

use std::time::Duration;

use rusb::{DeviceHandle, UsbContext};

use crate::error::{IdongError, Result};
use crate::protocol::{
    frames::Frame, EP_IN, EP_OUT, IO_RETRIES, IO_RETRY_DELAY, IFACE_VENDOR, PID_BOOTLOADER,
    PID_RUNTIME, PID_V10, PID_V20, PID_V21, VID,
};

/// Which state a discovered device is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// 0471:2600 — waiting for firmware upload.
    Bootloader,
    /// 0471:2500 — firmware running, vendor interface available.
    Runtime,
    /// 0471:2100 / 0668 / 0666 — older generation, identification only.
    Legacy(u16),
}

/// A discovered device summary.
#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    pub bus: u8,
    pub address: u8,
    pub pid: u16,
    pub state: DeviceState,
    pub serial: Option<String>,
}

impl UsbDeviceInfo {
    pub fn vid_pid(&self) -> String {
        format!("{VID:04x}:{self.pid:04x}")
    }
}

/// Enumerate every idong sensor on the bus (bootloader, runtime, legacy).
pub fn list_devices() -> Vec<UsbDeviceInfo> {
    let mut out = Vec::new();
    if let Ok(ctx) = rusb::Context::new() {
        if let Ok(devices) = ctx.devices() {
            for d in devices.iter() {
                let Ok(desc) = d.device_descriptor() else { continue };
                if desc.vendor_id() != VID {
                    continue;
                }
                let pid = desc.product_id();
                let state = match pid {
                    PID_BOOTLOADER => DeviceState::Bootloader,
                    PID_RUNTIME => DeviceState::Runtime,
                    PID_V21 | PID_V20 | PID_V10 => DeviceState::Legacy(pid),
                    _ => continue,
                };
                let serial = desc
                    .serial_number_string()
                    .and_then(|i| {
                        d.open()
                            .ok()
                            .and_then(|mut h| h.read_string_descriptor_ascii(i).ok())
                    });
                out.push(UsbDeviceInfo {
                    bus: d.bus_number(),
                    address: d.address(),
                    pid,
                    state,
                    serial,
                });
            }
        }
    }
    out
}

/// Open bulk transport to a device face.
pub struct Transport {
    handle: DeviceHandle<rusb::Context>,
    iface: u8,
    /// maxpacket-sized framing for the bootloader; chunked for runtime.
    mode: DeviceState,
}

impl Transport {
    /// Open the bootloader (0471:2600, interface 0).
    pub fn open_bootloader() -> Result<Self> {
        Self::open(PID_BOOTLOADER, 0, DeviceState::Bootloader)
    }

    /// Open the runtime vendor interface (0471:2500, interface 4).
    ///
    /// **Do not** detach the UVC/UAC interfaces' drivers — only the vendor
    /// interface is claimed, leaving video/audio to the OS.
    pub fn open_runtime() -> Result<Self> {
        Self::open(PID_RUNTIME, IFACE_VENDOR, DeviceState::Runtime)
    }

    fn open(pid: u16, iface: u8, mode: DeviceState) -> Result<Self> {
        let ctx = rusb::Context::new()?;
        let devices = ctx.devices()?;
        let mut found = None;
        for d in devices.iter() {
            let desc = d.device_descriptor()?;
            if desc.vendor_id() == VID && desc.product_id() == pid {
                found = Some((d, desc));
                break;
            }
        }
        let (dev, _desc) = found.ok_or(IdongError::NotFound)?;
        let mut handle = dev.open()?;
        // Detach a kernel driver from OUR interface only (defensive; on
        // Linux nothing binds to the vendor interface).
        if matches!(handle.kernel_driver_active(iface), Ok(true)) {
            handle.detach_kernel_driver(iface)?;
        }
        handle.claim_interface(iface)?;
        Ok(Transport {
            handle,
            iface,
            mode,
        })
    }

    pub fn state(&self) -> DeviceState {
        self.mode
    }

    /// Write one frame to EP 0x02 with the EasyUSB retry policy.
    pub fn write_frame(&self, f: &Frame, timeout: Duration) -> Result<()> {
        let bytes = f.to_bytes();
        let mut last = None;
        for attempt in 0..IO_RETRIES {
            match self.handle.write_bulk(EP_OUT, &bytes, timeout) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    log::warn!("bulk write attempt {attempt} failed: {e}");
                    last = Some(e);
                    std::thread::sleep(IO_RETRY_DELAY);
                }
            }
        }
        Err(last.map(IdongError::Usb).unwrap_or(IdongError::NotFound))
    }

    /// Raw bulk read from EP 0x82 (64 B in bootloader mode, 4096 B chunks
    /// in runtime mode).
    pub fn read_raw(&self, max: usize, timeout: Duration) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; max];
        let n = self.handle.read_bulk(EP_IN, &mut buf, timeout)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Read one full frame (bootloader mode: a 64-byte bulk packet is one
    /// frame; runtime mode: chunked, assembled by the caller's decoder).
    pub fn read_frame(&self, timeout: Duration) -> Result<Frame> {
        let (max, want_magic) = match self.mode {
            DeviceState::Bootloader => (64, crate::protocol::MAGIC_D2H),
            _ => (crate::protocol::RUNTIME_CHUNK, crate::protocol::MAGIC_D2H),
        };
        let mut decoder = crate::protocol::frames::FrameDecoder::new();
        loop {
            let chunk = self.read_raw(max, timeout)?;
            for f in decoder.push(&chunk) {
                if f.magic == want_magic {
                    return Ok(f);
                }
            }
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(self.iface);
    }
}

/// Wait for the device to re-enumerate as the runtime composite device
/// after a firmware upload. Uses hotplug when available, else polls
/// `list_devices()` with backoff.
pub fn wait_runtime_reenum(timeout: Duration) -> Result<Transport> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if list_devices()
            .iter()
            .any(|d| d.state == DeviceState::Runtime)
        {
            return Transport::open_runtime();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(IdongError::NoReenumeration(timeout))
}
