//! Boot / firmware-upload state machine — a faithful Rust transcription of
//! `_InitBlobUpload` (xEyeUSB.dll, disassembly in
//! `analysis/xeyez_boot_protocol.txt`).
//!
//! Sequence (DRAFT §1.4):
//! 1. TX DSP-start → RX type 2 `"Ready to load program!"` (3 s)
//! 2. TX type 2: packet count + CRC of the blob
//! 3. TX type 3 × 64-byte packets (6 s each)
//! 4. RX type 3: CRC result — payload word 0xF at frame offset +8 aborts
//! 5. RX type 2: device ID / DSP version (300 ms × ≤ 10 polls)
//! 6. "Get DSP Reset ACK OK" → TX type 7 (1 s)
//!
//! **GATE (safety rule #2):** `run()` refuses to execute on real hardware
//! until [`BOOT_CONFIRMED`] is flipped after a USBPcap transcript of the
//! vendor stack byte-confirms the sequence. Until then it returns
//! [`IdongError::Boot`] with reason `"unconfirmed"` — the state machine is
//! exercised in unit tests only.

use std::time::Duration;

use super::frames::{Frame, FrameDecoder};
use super::{
    MAGIC_D2H, TIMEOUT_ACK, TIMEOUT_ID_POLL, TIMEOUT_PACKET, TIMEOUT_READY, TYPE_COUNT_CRC,
    TYPE_DSP_RESET, TYPE_FIRMWARE_PACKET, TYPE_READY_TO_LOAD, CRC_FAIL_WORD, ID_POLL_ATTEMPTS,
};
use crate::error::{BootStep, IdongError, Result};
use crate::usb::Transport;

/// Safety gate. Flipped to `true` only after milestone M3 (USBPcap
/// byte-confirmation of the vendor upload, DRAFT §2.5).
pub const BOOT_CONFIRMED: bool = false;

/// Identity reported at the end of a successful boot.
#[derive(Debug, Clone, Default)]
pub struct BootInfo {
    /// Raw "DEVICE ID =" payload (layout pending open item #3).
    pub device_id_raw: Vec<u8>,
    /// DSP version string if ASCII-decodable.
    pub dsp_version: Option<String>,
}

