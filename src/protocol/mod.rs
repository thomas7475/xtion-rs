//! Wire-protocol constants ported 1:1 from the OpenNI 2 PS1080 driver
//! (`Source/Drivers/PS1080/**` of the sha256-verified OpenNI 2.2.0 source,
//! Apache-2.0, PrimeSense Ltd.). Every constant cites its origin file.

pub mod algorithm;
pub mod command;
pub mod fixed_params;
pub mod identity;
pub mod presets;
pub mod system;

pub use command::CommandLink;
pub use presets::CmosPreset;

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
    /// Firmware log buffer (fw >= 3.0). XnHostProtocolGetLog.
    GetLog = 7,
    AlgorithmParams = 22,
    /// Read one I2C bus. XnHostProtocolReadI2C.
    I2cRead = 11,
    /// Read a device flash file (READ side only; upload is excluded).
    DownloadFile = 16,
    /// Flash-usage table (read-only).
    GetFlashMap = 18,
    /// Flash file-system listing (read-only).
    GetFileList = 19,
    /// Raw SoC bus read (the WRITE variant is permanently excluded).
    ReadAhb = 20,
    /// Raw flash read (read-only).
    ReadFlash = 25,
    /// Firmware CPU utilization. XnHostProtocolGetCPUStats.
    GetCpuStats = 27,
    /// Built-in self-test (the updater runs it continuously).
    Bist = 28,
    /// TEC status, pre-5.4 layout (fw >= 5.4 maps from opcode 38).
    GetTecData = 30,
    /// IR emitter status/power.
    GetEmitterData = 32,
    /// Current vertical blanking of one CMOS.
    GetCmosBlanking = 35,
    /// Supported stream modes of one CMOS. Source: XnHostProtocol.h
    /// `EPsProtocolOpCodes` (fw >= 5.3.29; XnHostProtocol.cpp:585-589).
    CmosPresets = 36,
    /// TEC status, post-5.4 layout.
    GetFastConvergenceTec = 38,
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

/// Firmware operating modes read with GET_MODE (opcode 5) and written
/// with SET_MODE (opcode 6). Source: Sensor/XnHostProtocol.h
/// `XnHostProtocolMode` (0-8). The reference driver reads the mode at
/// every init (XnSensorFirmware.cpp:44-46), refuses SAFE_MODE, and soft-
/// resets the device "to start clean" (bResetOnStartup defaults to TRUE,
/// XnSensor.cpp:68); SET_MODE is its transport
/// (XnHostProtocol.cpp:1832-1852, mode 3 = SOFT_RESET).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HostMode {
    /// UVC-style webcam mode (the Pro Live's color/mic hardware).
    Webcam = 0,
    /// Normal PrimeSense operation (what the reference expects).
    Ps = 1,
    Maintenance = 2,
    /// Boot into a fresh application state (the C's init reset).
    SoftReset = 3,
    Reboot = 4,
    Suspend = 5,
    Resume = 6,
    Init = 7,
    SystemRestore = 8,
    /// Waiting for re-enumeration (XnHostProtocol.h:178).
    WaitForEnum = 9,
    /// Boot-loader safe mode: streams refuse to start
    /// (XnSensorFirmware::Init refuses on this).
    SafeMode = 10,
}

impl HostMode {
    pub fn as_u16(self) -> u16 {
        self as u16
    }

    pub fn from_u16(value: u16) -> HostMode {
        match value {
            0 => HostMode::Webcam,
            1 => HostMode::Ps,
            2 => HostMode::Maintenance,
            3 => HostMode::SoftReset,
            4 => HostMode::Reboot,
            5 => HostMode::Suspend,
            6 => HostMode::Resume,
            7 => HostMode::Init,
            8 => HostMode::SystemRestore,
            9 => HostMode::WaitForEnum,
            10 => HostMode::SafeMode,
            _ => HostMode::Ps,
        }
    }
}

/// Firmware parameters addressable through GET_PARAM / SET_PARAM.
/// Source: Sensor/XnParams.h `EPsProtocolParams`.
///
/// Only parameters that are safe runtime configuration are modeled; the
/// flash-file-system parameter (85) and other persistent-state writes are
/// excluded on purpose (see docs/FEATURE_PLAN.md, "Excluded features").
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
    ImageAgc = 15,
    ImageQuality = 16,
    ImageFlickerDetection = 17,
    DepthFormat = 18,
    DepthResolution = 19,
    DepthFps = 20,
    DepthAgc = 21,
    DepthHoleFilter = 22,
    DepthMirror = 23,
    IrResolution = 26,
    IrFps = 27,
    DepthGmcMode = 36,
    ImageMirror = 71,
    IrMirror = 72,
    ImageAutoWhiteBalanceMode = 77,
    ImageAutoExposureMode = 80,
    ImageExposureBar = 81,
    DepthCloseRange = 84,
}

impl Param {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// USB core of the device, read with OPCODE_GET_USB_CORE_TYPE.
/// Source: Sensor/XnHostProtocol.h `XnHostProtocolUsbCore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum UsbCore {
    /// The "Jango" USB core (fw >= 5.3.16): the fast one; the reference
    /// driver uses it to shorten its inter-command delays.
    Jango = 0,
    /// gadgetfs-based core (older firmware).
    Gadgetfs = 1,
}

impl UsbCore {
    pub fn from_u16(v: u16) -> Option<UsbCore> {
        match v {
            0 => Some(UsbCore::Jango),
            1 => Some(UsbCore::Gadgetfs),
            _ => None,
        }
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
/// The C's image default on non-Windows hosts is the PS-COMPRESSED
/// format: `#define XN_IMAGE_STREAM_DEFAULT_INPUT_FORMAT
/// XN_IO_IMAGE_FORMAT_YUV422` with the vendor comment "non win32 works
/// with BULK, so it's better to compress the image"
/// (XnSensorImageStream.h:36-41). Windows/ISO defaults to the
/// uncompressed variant. Hardware context (fw 5.8.22, 2026-11-22): the
/// firmware NACKs stream starts of the uncompressed mode while the
/// compressed mode is the vendor's bulk path.
pub const IMAGE_DEFAULT_INPUT_FORMAT: ImageFormat = ImageFormat::Yuv422;

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

/// A parsed bulk-stream packet header.
///
/// Field order and endianness follow the reference driver's parse in
/// `Sensor/XnDeviceSensorProtocol.cpp`: on a little-endian host every
/// `XN_PREPARE_VAR16_IN_BUFFER` is a no-op, and `nBufSize` alone gets an
/// additional **unconditional** `xnOSEndianSwapUINT16` — i.e. the
/// firmware writes magic, type, packet id and timestamp little-endian,
/// but `buf_size` **big-endian**. Verified live on firmware 5.8.22:
/// reading it little-endian makes 512-byte data packets parse with
/// `buf_size = 2`, so every real data packet contributes zero payload
/// and frames fill with junk from false-magic resyncs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    /// Always the firmware magic ("SP" 0x5350 / "BR" 0x4252).
    pub magic: u16,
    /// One of [`packet_type`].
    pub packet_type: u16,
    pub packet_id: u16,
    /// Buffer size **including** the 12-byte header, big-endian.
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
            // Sole big-endian field: the reference driver applies an
            // unconditional xnOSEndianSwapUINT16 to exactly this field.
            buf_size: u16::from_be_bytes([buf[6], buf[7]]),
            timestamp: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}
