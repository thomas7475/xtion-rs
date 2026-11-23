//! High-level device API: open, handshake, calibration, streaming.
//!
//! Port of `XnDeviceSensorInit.cpp::XnDeviceSensorConfigureVersion` (the
//! version handshake with its retry quirks), the fixed-params/depth-info
//! calibration flow from `XnSensorDepthStream.cpp`, and the stream
//! start/stop parameter sequences.

use std::time::{Duration, Instant};

use crate::color::yuv422_to_rgb888;
use crate::depth::packed11::unpack_packed11;
use crate::depth::shift_to_depth::{ShiftToDepthConfig, ShiftToDepthTables};
use crate::error::{Nack, Result, XtionError};
use crate::ir::{decode_ir_frame, ir_wire_size, wire_extra_rows, IrConfig, IrFrame};
use crate::protocol::algorithm::{
    read_cmos_blanking, read_depth_information, read_device_frequency,
};
use crate::protocol::command::{CommandLink, ControlTransport, ProtocolVersion};
use crate::protocol::fixed_params::{read_fixed_params, FixedParams};
use crate::protocol::identity::{read_platform_string, read_usb_core_type, set_emitter_state};
use crate::protocol::presets::{read_cmos_presets, CmosPreset, CMOS_TYPE_IMAGE};
use crate::protocol::system as sys;
use crate::protocol::{
    DepthFormat, HostMode, ImageFormat, Opcode, Param, Resolution, UsbCore, CROP_MODE_DISABLED,
    CROP_MODE_INCREASED_FPS, CROP_MODE_NORMAL, SHIFTS_PARAM_COEFFICIENT, SHIFTS_SHIFT_SCALE,
    STREAM_MODE_COLOR, STREAM_MODE_DEPTH, STREAM_MODE_IR, STREAM_MODE_OFF,
};
use crate::stream::{DeviceEvent, StreamAssembler};
use crate::usb::UsbDevice;

/// How long a single command exchange may take.
const COMMAND_TIMEOUT: Duration = Duration::from_millis(5_000);
/// The reference driver waits 2 s before retrying the very first
/// GetVersion (known firmware quirk).
const FIRST_COMMAND_RETRY_DELAY: Duration = Duration::from_millis(2_000);
/// Firmware version reply is a 33-byte packed `XnVersions`.
const VERSION_REPLY_LEN: usize = 33;

/// Firmware/hardware version of an opened device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceVersion {
    pub major: u8,
    pub minor: u8,
    /// Build number with the firmware >= 5.x hex-to-decimal quirk
    /// applied (`sprintf("%x")` then `atoi`).
    pub build: u16,
    pub chip: u32,
    pub fpga: u16,
    pub system_version: u16,
}

impl DeviceVersion {
    /// Size of the base version fields every firmware sends
    /// (`nMajor .. nSystemVersion`).
    const BASE_LEN: usize = 12;

    /// Parse the `XnVersions` reply payload.
    ///
    /// Port of `XnHostProtocolGetVersion`, quirks included:
    /// - Only the first 12 bytes carry fields the driver actually uses;
    ///   SDK/HWVer/FWVer/SensorVer/ChipVer are recomputed host-side. The
    ///   C memcpy's the full 33-byte struct out of a buffer that holds
    ///   whatever arrived (shorter replies read zeros), so a reply with
    ///   only the base struct is valid — we zero-pad the same way.
    /// - The first two bytes on the wire are **minor, major** (the C
    ///   byte-swaps the struct's first u16 after the memcpy).
    /// - Firmware >= 5.x: the build number is a hex value re-parsed as
    ///   decimal (`sprintf("%x")` + `atoi`, e.g. 0x33 -> 33).
    fn parse(buf: &[u8]) -> Result<DeviceVersion> {
        if buf.len() < Self::BASE_LEN {
            let raw: String = buf
                .iter()
                .take(32)
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            return Err(XtionError::BadConfig(format!(
                "version reply too short: {} bytes (raw: {raw})",
                buf.len()
            )));
        }
        // zero-pad to the full struct size, like the C's memcpy from a
        // partially-filled buffer
        let mut b = [0u8; VERSION_REPLY_LEN];
        let n = buf.len().min(VERSION_REPLY_LEN);
        b[..n].copy_from_slice(&buf[..n]);
        let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

        let mut version = DeviceVersion {
            // wire order is minor, major (first-u16 byte-swap quirk)
            major: b[1],
            minor: b[0],
            build: u16_at(2),
            chip: u32_at(4),
            fpga: u16_at(8),
            system_version: u16_at(10),
        };

        // FW >= 5.0: build is a hex number printed and re-parsed as decimal
        if version.major >= 5 {
            version.build = format!("{:x}", version.build)
                .parse::<u16>()
                .unwrap_or(version.build);
        }
        Ok(version)
    }

    /// True when the firmware speaks protocol v2.6 (>= 1.2.0).
    fn uses_protocol_v26(&self) -> bool {
        self.major > 1 || (self.major == 1 && self.minor >= 2)
    }
}

impl std::fmt::Display for DeviceVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{} (chip {:#010x}, FPGA {:#06x})",
            self.major, self.minor, self.build, self.chip, self.fpga
        )
    }
}

/// Depth stream configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthConfig {
    pub resolution: Resolution,
    pub fps: u16,
    pub input_format: DepthFormat,
}

impl Default for DepthConfig {
    fn default() -> Self {
        DepthConfig {
            resolution: crate::protocol::DEPTH_DEFAULT_RESOLUTION,
            fps: crate::protocol::DEPTH_DEFAULT_FPS,
            input_format: crate::protocol::DEPTH_DEFAULT_INPUT_FORMAT,
        }
    }
}

impl DepthConfig {
    pub fn dimensions(&self) -> (u32, u32) {
        self.resolution.dimensions()
    }
}

/// Compact statistics of the raw shift values behind a depth frame
/// (diagnostics): how the pixels distribute across the shift range
/// before the shift-to-depth conversion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShiftStats {
    /// Shift == 0 (invalid).
    pub zeros: usize,
    /// Shift == 2047 (the packed-11 "no depth" marker).
    pub markers: usize,
    /// 1..=1023 (valid range of a 1024-entry table).
    pub low: usize,
    /// 1024..=2046 (zeroed by a 1024-entry table).
    pub high: usize,
    /// Smallest shift that is neither 0 nor 2047.
    pub min: Option<u16>,
    /// Largest shift that is neither 0 nor 2047.
    pub max: Option<u16>,
}

impl ShiftStats {
    fn from_shifts(shifts: &[u16]) -> ShiftStats {
        let mut s = ShiftStats::default();
        for &v in shifts {
            match v {
                0 => s.zeros += 1,
                2047 => s.markers += 1,
                1..=1023 => {
                    s.low += 1;
                    s.min = Some(s.min.map_or(v, |m| m.min(v)));
                    s.max = Some(s.max.map_or(v, |m| m.max(v)));
                }
                _ => {
                    s.high += 1;
                    s.min = Some(s.min.map_or(v, |m| m.min(v)));
                    s.max = Some(s.max.map_or(v, |m| m.max(v)));
                }
            }
        }
        s
    }
}

/// A sub-window of the full sensor frame, in full-frame pixel
/// coordinates.
///
/// Written to the per-stream firmware crop parameters (46-50 image,
/// 51-55 depth, 56-60 IR; `XnParams.h`) and applied by the FIRMWARE:
/// while a crop is active the device transmits only the window, so
/// frames arrive crop-sized on the wire
/// (`XnDepthProcessor::CalculateExpectedSize`). `x`/`y` select the
/// window's top-left corner inside the active resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Crop {
    /// Validate against the active stream resolution and wire format,
    /// exactly like the reference (`XnPixelStream::ValidateCropping`,
    /// XnPixelStream.cpp:200-219: non-zero size, window inside the
    /// frame) plus the pixel-group divisibility each packed wire format
    /// needs (the firmware packs the readout as a flat bitstream:
    /// 8 pixels per 11 bytes for packed-11 depth, 4 per 5 bytes for
    /// packed-10 IR — a window whose pixel count is not a whole number
    /// of groups has undefined wire framing).
    fn validate(&self, res_w: u32, res_h: u32, group: usize) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(XtionError::BadConfig(
                "crop window is empty: width and height must be non-zero".into(),
            ));
        }
        if self.x as u32 + self.width as u32 > res_w || self.y as u32 + self.height as u32 > res_h {
            return Err(XtionError::BadConfig(format!(
                "crop window {}x{} at ({},{}) does not fit the {}x{} frame",
                self.width, self.height, self.x, self.y, res_w, res_h
            )));
        }
        if self.width as usize * self.height as usize % group != 0 {
            return Err(XtionError::BadConfig(format!(
                "crop {}x{} = {} pixels is not a whole number of {group}-pixel \
                 wire groups; pick dimensions whose product is divisible by {group}",
                self.width,
                self.height,
                self.width as usize * self.height as usize
            )));
        }
        Ok(())
    }
}

/// Crop state kept per stream: the window plus the firmware mode it is
/// written with (1 = normal, 2 = increased fps).
#[derive(Debug, Clone, Copy)]
struct CropState {
    crop: Crop,
    mode: u16,
}

/// The five crop parameters of one stream (`XnParams.h` 46-60).
#[derive(Clone, Copy)]
struct CropParams {
    size_x: Param,
    size_y: Param,
    offset_x: Param,
    offset_y: Param,
    mode: Param,
}

const IMAGE_CROP_PARAMS: CropParams = CropParams {
    size_x: Param::ImageCropSizeX,
    size_y: Param::ImageCropSizeY,
    offset_x: Param::ImageCropOffsetX,
    offset_y: Param::ImageCropOffsetY,
    mode: Param::ImageCropMode,
};
const DEPTH_CROP_PARAMS: CropParams = CropParams {
    size_x: Param::DepthCropSizeX,
    size_y: Param::DepthCropSizeY,
    offset_x: Param::DepthCropOffsetX,
    offset_y: Param::DepthCropOffsetY,
    mode: Param::DepthCropMode,
};
const IR_CROP_PARAMS: CropParams = CropParams {
    size_x: Param::IrCropSizeX,
    size_y: Param::IrCropSizeY,
    offset_x: Param::IrCropOffsetX,
    offset_y: Param::IrCropOffsetY,
    mode: Param::IrCropMode,
};

/// A decoded depth frame: metric depth in millimeters.
#[derive(Debug, Clone)]
pub struct DepthFrame {
    pub width: u32,
    pub height: u32,
    /// The sub-window this frame represents inside the configured
    /// resolution (`None` = full frame). Pixel (0,0) of `data` is the
    /// window's top-left corner; [`to_world`](Self::to_world) accounts
    /// for the offset.
    pub crop: Option<Crop>,
    /// Depth in millimeters per pixel (row-major); 0 = invalid.
    pub data: Vec<u16>,
    /// Raw device timestamp of the frame's last packet (32-bit ticks of
    /// the ~60 MHz device clock — wraps every 71.6 s; see
    /// [`timestamp_us`](Self::timestamp_us)).
    pub timestamp: u32,
    /// Host-referenced timestamp in microseconds since the first frame
    /// of this session, with wrap handling applied
    /// ([`crate::TimestampConverter`]); `None` when the device clock
    /// rate could not be read.
    pub timestamp_us: Option<u64>,
    /// Distribution of the raw shift values (diagnostics).
    pub shift_stats: ShiftStats,
}

impl DepthFrame {
    /// Smallest non-zero depth in the frame, if any.
    pub fn min_depth(&self) -> Option<u16> {
        self.data.iter().copied().filter(|&d| d != 0).min()
    }

    /// Number of valid (non-zero) pixels.
    pub fn valid_pixels(&self) -> usize {
        self.data.iter().filter(|&&d| d != 0).count()
    }

    /// Convert one pixel to sensor-frame world coordinates — millimetres,
    /// X right, Y up, Z forward ([`crate::geometry`] for the convention).
    /// `None` for invalid pixels.
    pub fn to_world(
        &self,
        intrinsics: &crate::geometry::CameraIntrinsics,
        x: u32,
        y: u32,
    ) -> Option<(f64, f64, f64)> {
        let idx = y as usize * self.width as usize + x as usize;
        let z = *self.data.get(idx)?;
        // A cropped frame's pixels are shifted by the window origin in
        // the full-frame coordinate system the intrinsics describe.
        match self.crop {
            Some(crop) => intrinsics.to_world(x + crop.x as u32, y + crop.y as u32, z),
            None => intrinsics.to_world(x, y, z),
        }
    }

    /// The whole frame as a world-coordinate point cloud: one `(x, y, z)`
    /// millimetre triple per valid pixel, row-major.
    pub fn point_cloud(
        &self,
        intrinsics: &crate::geometry::CameraIntrinsics,
    ) -> Vec<(f64, f64, f64)> {
        let mut cloud = Vec::with_capacity(self.valid_pixels());
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(point) = self.to_world(intrinsics, x, y) {
                    cloud.push(point);
                }
            }
        }
        cloud
    }
}

/// A decoded color frame: interleaved RGB888.
#[derive(Debug, Clone)]
pub struct ColorFrame {
    pub width: u32,
    pub height: u32,
    /// The sub-window this frame represents (`None` = full frame).
    pub crop: Option<Crop>,
    pub data: Vec<u8>,
    pub timestamp: u32,
    /// Host-referenced microseconds since the session's first frame
    /// (see [`DepthFrame::timestamp_us`]).
    pub timestamp_us: Option<u64>,
}

