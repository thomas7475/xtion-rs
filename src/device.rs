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
use crate::error::{Result, XtionError};
use crate::ir::{decode_ir_frame, expected_wire_size, IrConfig, IrFrame};
use crate::protocol::algorithm::{read_cmos_blanking, read_depth_information};
use crate::protocol::command::{CommandLink, ControlTransport, ProtocolVersion};
use crate::protocol::fixed_params::{read_fixed_params, FixedParams};
use crate::protocol::identity::{read_platform_string, read_usb_core_type, set_emitter_state};
use crate::protocol::{
    DepthFormat, ImageFormat, Opcode, Param, Resolution, UsbCore, SHIFTS_PARAM_COEFFICIENT,
    SHIFTS_SHIFT_SCALE, STREAM_MODE_COLOR, STREAM_MODE_DEPTH, STREAM_MODE_IR, STREAM_MODE_OFF,
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

/// A decoded depth frame: metric depth in millimeters.
#[derive(Debug, Clone)]
pub struct DepthFrame {
    pub width: u32,
    pub height: u32,
    /// Depth in millimeters per pixel (row-major); 0 = invalid.
    pub data: Vec<u16>,
    /// Raw device timestamp of the frame's last packet.
    pub timestamp: u32,
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
}

/// A decoded color frame: interleaved RGB888.
#[derive(Debug, Clone)]
pub struct ColorFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub timestamp: u32,
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
    color_config: Option<Resolution>,
    ir_config: Option<IrConfig>,
    depth_assembler: StreamAssembler,
    color_assembler: StreamAssembler,
    ir_assembler: StreamAssembler,
    misc_assembler: StreamAssembler,
}

