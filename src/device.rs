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
use crate::protocol::command::{CommandLink, ControlTransport, ProtocolVersion};
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
        // Bulk read buffer: 40 x max packet size (the reference driver's
        // Linux bulk multiplier). Its length MUST stay a multiple of each
        // endpoint's max packet size - the Linux USB stack completes
        // bulk-IN URBs whose length is not a multiple with EOVERFLOW
        // (observed on hardware when the buffer was whole-frame sized).
        // Sizing on the larger of the two endpoints keeps it a multiple of
        // both (max packet sizes are powers of two).
        let chunk_len =
            UsbDevice::bulk_chunk_size(usb.depth_max_packet_size.max(usb.image_max_packet_size));
        let fw_magic = link.protocol().fw_magic();

        Ok(Xtion {
            depth_assembler: StreamAssembler::new(fw_magic, ignore),
            color_assembler: StreamAssembler::new(fw_magic, ignore),
            chunk: vec![0u8; chunk_len],
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

        self.color_config = Some(resolution);
        Ok(())
    }

    /// Stop the color stream.
    pub fn stop_color(&mut self) -> Result<()> {
        self.set_param(Param::Stream0Mode, STREAM_MODE_OFF)?;
        self.color_config = None;
        Ok(())
    }

    /// One bulk read into the chunk buffer. A USB timeout means "no data
    /// yet" and maps to 0 — the caller's deadline governs the wait. (rusb
    /// returns Ok(n) when a timeout still delivered partial data, so no
    /// stream bytes are lost.)
    fn bulk_read_slice(&mut self, endpoint: u8, timeout: Duration) -> Result<usize> {
        match self.usb.bulk_read(endpoint, &mut self.chunk, timeout) {
            Ok(n) => Ok(n),
            Err(XtionError::Usb(rusb::Error::Timeout))
            | Err(XtionError::Usb(rusb::Error::Interrupted)) => Ok(0),
            Err(e) => Err(e),
        }
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
            let n = self.bulk_read_slice(self.usb.endpoints.depth, slice)?;
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
            let n = self.bulk_read_slice(self.usb.endpoints.image, slice)?;
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
}
