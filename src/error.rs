//! Error types for the xtion driver.

use std::fmt;

/// NACK error codes returned by the firmware in command replies.
///
/// Source: `Source/Drivers/PS1080/Sensor/XnHostProtocol.h` (enum XnHostProtocolNacks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Nack {
    UnknownError = 1,
    InvalidCommand = 2,
    BadPacketCrc = 3,
    BadPacketSize = 4,
    BadParams = 5,
    I2cTransactionFailed = 6,
    FileNotFound = 7,
    FileCreateFailure = 8,
    FileWriteFailure = 9,
    FileDeleteFailure = 10,
    FileReadFailure = 11,
    BadCommandSize = 12,
    NotReady = 13,
    Overflow = 14,
    OverlayNotLoaded = 15,
    FileSystemLocked = 16,
}

impl Nack {
    pub fn from_u16(v: u16) -> Nack {
        use Nack::*;
        match v {
            2 => InvalidCommand,
            3 => BadPacketCrc,
            4 => BadPacketSize,
            5 => BadParams,
            6 => I2cTransactionFailed,
            7 => FileNotFound,
            8 => FileCreateFailure,
            9 => FileWriteFailure,
            10 => FileDeleteFailure,
            11 => FileReadFailure,
            12 => BadCommandSize,
            13 => NotReady,
            14 => Overflow,
            15 => OverlayNotLoaded,
            16 => FileSystemLocked,
            _ => UnknownError,
        }
    }
}

impl fmt::Display for Nack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "device NACK {:?} ({})", self, *self as u16)
    }
}

impl std::error::Error for Nack {}

/// Errors raised while validating a firmware reply packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("reply magic not found (expected firmware magic in reply buffer)")]
    BadMagic,
    #[error("reply id {actual} does not match request id {expected}")]
    WrongId { expected: u16, actual: u16 },
    #[error("reply opcode {actual} does not match expected {expected}")]
    WrongOpcode { expected: u16, actual: u16 },
    #[error("reply too short for a valid packet")]
    ReplyTooShort,
}

/// Top-level error type of the crate.
#[derive(Debug, thiserror::Error)]
pub enum XtionError {
    #[error("no ASUS Xtion device found (USB idVendor 0x1d27, expected 1d27:0601 or 1d27:0400)")]
    DeviceNotFound,
    #[error("USB error: {0}")]
    Usb(#[from] rusb::Error),
    #[error(
        "USB interface busy: the Xtion is already open in another process — or another handle in this process (e.g. parallel tests); run hardware tests with --test-threads=1 or open the device once"
    )]
    DeviceBusy,
    #[error("firmware rejected a command: {0}")]
    Nack(#[from] Nack),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("control transfer timed out after retries")]
    ControlTimeout,
    #[error(
        "device is too old: bulk-endpoint control protocol (pre-3.0 firmware) is not supported yet"
    )]
    UnsupportedFirmware,
    #[error("device returned no data for an ALGORITHM_PARAMS request")]
    AlgorithmParamsEmpty,
    #[error("bad stream configuration: {0}")]
    BadConfig(String),
    #[error("stream is not running")]
    StreamNotRunning,
}

pub type Result<T> = std::result::Result<T, XtionError>;