impl Xtion {
    /// Open the first Xtion device and run the full initialization:
    /// version handshake, fixed params, depth calibration, S2D tables.
    pub fn open() -> Result<Xtion> {
        let mut usb = UsbDevice::open_first()?;

        // Version handshake (port of XnDeviceSensorConfigureVersion):
        // try protocol v2.6 first; retry once after the documented 2 s
        // delay on a timeout; fall back to v2.5 if that fails.
        let (link, version) = Self::handshake(&mut usb)?;

        let mut link = link;
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
            depth_assembler: StreamAssembler::new(fw_magic, ignore),
            color_assembler: StreamAssembler::new(fw_magic, ignore),
            ir_assembler: StreamAssembler::new(fw_magic, ignore),
            misc_assembler: StreamAssembler::new(fw_magic, ignore),
            usb,
            link,
            version,
            fixed_params: fixed,
            depth_const_shift: depth_info.const_shift,
            shift_to_depth,
            depth_config: None,
            color_config: None,
            ir_config: None,
        })
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
                log::warn!("usb core probe failed ({err}); assuming Jango core like the reference driver");
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
    pub fn set_emitter_enabled(&mut self, active: bool) -> Result<()> {
        set_emitter_state(&mut self.link, &mut self.usb, active, COMMAND_TIMEOUT)
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

    /// Configure and start the depth stream (stream1 in firmware terms).
    ///
    /// Parameter order mirrors the reference driver's property pipeline:
    /// format -> resolution -> fps -> stream mode.
    pub fn start_depth(&mut self, config: DepthConfig) -> Result<()> {
        self.set_param(Param::DepthFormat, config.input_format.as_u16())?;
        self.set_param(Param::DepthResolution, config.resolution.as_u16())?;
        self.set_param(Param::DepthFps, config.fps)?;
        // CMOS timing: the reference depth stream ends its configuration
        // with SetCmosConfig(XN_CMOS_TYPE_DEPTH, ...) — a blanking read
        // for the requested mode (XnSensorDepthStream::ConfigureStreamImpl).
        read_cmos_blanking(&mut self.link, &mut self.usb, config.resolution, config.fps, COMMAND_TIMEOUT)?;
        self.set_param(Param::Stream1Mode, STREAM_MODE_DEPTH)?;

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
    /// Only YUV422 input formats are supported in 0.1. The color camera
    /// and the IR stream share firmware stream0, so they cannot run at
    /// the same time (stop one before starting the other).
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
        match format {
            ImageFormat::UncompressedYuv422 | ImageFormat::Yuv422 => {}
            _ => {
                return Err(XtionError::BadConfig(
                    "only YUV422 input formats are supported in 0.1".into(),
                ))
            }
        }
        self.set_param(Param::ImageFormat, format.as_u16())?;
        self.set_param(Param::ImageResolution, resolution.as_u16())?;
        self.set_param(Param::ImageFps, fps)?;
        // CMOS timing: the reference image stream arms the color CMOS with
        // a blanking read before the mode is written
        // (XnSensorImageStream::ConfigureStreamImpl -> SetCmosConfig).
        read_cmos_blanking(&mut self.link, &mut self.usb, resolution, fps, COMMAND_TIMEOUT)?;
        self.set_param(Param::Stream0Mode, STREAM_MODE_COLOR)?;

        self.color_ring = Some(crate::ring::BulkRing::start(
            self.usb.raw_context(),
            self.usb.raw_handle(),
            self.usb.endpoints.image,
            self.usb.image_max_packet_size,
            crate::ring::RING_BUFFERS,
        )?);
        self.ensure_misc_ring();

        self.color_config = Some(resolution);
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
        configure_ir_via(&mut self.link, &mut self.usb, config, COMMAND_TIMEOUT)?;

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
        let (width, height) = config.dimensions();
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
                return self.decode_depth_frame(&frame, width, height, expected_pixels, config);
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
    pub fn read_color_frame(&mut self, timeout: Duration) -> Result<ColorFrame> {
        let resolution = self.color_config.ok_or(XtionError::StreamNotRunning)?;
        let (width, height) = resolution.dimensions();
        let deadline = Instant::now() + timeout;

        loop {
            while let Some(frame) = self.color_assembler.pop_frame() {
                if frame.corrupted {
                    log::warn!("corrupted color frame dropped");
                    continue;
                }
                let mut rgb = Vec::with_capacity((width * height * 3) as usize);
                yuv422_to_rgb888(&frame.data, &mut rgb);
                return Ok(ColorFrame {
                    width,
                    height,
                    data: rgb,
                    timestamp: frame.timestamp,
                });
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
                        return Err(XtionError::ControlTimeout);
                    }
                }
            }
        }
    }

    /// Block until one infrared frame is decoded (16-bit grayscale).
    /// Corrupted frames are dropped and reading continues.
    pub fn read_ir_frame(&mut self, timeout: Duration) -> Result<IrFrame> {
        let config = self.ir_config.ok_or(XtionError::StreamNotRunning)?;
        let (width, height) = config.dimensions();
        let expected = expected_wire_size((width * height) as usize);
        let deadline = Instant::now() + timeout;

        loop {
            while let Some(frame) = self.ir_assembler.pop_frame() {
                if frame.corrupted {
                    log::warn!("corrupted ir frame dropped");
                    continue;
                }
                if frame.data.len() != expected {
                    log::warn!(
                        "ir frame is corrupt: {} bytes != expected {expected}, dropped",
                        frame.data.len()
                    );
                    continue;
                }
                return decode_ir_frame(&frame.data, width, height, frame.timestamp);
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
            data,
            timestamp: frame.timestamp,
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

/// IR stream parameter sequence, transport-generic for testing.
///
/// Port of `XnSensorIRStream::ConfigureStreamImpl` + `MapPropertiesToFirmware`:
/// resolution -> fps -> mirror forced off -> stream0 into IR mode.
fn configure_ir_via<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    config: IrConfig,
    timeout: Duration,
) -> Result<()> {
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
    /// + OpenStreamImpl: IrResolution -> IrFps -> IrMirror forced off ->
    /// CMOS blanking read (SetCmosConfig parity) -> stream0 into IR mode.
    #[test]
    fn configure_ir_sequence_matches_reference() {
        let (mut link, mut fake) = (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies: vec![
                    setparam_ack_reply(0, 1),
                    setparam_ack_reply(1, 30),
                    setparam_ack_reply(2, 0),
                    algorithm_reply(3, 16),
                    setparam_ack_reply(4, 3),
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
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(fake.sent.len(), 5);
        // three SET_PARAMs ...
        let params: Vec<(u16, u16)> = fake.sent[..3]
            .iter()
            .chain(fake.sent[4..5].iter())
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
                (26, 1),  // PARAM_IR_RESOLUTION = VGA
                (27, 30), // PARAM_IR_FPS
                (72, 0),  // PARAM_IR_MIRROR forced off
                (5, 3),   // PARAM_GENERAL_STREAM0_MODE = IR
            ]
        );
        // ... then the CMOS blanking read between mirror and stream mode
        let req = &fake.sent[3];
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
}