/// Active color-stream configuration (resolution + wire input format).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColorConfig {
    resolution: Resolution,
    format: ImageFormat,
}

/// An opened, calibrated Xtion device.
pub struct Xtion {
    /// Async bulk rings; declared BEFORE `usb` so they are dropped first
    /// (their transfers must not outlive the device handle).
    depth_ring: Option<crate::ring::BulkRing>,
    color_ring: Option<crate::ring::BulkRing>,
    ir_ring: Option<crate::ring::BulkRing>,
    /// Misc endpoint (0x83) ring, active while any stream runs, so
    /// OVERHEAT / PROJECTOR_FAULT events reach the host.
    misc_ring: Option<crate::ring::BulkRing>,
    usb: UsbDevice,
    link: CommandLink,
    pub version: DeviceVersion,
    pub fixed_params: FixedParams,
    depth_const_shift: u16,
    shift_to_depth: ShiftToDepthTables,
    depth_config: Option<DepthConfig>,
    color_config: Option<ColorConfig>,
    ir_config: Option<IrConfig>,
    /// Crop windows, kept per stream and re-applied at every stream
    /// start (C parity: `OpenStreamImpl` writes the crop geometry +
    /// mode after the stream-on param; XnSensorDepthStream.cpp:412-425,
    /// XnSensorImageStream.cpp:359-372, XnSensorIRStream.cpp:190-203).
    depth_crop: Option<CropState>,
    color_crop: Option<CropState>,
    ir_crop: Option<CropState>,
    /// USB core of the device (Jango on all known firmware; read at
    /// open with a documented fallback).
    usb_core: UsbCore,
    depth_assembler: StreamAssembler,
    color_assembler: StreamAssembler,
    ir_assembler: StreamAssembler,
    misc_assembler: StreamAssembler,
    /// Device clock rate in ticks per microsecond (ALGORITHM_PARAMS
    /// 0x80), and the wrap-aware converter built from it. `None` while
    /// the frequency has not been read yet (or the read failed; the
    /// frame timestamps then stay raw).
    device_frequency: Option<f64>,
    timestamps: Option<crate::timestamp::TimestampConverter>,
}

impl Xtion {
    /// Open the first Xtion device and run the full initialization:
    /// version handshake, the reference driver's init soft reset, fixed
    /// params, depth calibration, S2D tables.
    pub fn open() -> Result<Xtion> {
        let mut usb = UsbDevice::open_first()?;

        // Version handshake (port of XnDeviceSensorConfigureVersion):
        // try protocol v2.6 first; retry once after the documented 2 s
        // delay on a timeout; fall back to v2.5 if that fails.
        let (link, version) = Self::handshake(&mut usb)?;

        let mut link = link;

        // USB core (Jango on fw >= 5.3.16). Some 5.x builds NACK the
        // opcode; the C then assumes Jango (XnDeviceSensorInit.cpp:233).
        // Read before the reset because the reset delay depends on it.
        let usb_core =
            read_usb_core_type(&mut link, &mut usb, COMMAND_TIMEOUT).unwrap_or(UsbCore::Jango);

        // The reference driver SOFT-RESETS the firmware at EVERY init
        // (XnSensorFirmware::Init, bResetOnStartup=TRUE default) "to
        // start clean": any state a previous session left behind — half-
        // configured stream params, an armed image pipe — is discarded,
        // and every open starts from the firmware's power-on defaults.
        // Without this, mode-parameter writes after a failed stream
        // start are rejected with NACK(BadParams) (observed on fw
        // 5.8.22, 2026-11-22).
        Self::firmware_init_reset(&mut link, &mut usb, version, usb_core)?;

        let fixed = read_fixed_params(&mut link, &mut usb, COMMAND_TIMEOUT)?;
        let depth_info = read_depth_information(&mut link, &mut usb, COMMAND_TIMEOUT)?;

        let s2d_config = ShiftToDepthConfig {
            zero_plane_distance: fixed.reference_distance as u16,
            zero_plane_pixel_size: fixed.reference_pixel_size,
            emitter_d_cmos_distance: fixed.d_cmos_emitter_distance,
            // XN_SHIFTS_MAX_SHIFT (XnSensorDepthStream.cpp): the packed-11
            // wire values span 0..2047; a 1024-entry table silently zeroes
            // every shift >= 1024 (5.5 m and beyond).
            device_max_shift: 2047,
            device_max_depth: 10_000,
            const_shift: depth_info.const_shift as u32,
            pixel_size_factor: 1,
            param_coeff: SHIFTS_PARAM_COEFFICIENT,
            shift_scale: SHIFTS_SHIFT_SCALE,
            depth_min_cutoff: 0,
            depth_max_cutoff: 10_000,
        };
        let shift_to_depth = ShiftToDepthTables::build(&s2d_config);

        // The device clock rate for host-referenced frame timestamps.
        // The reference driver reads this during init
        // (XnSensor.cpp:631-633, XN_HOST_PROTOCOL_ALGORITHM_FREQUENCY) and
        // fails hard when it cannot; a diagnostic exchange must not take
        // the whole driver down, so keep it optional and retry lazily on
        // the first frame conversion instead.
        let device_frequency = read_device_frequency(&mut link, &mut usb, COMMAND_TIMEOUT)
            .map(|f| f as f64)
            .map_err(|err| {
                log::warn!("device frequency unavailable: {err}; frame timestamps stay raw");
                err
            })
            .ok();

        // Firmware < 5.0 emits one chunk of garbage per endpoint when
        // streaming starts (XnDeviceSensorInit.cpp: nIgnoreBytes).
        let ignore = if version.major >= 5 {
            0
        } else {
            UsbDevice::bulk_chunk_size(usb.depth_max_packet_size)
        };
        let fw_magic = link.protocol().fw_magic();

        Ok(Xtion {
            depth_ring: None,
            color_ring: None,
            ir_ring: None,
            misc_ring: None,
            depth_crop: None,
            color_crop: None,
            ir_crop: None,
            depth_assembler: StreamAssembler::new(fw_magic, ignore),
            color_assembler: StreamAssembler::new(fw_magic, ignore),
            ir_assembler: StreamAssembler::new(fw_magic, ignore),
            misc_assembler: StreamAssembler::new(fw_magic, ignore),
            device_frequency,
            timestamps: None,
            usb,
            link,
            version,
            fixed_params: fixed,
            depth_const_shift: depth_info.const_shift,
            shift_to_depth,
            usb_core,
            depth_config: None,
            color_config: None,
            ir_config: None,
        })
    }

