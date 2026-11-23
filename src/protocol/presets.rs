//! CMOS preset enumeration: the firmware reports which (format,
//! resolution, fps) modes each CMOS supports via opcode 36
//! (GET_CMOS_PRESETS). Port of `XnHostProtocolGetCmosPresets`
//! (Sensor/XnHostProtocol.cpp:3253-3296), which the reference driver uses
//! to validate image stream modes at init (XnSensorImageStream::Init
//! reads the list and adapts its defaults to a supported entry). A mode
//! outside this list is rejected by the firmware with NACK(BadParams).
//!
//! Semantics (0.2.8): the list describes the CMOS's capability across
//! BOTH USB interface styles. On the Xtion's bulk endpoints the vendor
//! stack uses the PS-COMPRESSED format 1 (the C's non-Windows default,
//! XnSensorImageStream.h:36-41); the uncompressed modes (5/7) belong to
//! the Windows/ISO path and their stream starts were rejected on this
//! device (fw 5.8.22, 2026-11-22).
//!
//! Hardware note (Xtion Pro Live, fw 5.8.22): the image CMOS reports 25
//! modes — formats 1 (PS-compressed Yuv422), 5 (uncompressed Yuv422) and
//! 7 (uncompressed Yuyv) at QVGA/VGA, Bayer 0/6 at the wide modes, and
//! one (9, VGA, 30) entry with a format id OpenNI never exposes.
//!
//! Resolution ids render with their actual dimensions via
//! [`resolution_info`]: the firmware table uses raw numbers
//! (`format 0 @ res 14 @ 30 fps`), while the C source names them
//! (`XN_RESOLUTION_720P`); both views are shown now.

use std::time::Duration;

use super::command::{CommandLink, ControlTransport};
use super::Opcode;
use crate::error::{Result, XtionError};

/// CMOS function types (Include/PS1080.h:245-246). Note the order: the
/// IMAGE CMOS is 0, the DEPTH CMOS is 1.
pub const CMOS_TYPE_IMAGE: u16 = 0;
pub const CMOS_TYPE_DEPTH: u16 = 1;

/// The firmware's resolution id decoded into (name, width, height).
///
/// Source: `XnResolutions`, Include/XnStreamParams.h:183-202 — these
/// ARE the values on the wire (presets, params 13/19): 0=QVGA, 1=VGA,
/// 2=SXGA (the depth CMOS's native size), 14=720P, 15=1280x960, etc.
/// Codes the C table does not name return `None`.
pub fn resolution_info(code: u16) -> Option<(&'static str, u16, u16)> {
    let (name, w, h) = match code {
        0 => ("QVGA", 320, 240),
        1 => ("VGA", 640, 480),
        2 => ("SXGA", 1280, 1024),
        3 => ("UXGA", 1600, 1200),
        4 => ("QQVGA", 160, 120),
        5 => ("QCIF", 176, 144),
        6 => ("240P", 432, 240),
        7 => ("CIF", 352, 288),
        8 => ("WVGA", 640, 360),
        9 => ("480P", 864, 480),
        10 => ("800x448", 800, 448),
        11 => ("SVGA", 800, 600),
        12 => ("576P", 1024, 576),
        13 => ("DV", 960, 720),
        14 => ("720P", 1280, 720),
        15 => ("1280x960", 1280, 960),
        _ => return None,
    };
    Some((name, w, h))
}

/// One stream mode supported by a CMOS (`XnCmosPreset`,
/// Include/XnStreamParams.h:251-256).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmosPreset {
    /// Input format id (`XnIOImageFormats`, Include/PS1080.h:253-261:
    /// 0=Bayer, 1=PS-compressed Yuv422, 5=uncompressed Yuv422,
    /// 6=uncompressed Bayer, 7=uncompressed Yuyv).
    pub format: u16,
    /// Resolution id (`XnResolutions`, XnStreamParams.h:183-202:
    /// 0=QVGA, 1=VGA, 2=SXGA, 14=1280x720, 15=1280x960).
    pub resolution: u16,
    /// Frame rate in Hz.
    pub fps: u16,
}

impl CmosPreset {
    /// Wire size of one preset: three little-endian u16 fields.
    const WIRE_SIZE: usize = 6;

    fn parse(data: &[u8]) -> CmosPreset {
        CmosPreset {
            format: u16::from_le_bytes([data[0], data[1]]),
            resolution: u16::from_le_bytes([data[2], data[3]]),
            fps: u16::from_le_bytes([data[4], data[5]]),
        }
    }
}

