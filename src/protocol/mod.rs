//! Wire-protocol constants ported 1:1 from the OpenNI 2 PS1080 driver
//! (`Source/Drivers/PS1080/**` of the sha256-verified OpenNI 2.2.0 source,
//! Apache-2.0, PrimeSense Ltd.). Every constant cites its origin file.

pub mod algorithm;
pub mod command;
pub mod fixed_params;

pub use command::CommandLink;

/// ASUS / PrimeSense USB vendor id.
pub const USB_VID: u16 = 0x1d27;
/// Xtion Pro Live / Xtion (PS1080) product ids.
pub const USB_PIDS: [u16; 2] = [0x0601, 0x0400];

/// Host magic for protocol v2.5 ("PS"), sent by the PC.
/// Source: XnHostProtocol.h `XN_HOST_MAGIC_25`
pub const HOST_MAGIC_V25: u16 = 0x5053;
/// Firmware magic for protocol v2.5 ("SP"), sent by the device.
pub const FW_MAGIC_V25: u16 = 0x5350;
/// Host magic for protocol v2.6 ("MG"), firmware >= 1.2.
pub const HOST_MAGIC_V26: u16 = 0x4d47;
/// Firmware magic for protocol v2.6 ("BR"). Also the magic that prefixes
/// every bulk-stream packet header.
pub const FW_MAGIC_V26: u16 = 0x4252;

/// Command opcodes. Source: XnHostProtocol.h `EPsProtocolOpCodes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Opcode {
    GetVersion = 0,
    KeepAlive = 1,
    GetParam = 2,
    SetParam = 3,
    GetFixedParams = 4,
    GetMode = 5,
    SetMode = 6,
    GetLog = 7,
    AlgorithmParams = 22,
    GetSerialNumber = 37,
    GetPlatformString = 39,
    GetUsbCoreType = 40,
    SetLedState = 41,
    EnableEmitter = 42,
}

impl Opcode {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Firmware parameters addressable through GET_PARAM / SET_PARAM.
/// Source: Sensor/XnParams.h `EPsProtocolParams`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Param {
    RegistrationEnable = 2,
    Stream0Mode = 5,
    Stream1Mode = 6,
    Stream2Mode = 7,
    ImageFormat = 12,
    ImageResolution = 13,
    ImageFps = 14,
    DepthFormat = 18,
    DepthResolution = 19,
    DepthFps = 20,
    DepthAgc = 21,
    DepthHoleFilter = 22,
    DepthMirror = 23,
}

impl Param {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Video stream on/off values written to PARAM_STREAM*_MODE.
/// Source: Include/XnStreamParams.h `XnVideoStreamMode`.
pub const STREAM_MODE_OFF: u16 = 0;
pub const STREAM_MODE_COLOR: u16 = 1;
pub const STREAM_MODE_DEPTH: u16 = 2;
pub const STREAM_MODE_IR: u16 = 3;

/// Resolution ids written to PARAM_IMAGE_RESOLUTION / PARAM_DEPTH_RESOLUTION.
/// Source: Include/XnStreamParams.h `XnResolutions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Resolution {
    Qvga = 0,  // 320x240
    Vga = 1,   // 640x480
    Sxga = 2,  // 1280x1024
    Qqvga = 4, // 160x120
}

impl Resolution {
    pub fn as_u16(self) -> u16 {
        self as u16
    }

    /// Pixel dimensions (width, height) of this resolution id.
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Resolution::Qvga => (320, 240),
            Resolution::Vga => (640, 480),
            Resolution::Sxga => (1280, 1024),
            Resolution::Qqvga => (160, 120),
        }
    }
}

/// Depth input formats written to PARAM_DEPTH_FORMAT.
/// Source: Include/PS1080.h `XnIODepthFormats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DepthFormat {
    Uncompressed16Bit = 0,
    /// "PS compressed": 4 x 11-bit shift values packed into 11 bytes.
    Packed11 = 1,
    Uncompressed10Bit = 2,
    Uncompressed11Bit = 3,
    Uncompressed12Bit = 4,
}

impl DepthFormat {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Image input formats written to PARAM_IMAGE_FORMAT.
/// Source: Include/PS1080.h `XnIOImageFormats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ImageFormat {
    Bayer = 0,
    Yuv422 = 1,
    Jpeg = 2,
    Jpeg420 = 3,
    JpegMono = 4,
    UncompressedYuv422 = 5,
    UncompressedBayer = 6,
    UncompressedYuyv = 7,
}

