//! Error model. Every boot failure pins the exact state-machine step so a
//! hardware log maps 1:1 onto the disassembly-derived sequence
//! (DRAFT §1.4).

use std::path::PathBuf;

/// Boot/upload state-machine steps, mirroring `_InitBlobUpload`
/// (xEyeUSB.dll) step for step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStep {
    /// TX DSP-start; expect RX type 2 "Ready to load program!" (3 s).
    DspStart,
    /// TX type 2: packet count + CRC ("Tx packet count & crc!").
    CountAndCrc,
    /// TX type 3 × 64-byte firmware packets (6 s per packet).
    PacketStream,
    /// RX type 3 CRC result; abort on status word 0xF at frame offset +8.
    CrcAck,
    /// RX type 2 device-ID / DSP version (300 ms poll × ≤ 10).
    DeviceId,
    /// "Get DSP Reset ACK OK" → TX type 7 (1 s).
    DspReset,
    /// Handle-config read/write + lens parameters.
    HandleConfig,
}

impl BootStep {
    pub fn as_str(self) -> &'static str {
        match self {
            BootStep::DspStart => "dsp-start",
            BootStep::CountAndCrc => "count-and-crc",
            BootStep::PacketStream => "packet-stream",
            BootStep::CrcAck => "crc-ack",
            BootStep::DeviceId => "device-id",
            BootStep::DspReset => "dsp-reset",
            BootStep::HandleConfig => "handle-config",
        }
    }
}

/// Wire-protocol violations that are not boot failures.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("bad magic {magic:#010x} (expected {expected:#010x})")]
    BadMagic { magic: u32, expected: u32 },
    #[error("unexpected frame type {kind} (expected {expected})")]
    BadType { kind: u16, expected: u16 },
    #[error("payload too short: {len} bytes, need {need}")]
    Short { len: usize, need: usize },
    #[error("device reported CRC failure (status word 0xF)")]
    CrcFailed,
    #[error("keep-alive not answered within {0:?}")]
    KeepaliveTimeout(std::time::Duration),
    #[error("read timed out after {0:?}")]
    ReadTimeout(std::time::Duration),
}

/// Top-level crate error.
#[derive(Debug, thiserror::Error)]
pub enum IdongError {
    #[error("no idong sensor found (looked for {vid:#06x}:{boot:#06x} bootloader or {vid:#06x}:{run:#06x} runtime)",
            vid = crate::protocol::VID,
            boot = crate::protocol::PID_BOOTLOADER,
            run = crate::protocol::PID_RUNTIME)]
    NotFound,
    #[error("device {vid:#06x}:{pid:#06x} is in bootloader state; firmware upload required (see Idong::open_or_boot)",
            vid = crate::protocol::VID, pid = crate::protocol::PID_BOOTLOADER)]
    BootloaderState,
    #[error("boot failed at step {step} ({step_str}): {reason}", step_str = .step.as_str())]
    Boot {
        step: BootStep,
        reason: String,
    },
    #[error("CRC mismatch after upload: announced {announced:#010x}, device reported {device:#010x}")]
    CrcMismatch { announced: u32, device: u32 },
    #[error("firmware file {path} failed integrity check (sha256 {got}, expected {want})")]
    FirmwareIntegrity {
        path: PathBuf,
        got: String,
        want: String,
    },
    #[error("device did not re-enumerate as runtime within {0:?}")]
    NoReenumeration(std::time::Duration),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("USB error: {0}")]
    Usb(#[from] rusb::Error),
}

impl IdongError {
    /// Attach the boot step to an error, for `BootUploader` internals.
    pub(crate) fn boot(step: BootStep, reason: impl Into<String>) -> Self {
        IdongError::Boot {
            step,
            reason: reason.into(),
        }
    }
}

pub type Result<T, E = IdongError> = std::result::Result<T, E>;
