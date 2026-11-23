//! USB transport: device discovery, endpoint probing and control/bulk
//! transfers, on top of `rusb` (libusb). Port of
//! `ThirdParty/PSCommon/XnLib/Source/Linux/XnLinuxUSB.cpp` and
//! `Sensor/XnDeviceSensorIO.cpp::OpenDevice/OpenDataEndPoints`.

use std::time::Duration;

use rusb::{Context, DeviceHandle, UsbContext};

use crate::error::{Result, XtionError};

use crate::protocol::endpoint;

/// Vendor control transfer constants used by the PsMonitor protocol.
const CONTROL_OUT_REQUEST_TYPE: u8 = 0x40; // LIBUSB_REQUEST_TYPE_VENDOR | ENDPOINT_OUT
const CONTROL_IN_REQUEST_TYPE: u8 = 0xC0; // LIBUSB_REQUEST_TYPE_VENDOR | ENDPOINT_IN
const CONTROL_REQUEST: u8 = 0;

/// Data endpoints of an opened device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoints {
    /// Bulk-in endpoint carrying depth packets.
    pub depth: u8,
    /// Bulk-in endpoint carrying image packets.
    pub image: u8,
    /// Bulk-in endpoint carrying misc packets (GMC, audio, debug).
    pub misc: Option<u8>,
    /// True when the device uses the >= 3.0/4.0 endpoint layout
    /// (depth 0x81, image 0x82, misc 0x83 and control over EP0).
    pub new_layout: bool,
}

/// An opened Xtion USB device.
pub struct UsbDevice {
    /// Kept alive so the raw context pointer used by the async bulk
    /// rings stays valid for the device's lifetime.
    context: Context,
    handle: DeviceHandle<Context>,
    /// Negotiated bus speed (diagnostics: 640x480@30 depth needs ~13 MB/s
    /// and only fits high speed; full speed caps at ~1.2 MB/s).
    speed: rusb::Speed,
    pub endpoints: Endpoints,
    /// Max packet size of the depth endpoint.
    pub depth_max_packet_size: u32,
    /// Max packet size of the image endpoint.
    pub image_max_packet_size: u32,
    /// Max packet size of the misc endpoint, if present (512 fallback).
    pub misc_max_packet_size: u32,
}

/// A USB device found during enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDeviceInfo {
    pub bus: u8,
    pub address: u8,
    pub pid: u16,
}

/// Enumerate connected Xtion devices (VID 0x1d27, PID 0x0601 / 0x0400).
pub fn list_devices() -> Result<Vec<UsbDeviceInfo>> {
    let context = Context::new()?;
    let mut found = Vec::new();
    for device in context.devices()?.iter() {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };
        if desc.vendor_id() != crate::protocol::USB_VID {
            continue;
        }
        if !crate::protocol::USB_PIDS.contains(&desc.product_id()) {
            continue;
        }
        found.push(UsbDeviceInfo {
            bus: device.bus_number(),
            address: device.address(),
            pid: desc.product_id(),
        });
    }
    Ok(found)
}

/// Open a device handle after a port reset, retrying while the
/// re-enumeration settles (the kernel briefly holds the port; a
/// too-early open fails with NotFound/NoDevice). Falls back to a fresh
/// VID/PID enumeration when the cloned `Device` reference went stale
/// (the kernel re-enumerated the device at a new bus address).
fn reopen_handle(device: &rusb::Device<Context>, ctx: &Context) -> Result<DeviceHandle<Context>> {
    const SETTLE: Duration = Duration::from_millis(250);
    const TRIES: usize = 12; // ~3 s worst case

    let mut last_err = rusb::Error::Other;
    for _ in 0..TRIES {
        match device.open() {
            Ok(handle) => return Ok(handle),
            Err(e) => last_err = e,
        }
        std::thread::sleep(SETTLE);
    }

    // Stale device reference: re-enumerate by VID/PID and take the
    // first Xtion we can open.
    for candidate in ctx.devices()?.iter() {
        let desc = match candidate.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };
        if desc.vendor_id() == crate::protocol::USB_VID
            && crate::protocol::USB_PIDS.contains(&desc.product_id())
        {
            match candidate.open() {
                Ok(handle) => return Ok(handle),
                Err(e) => last_err = e,
            }
        }
    }
    Err(XtionError::Usb(last_err))
}

