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
use crate::protocol::algorithm::read_depth_information;
use crate::protocol::command::{CommandLink, ProtocolVersion};
use crate::protocol::fixed_params::{read_fixed_params, FixedParams};
use crate::protocol::{
    DepthFormat, ImageFormat, Opcode, Param, Resolution, SHIFTS_PARAM_COEFFICIENT,
    SHIFTS_SHIFT_SCALE, STREAM_MODE_COLOR, STREAM_MODE_DEPTH, STREAM_MODE_OFF,
};
use crate::stream::StreamAssembler;
use crate::usb::UsbDevice;

/// How long a single command exchange may take.
const COMMAND_TIMEOUT: Duration = Duration::from_millis(5_000);
/// The reference driver waits 2 s before retrying the very first
/// GetVersion (known firmware quirk).
const FIRST_COMMAND_RETRY_DELAY: Duration = Duration::from_millis(2_000);
/// Firmware version reply is a 33-byte packed `XnVersions`.
const VERSION_REPLY_LEN: usize = 33;
/// Bulk read polling slice while waiting for frames.
const BULK_POLL_SLICE: Duration = Duration::from_millis(100);

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
    /// Parse the 33-byte `XnVersions` reply payload.
    fn parse(buf: &[u8]) -> Result<DeviceVersion> {
        if buf.len() < VERSION_REPLY_LEN {
            return Err(XtionError::BadConfig(format!(
                "version reply too short: {} bytes",
                buf.len()
            )));
        }
        let u16_at = |i: usize| u16::from_le_bytes([buf[i], buf[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);

        let mut version = DeviceVersion {
            major: buf[0],
            minor: buf[1],
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

/// A decoded depth frame: metric depth in millimeters.
#[derive(Debug, Clone)]
pub struct DepthFrame {
    pub width: u32,
    pub height: u32,
    /// Depth in millimeters per pixel (row-major); 0 = invalid.
    pub data: Vec<u16>,
    /// Raw device timestamp of the frame's last packet.
    pub timestamp: u32,
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
    usb: UsbDevice,
    link: CommandLink,
    pub version: DeviceVersion,
    pub fixed_params: FixedParams,
    depth_const_shift: u16,
    shift_to_depth: ShiftToDepthTables,
    depth_config: Option<DepthConfig>,
    color_config: Option<Resolution>,
    depth_assembler: StreamAssembler,
    color_assembler: StreamAssembler,
    chunk: Vec<u8>,
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
            device_max_shift: 1024,
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
            depth_assembler: StreamAssembler::new(fw_magic, ignore),
            color_assembler: StreamAssembler::new(fw_magic, ignore),
            chunk: vec![0u8; UsbDevice::bulk_chunk_size(usb.depth_max_packet_size)],
            usb,
            link,
            version,
            fixed_params: fixed,
            depth_const_shift: depth_info.const_shift,
            shift_to_depth,
            depth_config: None,
            color_config: None,
        })
    }

    /// Version handshake with the documented retry semantics.
    fn handshake(usb: &mut UsbDevice) -> Result<(CommandLink, DeviceVersion)> {
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

    /// Device serial number (OPCODE_GET_SERIAL_NUMBER, ASCII reply).
    pub fn serial_number(&mut self) -> Result<String> {
        let data =
            self.link
                .exchange(&mut self.usb, Opcode::GetSerialNumber, &[], COMMAND_TIMEOUT)?;
        Ok(String::from_utf8_lossy(&data)
            .trim_end_matches('\0')
            .to_string())
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
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&param.as_u16().to_le_bytes());
        payload.extend_from_slice(&value.to_le_bytes());
        self.link
            .exchange(&mut self.usb, Opcode::SetParam, &payload, COMMAND_TIMEOUT)?;
        Ok(())
    }

    /// Configure and start the depth stream (stream1 in firmware terms).
    ///
    /// Parameter order mirrors the reference driver's property pipeline:
    /// format -> resolution -> fps -> stream mode.
    pub fn start_depth(&mut self, config: DepthConfig) -> Result<()> {
        self.set_param(Param::DepthFormat, config.input_format.as_u16())?;
        self.set_param(Param::DepthResolution, config.resolution.as_u16())?;
        self.set_param(Param::DepthFps, config.fps)?;
        self.set_param(Param::Stream1Mode, STREAM_MODE_DEPTH)?;

        // Make sure the bulk read buffer can hold one frame's wire size.
        let (width, height) = config.dimensions();
        let pixels = (width * height) as usize;
        let wire_size = match config.input_format {
            DepthFormat::Uncompressed16Bit => pixels * 2,
            _ => pixels * 11 / 8 + 11,
        };
        if wire_size > self.chunk.len() {
            self.chunk.resize(wire_size, 0);
        }

        self.depth_config = Some(config);
        Ok(())
    }

    /// Stop the depth stream.
    pub fn stop_depth(&mut self) -> Result<()> {
        self.set_param(Param::Stream1Mode, STREAM_MODE_OFF)?;
        self.depth_config = None;
        Ok(())
    }

    /// Configure and start the color stream (stream0 in firmware terms).
    /// Only YUV422 input formats are supported in 0.1.
    pub fn start_color(
        &mut self,
        resolution: Resolution,
        fps: u16,
        format: ImageFormat,
    ) -> Result<()> {
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
        self.set_param(Param::Stream0Mode, STREAM_MODE_COLOR)?;

        // one YUV422 frame is 2 bytes/pixel on the wire
        let (width, height) = resolution.dimensions();
        let wire_size = (width * height * 2) as usize;
        if wire_size > self.chunk.len() {
            self.chunk.resize(wire_size, 0);
        }
        self.color_config = Some(resolution);
        Ok(())
    }

    /// Stop the color stream.
    pub fn stop_color(&mut self) -> Result<()> {
        self.set_param(Param::Stream0Mode, STREAM_MODE_OFF)?;
        self.color_config = None;
        Ok(())
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
                return self.decode_depth_frame(&frame, width, height, expected_pixels, config);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(XtionError::ControlTimeout);
            }
            let slice = (deadline - now).min(BULK_POLL_SLICE);
            let n = self
                .usb
                .bulk_read(self.usb.endpoints.depth, &mut self.chunk, slice)?;
            if n > 0 {
                self.depth_assembler.feed(&self.chunk[..n]);
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
            let now = Instant::now();
            if now >= deadline {
                return Err(XtionError::ControlTimeout);
            }
            let slice = (deadline - now).min(BULK_POLL_SLICE);
            let n = self
                .usb
                .bulk_read(self.usb.endpoints.image, &mut self.chunk, slice)?;
            if n > 0 {
                self.color_assembler.feed(&self.chunk[..n]);
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
        Ok(DepthFrame {
            width,
            height,
            data,
            timestamp: frame.timestamp,
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