/// CRC of the firmware blob as announced to the device.
///
/// Open item #2: the vendor variant is not yet identified (CRC16-CCITT vs
/// CRC32, endianness). `crc32()` is the placeholder used by unit tests;
/// the real one is pinned by the USBPcap capture before any hardware run.
pub fn blob_crc(blob: &[u8]) -> u32 {
    // CRC-32/ISO-HDLC (zlib) — placeholder pending M3 confirmation.
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in blob {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// The upload state machine. One instance uploads **one** firmware blob;
/// the multi-file staging (BF527 `.ldr`, Spartan-3 `.bit`, 8051 `.hex`)
/// is orchestrated by [`crate::device::FirmwareSet`] once open item #1 is
/// resolved.
pub struct BootUploader<'a> {
    transport: &'a Transport,
    blob: &'a [u8],
    decoder: FrameDecoder,
}

impl<'a> BootUploader<'a> {
    pub fn new(transport: &'a Transport, blob: &'a [u8]) -> Self {
        BootUploader {
            transport,
            blob,
            decoder: FrameDecoder::new(),
        }
    }

    /// Run the full upload. See the module docs for the safety gate.
    pub fn run(&mut self) -> Result<BootInfo> {
        if !BOOT_CONFIRMED {
            return Err(IdongError::boot(
                BootStep::DspStart,
                "upload sequence not yet confirmed against a USBPcap capture \
                 (safety gate, DRAFT §2.5 M3); refusing to write",
            ));
        }
        self.dsp_start()?;
        self.count_and_crc()?;
        self.packet_stream()?;
        self.crc_ack()?;
        let info = self.device_id()?;
        self.dsp_reset()?;
        Ok(info)
    }

    /// Step 1: TX DSP-start → RX type 2 "Ready to load program!".
    fn dsp_start(&mut self) -> Result<()> {
        // Exact DSP-start payload bytes are open item #3; the disassembly
        // shows a 64-byte scratch buffer sent through the TX builder with
        // a 3 s RX wait. Placeholder = empty type-1 frame; pinned by M3.
        let start = Frame::h2d(1, &[]);
        self.transport
            .write_frame(&start, TIMEOUT_ACK)
            .map_err(|e| IdongError::boot(BootStep::DspStart, e.to_string()))?;
        let f = self
            .read_frame(TIMEOUT_READY)
            .map_err(|e| IdongError::boot(BootStep::DspStart, e.to_string()))?;
        if f.magic != super::MAGIC_D2H || f.kind != TYPE_READY_TO_LOAD {
            return Err(IdongError::boot(
                BootStep::DspStart,
                format!("expected type {TYPE_READY_TO_LOAD} ready frame, got kind {}", f.kind),
            ));
        }
        Ok(())
    }

    /// Step 2: announce packet count + CRC (type 2).
    fn count_and_crc(&mut self) -> Result<()> {
        let packets = self.blob.len().div_ceil(64);
        let mut p = Vec::with_capacity(8);
        p.extend_from_slice(&(packets as u32).to_le_bytes());
        p.extend_from_slice(&blob_crc(self.blob).to_le_bytes());
        let f = Frame::h2d(TYPE_COUNT_CRC, &p);
        self.transport
            .write_frame(&f, TIMEOUT_ACK)
            .map_err(|e| IdongError::boot(BootStep::CountAndCrc, e.to_string()))
    }

    /// Step 3: stream 64-byte type-3 packets, 6 s timeout each.
    fn packet_stream(&mut self) -> Result<()> {
        for chunk in self.blob.chunks(64) {
            let f = Frame::h2d(TYPE_FIRMWARE_PACKET, chunk);
            self.transport
                .write_frame(&f, TIMEOUT_PACKET)
                .map_err(|e| IdongError::boot(BootStep::PacketStream, e.to_string()))?;
        }
        Ok(())
    }

    /// Step 4: RX type 3 CRC result; word 0xF at frame offset +8 = failure.
    fn crc_ack(&mut self) -> Result<()> {
        let f = self
            .read_frame(TIMEOUT_READY)
            .map_err(|e| IdongError::boot(BootStep::CrcAck, e.to_string()))?;
        if f.kind != TYPE_FIRMWARE_PACKET {
            return Err(IdongError::boot(
                BootStep::CrcAck,
                format!("expected type {TYPE_FIRMWARE_PACKET} CRC verdict, got {}", f.kind),
            ));
        }
        // payload[2..4] == frame offset +8 (payload starts at frame +6)
        if f.payload.len() >= 4 {
            let word = u16::from_le_bytes([f.payload[2], f.payload[3]]);
            if word == CRC_FAIL_WORD {
                return Err(IdongError::boot(
                    BootStep::CrcAck,
                    "device reported CRC failure (status word 0xF)",
                ));
            }
        }
        Ok(())
    }

    /// Step 5: device-ID / DSP version, 300 ms poll × ≤ 10.
    fn device_id(&mut self) -> Result<BootInfo> {
        for _ in 0..ID_POLL_ATTEMPTS {
            if let Ok(f) = self.read_frame(TIMEOUT_ID_POLL) {
                if f.kind == TYPE_READY_TO_LOAD && !f.payload.is_empty() {
                    let dsp_version = if f.payload.iter().all(|b| b.is_ascii()) {
                        Some(String::from_utf8_lossy(&f.payload).into_owned())
                    } else {
                        None
                    };
                    return Ok(BootInfo {
                        device_id_raw: f.payload,
                        dsp_version,
                    });
                }
            }
        }
        Err(IdongError::boot(BootStep::DeviceId, "no device-ID frame within 3 s"))
    }

    /// Step 6: TX type 7 (DSP reset) after "Get DSP Reset ACK OK".
    fn dsp_reset(&mut self) -> Result<()> {
        let f = Frame::h2d(TYPE_DSP_RESET, &[]);
        self.transport
            .write_frame(&f, TIMEOUT_ACK)
            .map_err(|e| IdongError::boot(BootStep::DspReset, e.to_string()))
    }

    fn read_frame(&mut self, timeout: Duration) -> Result<Frame> {
        loop {
            if let Some(f) = self.decoder.push(&[]).into_iter().next() {
                return Ok(f);
            }
            let chunk = self.transport.read_raw(64, timeout)?;
            let frames = self.decoder.push(&chunk);
            if let Some(f) = frames.into_iter().next() {
                return Ok(f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_placeholder_is_stable() {
        let a = blob_crc(b"hello idong");
        let b = blob_crc(b"hello idong");
        assert_eq!(a, b);
        assert_ne!(blob_crc(b"hello idong"), blob_crc(b"hello idong!"));
    }

    #[test]
    fn packet_count_matches_ceil() {
        // 235,930 B (ens_code.ldr size) → 3687 packets
        let n = 235_930_usize.div_ceil(64);
        assert_eq!(n, 3687);
    }
}