impl UsbDevice {
    /// Open the first connected Xtion device.
    pub fn open_first() -> Result<UsbDevice> {
        let context = Context::new()?;
        for device in context.devices()?.iter() {
            let desc = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };
            if desc.vendor_id() == crate::protocol::USB_VID
                && crate::protocol::USB_PIDS.contains(&desc.product_id())
            {
                return UsbDevice::open(device.open()?);
            }
        }
        Err(XtionError::DeviceNotFound)
    }

    /// Open a specific device handle and probe its endpoints.
    pub fn open(handle: DeviceHandle<Context>) -> Result<UsbDevice> {
        // The Xtion has no kernel driver binding by default, but be
        // defensive if one ever does.
        let _ = handle.set_auto_detach_kernel_driver(true);

        let endpoints = Self::probe_endpoints(&handle)?;
        if !endpoints.new_layout {
            // Pre-3.0 firmware carries control traffic over bulk endpoints
            // 0x04/0x85; not supported by this driver version.
            return Err(XtionError::UnsupportedFirmware);
        }

        handle.claim_interface(0).map_err(|e| match e {
            rusb::Error::Busy => XtionError::DeviceBusy,
            other => other.into(),
        })?;

        // The misc endpoint (0x83) may live on a second interface (the
        // USB Audio Class one). Claim any extra interfaces best-effort:
        // failure only means device events cannot be captured, streaming
        // still works.
        for iface in 1..=2 {
            let _ = handle.claim_interface(iface);
        }

        let (depth_mps, image_mps) = Self::max_packet_sizes(&handle)?;
        let misc_mps = endpoints
            .misc
            .and_then(|ep| Self::find_endpoint(&handle, ep, true))
            .unwrap_or(512);
        let context = handle.device().context().clone();
        let speed = handle.device().speed();
        Ok(UsbDevice {
            context,
            handle,
            speed,
            endpoints,
            depth_max_packet_size: depth_mps,
            image_max_packet_size: image_mps,
            misc_max_packet_size: misc_mps,
        })
    }

    /// Negotiated USB bus speed.
    pub fn speed(&self) -> rusb::Speed {
        self.speed
    }

    /// USB port reset (`libusb_reset_device` → Linux `USBDEVFS_RESET`)
    /// followed by an in-place reopen of THIS device — the software
    /// equivalent of an unplug/replug (a full firmware re-arm; the
    /// standard cure when the PS1080 firmware wedges its stream state
    /// machine but still answers the control plane, or stops answering
    /// altogether).
    ///
    /// Reference semantics (`libusb` docs + Linux usbfs): the reset
    /// invalidates every open handle — including this one — so the
    /// caller MUST have dropped all bulk rings first (the rings hold
    /// raw pointers into this handle; `Xtion::restart_camera` does it).
    /// After the reset the device re-enumerates on the same port, the
    /// kernel re-runs its configuration, and we reopen from the cloned
    /// `Device` reference; if that reference went stale (the kernel
    /// re-enumerated at a new address) we fall back to a fresh
    /// VID/PID enumeration.
    ///
    /// The Xtion keeps its bus address across a port reset, so the
    /// endpoint layout, max packet sizes and negotiated speed are
    /// re-probed anyway (they are cheap and protect against the
    /// pathological case).
    pub fn reset_and_reopen(&mut self) -> Result<()> {
        let device = self.handle.device().clone();

        // The reset itself. Error::NoDevice here means the firmware is
        // gone from the bus entirely (user unplugged); surface it.
        self.handle.reset()?;

        // Close the (now invalidated) handle before opening a new one.
        // Dropping is all it takes; reassign via a placeholder to keep
        // the borrow checker happy.
        self.handle = reopen_handle(&device, &self.context)?;

        // Re-claim and re-probe exactly like UsbDevice::open().
        let _ = self.handle.set_auto_detach_kernel_driver(true);
        let endpoints = Self::probe_endpoints(&self.handle)?;
        if !endpoints.new_layout {
            return Err(XtionError::UnsupportedFirmware);
        }
        self.handle
            .claim_interface(0)
            .map_err(|e| match e {
                rusb::Error::Busy => XtionError::DeviceBusy,
                other => other.into(),
            })?;
        for iface in 1..=2 {
            let _ = self.handle.claim_interface(iface);
        }
        let (depth_mps, image_mps) = Self::max_packet_sizes(&self.handle)?;
        let misc_mps = endpoints
            .misc
            .and_then(|ep| Self::find_endpoint(&self.handle, ep, true))
            .unwrap_or(512);
        self.speed = self.handle.device().speed();
        self.endpoints = endpoints;
        self.depth_max_packet_size = depth_mps;
        self.image_max_packet_size = image_mps;
        self.misc_max_packet_size = misc_mps;
        Ok(())
    }

    /// Raw libusb context pointer for the async bulk rings. Valid while
    /// this UsbDevice is alive (it owns the Context).
    pub(crate) fn raw_context(&self) -> *mut libusb1_sys::libusb_context {
        use rusb::UsbContext;
        self.context.as_raw()
    }

    /// Raw libusb device-handle pointer for the async bulk rings.
    pub(crate) fn raw_handle(&self) -> *mut libusb1_sys::libusb_device_handle {
        self.handle.as_raw()
    }

    /// Probe the bulk-in endpoints, mirroring `XnSensorIO::OpenDataEndPoints`:
    /// try 0x81 for depth; if absent, fall back to the old layout
    /// (depth 0x82, image 0x83, misc 0x86).
    fn probe_endpoints<T: UsbContext>(handle: &DeviceHandle<T>) -> Result<Endpoints> {
        let has = |addr: u8, bulk_in: bool| -> bool {
            Self::find_endpoint(handle, addr, bulk_in).is_some()
        };

        // "new USB" layout: depth bulk-in at 0x81
        if has(endpoint::DEPTH_NEW, true) {
            let misc = if has(endpoint::MISC_NEW, true) {
                Some(endpoint::MISC_NEW)
            } else {
                None
            };
            return Ok(Endpoints {
                depth: endpoint::DEPTH_NEW,
                image: endpoint::IMAGE_NEW,
                misc,
                new_layout: true,
            });
        }

        // old layout: depth 0x82, image 0x83, audio 0x86, control bulk
        if has(endpoint::DEPTH_OLD, true) {
            let misc = if has(endpoint::MISC_OLD, true) {
                Some(endpoint::MISC_OLD)
            } else {
                None
            };
            return Ok(Endpoints {
                depth: endpoint::DEPTH_OLD,
                image: endpoint::IMAGE_OLD,
                misc,
                new_layout: false,
            });
        }

        Err(XtionError::DeviceNotFound)
    }

    /// Find a bulk endpoint by address; returns its max packet size.
    fn find_endpoint<T: UsbContext>(
        handle: &DeviceHandle<T>,
        address: u8,
        bulk_in: bool,
    ) -> Option<u32> {
        let config = handle.device().config_descriptor(0).ok()?;
        for iface in config.interfaces() {
            for alt in iface.descriptors() {
                for ep in alt.endpoint_descriptors() {
                    if ep.address() != address {
                        continue;
                    }
                    let is_in = ep.direction() == rusb::Direction::In;
                    let is_bulk = ep.transfer_type() == rusb::TransferType::Bulk;
                    if is_bulk && is_in == bulk_in {
                        return Some(ep.max_packet_size() as u32);
                    }
                }
            }
        }
        None
    }

    fn max_packet_sizes<T: UsbContext>(handle: &DeviceHandle<T>) -> Result<(u32, u32)> {
        let depth = Self::find_endpoint(handle, endpoint::DEPTH_NEW, true).unwrap_or(512);
        let image = Self::find_endpoint(handle, endpoint::IMAGE_NEW, true).unwrap_or(512);
        Ok((depth, image))
    }

    /// Read one bulk transfer from an endpoint.
    pub fn bulk_read(&mut self, endpoint: u8, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        Ok(self.handle.read_bulk(endpoint, buf, timeout)?)
    }

    /// Recommended read chunk size for a stream endpoint (Linux bulk
    /// multiplier 40 x max packet size, from `XnDeviceSensorInit.h`).
    pub fn bulk_chunk_size(max_packet_size: u32) -> usize {
        40 * max_packet_size as usize
    }

    pub fn handle_mut(&mut self) -> &mut DeviceHandle<Context> {
        &mut self.handle
    }
}