    /// Port of `XnSensorFirmware::Init(bReset = TRUE)` — the reference
    /// driver's init-time reset, run at EVERY open (bResetOnStartup
    /// defaults to TRUE, XnSensor.cpp:68):
    ///
    /// 1. GET_MODE, refuse SAFE_MODE (mode 10);
    /// 2. up to 5 keep-alives to prove liveness;
    /// 3. SET_MODE(SOFT_RESET) fire-and-forget — "there is nobody to
    ///    ACK" while the firmware restarts (XnHostProtocol.cpp:1849);
    /// 4. wait `nUSBDelaySoftReset` — **1 ms** on Jango cores of
    ///    firmware >= 5.3.16 (XnHostProtocol.cpp:559-564), 800 ms
    ///    otherwise;
    /// 5. up to 10 keep-alives (10 ms sleep between failures);
    /// 6. GET_MODE again, refuse SAFE_MODE.
    fn firmware_init_reset<T: ControlTransport>(
        link: &mut CommandLink,
        transport: &mut T,
        version: DeviceVersion,
        usb_core: UsbCore,
    ) -> Result<()> {
        let refuse_safe_mode = |mode: HostMode| -> Result<()> {
            if mode.as_u16() == 10 {
                return Err(XtionError::BadConfig(
                    "device is in safe mode; cannot start".into(),
                ));
            }
            Ok(())
        };

        // 1. current mode
        let mode = HostMode::from_u16(Self::raw_get_mode(link, transport)?);
        refuse_safe_mode(mode)?;

        // 2. liveness: up to 5 keep-alives, abort if the last failed
        let mut alive = Err(XtionError::ControlTimeout);
        for _ in 0..5 {
            alive = Self::raw_keep_alive(link, transport);
            if alive.is_ok() {
                break;
            }
        }
        alive?;

        // 3. fire-and-forget soft reset (SET_MODE, reply discarded)
        let _ = link.exchange(
            transport,
            Opcode::SetMode,
            &HostMode::SoftReset.as_u16().to_le_bytes(),
            Duration::from_millis(300),
        );

        // 4. the C's reset delay: 1 ms for Jango on fw >= 5.3.16
        let reset_delay = if version.major > 5
            || (version.major == 5
                && (version.minor > 3 || (version.minor == 3 && version.build >= 16)))
        {
            if usb_core == UsbCore::Jango {
                Duration::from_millis(1)
            } else {
                Duration::from_millis(800)
            }
        } else {
            Duration::from_millis(800)
        };
        std::thread::sleep(reset_delay);

        // 5. liveness again: up to 10 keep-alives, 10 ms between failures
        let mut ok = false;
        for _ in 0..10 {
            if Self::raw_keep_alive(link, transport).is_ok() {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !ok {
            return Err(XtionError::ControlTimeout);
        }

        // 6. fresh mode, refuse safe mode
        let mode = HostMode::from_u16(Self::raw_get_mode(link, transport)?);
        refuse_safe_mode(mode)?;

        // 7. kill all three streams (best effort). The reference driver
        //    writes every stream mode to OFF both in its pre-5.2 reset
        //    path and at every clean device shutdown
        //    (XnHostProtocol.cpp:1804-1813, XnSensor.cpp:315-317), so a
        //    session that ENDS normally always leaves the firmware with
        //    all streams off. Its own stream-mode writes are guarded by
        //    "we require that every change to mode will go through OFF"
        //    (XnSensorFirmwareParams::SetStreamMode) — the firmware side
        //    of the same rule rejects a stream-on write with
        //    NACK(BadParams) when it believes the stream is still in
        //    another mode. Sessions that end abnormally (a panic between
        //    start and stop, a killed process) leave that stale state
        //    behind, so every open forces the OFF baseline first. The
        //    audio param may be unsupported on some units; a rejection
        //    here must not fail the open.
        for param in [Param::Stream0Mode, Param::Stream1Mode, Param::Stream2Mode] {
            let _ = set_param_via(link, transport, param, STREAM_MODE_OFF, COMMAND_TIMEOUT);
        }
        Ok(())
    }

    /// Raw GET_MODE exchange (u16 reply).
    fn raw_get_mode<T: ControlTransport>(link: &mut CommandLink, transport: &mut T) -> Result<u16> {
        let data = link.exchange(transport, Opcode::GetMode, &[], COMMAND_TIMEOUT)?;
        if data.len() < 2 {
            return Err(XtionError::BadConfig("mode reply too short".into()));
        }
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }

    /// Raw KEEP_ALIVE exchange.
    fn raw_keep_alive<T: ControlTransport>(
        link: &mut CommandLink,
        transport: &mut T,
    ) -> Result<()> {
        link.exchange(transport, Opcode::KeepAlive, &[], COMMAND_TIMEOUT)?;
        Ok(())
    }

    /// Version handshake with the documented retry semantics.
    fn handshake<T: ControlTransport>(usb: &mut T) -> Result<(CommandLink, DeviceVersion)> {
        // First guess: firmware 5.1, protocol v2.6.
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let mut reply = link.exchange(usb, Opcode::GetVersion, &[], COMMAND_TIMEOUT);

        if reply.is_err() {
            // Known quirk: the first command to a freshly powered device
            // may time out; wait and try once more.
            std::thread::sleep(FIRST_COMMAND_RETRY_DELAY);
            reply = link.exchange(usb, Opcode::GetVersion, &[], COMMAND_TIMEOUT);
        }

        if let Ok(data) = reply {
            let version = DeviceVersion::parse(&data)?;
            let protocol = if version.uses_protocol_v26() {
                ProtocolVersion::V26
            } else {
                ProtocolVersion::V25
            };
            return Ok((CommandLink::new(protocol), version));
        }

        // Fall back to protocol v2.5 (pre-1.2 firmware).
        let mut old = CommandLink::new(ProtocolVersion::V25);
        let data = old.exchange(usb, Opcode::GetVersion, &[], COMMAND_TIMEOUT)?;
        let version = DeviceVersion::parse(&data)?;
        Ok((old, version))
    }

    // ------------------------------------------------------------------
    // Identity (Phase 1, docs/FEATURE_PLAN.md)
    // ------------------------------------------------------------------

    /// Device serial number (OPCODE_GET_SERIAL_NUMBER, ASCII reply).
    pub fn serial_number(&mut self) -> Result<String> {
        let data =
            self.link
                .exchange(&mut self.usb, Opcode::GetSerialNumber, &[], COMMAND_TIMEOUT)?;
        Ok(String::from_utf8_lossy(&data)
            .trim_end_matches('\0')
            .to_string())
    }

    /// Sensor platform string, e.g. "RD108x-112"
    /// (OPCODE_GET_PLATFORM_STRING; one char per 16-bit word).
    pub fn platform_string(&mut self) -> Result<String> {
        read_platform_string(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// USB core type (OPCODE_GET_USB_CORE_TYPE). Firmware >= 5.3.16 uses
    /// the fast "Jango" core.
    ///
    /// The reference driver probes this once during open and deliberately
    /// ignores the outcome — "don't check error code. If this fails,
    /// assume JANGO" (XnDeviceSensorInit.cpp:233): several firmware
    /// builds answer with NACK(InvalidCommand) even though the platform
    /// string works. We mirror that: a failed probe falls back to
    /// `UsbCore::Jango` instead of failing the identity surface.
    pub fn usb_core_type(&mut self) -> Result<UsbCore> {
        match read_usb_core_type(&mut self.link, &mut self.usb, COMMAND_TIMEOUT) {
            Ok(core) => Ok(core),
            Err(err) => {
                log::warn!(
                    "usb core probe failed ({err}); assuming Jango core like the reference driver"
                );
                Ok(UsbCore::Jango)
            }
        }
    }

    // ------------------------------------------------------------------
    // Device events (Phase 1)
    // ------------------------------------------------------------------

    /// Enable or disable the IR projector (OPCODE_ENABLE_EMITTER).
    ///
    /// Runtime firmware state: power-cycling the camera restores the
    /// emitter. The *calibration* opcodes for the emitter are excluded on
    /// purpose (docs/FEATURE_PLAN.md, "Excluded features").
    ///
    /// The reference driver only ever issues the emitter opcode on
    /// firmware >= 5.8.15 (`XnHostProtocol.cpp:660-664` gates the opcode
    /// assignment); below that the opcode stays invalid and the command
    /// is never sent. Mirror the gate so unsupported builds get a clean
    /// error instead of a raw NACK.
    ///
    /// **Firmware reality check (hw 5.8.22, serial 1408090603):** this
    /// build NACKs the toggle with BadParams even though it meets the
    /// version gate and the request is byte-identical to the reference's
    /// (`XnVSetEmitterStateRequest` = one u16, XnHostProtocol.cpp:
    /// 3420-3445) — BadParams, not InvalidCommand, so the opcode IS
    /// dispatched and the handler refuses the payload. The C has no
    /// further gate, so this is a build-specific refusal (likely
    /// laser-safety related). The error is therefore rewritten with that
    /// context instead of surfacing a bare NACK.
    pub fn set_emitter_enabled(&mut self, active: bool) -> Result<()> {
        if (self.version.major, self.version.minor, self.version.build) < (5, 8, 15) {
            return Err(XtionError::BadConfig(format!(
                "runtime emitter toggle needs firmware >= 5.8.15, device reports {}.{}.{}",
                self.version.major, self.version.minor, self.version.build
            )));
        }
        set_emitter_state(&mut self.link, &mut self.usb, active, COMMAND_TIMEOUT).map_err(|err| {
            match err {
                XtionError::Nack(Nack::BadParams) => XtionError::BadConfig(
                    "this firmware build rejects the runtime emitter toggle (NACK BadParams) \
                     even though the request matches the reference driver exactly — the \
                     projector stays on; power-cycling also restores it"
                        .into(),
                ),
                other => other,
            }
        })
    }

    /// Drain pending device events (OVERHEAT / PROJECTOR_FAULT) from all
    /// stream assemblers. Events accumulate while streams are being read;
    /// call this between frame reads.
    pub fn drain_events(&mut self) -> Vec<DeviceEvent> {
        let mut events = self.depth_assembler.drain_events();
        events.extend(self.color_assembler.drain_events());
        events.extend(self.ir_assembler.drain_events());
        events.extend(self.misc_assembler.drain_events());
        events
    }

    /// Feed any chunks the misc ring has collected into its assembler
    /// (non-blocking; called from the read loops so events surface while
    /// frames are being waited for).
    fn pump_misc(&mut self) {
        let Some(ring) = self.misc_ring.as_ref() else {
            return;
        };
        let mut chunks = Vec::new();
        ring.drain(&mut chunks);
        for chunk in chunks {
            self.misc_assembler.feed(&chunk);
        }
    }

    /// Start the misc-endpoint ring so device events reach the host.
    /// Best-effort: devices without the endpoint (or with it unclaimable)
    /// still stream fine, they just may miss events.
    fn ensure_misc_ring(&mut self) {
        if self.misc_ring.is_some() {
            return;
        }
        let Some(endpoint) = self.usb.endpoints.misc else {
            return;
        };
        match crate::ring::BulkRing::start(
            self.usb.raw_context(),
            self.usb.raw_handle(),
            endpoint,
            self.usb.misc_max_packet_size,
            crate::ring::RING_BUFFERS,
        ) {
            Ok(ring) => self.misc_ring = Some(ring),
            Err(e) => log::warn!("misc endpoint unavailable, device events may be missed: {e}"),
        }
    }

    /// Stop the misc ring once no data stream is running.
    fn maybe_stop_misc_ring(&mut self) {
        if self.depth_ring.is_none() && self.color_ring.is_none() && self.ir_ring.is_none() {
            self.misc_ring = None;
        }
    }

    // ------------------------------------------------------------------
    // System / diagnostic commands (Phase 3, 0.2.8). Everything here is
    // strictly READ-ONLY — flash file-system READS, telemetry and logs;
    // the write-side commands (file upload/delete/execute, calibrations,
    // I2C/AHB writes, TAKE_SNAPSHOT) stay excluded per
    // docs/FEATURE_PLAN.md.
    // ------------------------------------------------------------------

    /// Read the firmware log (OPCODE_GET_LOG, fw >= 3.0) and render it
    /// as text. UTF-16 text entries are decoded; overflow and structured
    /// entries are rendered like the reference driver does.
    pub fn firmware_log(&mut self) -> Result<String> {
        sys::read_firmware_log(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Read the firmware log as structured entries — the raw header
    /// fields of each entry (timestamp, log type, payload) before
    /// rendering, for diagnostics.
    pub fn firmware_log_entries(&mut self) -> Result<Vec<sys::FirmwareLogEntry>> {
        sys::read_firmware_log_entries(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Read the device clock rate — ticks per microsecond, as reported
    /// by ALGORITHM_PARAMS 0x80 (`fDeviceFrequency`; ~60.0 for the
    /// PS1080's 60 MHz tick, the rate the frame timestamps advance
    /// with). Read once at open; `Err` only if it has never been read
    /// successfully.
    pub fn device_frequency(&mut self) -> Result<f64> {
        self.device_frequency.ok_or_else(|| {
            XtionError::BadConfig(
                "device frequency was not available at open (ALGORITHM_PARAMS 0x80 failed)".into(),
            )
        })
    }

    /// The pinhole intrinsics of the depth (and IR) sensor at one
    /// resolution, derived from the on-device calibration (ZPD / ZPPS
    /// from GET_FIXED_PARAMS — no device I/O happens here).
    ///
    /// The FOV is fixed by the optics (hfov spans the CMOS's native
    /// 1280-pixel line, vfov its 960-pixel height) and does not change
    /// with the streamed resolution; `fx`/`fy`/`cx`/`cy` adapt to
    /// `resolution`. See [`crate::geometry`] for the coordinate
    /// convention and the reference formulas.
    pub fn depth_intrinsics(&self, resolution: Resolution) -> crate::geometry::CameraIntrinsics {
        crate::geometry::CameraIntrinsics::from_calibration(
            self.fixed_params.reference_distance as f64,
            self.fixed_params.reference_pixel_size as f64,
            resolution,
        )
    }

    /// Feed one raw device timestamp through the wrap-aware converter
    /// ([`crate::TimestampConverter`], the port of
    /// `XnDataProcessor::CreateTimestampFromDevice`): microseconds since
    /// the session's first frame. The device frequency is retried lazily
    /// if it was unavailable at open; `None` if it cannot be read at
    /// all (frame timestamps then stay raw).
    fn convert_timestamp(&mut self, device_ts: u32) -> Option<u64> {
        if self.device_frequency.is_none() {
            match read_device_frequency(&mut self.link, &mut self.usb, COMMAND_TIMEOUT) {
                Ok(f) => {
                    self.device_frequency = Some(f as f64);
                }
                Err(err) => {
                    log::warn!("device frequency retry failed: {err}");
                    return None;
                }
            }
        }
        let freq = self.device_frequency?;
        self.timestamps
            .get_or_insert(crate::timestamp::TimestampConverter::new(freq))
            .update(device_ts)
            .into()
    }

    /// List the device's flash file system (OPCODE_GET_FILE_LIST).
    /// Read-only view of the calibration/parameter files the firmware
    /// boots with.
    ///
    /// **fw 5.8.22:** the firmware's reply rows drift +2 bytes per record
    /// (it walks the packed 22-byte on-flash records with its unpacked
    /// 24-byte struct) — prefer [`flash_file_table`](Self::flash_file_table)
    /// for a correct enumeration.
    pub fn flash_file_list(&mut self) -> Result<Vec<sys::FlashFile>> {
        sys::get_file_list(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Read and parse the on-flash file table — the ground-truth file
    /// list (OPCODE_READ_FLASH over the table region + packed-record
    /// parse; see `parse_flash_file_table`).
    pub fn flash_file_table(&mut self) -> Result<Vec<sys::FlashFile>> {
        sys::read_file_table(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Read one file's payload straight from flash (OPCODE_READ_FLASH at
    /// the record's word offset) and check it against the table's
    /// `crc` ([`flash_file_crc`](sys::flash_file_crc)). `Ok(true)` =
    /// intact, `Ok(false)` = CRC mismatch (stale table entry or a
    /// corrupted region), `Err` = the read itself failed.
    pub fn verify_flash_file(&mut self, file: &sys::FlashFile) -> Result<bool> {
        let payload = self.read_flash(file.offset, file.size_bytes() as u32)?;
        Ok(sys::flash_file_crc(&payload) == file.crc)
    }

    /// Read the device flash map (OPCODE_GET_FLASH_MAP): where each file
    /// type lives in flash, its size and its version.
    pub fn flash_map(&mut self) -> Result<Vec<sys::FlashEntry>> {
        sys::get_flash_map(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Download one flash file by type (OPCODE_DOWNLOAD_FILE), e.g. the
    /// per-sensor calibration blobs (0x0A ImageCmos, 0x0B DepthCmos).
    /// This READS the factory data the device was flashed with.
    pub fn download_flash_file(&mut self, file_type: u16) -> Result<Vec<u8>> {
        sys::download_file(&mut self.link, &mut self.usb, file_type, COMMAND_TIMEOUT)
    }

    /// Read raw bytes from device flash (OPCODE_READ_FLASH).
    /// `offset_words` is a **16-bit-word address** (byte address =
    /// value * 2 — every flash address on this sensor is word-based);
    /// `size_bytes` is a byte length (rounded down to whole words).
    pub fn read_flash(&mut self, offset_words: u32, size_bytes: u32) -> Result<Vec<u8>> {
        sys::read_flash(
            &mut self.link,
            &mut self.usb,
            offset_words,
            size_bytes,
            COMMAND_TIMEOUT,
        )
    }

    /// Read one 32-bit value from the SoC's AHB bus (OPCODE_READ_AHB).
    /// Diagnostic; the write variant is permanently excluded.
    pub fn read_ahb(&mut self, address: u32) -> Result<u32> {
        sys::read_ahb(&mut self.link, &mut self.usb, address, COMMAND_TIMEOUT)
    }

    /// Firmware CPU utilization per task (OPCODE_GET_CPU_STATS).
    pub fn cpu_stats(&mut self) -> Result<Vec<sys::CpuTaskStats>> {
        sys::get_cpu_stats(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Run the built-in self-test for the selected modules
    /// (OPCODE_BIST). `timeout` should be generous (the C allows 300 s);
    /// returns the failure bitmask, 0 = all green.
    pub fn run_bist(&mut self, tests_mask: u16, timeout: Duration) -> Result<u32> {
        sys::run_bist(&mut self.link, &mut self.usb, tests_mask, timeout)
    }

    /// TEC (thermoelectric cooler) telemetry. Firmware >= 5.4 answers
    /// with the fast-convergence layout via opcode 38; this camera's
    /// 5.8.22 always takes that path.
    pub fn tec_data(&mut self) -> Result<sys::TecFastConvergenceData> {
        sys::get_tec_fast_convergence_data(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// IR emitter telemetry (OPCODE_GET_EMITTER_DATA): state, set point,
    /// photodiode reading, APC status.
    pub fn emitter_data(&mut self) -> Result<sys::EmitterData> {
        sys::get_emitter_data(&mut self.link, &mut self.usb, COMMAND_TIMEOUT)
    }

    /// Current vertical blanking of one CMOS, read directly
    /// (OPCODE_GET_CMOS_BLANKING). `cmos_type`: 0 = image, 1 = depth
    /// ([`crate::protocol::presets::CMOS_TYPE_IMAGE`] /
    /// [`crate::protocol::presets::CMOS_TYPE_DEPTH`]).
    pub fn cmos_blanking_lines(&mut self, cmos_type: u16) -> Result<u32> {
        sys::get_cmos_blanking_lines(&mut self.link, &mut self.usb, cmos_type, COMMAND_TIMEOUT)
    }

    /// Read one register of one of the camera CMOS sensors through the
    /// I2C path (OPCODE_I2C_READ). The bus and slave address come from
    /// the fixed-params block, like the reference driver's
    /// `XnHostProtocolGetCMOSRegisterI2C`. `cmos_type`: 0 = image,
    /// 1 = depth.
    pub fn read_cmos_register(&mut self, cmos_type: u16, address: u16) -> Result<u16> {
        let (bus, slave) = match cmos_type {
            0 => (
                self.fixed_params.image_cmos_i2c_bus,
                self.fixed_params.image_cmos_i2c_address,
            ),
            _ => (
                self.fixed_params.depth_cmos_i2c_bus,
                self.fixed_params.depth_cmos_i2c_address,
            ),
        };
        sys::read_cmos_register(
            &mut self.link,
            &mut self.usb,
            bus as u16,
            slave as u16,
            address,
            COMMAND_TIMEOUT,
        )
    }

    // ------------------------------------------------------------------
    // Runtime stream options (Phase 2) — all volatile firmware state,
    // restored to defaults on power-cycle.
    // ------------------------------------------------------------------

    /// Enable or disable **hardware registration**: the firmware aligns
    /// the depth image onto the color image (PARAM_REGISTRATION_ENABLE).
    ///
    /// On PS1080 the registration happens in firmware
    /// (`XnSensorDepthStream::DecideFirmwareRegistration`: PS1080 supports
    /// hardware registration at every resolution), so no host-side table
    /// processing is needed. The color stream must be running for the
    /// result to be meaningful.
    pub fn set_registration(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::RegistrationEnable, enable as u16)
    }

    /// Whether registration is currently enabled.
    pub fn registration_enabled(&mut self) -> Result<bool> {
        Ok(self.get_param(Param::RegistrationEnable)? != 0)
    }

    /// Flip the depth image horizontally (PARAM_DEPTH_MIRROR).
    pub fn set_depth_mirror(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::DepthMirror, enable as u16)
    }

    /// Enable the firmware depth hole filter (PARAM_DEPTH_HOLE_FILTER):
    /// fills single-pixel holes in the depth image.
    pub fn set_hole_filter(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::DepthHoleFilter, enable as u16)
    }

    /// Enable depth automatic gain control (PARAM_DEPTH_AGC).
    pub fn set_depth_agc(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::DepthAgc, enable as u16)
    }

    /// Switch the depth stream to close-range mode (PARAM_DEPTH_CLOSE_RANGE,
    /// firmware >= 5.6): trades long range for sub-50 cm sensitivity using
    /// the same ConstShift calibration this driver already reads.
    pub fn set_close_range(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::DepthCloseRange, enable as u16)
    }

    /// Enable global-motion-compensation processing on the depth stream
    /// (PARAM_DEPTH_GMC_MODE).
    pub fn set_gmc_mode(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::DepthGmcMode, enable as u16)
    }

    /// Set, change, or clear the DEPTH crop window (params 51-55).
    ///
    /// `Some(crop)` activates the window — the firmware then transmits
    /// ONLY those pixels: [`DepthFrame`](crate::DepthFrame) arrives
    /// `crop.width x crop.height` in size, with
    /// [`crop`](crate::DepthFrame::crop) carrying the origin, and
    /// [`to_world`](crate::DepthFrame::to_world) still returns correct
    /// full-frame world coordinates. `None` disables cropping (mode 0,
    /// geometry untouched — the reference sequence) and full frames
    /// return.
    ///
    /// `increased_fps` selects firmware crop mode 2 (fw >= 5.7.2): the
    /// CMOS reads out only the window, which makes frame rates above
    /// the full-frame maximum possible (pair it with a higher `fps` in
    /// [`DepthConfig`] where the firmware accepts it).
    ///
    /// The window is validated against the ACTIVE depth resolution
    /// (start the stream first) and against the packed-11 wire format
    /// (pixel count divisible by 8 — e.g. 320x240 or 160x120, not
    /// 161x121). Changes while streaming are plain parameter writes
    /// ("allow while open" in the reference); the frame in flight when
    /// the window changes is dropped as corrupt, exactly like the C.
    /// The window is kept and re-applied by the next
    /// [`start_depth`](Self::start_depth) after a stop.
    pub fn set_depth_crop(&mut self, crop: Option<Crop>, increased_fps: bool) -> Result<()> {
        let config = self.depth_config.as_ref().ok_or_else(|| {
            XtionError::BadConfig(
                "start the depth stream before setting a crop: the window is \
                 validated against the active resolution"
                    .into(),
            )
        })?;
        let (res_w, res_h) = config.dimensions();
        // packed-11 consumes pixels in groups of 8 (11 bytes); the
        // 16-bit format has no packing constraint.
        let group = match config.input_format {
            DepthFormat::Uncompressed11Bit => 8,
            _ => 1,
        };
        let state = self.crop_state(crop, increased_fps, res_w, res_h, group)?;
        write_crop_via(
            &mut self.link,
            &mut self.usb,
            DEPTH_CROP_PARAMS,
            state,
            COMMAND_TIMEOUT,
        )?;
        self.depth_crop = state;
        Ok(())
    }

    /// Set, change, or clear the COLOR crop window (params 46-50).
    ///
    /// Same contract as [`set_depth_crop`](Self::set_depth_crop) for the
    /// image pipe (requires the color stream to be running on units
    /// that have a color sensor).
    pub fn set_color_crop(&mut self, crop: Option<Crop>, increased_fps: bool) -> Result<()> {
        let config = self.color_config.as_ref().ok_or_else(|| {
            XtionError::BadConfig(
                "start the color stream before setting a crop: the window is \
                 validated against the active resolution"
                    .into(),
            )
        })?;
        let (res_w, res_h) = config.resolution.dimensions();
        // YUV422 is byte-per-component on the wire: no packing groups.
        let state = self.crop_state(crop, increased_fps, res_w, res_h, 1)?;
        write_crop_via(
            &mut self.link,
            &mut self.usb,
            IMAGE_CROP_PARAMS,
            state,
            COMMAND_TIMEOUT,
        )?;
        self.color_crop = state;
        Ok(())
    }

    /// Set, change, or clear the IR crop window (params 56-60).
    ///
    /// Same contract as [`set_depth_crop`](Self::set_depth_crop), with
    /// two IR specifics: the window's pixel count must be divisible by
    /// 4 (packed-10 wire format, 4 pixels per 5 bytes), and a cropped IR
    /// frame carries NO extra CMOS rows — the MT9M001's `height + 8`
    /// quirk applies to full frames only
    /// (`XnIRProcessor.cpp:249-255`).
    pub fn set_ir_crop(&mut self, crop: Option<Crop>, increased_fps: bool) -> Result<()> {
        let config = self.ir_config.as_ref().ok_or_else(|| {
            XtionError::BadConfig(
                "start the IR stream before setting a crop: the window is \
                 validated against the active resolution"
                    .into(),
            )
        })?;
        let (res_w, res_h) = config.dimensions();
        // packed-10 consumes pixels in groups of 4 (5 bytes).
        let state = self.crop_state(crop, increased_fps, res_w, res_h, 4)?;
        write_crop_via(
            &mut self.link,
            &mut self.usb,
            IR_CROP_PARAMS,
            state,
            COMMAND_TIMEOUT,
        )?;
        self.ir_crop = state;
        Ok(())
    }

    /// Validate a requested crop and build the state to write
    /// (`None` disables). Shared core of the three stream setters.
    fn crop_state(
        &self,
        crop: Option<Crop>,
        increased_fps: bool,
        res_w: u32,
        res_h: u32,
        group: usize,
    ) -> Result<Option<CropState>> {
        match crop {
            Some(crop) => {
                crop.validate(res_w, res_h, group)?;
                Ok(Some(CropState {
                    crop,
                    mode: if increased_fps {
                        CROP_MODE_INCREASED_FPS
                    } else {
                        CROP_MODE_NORMAL
                    },
                }))
            }
            None => Ok(None),
        }
    }

    /// Flip the color image horizontally (PARAM_IMAGE_MIRROR).
    pub fn set_image_mirror(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::ImageMirror, enable as u16)
    }

    /// Enable the color camera's automatic exposure loop
    /// (PARAM_IMAGE_AUTO_EXPOSURE_MODE).
    pub fn set_auto_exposure(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::ImageAutoExposureMode, enable as u16)
    }

    /// Manual exposure level (PARAM_IMAGE_EXPOSURE_BAR), meaningful only
    /// with auto exposure off.
    pub fn set_exposure(&mut self, value: u16) -> Result<()> {
        self.set_param(Param::ImageExposureBar, value)
    }

    /// Enable the color camera's automatic white balance
    /// (PARAM_IMAGE_AUTO_WHITE_BALANCE_MODE).
    pub fn set_auto_white_balance(&mut self, enable: bool) -> Result<()> {
        self.set_param(Param::ImageAutoWhiteBalanceMode, enable as u16)
    }

    /// Color camera gain / AGC level (PARAM_IMAGE_AGC).
    pub fn set_image_gain(&mut self, value: u16) -> Result<()> {
        self.set_param(Param::ImageAgc, value)
    }

    /// Color camera quality setting (PARAM_IMAGE_QUALITY).
    pub fn set_image_quality(&mut self, value: u16) -> Result<()> {
        self.set_param(Param::ImageQuality, value)
    }

    /// Anti-flicker frequency in Hz (PARAM_IMAGE_FLICKER_DETECTION):
    /// 50 or 60.
    pub fn set_flicker(&mut self, frequency_hz: u16) -> Result<()> {
        self.set_param(Param::ImageFlickerDetection, frequency_hz)
    }

    /// Read one firmware parameter.
    pub fn get_param(&mut self, param: Param) -> Result<u16> {
        let data = self.link.exchange(
            &mut self.usb,
            Opcode::GetParam,
            &param.as_u16().to_le_bytes(),
            COMMAND_TIMEOUT,
        )?;
        if data.len() < 2 {
            return Err(XtionError::BadConfig("param reply too short".into()));
        }
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }

    /// Write one firmware parameter.
    pub fn set_param(&mut self, param: Param, value: u16) -> Result<()> {
        set_param_via(&mut self.link, &mut self.usb, param, value, COMMAND_TIMEOUT)
    }

    /// Read the firmware operating mode (GET_MODE, opcode 5).
    pub fn get_mode(&mut self) -> Result<HostMode> {
        Ok(HostMode::from_u16(Self::raw_get_mode(
            &mut self.link,
            &mut self.usb,
        )?))
    }

    /// Soft-reset the firmware (SET_MODE, opcode 6, mode 3) and wait for
    /// it to come back. Port of the reference init path
    /// (XnSensorFirmware::Init with bResetOnStartup=TRUE); `open()` runs
    /// this automatically via [`Self::firmware_init_reset`].
    pub fn soft_reset(&mut self) -> Result<()> {
        Self::firmware_init_reset(&mut self.link, &mut self.usb, self.version, self.usb_core)
    }

    /// Configure and start the depth stream (stream1 in firmware terms).
    ///
    /// Parameter order mirrors the reference driver's property pipeline:
    /// format -> resolution -> fps -> stream mode.
    pub fn start_depth(&mut self, config: DepthConfig) -> Result<()> {
        // Stream-mode changes go through OFF first (see
        // firmware_init_reset step 7; reference guard
        // XnSensorFirmwareParams::SetStreamMode).
        self.set_param(Param::Stream1Mode, STREAM_MODE_OFF)?;
        self.set_param(Param::DepthFormat, config.input_format.as_u16())?;
        self.set_param(Param::DepthResolution, config.resolution.as_u16())?;
        self.set_param(Param::DepthFps, config.fps)?;
        // CMOS timing: the reference depth stream ends its configuration
        // with SetCmosConfig(XN_CMOS_TYPE_DEPTH, ...) — a blanking read
        // for the requested mode (XnSensorDepthStream::ConfigureStreamImpl).
        read_cmos_blanking(
            &mut self.link,
            &mut self.usb,
            config.resolution,
            config.fps,
            COMMAND_TIMEOUT,
        )?;
        self.set_param(Param::Stream1Mode, STREAM_MODE_DEPTH)?;
        // OpenStreamImpl tail (XnSensorDepthStream.cpp:398-425): the crop
        // geometry is re-applied when a window is active, then the crop
        // mode — ALWAYS written, so stale firmware state from a previous
        // session can never leak a crop into this stream.
        write_crop_via(
            &mut self.link,
            &mut self.usb,
            DEPTH_CROP_PARAMS,
            self.depth_crop,
            COMMAND_TIMEOUT,
        )?;

        // Start the async URB ring BEFORE the device begins to stream:
        // pre-submitted transfers mean the FPGA FIFO always has somewhere
        // to DMA into (a sync read loop loses data in the gaps).
        self.depth_ring = Some(crate::ring::BulkRing::start(
            self.usb.raw_context(),
            self.usb.raw_handle(),
            self.usb.endpoints.depth,
            self.usb.depth_max_packet_size,
            crate::ring::RING_BUFFERS,
        )?);
        self.ensure_misc_ring();

        self.depth_config = Some(config);
        Ok(())
    }

    /// Stop the depth stream.
    pub fn stop_depth(&mut self) -> Result<()> {
        // Cancel the URB ring first so no transfers are pending when the
        // firmware stops sending.
        self.depth_ring = None;
        self.set_param(Param::Stream1Mode, STREAM_MODE_OFF)?;
        self.depth_config = None;
        self.maybe_stop_misc_ring();
        Ok(())
    }

    /// Configure and start the color stream (stream0 in firmware terms).
    ///
    /// # Capability gate
    ///
    /// Units without a color sensor report `image_cmos_type = 0`
    /// (`XN_IMAGE_CMOS_NONE`, fixed-params byte 76; PS1080.h:425-431) —
    /// this method refuses them up front, mirroring the reference
    /// driver, which never creates the image stream when
    /// `bImageSupported` is false (XnSensorFirmware.cpp:128-140,
    /// XnSensor.cpp:274-281). Hardware-validated 2026-11-23: the
    /// project's fw 5.8.22 unit reports NONE, its firmware NACKs
    /// STREAM0_MODE=COLOR with BadParams (every combination since
    /// 0.2.4), while IR — the other stream0 source, fed by the depth
    /// CMOS — starts fine. Depth and IR are unaffected by the gate.
    ///
    /// Wire input formats (`XnIOImageFormats`, Include/PS1080.h:253-261):
    ///
    /// - [`ImageFormat::Yuv422`] (1) is the **PS-compressed** variant —
    ///   the reference driver's default on non-Windows hosts, with the
    ///   vendor comment *"non win32 works with BULK, so it's better to
    ///   compress the image"* (XnSensorImageStream.h:36-41) and the
    ///   recommended mode on the Xtion's bulk endpoints.
    /// - [`ImageFormat::UncompressedYuv422`] (5) is plain UYVY — the
    ///   Windows/ISO default. Supported for completeness; may be
    ///   rejected by the firmware on bulk endpoints.
    ///
    /// The requested (resolution, fps, format) mode is validated against
    /// the preset list the firmware reports (opcode 36) exactly like the
    /// reference driver. The color camera and the IR stream share
    /// firmware stream0, so they cannot run at the same time (stop one
    /// before starting the other). Compressed frames are decoded by
    /// [`crate::color::uncompress_yuv_image_ps`]
    /// (`XnStreamUncompressYUVImagePS`).
    pub fn start_color(
        &mut self,
        resolution: Resolution,
        fps: u16,
        format: ImageFormat,
    ) -> Result<()> {
        if self.ir_config.is_some() {
            return Err(XtionError::BadConfig(
                "the IR stream is running; color and IR share the image pipe".into(),
            ));
        }
        // C parity: bImageSupported (XnSensorFirmware.cpp:128-140). For
        // fw >= 5.6.9 (bGetImageCmosTypeSupported) the gate is exactly
        // `image_cmos_type != 0`; the C then never even creates the
        // image stream (XnSensor.cpp:274-281), so it never sees the
        // BadParams NACK this firmware answers to STREAM0_MODE=COLOR on
        // a sensorless unit. Fail fast BEFORE writing any image params
        // so device state is left clean.
        if (self.version.major, self.version.minor, self.version.build) >= (5, 6, 9)
            && self.fixed_params.image_cmos_type == 0
        {
            return Err(XtionError::BadConfig(
                "start_color: this device has no color sensor — the fixed-params \
                 block reports image_cmos_type = XN_IMAGE_CMOS_NONE (0; 1 = MT9M112, \
                 2 = MT9D131, 3 = MT9M114). This is a depth+IR unit: the firmware \
                 rejects COLOR on stream0 with NACK(BadParams), and the reference \
                 driver refuses to create the image stream on such units as well \
                 (bImageSupported, XnSensorFirmware.cpp:128-140). Depth and IR are \
                 unaffected; see docs/PROTOCOL.md section 7"
                    .into(),
            ));
        }
        match format {
            ImageFormat::Yuv422 | ImageFormat::UncompressedYuv422 => {}
            _ => {
                return Err(XtionError::BadConfig(
                    "only the Yuv422 (PS-compressed, format 1) and \
                     UncompressedYuv422 (format 5) input formats are supported"
                        .into(),
                ))
            }
        }
        // C parity: the reference validates the requested image mode
        // against the preset list the firmware reports (opcode 36,
        // XnHostProtocolGetCmosPresets -> XnSensorImageStream::Init) and
        // never writes a mode outside it — the firmware rejects such a
        // write with NACK(BadParams) (observed on fw 5.8.22, 2026-11-22).
        let presets = read_cmos_presets(
            &mut self.link,
            &mut self.usb,
            CMOS_TYPE_IMAGE,
            COMMAND_TIMEOUT,
        )
        .map_err(|err| XtionError::BadConfig(format!("start_color: get image presets: {err}")))?;
        if presets.is_empty() {
            return Err(XtionError::BadConfig(
                "start_color: the device reports no color (image CMOS) presets".into(),
            ));
        }
        let list = presets
            .iter()
            .map(|preset| preset.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        if !presets
            .iter()
            .any(|p| p.resolution == resolution.as_u16() && p.fps == fps)
        {
            return Err(XtionError::BadConfig(format!(
                "start_color: {resolution:?}@{fps} fps is not a color mode of this camera; \
                 supported modes: {list}"
            )));
        }
        if !presets.iter().any(|p| {
            p.resolution == resolution.as_u16() && p.fps == fps && p.format == format.as_u16()
        }) {
            return Err(XtionError::BadConfig(format!(
                "start_color: input format {} is not offered at {resolution:?}@{fps} fps; \
                 supported modes: {list}",
                format.as_u16()
            )));
        }

        self.set_param(Param::ImageFormat, format.as_u16())
            .map_err(|err| XtionError::BadConfig(format!("start_color: image format: {err}")))?;
        self.set_param(Param::ImageResolution, resolution.as_u16())
            .map_err(|err| {
                XtionError::BadConfig(format!("start_color: image resolution: {err}"))
            })?;
        self.set_param(Param::ImageFps, fps)
            .map_err(|err| XtionError::BadConfig(format!("start_color: image fps: {err}")))?;
        configure_color_via(
            &mut self.link,
            &mut self.usb,
            resolution,
            fps,
            STREAM_MODE_COLOR,
            self.color_crop,
            COMMAND_TIMEOUT,
        )
        .map_err(|err| {
            // Self-diagnosing error: a NACK on the stream-mode write is
            // always about firmware STATE, so report what the device
            // thinks the stream0 selector and the operating mode are.
            let stream0 = self
                .get_param(Param::Stream0Mode)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "?".into());
            let mode = self
                .get_mode()
                .map(|m| format!("{m:?}"))
                .unwrap_or_else(|_| "?".into());
            XtionError::BadConfig(format!(
                "start_color: stream-on rejected: {err}; \
                 device state at failure: stream0={stream0}, mode={mode}"
            ))
        })?;

        self.color_ring = Some(crate::ring::BulkRing::start(
            self.usb.raw_context(),
            self.usb.raw_handle(),
            self.usb.endpoints.image,
            self.usb.image_max_packet_size,
            crate::ring::RING_BUFFERS,
        )?);
        self.ensure_misc_ring();

        self.color_config = Some(ColorConfig { resolution, format });
        Ok(())
    }

    /// Stop the color stream.
    pub fn stop_color(&mut self) -> Result<()> {
        self.color_ring = None;
        self.set_param(Param::Stream0Mode, STREAM_MODE_OFF)?;
        self.color_config = None;
        self.maybe_stop_misc_ring();
        Ok(())
    }

    /// Configure and start the infrared stream.
    ///
    /// IR reuses firmware stream0 (the image pipe): the frames arrive on
    /// the image endpoint as IMAGE packets with 4x10-bit packed pixels
    /// and are decoded to 16-bit grayscale. The firmware IR mirror is
    /// forced off, exactly like the reference driver
    /// (`XnSensorIRStream::ConfigureStreamImpl`), which mirrors IR
    /// host-side instead. Color and IR are mutually exclusive; IR pairs
    /// with depth (that is the combination hardware registration is
    /// designed for).
    pub fn start_ir(&mut self, config: IrConfig) -> Result<()> {
        if self.color_config.is_some() {
            return Err(XtionError::BadConfig(
                "the color stream is running; color and IR share the image pipe".into(),
            ));
        }
        configure_ir_via(
            &mut self.link,
            &mut self.usb,
            config,
            self.ir_crop,
            COMMAND_TIMEOUT,
        )?;

        self.ir_ring = Some(crate::ring::BulkRing::start(
            self.usb.raw_context(),
            self.usb.raw_handle(),
            self.usb.endpoints.image,
            self.usb.image_max_packet_size,
            crate::ring::RING_BUFFERS,
        )?);
        self.ensure_misc_ring();

        self.ir_config = Some(config);
        Ok(())
    }

    /// Stop the infrared stream.
    pub fn stop_ir(&mut self) -> Result<()> {
        self.ir_ring = None;
        self.set_param(Param::Stream0Mode, STREAM_MODE_OFF)?;
        self.ir_config = None;
        self.maybe_stop_misc_ring();
        Ok(())
    }

    /// Negotiated USB bus speed (diagnostics).
    pub fn usb_speed(&self) -> rusb::Speed {
        self.usb.speed()
    }

    /// Max packet size of the depth and image bulk endpoints
    /// (diagnostics; the bulk ring reads 40 x this value per URB).
    pub fn usb_max_packet_sizes(&self) -> (u32, u32) {
        (
            self.usb.depth_max_packet_size,
            self.usb.image_max_packet_size,
        )
    }

    /// Transfer statistics of the IR (image-pipe) bulk ring, for
    /// diagnostics: `completions` counts URBs with data, so a zero while
    /// the stream should be running means the device never sent anything.
    pub fn ir_ring_stats(&self) -> Option<crate::ring::RingStats> {
        self.ir_ring.as_ref().map(|ring| ring.stats())
    }

    /// Transfer statistics of the color bulk ring, if streaming
    /// (diagnostics; the color stream uses the same image pipe as IR).
    pub fn color_ring_stats(&self) -> Option<crate::ring::RingStats> {
        self.color_ring.as_ref().map(|ring| ring.stats())
    }

    /// The C's `bImageSupported` capability gate (fw >= 5.6.9:
    /// `fixed_params.image_cmos_type != 0`; XnSensorFirmware.cpp:128-140).
    ///
    /// `false` means this unit has **no color sensor**: `start_color`
    /// refuses up front, and the reference driver never creates the
    /// image stream on such devices either (XnSensor.cpp:274-281 —
    /// OpenNI lists no color node for them). Depth and IR are
    /// unaffected: they are served by the depth CMOS.
    pub fn image_supported(&self) -> bool {
        self.fixed_params.image_cmos_type != 0
    }

    /// Stream modes the color (image) CMOS supports, as reported by the
    /// firmware (opcode 36). `start_color` validates against this list.
    /// NOTE: this is a PLATFORM capability list baked into the firmware
    /// (RD108x) — on units with `image_cmos_type = 0` it still lists
    /// color modes the absent sensor can never deliver.
    pub fn image_presets(&mut self) -> Result<Vec<CmosPreset>> {
        self.cmos_presets(CMOS_TYPE_IMAGE)
    }

    /// Read the stream modes one CMOS supports (0 = image, 1 = depth;
    /// `CMOS_TYPE_*` in `protocol::presets`).
    pub fn cmos_presets(&mut self, cmos: u16) -> Result<Vec<CmosPreset>> {
        read_cmos_presets(&mut self.link, &mut self.usb, cmos, COMMAND_TIMEOUT)
    }

    /// Counters of the depth bulk ring, if streaming (diagnostics).
    pub fn depth_ring_stats(&self) -> Option<crate::ring::RingStats> {
        self.depth_ring.as_ref().map(|r| r.stats())
    }

    /// Firmware magic used by the bulk stream packet headers
    /// (protocol-version dependent; "BR" 0x4252 on protocol 2.6).
    pub fn fw_magic(&self) -> u16 {
        self.link.protocol().fw_magic()
    }

    /// Capture up to `bytes` of RAW bytes from the depth stream endpoint
    /// into `path` (diagnostics; the depth stream must be running). The
    /// data is NOT fed through the frame assembler. Returns the number of
    /// bytes written.
    pub fn tap_depth_stream(
        &mut self,
        path: &std::path::Path,
        bytes: usize,
        timeout: Duration,
    ) -> Result<usize> {
        use std::io::Write as _;
        let deadline = Instant::now() + timeout;
        let mut file = std::fs::File::create(path)
            .map_err(|e| XtionError::Io(format!("{}: {e}", path.display())))?;
        let mut written = 0usize;
        while written < bytes {
            let ring = self
                .depth_ring
                .as_ref()
                .ok_or(XtionError::StreamNotRunning)?;
            let chunk = ring.pop(deadline).ok_or(XtionError::ControlTimeout)?;
            let take = chunk.len().min(bytes - written);
            file.write_all(&chunk[..take])
                .map_err(|e| XtionError::Io(format!("{}: {e}", path.display())))?;
            written += take;
        }
        Ok(written)
    }

    /// Block until one depth frame is decoded. Corrupted frames are
    /// dropped and reading continues.
    pub fn read_depth_frame(&mut self, timeout: Duration) -> Result<DepthFrame> {
        let config = self.depth_config.ok_or(XtionError::StreamNotRunning)?;
        // While a crop window is active the firmware transmits ONLY the
        // window: frames are crop-sized and report the crop dimensions
        // and origin (XnDepthProcessor::CalculateExpectedSize / OnEndOfFrame,
        // XnDepthProcessor.cpp:112-172).
        let crop = self.depth_crop.map(|cs| cs.crop);
        let (width, height) = match crop {
            Some(c) => (c.width as u32, c.height as u32),
            None => config.dimensions(),
        };
        let expected_pixels = (width * height) as usize;
        let deadline = Instant::now() + timeout;

        loop {
            // drain completed frames
            while let Some(frame) = self.depth_assembler.pop_frame() {
                if frame.corrupted {
                    log::warn!("corrupted depth frame dropped");
                    continue;
                }
                // C: XnDepthProcessor::OnEndOfFrame marks a frame corrupt
                // when the assembled size differs from the expected size
                // (this is how the partial first frame of a stream is
                // dropped instead of being delivered).
                let expected_bytes = match config.input_format {
                    DepthFormat::Uncompressed11Bit => {
                        // 8 x 11-bit values share 11 bytes
                        expected_pixels * 11 / 8
                    }
                    DepthFormat::Uncompressed16Bit => expected_pixels * 2,
                    _ => expected_pixels * 2,
                };
                if frame.data.len() != expected_bytes {
                    log::warn!(
                        "depth frame is corrupt: {} bytes != expected {expected_bytes}, dropped",
                        frame.data.len()
                    );
                    continue;
                }
                return {
                    let mut frame =
                        self.decode_depth_frame(&frame, width, height, expected_pixels, config)?;
                    frame.crop = crop;
                    frame.timestamp_us = self.convert_timestamp(frame.timestamp);
                    Ok(frame)
                };
            }
            self.pump_misc();
            // take one completed chunk from the ring (borrow ends here)
            let chunk = {
                let ring = self
                    .depth_ring
                    .as_ref()
                    .ok_or(XtionError::StreamNotRunning)?;
                ring.pop(deadline)
            };
            match chunk {
                Some(data) => self.depth_assembler.feed(&data),
                None => {
                    if let Some(err) = self.depth_ring.as_ref().unwrap().error() {
                        return Err(err);
                    }
                    if Instant::now() >= deadline {
                        return Err(XtionError::ControlTimeout);
                    }
                }
            }
        }
    }

    /// Block until one color frame is decoded (RGB888).
    ///
    /// The color pipe has NO extra CMOS rows (unlike IR): the reference
    /// validates color frames against exactly `XRes * YRes * bpp`
    /// (`XnImageProcessor::CalculateExpectedSize`, bpp = 2 for YUV422)
    /// and marks anything else corrupt — this is also how the partial
    /// first frame of a stream is dropped. Following the IR lesson
    /// (0.2.2), a stream that keeps producing unusable frames fails fast
    /// with the actual sizes and counters instead of looping into a
    /// silent timeout.
    ///
    /// The wire format decides the gate: PS-compressed frames
    /// ([`ImageFormat::Yuv422`]) have a VARIABLE raw size (the C streams
    /// them through `XnStreamUncompressYUVImagePS` with a carry buffer),
    /// so any non-corrupted frame is decompressed and the DECODED size
    /// must be exactly `width * height * 2`; uncompressed frames
    /// ([`ImageFormat::UncompressedYuv422`]) are gated on the exact wire
    /// size before the BT.601 conversion.
    pub fn read_color_frame(&mut self, timeout: Duration) -> Result<ColorFrame> {
        let config = self.color_config.ok_or(XtionError::StreamNotRunning)?;
        // While a crop window is active the firmware transmits only the
        // window (XnSensorImageStream::CalculateExpectedSize,
        // XnSensorImageStream.cpp:786-817): frames are crop-sized.
        let crop = self.color_crop.map(|cs| cs.crop);
        let (width, height) = match crop {
            Some(c) => (c.width as u32, c.height as u32),
            None => config.resolution.dimensions(),
        };
        // YUV422 decoded size: 2 bytes per pixel, no extra rows
        // (XnImageProcessor.cpp:75-88).
        let expected = (width * height * 2) as usize;
        let compressed = config.format == ImageFormat::Yuv422;
        let deadline = Instant::now() + timeout;
        let mut unusable = 0u32;

        loop {
            while let Some(frame) = self.color_assembler.pop_frame() {
                let decoded = if compressed {
                    if frame.corrupted {
                        None
                    } else {
                        // XnPSCompressedImageProcessor: the whole frame
                        // payload flows through the decompressor; a good
                        // frame yields exactly width*height*2 UYVY bytes.
                        let mut yuv = Vec::new();
                        crate::color::uncompress_yuv_image_ps(
                            &frame.data,
                            (width * 2) as usize,
                            &mut yuv,
                        );
                        if yuv.len() == expected {
                            Some(yuv)
                        } else {
                            log::warn!(
                                "compressed color frame decompressed to {} bytes \
                                 (expected {expected}); raw {} bytes",
                                yuv.len(),
                                frame.data.len()
                            );
                            None
                        }
                    }
                } else if frame.corrupted {
                    None
                } else if frame.data.len() != expected {
                    log::warn!(
                        "dropping unusable color frame: {} bytes (expected \
                         {expected} for {width}x{height} YUV422)",
                        frame.data.len()
                    );
                    None
                } else {
                    Some(frame.data.clone())
                };
                match decoded {
                    Some(yuv) => {
                        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
                        yuv422_to_rgb888(&yuv, &mut rgb);
                        return {
                            let timestamp_us = self.convert_timestamp(frame.timestamp);
                            Ok(ColorFrame {
                                width,
                                height,
                                crop,
                                data: rgb,
                                timestamp: frame.timestamp,
                                timestamp_us,
                            })
                        };
                    }
                    None => {
                        unusable += 1;
                        log::warn!("dropping unusable color frame #{unusable}");
                        if unusable >= 5 {
                            return Err(XtionError::BadConfig(format!(
                                "color stream produces unusable frames: \
                                 frame #{unusable}; assembler {:?}, ring {:?}",
                                self.color_assembler.stats,
                                self.color_ring.as_ref().map(|ring| ring.stats()),
                            )));
                        }
                    }
                }
            }
            self.pump_misc();
            let chunk = {
                let ring = self
                    .color_ring
                    .as_ref()
                    .ok_or(XtionError::StreamNotRunning)?;
                ring.pop(deadline)
            };
            match chunk {
                Some(data) => self.color_assembler.feed(&data),
                None => {
                    if let Some(err) = self.color_ring.as_ref().unwrap().error() {
                        return Err(err);
                    }
                    if Instant::now() >= deadline {
                        // distinguishable from "unusable frames": a healthy
                        // ring with no frames means sensor timing was never
                        // armed, a dead ring means no data at all
                        let stats = self.color_ring.as_ref().unwrap().stats();
                        log::error!(
                            "no color frame within {timeout:?}: ring saw {} chunks / {} bytes / {} failures",
                            stats.completions,
                            stats.bytes,
                            stats.failures
                        );
                        return Err(XtionError::ControlTimeout);
                    }
                }
            }
        }
    }

    /// Block until one infrared frame is decoded (16-bit grayscale).
    ///
    /// The first frame of a stream can be partial (the bulk ring starts
    /// just after the stream-mode write), so a few unusable frames are
    /// dropped — but if the stream keeps producing frames of the wrong
    /// size, reading fails fast with the actual sizes and counters
    /// instead of silently looping into a timeout that hides the cause.
    pub fn read_ir_frame(&mut self, timeout: Duration) -> Result<IrFrame> {
        let config = self.ir_config.ok_or(XtionError::StreamNotRunning)?;
        let cmos = self.fixed_params.depth_cmos_type;
        // While a crop window is active the IR CMOS transmits exactly the
        // window — NO extra rows (the MT9M001 `height + 8` quirk applies
        // to full frames only; XnIRProcessor.cpp:249-255).
        let crop = self.ir_crop.map(|cs| cs.crop);
        let (width, height, expected, wire_rows, extra) = match crop {
            Some(c) => {
                let (w, h) = (c.width as u32, c.height as u32);
                let expected = crate::ir::expected_wire_size((w * h) as usize);
                (w, h, expected, h as i32, 0i32)
            }
            None => {
                let (w, h) = config.dimensions();
                let extra = wire_extra_rows(cmos, config.resolution);
                (
                    w,
                    h,
                    ir_wire_size(w, h, cmos, config.resolution),
                    h as i32 + extra,
                    extra,
                )
            }
        };
        let deadline = Instant::now() + timeout;
        let mut unusable = 0u32;

        loop {
            while let Some(frame) = self.ir_assembler.pop_frame() {
                let reason = if frame.corrupted {
                    Some("corrupted".to_string())
                } else if frame.data.len() != expected {
                    Some(format!(
                        "{} bytes (expected {expected} for {width}x{wire_rows} packed-10: \
                         {height} visible + {extra} CMOS rows, depth cmos {cmos})",
                        frame.data.len()
                    ))
                } else {
                    None
                };
                match reason {
                    None => {
                        let timestamp_us = self.convert_timestamp(frame.timestamp);
                        let mut frame = match crop {
                            Some(crop) => crate::ir::decode_ir_frame_cropped(
                                &frame.data,
                                crop,
                                frame.timestamp,
                            )?,
                            None => decode_ir_frame(
                                &frame.data,
                                width,
                                height,
                                config.resolution,
                                cmos,
                                frame.timestamp,
                            )?,
                        };
                        frame.timestamp_us = timestamp_us;
                        return Ok(frame);
                    }
                    Some(reason) => {
                        unusable += 1;
                        log::warn!("dropping unusable ir frame #{unusable}: {reason}");
                        if unusable >= 5 {
                            return Err(XtionError::BadConfig(format!(
                                "ir stream produces unusable frames: frame #{unusable} {reason}; \
                                 assembler {:?}, ring {:?}",
                                self.ir_assembler.stats,
                                self.ir_ring.as_ref().map(|ring| ring.stats()),
                            )));
                        }
                    }
                }
            }
            self.pump_misc();
            let chunk = {
                let ring = self.ir_ring.as_ref().ok_or(XtionError::StreamNotRunning)?;
                ring.pop(deadline)
            };
            match chunk {
                Some(data) => self.ir_assembler.feed(&data),
                None => {
                    if let Some(err) = self.ir_ring.as_ref().unwrap().error() {
                        return Err(err);
                    }
                    if Instant::now() >= deadline {
                        // make the two possible failure modes distinguishable
                        // from the log: no data at all (sensor timing was
                        // never armed) vs data but unframeable packets.
                        let stats = self.ir_ring.as_ref().unwrap().stats();
                        log::error!(
                            "no ir frame within {timeout:?}: ring saw {} chunks / {} bytes / {} failures",
                            stats.completions,
                            stats.bytes,
                            stats.failures
                        );
                        return Err(XtionError::ControlTimeout);
                    }
                }
            }
        }
    }

    fn decode_depth_frame(
        &self,
        frame: &crate::stream::AssembledFrame,
        width: u32,
        height: u32,
        expected_pixels: usize,
        config: DepthConfig,
    ) -> Result<DepthFrame> {
        let shifts: Vec<u16> = match config.input_format {
            DepthFormat::Uncompressed11Bit => {
                // "uncompressed 11 bit" is packed 11 bits on the wire
                let mut out = Vec::with_capacity(expected_pixels);
                unpack_packed11(&frame.data, &mut out);
                out
            }
            DepthFormat::Uncompressed16Bit => frame
                .data
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect(),
            other => {
                return Err(XtionError::BadConfig(format!(
                    "depth input format {other:?} not supported in 0.1"
                )))
            }
        };
        let data = self.shift_to_depth.convert_shifts(&shifts);
        let shift_stats = ShiftStats::from_shifts(&shifts);
        Ok(DepthFrame {
            width,
            height,
            // set by the caller (read_depth_frame) from the active crop
            crop: None,
            data,
            timestamp: frame.timestamp,
            timestamp_us: None,
            shift_stats,
        })
    }

    /// The shift-to-depth tables built from the device calibration.
    pub fn shift_to_depth(&self) -> &ShiftToDepthTables {
        &self.shift_to_depth
    }

    /// The ConstShift calibration constant read from the device.
    pub fn depth_const_shift(&self) -> u16 {
        self.depth_const_shift
    }
}

impl Drop for Xtion {
    /// Port of the reference driver's device-shutdown stream kill
    /// (XnSensor.cpp:315-317): every OpenNI session ends with all three
    /// stream modes written to OFF, which is what keeps the firmware's
    /// "stream-mode changes must go through OFF" rule satisfied for the
    /// NEXT session. Best effort only — if the device is already gone
    /// the errors are ignored.
    fn drop(&mut self) {
        // Cancel the URB rings first so nothing is pending while the
        // firmware stops sending.
        self.depth_ring = None;
        self.color_ring = None;
        self.ir_ring = None;
        self.misc_ring = None;
        for param in [Param::Stream0Mode, Param::Stream1Mode, Param::Stream2Mode] {
            let _ = set_param_via(
                &mut self.link,
                &mut self.usb,
                param,
                STREAM_MODE_OFF,
                Duration::from_millis(500),
            );
        }
    }
}

/// Write one firmware parameter over any control transport
/// (the transport-generic core of [`Xtion::set_param`], so the exact
/// request sequences are unit-testable without hardware).
fn set_param_via<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    param: Param,
    value: u16,
    timeout: Duration,
) -> Result<()> {
    let mut payload = Vec::with_capacity(4);
    payload.extend_from_slice(&param.as_u16().to_le_bytes());
    payload.extend_from_slice(&value.to_le_bytes());
    link.exchange(transport, Opcode::SetParam, &payload, timeout)?;
    Ok(())
}

/// Crop write sequence, transport-generic for testing.
///
/// Port of `SetCroppingImpl` — the sequence is identical for all three
/// streams (XnSensorDepthStream.cpp:768-840, XnSensorImageStream.cpp:
/// 541-612, XnSensorIRStream.cpp:304-380): when ENABLING, the geometry
/// goes first — size X, size Y, offset X, offset Y — and the mode param
/// LAST; when DISABLING, only the mode param is written, geometry
/// untouched. Because the mode is written last, a NACK part-way through
/// leaves the previously active window in charge (the reference relies
/// on the same ordering for its rollback).
fn write_crop_via<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    params: CropParams,
    state: Option<CropState>,
    timeout: Duration,
) -> Result<()> {
    if let Some(cs) = state {
        set_param_via(link, transport, params.size_x, cs.crop.width, timeout)?;
        set_param_via(link, transport, params.size_y, cs.crop.height, timeout)?;
        set_param_via(link, transport, params.offset_x, cs.crop.x, timeout)?;
        set_param_via(link, transport, params.offset_y, cs.crop.y, timeout)?;
        set_param_via(link, transport, params.mode, cs.mode, timeout)?;
    } else {
        set_param_via(link, transport, params.mode, CROP_MODE_DISABLED, timeout)?;
    }
    Ok(())
}

/// Color (image) stream write sequence, transport-generic for testing.
///
/// Port of `XnSensorImageStream::ConfigureStreamImpl` +
/// `XnSensorStreamHelper::Open` + `OpenStreamImpl` write-for-write:
/// stream0 OFF baseline -> format -> resolution -> fps -> flicker ->
/// firmware mirror -> CMOS blanking read (SetCmosConfig parity) ->
/// device-frequency read (helper parity) -> stream-on -> crop mode ->
/// auto exposure -> auto white balance. The flicker/mirror writes and
/// the trailing crop/AE/AWB writes are in the reference sequence
/// (XnSensorImageStream.cpp:298-321, 356-388); the AE/AWB defaults on
/// fw >= 5.8.9 (bImageAdjustmentsSupported) are TRUE (1).
fn configure_color_via<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    resolution: Resolution,
    fps: u16,
    stream_on_value: u16,
    color_crop: Option<CropState>,
    timeout: Duration,
) -> Result<()> {
    // Stream-mode changes go through OFF first (see firmware_init_reset
    // step 7; reference guard XnSensorFirmwareParams::SetStreamMode).
    set_param_via(
        link,
        transport,
        Param::Stream0Mode,
        STREAM_MODE_OFF,
        timeout,
    )?;
    // ConfigureStreamImpl: flicker (param 17, default 0) and firmware
    // mirror (param 71, default off) are written on every configure.
    set_param_via(link, transport, Param::ImageFlickerDetection, 0, timeout)?;
    set_param_via(link, transport, Param::ImageMirror, 0, timeout)?;
    // CMOS timing: the reference image stream arms the color CMOS with
    // a blanking read before the mode is written
    // (ConfigureStreamImpl -> SetCmosConfig(XN_CMOS_TYPE_IMAGE, ...)).
    read_cmos_blanking(link, transport, resolution, fps, timeout)?;
    // The helper reads the device clock between configure and open
    // (XnSensorStreamHelper::Open).
    read_device_frequency(link, transport, timeout)?;
    // The stream-on write.
    set_param_via(
        link,
        transport,
        Param::Stream0Mode,
        stream_on_value,
        timeout,
    )?;
    // OpenStreamImpl tail (XnSensorImageStream.cpp:352-379): the crop
    // geometry is re-applied when a window is active, then the crop
    // mode — always written; fw >= 5.8.9 also gets auto exposure +
    // auto white balance, whose reference defaults are TRUE
    // (XnSensorImageStream.h:47-53).
    write_crop_via(link, transport, IMAGE_CROP_PARAMS, color_crop, timeout)?;
    set_param_via(link, transport, Param::ImageAutoExposureMode, 1, timeout)?;
    set_param_via(
        link,
        transport,
        Param::ImageAutoWhiteBalanceMode,
        1,
        timeout,
    )?;
    Ok(())
}

/// IR stream parameter sequence, transport-generic for testing.
///
/// Port of `XnSensorIRStream::ConfigureStreamImpl` + `MapPropertiesToFirmware`:
/// resolution -> fps -> mirror forced off -> stream0 into IR mode.
fn configure_ir_via<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    config: IrConfig,
    ir_crop: Option<CropState>,
    timeout: Duration,
) -> Result<()> {
    // Stream-mode changes go through OFF first (see firmware_init_reset
    // step 7; reference guard XnSensorFirmwareParams::SetStreamMode).
    set_param_via(
        link,
        transport,
        Param::Stream0Mode,
        STREAM_MODE_OFF,
        timeout,
    )?;
    set_param_via(
        link,
        transport,
        Param::IrResolution,
        config.resolution.as_u16(),
        timeout,
    )?;
    set_param_via(link, transport, Param::IrFps, config.fps, timeout)?;
    // "IR mirror is always off in firmware" — the reference driver
    // mirrors IR host-side instead.
    set_param_via(link, transport, Param::IrMirror, 0, timeout)?;
    // CMOS timing: the reference IR stream arms the depth CMOS (the IR
    // sensor) with a blanking read for the requested mode before the
    // stream mode is written (XnSensorIRStream::ConfigureStreamImpl
    // -> SetCmosConfig(XN_CMOS_TYPE_DEPTH, ...)). Without it real
    // hardware ACKs every command but never produces IR packets.
    read_cmos_blanking(link, transport, config.resolution, config.fps, timeout)?;
    set_param_via(link, transport, Param::Stream0Mode, STREAM_MODE_IR, timeout)?;
    // OpenStreamImpl tail (XnSensorIRStream.cpp:183-203): crop geometry
    // when a window is active, then the crop mode — always written.
    // (The reference flips offset X for mirrored IR because it mirrors
    // IR host-side AFTER cropping; our driver leaves the firmware IR
    // mirror off — `configure_ir_via` forces param 72 to 0 — so the
    // window coordinates pass through unflipped.)
    write_crop_via(link, transport, IR_CROP_PARAMS, ir_crop, timeout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::command::ControlTransport;
    use crate::protocol::{Opcode, FW_MAGIC_V26};
    use std::time::Duration;

    struct FakeTransport {
        replies: Vec<Vec<u8>>,
        sent: Vec<Vec<u8>>,
    }

    impl ControlTransport for FakeTransport {
        fn control_out(&mut self, data: &[u8], _t: Duration) -> Result<usize> {
            self.sent.push(data.to_vec());
            Ok(data.len())
        }
        fn control_in(&mut self, buf: &mut [u8], _t: Duration) -> Result<usize> {
            let reply = self.replies.remove(0);
            buf[..reply.len()].copy_from_slice(&reply);
            Ok(reply.len())
        }
    }

    /// `XnVersions` bytes as the device sends them (wire order!):
    /// byte 0 = minor, byte 1 = major (the C byte-swaps the first u16),
    /// build is the raw hex value, chip at 4, FPGA at 8, system at 10.
    fn version_struct_bytes() -> [u8; 33] {
        let mut b = [0u8; 33];
        b[0] = 3; // minor (wire first!)
        b[1] = 5; // major
        b[2..4].copy_from_slice(&51u16.to_le_bytes()); // build 0x33 -> "33"
        b[4..8].copy_from_slice(&0x0020_2020u32.to_le_bytes()); // PS1080
        b[8..10].copy_from_slice(&5u16.to_le_bytes()); // FPGA RD1082
        b[10..12].copy_from_slice(&7u16.to_le_bytes()); // system version
        b
    }

    fn version_reply(id: u16, struct_bytes: &[u8]) -> Vec<u8> {
        // pad to a whole word, like the device
        let mut data = struct_bytes.to_vec();
        if data.len() % 2 == 1 {
            data.push(0);
        }
        let mut v = Vec::new();
        v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
        v.extend_from_slice(&((data.len() / 2 + 1) as u16).to_le_bytes());
        v.extend_from_slice(&Opcode::GetVersion.as_u16().to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(&data);
        v
    }

    #[test]
    fn version_parse_applies_wire_quirks() {
        let v = DeviceVersion::parse(&version_struct_bytes()).unwrap();
        assert_eq!(v.major, 5);
        assert_eq!(v.minor, 3);
        assert_eq!(v.build, 33, "hex-build quirk: raw 0x33 -> 33");
        assert_eq!(v.chip, 0x0020_2020);
        assert_eq!(v.fpga, 5);
        assert_eq!(v.system_version, 7);
        assert!(v.uses_protocol_v26());
    }

    #[test]
    fn version_parse_tolerates_base_struct_only() {
        // some firmwares reply with only the 12-byte base struct; the C
        // zero-pads via memcpy and only uses those fields
        let bytes = version_struct_bytes();
        let v = DeviceVersion::parse(&bytes[..12]).unwrap();
        assert_eq!(v.major, 5);
        assert_eq!(v.minor, 3);
        assert_eq!(v.build, 33);
        assert_eq!(v.fpga, 5);
    }

    #[test]
    fn version_parse_rejects_garbage() {
        let err = DeviceVersion::parse(&[0x01, 0x02]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("too short"), "{msg}");
        assert!(
            msg.contains("01 02"),
            "raw bytes must be in the error: {msg}"
        );
    }

    /// The exact scenario of the first hardware run: the version reply
    /// arrives split across two control transfers (first one carries only
    /// 12 data bytes). Handshake must succeed and yield the right version.
    #[test]
    fn handshake_survives_split_version_reply() {
        let full = version_reply(0, &version_struct_bytes());
        assert_eq!(full.len(), 8 + 2 + 34);
        let first = full[..8 + 2 + 12].to_vec();
        let second = full[8 + 2 + 12..].to_vec();
        let mut fake = FakeTransport {
            replies: vec![first, second],
            sent: vec![],
        };
        let (link, version) = Xtion::handshake(&mut fake).unwrap();
        assert_eq!(link.protocol(), ProtocolVersion::V26);
        assert_eq!(version.major, 5);
        assert_eq!(version.minor, 3);
        assert_eq!(version.build, 33);
        assert_eq!(version.chip, 0x0020_2020);
    }

    #[test]
    fn handshake_falls_back_to_v25() {
        // A device that refuses the v2.6 dialect: both "MG" attempts are
        // NACKed, then the v2.5 (PS/SP) fallback succeeds.
        // (Takes ~2 s: the documented retry delay between the two v2.6
        // attempts.)
        let nack_v26 = |id: u16| -> Vec<u8> {
            // v2.6 header has no CRC field: magic, size, opcode, id, error
            let mut v = Vec::new();
            v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
            v.extend_from_slice(&1u16.to_le_bytes()); // 1 word: error only
            v.extend_from_slice(&Opcode::GetVersion.as_u16().to_le_bytes());
            v.extend_from_slice(&id.to_le_bytes());
            v.extend_from_slice(&1u16.to_le_bytes()); // NACK unknown error
            v
        };

        let mut data = version_struct_bytes().to_vec();
        data.push(0); // word pad
        let mut v25 = Vec::new();
        v25.extend_from_slice(&crate::protocol::FW_MAGIC_V25.to_le_bytes());
        v25.extend_from_slice(&((data.len() / 2 + 1) as u16).to_le_bytes());
        v25.extend_from_slice(&Opcode::GetVersion.as_u16().to_le_bytes());
        v25.extend_from_slice(&0u16.to_le_bytes()); // id 0
        v25.extend_from_slice(&0u16.to_le_bytes()); // crc
        v25.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v25.extend_from_slice(&data);

        // each GetVersion attempt carries the next request id
        let mut fake = FakeTransport {
            replies: vec![nack_v26(0), nack_v26(1), v25],
            sent: vec![],
        };
        let (link, version) = Xtion::handshake(&mut fake).unwrap();
        assert_eq!(link.protocol(), ProtocolVersion::V25);
        assert_eq!(version.major, 5);
        assert_eq!(version.minor, 3);
    }

    // ------------------------------------------------------------------
    // Phase 1-2 feature wire tests (docs/FEATURE_PLAN.md)
    // ------------------------------------------------------------------

    /// ACK reply for a SET_PARAM (the value is echoed back).
    fn setparam_ack_reply(id: u16, value: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes()); // error + value word
        v.extend_from_slice(&Opcode::SetParam.as_u16().to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(&value.to_le_bytes());
        v
    }

    /// Generic ALGORITHM_PARAMS reply carrying `data_bytes` payload bytes
    /// (error word + data), for the given request id.
    fn algorithm_reply(id: u16, data_bytes: usize) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
        v.extend_from_slice(&((1 + data_bytes / 2) as u16).to_le_bytes());
        v.extend_from_slice(&Opcode::AlgorithmParams.as_u16().to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend(std::iter::repeat(0u8).take(data_bytes));
        v
    }

    /// Wire-contract guard: every Param discriminant must equal the
    /// vendor's EPsProtocolParams id (XnParams.h), so a typo here can
    /// never silently configure the wrong thing.
    #[test]
    fn param_ids_match_xnparams() {
        assert_eq!(Param::RegistrationEnable.as_u16(), 2);
        assert_eq!(Param::Stream0Mode.as_u16(), 5);
        assert_eq!(Param::Stream1Mode.as_u16(), 6);
        assert_eq!(Param::Stream2Mode.as_u16(), 7);
        assert_eq!(Param::ImageAgc.as_u16(), 15);
        assert_eq!(Param::ImageQuality.as_u16(), 16);
        assert_eq!(Param::ImageFlickerDetection.as_u16(), 17);
        assert_eq!(Param::DepthAgc.as_u16(), 21);
        assert_eq!(Param::DepthHoleFilter.as_u16(), 22);
        assert_eq!(Param::DepthMirror.as_u16(), 23);
        assert_eq!(Param::IrResolution.as_u16(), 26);
        assert_eq!(Param::IrFps.as_u16(), 27);
        assert_eq!(Param::DepthGmcMode.as_u16(), 36);
        assert_eq!(Param::ImageCropSizeX.as_u16(), 46);
        assert_eq!(Param::ImageCropSizeY.as_u16(), 47);
        assert_eq!(Param::ImageCropOffsetX.as_u16(), 48);
        assert_eq!(Param::ImageCropOffsetY.as_u16(), 49);
        assert_eq!(Param::ImageCropMode.as_u16(), 50);
        assert_eq!(Param::DepthCropSizeX.as_u16(), 51);
        assert_eq!(Param::DepthCropSizeY.as_u16(), 52);
        assert_eq!(Param::DepthCropOffsetX.as_u16(), 53);
        assert_eq!(Param::DepthCropOffsetY.as_u16(), 54);
        assert_eq!(Param::DepthCropMode.as_u16(), 55);
        assert_eq!(Param::IrCropSizeX.as_u16(), 56);
        assert_eq!(Param::IrCropSizeY.as_u16(), 57);
        assert_eq!(Param::IrCropOffsetX.as_u16(), 58);
        assert_eq!(Param::IrCropOffsetY.as_u16(), 59);
        assert_eq!(Param::IrCropMode.as_u16(), 60);
        assert_eq!(Param::ImageMirror.as_u16(), 71);
        assert_eq!(Param::IrMirror.as_u16(), 72);
        assert_eq!(Param::ImageAutoWhiteBalanceMode.as_u16(), 77);
        assert_eq!(Param::ImageAutoExposureMode.as_u16(), 80);
        assert_eq!(Param::ImageExposureBar.as_u16(), 81);
        assert_eq!(Param::DepthCloseRange.as_u16(), 84);
    }

    #[test]
    fn set_param_sends_param_and_value_words() {
        let (mut link, mut fake) = (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies: vec![setparam_ack_reply(0, 1)],
                sent: vec![],
            },
        );
        set_param_via(
            &mut link,
            &mut fake,
            Param::DepthCloseRange,
            1,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(fake.sent.len(), 1);
        let req = &fake.sent[0];
        assert_eq!(req.len(), 8 + 4);
        assert_eq!(
            u16::from_le_bytes([req[4], req[5]]),
            Opcode::SetParam.as_u16()
        );
        // payload: param id then value
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 84);
        assert_eq!(u16::from_le_bytes([req[10], req[11]]), 1);
    }

    /// The IR start sequence must match XnSensorIRStream::ConfigureStreamImpl
    /// plus OpenStreamImpl: stream0 OFF baseline, IrResolution, IrFps,
    /// IrMirror forced off, the CMOS blanking read (SetCmosConfig parity)
    /// and stream0 into IR mode.
    #[test]
    fn configure_ir_sequence_matches_reference() {
        let (mut link, mut fake) = (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies: vec![
                    setparam_ack_reply(0, 0),
                    setparam_ack_reply(1, 1),
                    setparam_ack_reply(2, 30),
                    setparam_ack_reply(3, 0),
                    algorithm_reply(4, 16),
                    setparam_ack_reply(5, 3),
                    setparam_ack_reply(6, 0), // IrCropMode = 0
                ],
                sent: vec![],
            },
        );
        configure_ir_via(
            &mut link,
            &mut fake,
            IrConfig {
                resolution: Resolution::Vga,
                fps: 30,
            },
            None,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(fake.sent.len(), 7);
        // SET_PARAMs ...
        let params: Vec<(u16, u16)> = fake.sent[..4]
            .iter()
            .chain(fake.sent[5..7].iter())
            .map(|req| {
                (
                    u16::from_le_bytes([req[8], req[9]]),
                    u16::from_le_bytes([req[10], req[11]]),
                )
            })
            .collect();
        assert_eq!(
            params,
            vec![
                (5, 0),   // PARAM_GENERAL_STREAM0_MODE = OFF baseline
                (26, 1),  // PARAM_IR_RESOLUTION = VGA
                (27, 30), // PARAM_IR_FPS
                (72, 0),  // PARAM_IR_MIRROR forced off
                (5, 3),   // PARAM_GENERAL_STREAM0_MODE = IR
                (60, 0),  // PARAM_IR_CROP_MODE = disabled (OpenStreamImpl)
            ]
        );
        // ... then the CMOS blanking read between mirror and stream mode
        let req = &fake.sent[4];
        assert_eq!(
            u16::from_le_bytes([req[4], req[5]]),
            Opcode::AlgorithmParams.as_u16()
        );
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 0x06); // BLANKING
        assert_eq!(u16::from_le_bytes([req[10], req[11]]), 0); // format
        assert_eq!(u16::from_le_bytes([req[12], req[13]]), 1); // VGA
        assert_eq!(u16::from_le_bytes([req[14], req[15]]), 30); // fps
        assert_eq!(u16::from_le_bytes([req[16], req[17]]), 0); // offset words
    }

    /// The color start sequence must match XnSensorImageStream::
    /// ConfigureStreamImpl + XnSensorStreamHelper::Open + OpenStreamImpl
    /// write-for-write: stream0 OFF baseline, format, resolution, fps,
    /// flicker, firmware mirror, CMOS blanking read, device-frequency
    /// read, stream0 COLOR, crop mode disabled, auto exposure on, auto
    /// white balance on (fw >= 5.8.9 defaults).
    #[test]
    fn configure_color_sequence_matches_reference() {
        let (mut link, mut fake) = (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies: vec![
                    setparam_ack_reply(0, 0), // OFF baseline
                    setparam_ack_reply(1, 0), // flicker
                    setparam_ack_reply(2, 0), // mirror
                    algorithm_reply(3, 16),   // blanking
                    algorithm_reply(4, 4),    // frequency
                    setparam_ack_reply(5, 1), // stream0 = COLOR
                    setparam_ack_reply(6, 0), // crop mode
                    setparam_ack_reply(7, 1), // auto exposure
                    setparam_ack_reply(8, 1), // auto white balance
                ],
                sent: vec![],
            },
        );
        configure_color_via(
            &mut link,
            &mut fake,
            Resolution::Vga,
            30,
            STREAM_MODE_COLOR,
            None,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(fake.sent.len(), 9);
        let mut params: Vec<(u16, u16)> = Vec::new();
        let mut algorithms = Vec::new();
        for req in &fake.sent {
            let opcode = u16::from_le_bytes([req[4], req[5]]);
            if opcode == Opcode::SetParam.as_u16() {
                params.push((
                    u16::from_le_bytes([req[8], req[9]]),
                    u16::from_le_bytes([req[10], req[11]]),
                ));
            } else {
                assert_eq!(opcode, Opcode::AlgorithmParams.as_u16());
                algorithms.push(u16::from_le_bytes([req[8], req[9]]));
            }
        }
        assert_eq!(
            params,
            vec![
                (5, 0),  // stream0 OFF baseline
                (17, 0), // flicker default
                (71, 0), // firmware mirror off
                (5, 1),  // stream0 = COLOR (the stream-on write)
                (50, 0), // crop mode disabled
                (80, 1), // auto exposure on
                (77, 1), // auto white balance on
            ]
        );
        // NOTE: format/res/fps (12/13/14) are written by start_color
        // BEFORE configure_color_via, matching the C where they are
        // configured as stream properties.
        assert_eq!(algorithms, vec![0x06, 0x80]); // BLANKING, FREQUENCY
                                                  // frequency request uses resolution 0 / fps 0 like the C
        let freq_req = &fake.sent[4];
        assert_eq!(u16::from_le_bytes([freq_req[12], freq_req[13]]), 0);
        assert_eq!(u16::from_le_bytes([freq_req[14], freq_req[15]]), 0);
    }

    // ------------------------------------------------------------------
    // Crop windows (0.3.3)
    // ------------------------------------------------------------------

    /// The crop write must match `SetCroppingImpl` write-for-write:
    /// geometry first (size X, size Y, offset X, offset Y), the mode
    /// param LAST; disabling writes ONLY the mode (XnSensorDepthStream
    /// .cpp:788-805 — image and IR identical).
    #[test]
    fn crop_write_sequence_matches_reference() {
        let crop = Crop {
            x: 160,
            y: 120,
            width: 320,
            height: 240,
        };
        let state = CropState {
            crop,
            mode: CROP_MODE_NORMAL,
        };
        let (mut link, mut fake) = (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies: vec![
                    setparam_ack_reply(0, 320),
                    setparam_ack_reply(1, 240),
                    setparam_ack_reply(2, 160),
                    setparam_ack_reply(3, 120),
                    setparam_ack_reply(4, CROP_MODE_NORMAL),
                ],
                sent: vec![],
            },
        );
        write_crop_via(
            &mut link,
            &mut fake,
            DEPTH_CROP_PARAMS,
            Some(state),
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(fake.sent.len(), 5);
        let params: Vec<(u16, u16)> = fake
            .sent
            .iter()
            .map(|req| {
                (
                    u16::from_le_bytes([req[8], req[9]]),
                    u16::from_le_bytes([req[10], req[11]]),
                )
            })
            .collect();
        assert_eq!(
            params,
            vec![
                (51, 320), // PARAM_DEPTH_CROP_SIZE_X
                (52, 240), // PARAM_DEPTH_CROP_SIZE_Y
                (53, 160), // PARAM_DEPTH_CROP_OFFSET_X
                (54, 120), // PARAM_DEPTH_CROP_OFFSET_Y
                (55, 1),   // PARAM_DEPTH_CROP_MODE = NORMAL
            ]
        );

        // Disabling: one write, the mode only.
        let (mut link, mut fake) = (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies: vec![setparam_ack_reply(0, 0)],
                sent: vec![],
            },
        );
        write_crop_via(
            &mut link,
            &mut fake,
            IR_CROP_PARAMS,
            None,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(fake.sent.len(), 1);
        assert_eq!(
            u16::from_le_bytes([fake.sent[0][8], fake.sent[0][9]]),
            Param::IrCropMode.as_u16()
        );
        assert_eq!(
            u16::from_le_bytes([fake.sent[0][10], fake.sent[0][11]]),
            CROP_MODE_DISABLED
        );
    }

    /// Bounds and packing validation (`XnPixelStream::ValidateCropping`
    /// parity + the per-format pixel-group constraint).
    #[test]
    fn crop_validation_rejects_bad_windows() {
        let ok = Crop {
            x: 160,
            y: 120,
            width: 320,
            height: 240,
        };
        ok.validate(640, 480, 8).unwrap();
        ok.validate(640, 480, 1).unwrap();

        // zero-size
        assert!(Crop {
            x: 0,
            y: 0,
            width: 0,
            height: 100
        }
        .validate(640, 480, 1)
        .is_err());
        // out of frame
        assert!(Crop {
            x: 400,
            y: 0,
            width: 320,
            height: 240
        }
        .validate(640, 480, 1)
        .is_err());
        assert!(Crop {
            x: 0,
            y: 400,
            width: 320,
            height: 240
        }
        .validate(640, 480, 1)
        .is_err());
        // exactly full frame is allowed
        Crop {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        }
        .validate(640, 480, 1)
        .unwrap();
        // packed-11: 161x121 = 19481 pixels is not divisible by 8
        assert!(Crop {
            x: 0,
            y: 0,
            width: 161,
            height: 121
        }
        .validate(640, 480, 8)
        .is_err());
        // packed-10 (IR): 322x240 = 77280 IS divisible by 4 -> fine
        Crop {
            x: 0,
            y: 0,
            width: 322,
            height: 240,
        }
        .validate(640, 480, 4)
        .unwrap();
    }

    /// A cropped frame's `to_world` must shift pixels by the window
    /// origin, so world coordinates stay full-frame consistent
    /// (photogrammetry on a region of interest).
    #[test]
    fn cropped_frame_to_world_shifts_by_origin() {
        let intrinsics = crate::geometry::CameraIntrinsics {
            width: 640,
            height: 480,
            fx: 570.0,
            fy: 570.0,
            cx: 320.0,
            cy: 240.0,
            hfov: 1.0,
            vfov: 0.8,
            zpd: 120.0,
            zpps: 0.1052,
        };
        let crop = Crop {
            x: 160,
            y: 120,
            width: 320,
            height: 240,
        };
        let frame = DepthFrame {
            width: 320,
            height: 240,
            crop: Some(crop),
            data: vec![1000; 320 * 240],
            timestamp: 0,
            timestamp_us: None,
            shift_stats: ShiftStats::default(),
        };
        // pixel (10, 20) of the window is full-frame (170, 140)
        let (x, y, z) = frame.to_world(&intrinsics, 10, 20).unwrap();
        let (ex, ey, ez) = intrinsics.to_world(170, 140, 1000).unwrap();
        assert!((x - ex).abs() < 1e-9);
        assert!((y - ey).abs() < 1e-9);
        assert!((z - ez).abs() < 1e-9);
        // and a full frame behaves as before
        let full = DepthFrame {
            width: 640,
            height: 480,
            crop: None,
            data: vec![1000; 640 * 480],
            timestamp: 0,
            timestamp_us: None,
            shift_stats: ShiftStats::default(),
        };
        let (x, _, _) = full.to_world(&intrinsics, 10, 20).unwrap();
        let (ex, _, _) = intrinsics.to_world(10, 20, 1000).unwrap();
        assert!((x - ex).abs() < 1e-9);
    }
}