impl ImageFormat {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Bulk-stream packet types (little-endian u16 in the packet header).
/// Source: Sensor/XnDeviceSensorProtocol.h `XN_SENSOR_PROTOCOL_RESPONSE_*`.
pub mod packet_type {
    pub const DEPTH_START: u16 = 0x7100;
    pub const DEPTH_BUFFER: u16 = 0x7200;
    pub const DEPTH_END: u16 = 0x7500;
    pub const IMAGE_START: u16 = 0x8100;
    pub const IMAGE_BUFFER: u16 = 0x8200;
    pub const IMAGE_END: u16 = 0x8500;
    pub const AUDIO_BUFFER: u16 = 0x9200;
    pub const GMC: u16 = 0xa200;
    pub const OVERHEAT: u16 = 0xf31f;
    pub const PROJECTOR_FAULT_EVENT: u16 = 0xdead;
}

/// Bulk-in endpoint addresses for firmware >= 3.0/4.0 ("new USB layout").
/// Older firmware swaps them; the driver auto-detects by probing 0x81 first.
/// Source: Sensor/XnDeviceSensorIO.cpp.
pub mod endpoint {
    pub const DEPTH_NEW: u8 = 0x81;
    pub const IMAGE_NEW: u8 = 0x82;
    pub const MISC_NEW: u8 = 0x83;
    pub const DEPTH_OLD: u8 = 0x82;
    pub const IMAGE_OLD: u8 = 0x83;
    pub const MISC_OLD: u8 = 0x86;
    pub const CONTROL_OUT_OLD: u8 = 0x04; // bulk, pre-3.0 only
    pub const CONTROL_IN_OLD: u8 = 0x85; // bulk, pre-3.0 only
}

/// Algorithm ids for OPCODE_ALGORITM_PARAMS.
/// Source: XnHostProtocol.h `XnHostProtocolAlgorithmType`.
pub mod algorithm_ids {
    pub const DEPTH_INFO: u16 = 0x00;
    pub const REGISTRATION: u16 = 0x02;
    pub const PADDING: u16 = 0x03;
    pub const BLANKING: u16 = 0x06;
    pub const DEVICE_INFO: u16 = 0x07;
    pub const FREQUENCY: u16 = 0x80;
}

/// Shift-to-depth defaults used by the C driver on Linux.
/// Source: Sensor/XnSensorDepthStream.cpp lines 44-45.
pub const SHIFTS_PARAM_COEFFICIENT: u32 = 4;
pub const SHIFTS_SHIFT_SCALE: u32 = 10;

/// Default depth/video configuration mirroring the C driver.
pub const DEPTH_DEFAULT_RESOLUTION: Resolution = Resolution::Vga;
pub const DEPTH_DEFAULT_FPS: u16 = 30;
pub const DEPTH_DEFAULT_INPUT_FORMAT: DepthFormat = DepthFormat::Uncompressed11Bit;
pub const IMAGE_DEFAULT_INPUT_FORMAT: ImageFormat = ImageFormat::UncompressedYuv422;

/// Send retries for the control endpoint, and the sleep between them.
/// Source: Sensor/XnHostProtocol.cpp `XnHostProtocolUSBSend` /
/// `XN_USB_HOST_PROTOCOL_SEND_RETRIES`.
pub const CONTROL_SEND_RETRIES: usize = 5;
pub const CONTROL_SEND_RETRY_DELAY_MS: u64 = 100;

/// Overall receive timeout for one command exchange.
pub const COMMAND_TIMEOUT_MS: Duration = Duration::from_millis(5_000);

use std::time::Duration;

/// Size of the 12-byte bulk packet header
/// (`XnSensorProtocolResponseHeader`).
pub const PACKET_HEADER_SIZE: usize = 12;

/// A parsed bulk-stream packet header (little-endian on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    /// Always the firmware magic ("SP" 0x5350 / "BR" 0x4252).
    pub magic: u16,
    /// One of [`packet_type`].
    pub packet_type: u16,
    pub packet_id: u16,
    /// Buffer size **including** the 12-byte header.
    pub buf_size: u16,
    pub timestamp: u32,
}

impl PacketHeader {
    /// Bytes of payload following the header.
    pub fn payload_size(&self) -> usize {
        self.buf_size.saturating_sub(PACKET_HEADER_SIZE as u16) as usize
    }

    /// Parse from the first 12 bytes of `buf`.
    pub fn parse(buf: &[u8]) -> Option<PacketHeader> {
        if buf.len() < PACKET_HEADER_SIZE {
            return None;
        }
        Some(PacketHeader {
            magic: u16::from_le_bytes([buf[0], buf[1]]),
            packet_type: u16::from_le_bytes([buf[2], buf[3]]),
            packet_id: u16::from_le_bytes([buf[4], buf[5]]),
            buf_size: u16::from_le_bytes([buf[6], buf[7]]),
            timestamp: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}
