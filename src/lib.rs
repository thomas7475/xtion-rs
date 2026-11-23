//! # xtion
//!
//! Pure-Rust userspace driver for the ASUS Xtion / PrimeSense PS1080
//! coded-light depth camera (USB 1d27:0601 / 1d27:0400).
//!
//! This crate is a from-scratch Rust port of the USB protocol implemented
//! by the OpenNI 2 PS1080 C++ driver. See `docs/PROTOCOL.md` for the
//! extracted protocol reference and `docs/PLAN.md` for the porting plan
//! and current status.
//!
//! ```no_run
//! use std::time::Duration;
//! use xtion::{DepthConfig, Xtion};
//!
//! let mut cam = Xtion::open().expect("no Xtion found");
//! println!("firmware: {}", cam.version);
//! println!("serial:   {}", cam.serial_number().unwrap_or_default());
//! println!("platform: {}", cam.platform_string().unwrap_or_default());
//!
//! cam.start_depth(DepthConfig::default()).unwrap();
//! // hardware registration aligns depth onto the color image
//! cam.set_registration(true).unwrap();
//! let frame = cam.read_depth_frame(Duration::from_secs(2)).unwrap();
//! println!(
//!     "depth {}x{}, min {} mm, {} valid pixels",
//!     frame.width, frame.height,
//!     frame.min_depth().unwrap_or(0),
//!     frame.valid_pixels()
//! );
//! // OVERHEAT / PROJECTOR_FAULT events surface here
//! println!("events: {:?}", cam.drain_events());
//! ```
//!
//! The infrared stream reuses the image pipe (`start_ir` / `read_ir_frame`)
//! and runtime options (mirror, hole filter, AGC, close range, exposure,
//! white balance, emitter on/off) are one-liners — see the docs on
//! [`Xtion`](device::Xtion) and `docs/FEATURE_PLAN.md` for the full
//! surface and what is deliberately left out (flash and calibration
//! writes).
//!
//! Camera geometry: [`Xtion::depth_intrinsics`] returns the pinhole
//! intrinsics derived from the on-device calibration
//! ([`CameraIntrinsics`]), `DepthFrame::to_world` converts single pixels
//! to metric world coordinates and `DepthFrame::point_cloud` yields the
//! full cloud. Frame timestamps are raw 32-bit device ticks on the wire;
//! every decoded frame also carries `timestamp_us` — host-referenced
//! microseconds with wrap handling ([`TimestampConverter`]).

pub mod color;
pub mod depth;
pub mod device;
pub mod error;
pub mod geometry;
pub mod ir;
pub mod protocol;
pub mod ring;
pub mod stream;
pub mod timestamp;
pub mod usb;

pub use device::{
    AgcBin, ColorFrame, Crop, DepthConfig, DepthFrame, DeviceVersion, GmcConfig, Xtion,
};
pub use error::{Nack, ProtocolError, Result, XtionError};
pub use geometry::CameraIntrinsics;
pub use ir::{IrConfig, IrFrame};
pub use protocol::system::{
    bist, CpuTaskStats, EmitterData, FirmwareLogEntry, FlashEntry, FlashFile,
    TecFastConvergenceData,
};
pub use protocol::{
    CmosPreset, DepthFormat, HostMode, ImageFormat, Param, Resolution, UsbCore, CROP_MODE_DISABLED,
    CROP_MODE_INCREASED_FPS, CROP_MODE_NORMAL, MAX_SHIFT,
};
pub use ring::{RingStats, BULK_MULTIPLIER, RING_BUFFERS};
pub use stream::{
    DeviceEvent, GmcDebugEnd, GmcLastConfData, GmcLastPacketData, GmcPacket, GmcPoint, GmcPoints,
    GMC_LAST_PACKET_WIRE_SIZE, GMC_MAX_POINTS_IN_PACKET, GMC_POINT_WIRE_SIZE,
};
pub use timestamp::TimestampConverter;
pub use usb::{list_devices, UsbDeviceInfo};
