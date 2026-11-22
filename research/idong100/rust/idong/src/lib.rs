//! # idong
//!
//! Pure-Rust userspace driver for the **Toltech idong Sensor V2.6**
//! (consumer brand *i-dong / 爱动*), a stereo infrared tracking bar built
//! around the vendor's **idong100** image-processing chip
//! (USB `0471:2600` bootloader → `0471:2500` runtime).
//!
//! The sensor carries two BG0316 CMOS cameras (640×480 @ 60 fps) behind a
//! Spartan-3 FPGA and a Blackfin BF527, illuminates passive retro-reflective
//! markers with 850 nm IR LEDs, and — after the host uploads the vendor
//! firmware — streams **computed 3D marker coordinates** at ~50 fps over a
//! vendor-specific bulk interface. The runtime device is USB composite:
//! its video/audio interfaces are standard UVC/UAC and need no custom
//! driver; this crate owns only the vendor interface (MI_04).
//!
//! Protocol facts are decoded from the vendor's Windows stack
//! (`xeyez.sys`, `EasyUSB.dll`, `xEyeUSB.dll`, `xEyeClass.dll`) and
//! documented with citations in `docs/RUST_DRIVER_DRAFT.md` (research
//! branch `research/idong100-10-rust-driver-draft`).
//!
//! # Safety model
//!
//! 1. Nothing in this crate writes to the device except
//!    [`protocol::boot::BootUploader`] (the exact vendor upload sequence)
//!    and the documented runtime replies (keep-alive `0xFFE`, link-status
//!    word `0x11`, DSP reset type 7).
//! 2. No write-capable code path runs on real hardware before the boot
//!    upload is byte-confirmed against a USBPcap capture of the vendor
//!    stack (see DRAFT §2.5, milestone M3 gate).
//!
//! ```no_run
//! use std::time::Duration;
//! use idong::{Idong, MarkerFrame};
//!
//! // device already in runtime state (previously booted, or use
//! // Idong::open_or_boot with a verified FirmwareSet):
//! let mut dev = Idong::open().expect("no runtime idong sensor");
//! if let Ok(id) = dev.identity() {
//!     println!("serial: {}", id.serial);
//! }
//! dev.start_tracking(Default::default()).unwrap();
//! loop {
//!     let frame: MarkerFrame = dev.read_frame(Duration::from_millis(500)).unwrap();
//!     for m in &frame.markers {
//!         println!("slot {} at ({:.0}, {:.0}, {:.0}) mm, quality {}",
//!                  m.slot, m.x_mm, m.y_mm, m.z_mm, m.quality);
//!     }
//! }
//! ```

pub mod calib;
pub mod device;
pub mod error;
pub mod handle;
pub mod markers;
pub mod protocol;
pub mod stream;
pub mod usb;

pub use calib::{DeviceIdentity, StereoCalib};
pub use device::{FirmwareSet, Idong, TrackingConfig};
pub use error::{BootStep, IdongError, ProtocolError, Result};
pub use handle::{HandleConfig, HandleEvent};
pub use markers::{Marker, MarkerFrame, Quality};
pub use protocol::frames::{Frame, FrameDecoder};
pub use protocol::{EP_IN, EP_OUT, MAGIC_D2H, MAGIC_H2D, PID_BOOTLOADER, PID_RUNTIME, VID};
pub use stream::{StreamStats, Streamer};
pub use usb::{DeviceState, Transport, UsbDeviceInfo};