impl std::fmt::Display for CmosPreset {
    /// `format 1 @ res 0 (QVGA 320x240) @ 30 fps` — the raw wire code
    /// plus the actual resolution it stands for, so a preset listing is
    /// readable without the C header open.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match resolution_info(self.resolution) {
            Some((name, w, h)) => write!(
                f,
                "format {} @ res {} ({} {}x{}) @ {} fps",
                self.format, self.resolution, name, w, h, self.fps
            ),
            None => write!(
                f,
                "format {} @ res {} @ {} fps",
                self.format, self.resolution, self.fps
            ),
        }
    }
}

/// Read the stream modes one CMOS supports.
///
/// Wire format: the request payload is ONE little-endian u16 CMOS id;
/// the reply is an array of little-endian `{format, resolution, fps}`
/// u16 triples. The C copies the request and every reply field via
/// `XN_PREPARE_VAR16_IN_BUFFER`, which is the IDENTITY on x86 and Windows
/// (ThirdParty/PSCommon/XnLib/Include/Linux-x86/XnOSLinux-x86.h) — the
/// Nios II firmware is little-endian, like the rest of the packet
/// protocol. An earlier revision of this parser read the fields
/// big-endian, shifting every value by 256: the hardware table looked
/// like `format 256 @ res 0 @ 7680 fps` instead of
/// `format 1 @ res 0 @ 30 fps`, so the requested mode never matched
/// (found on camera, 2026-11-22).
/// `XnHostProtocolGetCmosPresets` also works around a firmware bug that
/// appends one extra preset with FPS 0 — such entries are skipped here
/// too.
pub fn read_cmos_presets<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    cmos: u16,
    timeout: Duration,
) -> Result<Vec<CmosPreset>> {
    let payload = cmos.to_le_bytes().to_vec();
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
    /// the ACK/error word, then the little-endian preset triples.
    fn preset_reply(entries: &[(u16, u16, u16)]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (format, res, fps) in entries {
            payload.extend_from_slice(&format.to_le_bytes());
            payload.extend_from_slice(&res.to_le_bytes());
            payload.extend_from_slice(&fps.to_le_bytes());
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
    fn parses_real_camera_first_entry() {
        // the first entry the Xtion Pro Live actually sends, as raw wire
        // bytes: (format 5 = uncompressed Yuv422, resolution 1 = VGA,
        // fps 30) little-endian
        let mut link1 = link();
        let mut transport1 = FakeTransport {
            replies: vec![preset_reply(&[(5, 1, 30)])],
            sent: Vec::new(),
        };
        let presets = read_cmos_presets(
            &mut link1,
            &mut transport1,
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
        // the 2-byte request payload is the CMOS id little-endian; use a
        // non-symmetric id so the byte order is actually pinned (id 0
        // would pass either way)
        let mut link2 = link();
        let mut transport2 = FakeTransport {
            replies: vec![preset_reply(&[(5, 1, 30)])],
            sent: Vec::new(),
        };
        read_cmos_presets(
            &mut link2,
            &mut transport2,
            CMOS_TYPE_DEPTH,
            Duration::from_secs(1),
        )
        .expect("depth presets");
        let sent = &transport2.sent[0];
        let tail = &sent[sent.len() - 2..];
        assert_eq!(tail, &[0x01, 0x00], "CMOS_TYPE_DEPTH (1) as LE u16");
    }

    #[test]
    fn parses_multiple_entries_and_skips_fps_zero() {
        // real cameras append an extra preset with fps 0 (fw bug)
        let mut link = link();
        let mut transport = FakeTransport {
            replies: vec![preset_reply(&[
                (5, 1, 30),
                (1, 1, 30),
                (7, 1, 30),
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
        assert_eq!(presets[1].resolution, 1);
        assert_eq!(presets[3].resolution, 0);
        assert_eq!(presets[3].fps, 30);
    }

    #[test]
    fn resolution_table_matches_the_c_header() {
        // spot checks against XnStreamParams.h:183-202
        assert_eq!(resolution_info(0), Some(("QVGA", 320, 240)));
        assert_eq!(resolution_info(1), Some(("VGA", 640, 480)));
        assert_eq!(resolution_info(2), Some(("SXGA", 1280, 1024)));
        assert_eq!(resolution_info(4), Some(("QQVGA", 160, 120)));
        assert_eq!(resolution_info(9), Some(("480P", 864, 480)));
        assert_eq!(resolution_info(14), Some(("720P", 1280, 720)));
        assert_eq!(resolution_info(15), Some(("1280x960", 1280, 960)));
        assert_eq!(resolution_info(16), None);
        // the Display renders the real dimensions
        let preset = CmosPreset {
            format: 1,
            resolution: 1,
            fps: 30,
        };
        assert_eq!(preset.to_string(), "format 1 @ res 1 (VGA 640x480) @ 30 fps");
        let wide = CmosPreset {
            format: 0,
            resolution: 14,
            fps: 30,
        };
        assert_eq!(wide.to_string(), "format 0 @ res 14 (720P 1280x720) @ 30 fps");
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