impl crate::protocol::command::ControlTransport for UsbDevice {
    fn control_out(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        Ok(self.handle.write_control(
            CONTROL_OUT_REQUEST_TYPE,
            CONTROL_REQUEST,
            0,
            0,
            data,
            timeout,
        )?)
    }

    fn control_in(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        Ok(self.handle.read_control(
            CONTROL_IN_REQUEST_TYPE,
            CONTROL_REQUEST,
            0,
            0,
            buf,
            timeout,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_devices_without_hardware_is_ok() {
        // On a machine with no Xtion attached this returns an empty list.
        // In containers without a /dev/bus/usb subsystem the libusb context
        // itself may fail; that is an environment property, not a driver
        // bug, so the test tolerates it.
        match list_devices() {
            Ok(devices) => {
                for d in &devices {
                    assert!(crate::protocol::USB_PIDS.contains(&d.pid));
                }
            }
            Err(e) => eprintln!("skipped: usb enumeration unavailable ({e})"),
        }
    }

    #[test]
    fn bulk_chunk_size_uses_linux_multiplier() {
        assert_eq!(UsbDevice::bulk_chunk_size(512), 40 * 512);
    }

    /// Regression for the on-hardware "USB error: Overflow": every bulk
    /// read length must be a multiple of the endpoint's max packet size.
    #[test]
    fn bulk_chunk_size_is_multiple_of_max_packet() {
        for mps in [64u32, 512, 1024] {
            let chunk = UsbDevice::bulk_chunk_size(mps);
            assert_eq!(chunk % mps as usize, 0, "mps {mps}");
        }
    }

    /// The device-level chunk buffer is sized on the larger endpoint so it
    /// is a valid multiple for BOTH bulk-in endpoints (max packet sizes
    /// are powers of two, so the larger is a multiple of the smaller).
    #[test]
    fn shared_chunk_size_is_multiple_for_both_endpoints() {
        for (depth_mps, image_mps) in [(512u32, 512), (512, 1024), (64, 512), (1024, 64)] {
            let chunk = UsbDevice::bulk_chunk_size(depth_mps.max(image_mps));
            assert_eq!(chunk % depth_mps as usize, 0, "depth mps {depth_mps}");
            assert_eq!(chunk % image_mps as usize, 0, "image mps {image_mps}");
        }
    }

    /// The old whole-frame sizes were NOT valid multiples - this is what
    /// produced EOVERFLOW on every frame read.
    #[test]
    fn whole_frame_sizes_are_not_multiples() {
        let packed11_frame = 640 * 480 * 11 / 8 + 11; // 422,411
        assert_ne!(packed11_frame % 512, 0);
        let yuv422_frame = 640 * 480 * 2; // 614,400
        assert_eq!(yuv422_frame % 512, 0); // this one *happens* to be one
        let yuv422_sxga = 1280 * 1024 * 2; // 2,621,440
        assert_eq!(yuv422_sxga % 512, 0);
    }
}
