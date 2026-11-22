//! CMOS preset enumeration: the firmware reports which (format,
//! resolution, fps) modes each CMOS supports via opcode 36
//! (GET_CMOS_PRESETS). Port of `XnHostProtocolGetCmosPresets`
//! (Sensor/XnHostProtocol.cpp:3253-3296), which the reference driver uses
//! to validate image stream modes at init (XnSensorImageStream::Init
//! reads the list and adapts its defaults to a supported entry). A mode
//! outside this list is rejected by the firmware with NACK(BadParams) —
//! observed on fw 5.8.22 when requesting the PS-compressed Yuv422 format
//! at VGA@30 (2026-11-22).

use std::time::Duration;

use super::command::{CommandLink, ControlTransport};
use super::Opcode;
use crate::error::{Result, XtionError};

/// CMOS function types (Include/PS1080.h:245-246). Note the order: the
/// IMAGE CMOS is 0, the DEPTH CMOS is 1.
pub const CMOS_TYPE_IMAGE: u16 = 0;
pub const CMOS_TYPE_DEPTH: u16 = 1;

/// One stream mode supported by a CMOS (`XnCmosPreset`,
/// Include/XnStreamParams.h:251-256).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmosPreset {
    /// Input format id (`XnIOImageFormats` for the image CMOS: 0=Bayer,
    /// 1=PS-compressed Yuv422, 5=uncompressed Yuv422, 7=uncompressed Yuyv).
    pub format: u16,
    /// Resolution id (`XnResolutions`: 0=QVGA, 1=VGA, 2=SXGA, 4=QQVGA).
    pub resolution: u16,
    /// Frame rate in Hz.
    pub fps: u16,
}

impl CmosPreset {
    /// Wire size of one preset: three big-endian u16 fields.
    const WIRE_SIZE: usize = 6;

    fn parse(data: &[u8]) -> CmosPreset {
        CmosPreset {
            format: u16::from_be_bytes([data[0], data[1]]),
            resolution: u16::from_be_bytes([data[2], data[3]]),
            fps: u16::from_be_bytes([data[4], data[5]]),
        }
    }
}

impl std::fmt::Display for CmosPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "format {} @ res {} @ {} fps",
            self.format, self.resolution, self.fps
        )
    }
}

/// Read the stream modes one CMOS supports.
///
/// Wire format (per the C): the request payload is ONE big-endian u16
/// CMOS id; the reply is an array of big-endian `{format, resolution,
/// fps}` u16 triples. Unlike most other replies these fields arrive
/// byte-swapped — the C applies `XN_PREPARE_VAR16_IN_BUFFER` to the
/// request payload and to every reply field individually.
/// `XnHostProtocolGetCmosPresets` also works around a firmware bug that
/// appends one extra preset with FPS 0 — such entries are skipped here
/// too.
pub fn read_cmos_presets<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    cmos: u16,
    timeout: Duration,
) -> Result<Vec<CmosPreset>> {
    let payload = cmos.to_be_bytes().to_vec();
    let data = link.exchange(transport, Opcode::CmosPresets, &payload, timeout)?;
    if data.len() % CmosPreset::WIRE_SIZE != 0 {
        return Err(XtionError::BadConfig(format!(
            "cmos {cmos} presets reply is {} bytes, not a multiple of {}",
            data.len(),
            CmosPreset::WIRE_SIZE
        )));
    }
    let mut presets = Vec::with_capacity(data.len() / CmosPreset::WIRE_SIZE);
    for chunk in data.chunks_exact(CmosPreset::WIRE_SIZE) {
        let preset = CmosPreset::parse(chunk);
        if preset.fps != 0 {
            presets.push(preset);
        }
    }
    Ok(presets)
}

#[cfg(test)]
mod tests {
    use super::super::command::ProtocolVersion;
    use super::*;
    use crate::error::XtionError;
    use crate::protocol::FW_MAGIC_V26;

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

    /// The exchange path is fully exercised by the command/link unit
    /// tests; here a real `CommandLink` is driven over the fake transport
    /// so the wire bytes and the parse are checked end to end.
    fn link() -> CommandLink {
        CommandLink::new(ProtocolVersion::V26)
    }

    /// A full GET_CMOS_PRESETS reply packet: V26 header (magic, size
    /// words, opcode, id — the first exchange always uses request id 0),
    /// the ACK/error word, then the big-endian preset triples.
    fn preset_reply(entries: &[(u16, u16, u16)]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (format, res, fps) in entries {
            payload.extend_from_slice(&format.to_be_bytes());
            payload.extend_from_slice(&res.to_be_bytes());
            payload.extend_from_slice(&fps.to_be_bytes());
        }
        let mut v = Vec::new();
        v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
        v.extend_from_slice(&((payload.len() / 2 + 1) as u16).to_le_bytes());
        v.extend_from_slice(&Opcode::CmosPresets.as_u16().to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // request id of the first exchange
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(&payload);
        v
    }

    #[test]
    fn request_payload_is_big_endian_cmos_id() {
        let mut link = link();
        let mut transport = FakeTransport {
            replies: vec![preset_reply(&[(5, 1, 30)])],
            sent: Vec::new(),
        };
        let presets = read_cmos_presets(
            &mut link,
            &mut transport,
            CMOS_TYPE_IMAGE,
            Duration::from_secs(1),
        )
        .expect("presets");
        assert_eq!(presets.len(), 1);
        assert_eq!(
            presets[0],
            CmosPreset {
                format: 5,
                resolution: 1,
                fps: 30
            }
        );
        // the 2-byte request payload must be the CMOS id big-endian
        let sent = &transport.sent[0];
        let tail = &sent[sent.len() - 2..];
        assert_eq!(tail, &[0x00, 0x00], "CMOS_TYPE_IMAGE (0) as BE u16");
    }

    #[test]
    fn parses_multiple_entries_and_skips_fps_zero() {
        // real cameras append an extra preset with fps 0 (fw bug)
        let mut link = link();
        let mut transport = FakeTransport {
            replies: vec![preset_reply(&[
                (5, 1, 30),
                (1, 0, 30),
                (1, 0, 60),
                (5, 0, 30),
                (0, 0, 0), // the buggy entry — must be dropped
            ])],
            sent: Vec::new(),
        };
        let presets = read_cmos_presets(
            &mut link,
            &mut transport,
            CMOS_TYPE_IMAGE,
            Duration::from_secs(1),
        )
        .expect("presets");
        assert_eq!(presets.len(), 4);
        assert_eq!(presets[0].format, 5);
        assert_eq!(presets[1].resolution, 0);
        assert_eq!(presets[3].fps, 30);
    }

    #[test]
    fn non_multiple_of_six_is_an_error() {
        // a legal packet carrying one word (2 bytes) of preset data —
        // 2 is not a multiple of the 6-byte preset size, so the parse
        // must fail with BadConfig instead of panicking
        let mut v = Vec::new();
        v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes()); // 1 error word + 1 data word
        v.extend_from_slice(&Opcode::CmosPresets.as_u16().to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // request id
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(&[0x00, 0x05]); // one truncated preset field
        let mut link = link();
        let mut transport = FakeTransport {
            replies: vec![v],
            sent: Vec::new(),
        };
        let err = read_cmos_presets(
            &mut link,
            &mut transport,
            CMOS_TYPE_IMAGE,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(matches!(err, XtionError::BadConfig(_)), "got {err:?}");
    }
}
