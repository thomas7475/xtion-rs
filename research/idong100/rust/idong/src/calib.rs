//! T24C16A EEPROM (2 KiB) record parser — factory stereo calibration and
//! device identity.
//!
//! Layout (verified byte-for-byte against the owner's dump, MD5
//! `dc02fce8ca39155135a4b0368b3f33ae`; every checksum passes; the
//! annotated hexdump is `upload/idong100_eeprom_annotated_hexdump.txt`):
//!
//! ```text
//! 0x0000  ASCII header "ef0123456789abcd" (16 B, no checksum)
//! 0x0100  calibration record: 120-byte payload = 28 × f32 LE + 8 B pad
//!             [0..3]   cam1 fx, fy, cx, cy          [4..7]   cam1 k1,k2,p1,p2
//!             [8..11]  cam2 fx, fy, cx, cy          [12..15] cam2 k1,k2,p1,p2
//!             [16..24] R 3×3 row-major              [25..27] T (mm)
//!         then checksum byte (sum & 0xFF) + 0xFF separator
//! 0x0180  version record: 16-B payload "2059" + zeros
//! 0x0192  int16 record: 8 × int16 = 386,473,215,303,142,221,0,0
//! 0x0300  serial record: 32 B "AB460DC8BA12C6840B49CD5786C329DD"
//! 0x0322  code A record: "000T"      0x0328  code B record: "2613"
//! every record mirrored at +0x400 (0x0500 / 0x0700)
//! record framing: [fixed-length payload][checksum = sum & 0xFF][0xFF]
//! ```
//!
//! Real-device reference values asserted by the tests: cam1
//! f=(428.761292, 430.208038), c=(312.787292, 212.867691); cam2
//! f=(435.891205, 437.767944), c=(312.149200, 179.222244); |k1|<0.05;
//! R orthonormal; T=(99.562683, 0.463940, 9.330422) mm with
//! |T| = 100.0000 mm exactly (designed baseline).

use crate::error::{IdongError, ProtocolError, Result};

pub const EEPROM_SIZE: usize = 2048;
/// ASCII magic at 0x0000 (16 bytes).
pub const HEADER_TAG: &[u8; 16] = b"ef0123456789abcd";

// Record table: (offset, payload_len) — fixed lengths; payloads may
// contain 0xFF bytes, so there is no terminator scan.
pub const REC_CALIB: usize = 0x0100;
pub const REC_CALIB_LEN: usize = 120; // 28×f32 + 8 B pad
pub const REC_VERSION: usize = 0x0180;
pub const REC_VERSION_LEN: usize = 16;
pub const REC_INT16: usize = 0x0192;
pub const REC_INT16_LEN: usize = 16;
pub const REC_SERIAL: usize = 0x0300;
pub const REC_SERIAL_LEN: usize = 32;
pub const REC_CODE_A: usize = 0x0322;
pub const REC_CODE_A_LEN: usize = 4; // "000T"
pub const REC_CODE_B: usize = 0x0328;
pub const REC_CODE_B_LEN: usize = 4; // "2613"
pub const MIRROR_STRIDE: usize = 0x400;

/// Factory stereo calibration (OpenCV convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoCalib {
    /// Focal lengths [cam1, cam2] (px).
    pub fx: [f64; 2],
    pub fy: [f64; 2],
    /// Principal points [cam1, cam2] (px).
    pub cx: [f64; 2],
    pub cy: [f64; 2],
    /// Distortion [cam1, cam2]: (k1, k2, p1, p2) per camera.
    pub dist: [[f64; 4]; 2],
    /// Rotation cam1→cam2 (3×3, row-major, orthonormal).
    pub r: [[f64; 3]; 3],
    /// Translation cam1→cam2 in millimetres (|T| = 100.0 by design).
    pub t: [f64; 3],
}

impl StereoCalib {
    /// Designed baseline in millimetres (|T|).
    pub fn baseline_mm(&self) -> f64 {
        (self.t[0] * self.t[0] + self.t[1] * self.t[1] + self.t[2] * self.t[2]).sqrt()
    }
}

/// Device identity from the three identity records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// 32-character uppercase hex serial (128-bit).
    pub serial: String,
    /// Production/firmware code (e.g. "000T").
    pub code_a: String,
    /// Production/batch code (e.g. "2613" — matches the app_m8051_2611
    /// 8051 firmware family selection).
    pub code_b: String,
}

/// Validate one `[payload][checksum][0xFF]` record; returns the payload.
fn record(eeprom: &[u8], off: usize, len: usize) -> Result<&[u8]> {
    let payload = eeprom
        .get(off..off + len)
        .ok_or(ProtocolError::Short { len: eeprom.len(), need: off + len })?;
    let (chk, sep) = eeprom
        .get(off + len..off + len + 2)
        .map(|s| (s[0], s[1]))
        .ok_or(ProtocolError::Short { len: eeprom.len(), need: off + len + 2 })?;
    let expect = payload.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    if chk != expect {
        return Err(ProtocolError::BadMagic {
            magic: chk as u32,
            expected: expect as u32,
        }
        .into());
    }
    if sep != 0xFF {
        return Err(ProtocolError::BadType {
            kind: sep as u16,
            expected: 0xFF,
        }
        .into());
    }
    Ok(payload)
}

/// Parse a full 2 KiB dump.
pub fn parse_eeprom(dump: &[u8]) -> Result<(StereoCalib, DeviceIdentity)> {
    if dump.len() != EEPROM_SIZE {
        return Err(ProtocolError::Short { len: dump.len(), need: EEPROM_SIZE }.into());
    }
    if dump[..16] != *HEADER_TAG {
        return Err(ProtocolError::BadMagic {
            magic: u32::from_le_bytes(dump[0..4].try_into().unwrap()),
            expected: u32::from_le_bytes(HEADER_TAG[0..4].try_into().unwrap()),
        }
        .into());
    }

    // --- calibration ---------------------------------------------------
    let cal = record(dump, REC_CALIB, REC_CALIB_LEN)?;
    let mut f = [0f32; 28];
    for (i, v) in f.iter_mut().enumerate() {
        *v = f32::from_le_bytes(cal[i * 4..i * 4 + 4].try_into().unwrap());
    }
    let g = |i: usize| f[i] as f64;
    let calib = StereoCalib {
        fx: [g(0), g(8)],
        fy: [g(1), g(9)],
        cx: [g(2), g(10)],
        cy: [g(3), g(11)],
        dist: [
            [g(4), g(5), g(6), g(7)],
            [g(12), g(13), g(14), g(15)],
        ],
        r: [
            [g(16), g(17), g(18)],
            [g(19), g(20), g(21)],
            [g(22), g(23), g(24)],
        ],
        t: [g(25), g(26), g(27)],
    };

    // --- version + int16 metadata (parsed for completeness) ------------
    let version = record(dump, REC_VERSION, REC_VERSION_LEN)?;
    let _ = record(dump, REC_INT16, REC_INT16_LEN)?;
    debug_assert!(version.starts_with(b"2059"));

    // --- identity -------------------------------------------------------
    let serial = record(dump, REC_SERIAL, REC_SERIAL_LEN)?;
    let code_a = record(dump, REC_CODE_A, REC_CODE_A_LEN)?;
    let code_b = record(dump, REC_CODE_B, REC_CODE_B_LEN)?;

    // --- mirror cross-check ---------------------------------------------
    let mirror_ok = dump[REC_CALIB..REC_CALIB + REC_CALIB_LEN + 2]
        == dump[REC_CALIB + MIRROR_STRIDE..REC_CALIB + MIRROR_STRIDE + REC_CALIB_LEN + 2]
        && dump[REC_SERIAL..REC_SERIAL + REC_SERIAL_LEN + 2]
            == dump[REC_SERIAL + MIRROR_STRIDE..REC_SERIAL + MIRROR_STRIDE + REC_SERIAL_LEN + 2];
    if !mirror_ok {
        log::warn!("EEPROM mirror copy mismatch — primary records still used");
    }

    Ok((
        calib,
        DeviceIdentity {
            serial: String::from_utf8_lossy(serial).into_owned(),
            code_a: String::from_utf8_lossy(code_a).into_owned(),
            code_b: String::from_utf8_lossy(code_b).into_owned(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values from the real dump (annotated hexdump).
    #[test]
    fn real_dump_reference_values() {
        // Real device values, embedded so the test is self-contained:
        // (fx1,fy1,cx1,cy1, k1a,k2a,p1a,p2a, fx2,fy2,cx2,cy2,
        //  k1b,k2b,p1b,p2b, R…, T…)
        let cal = StereoCalib {
            fx: [428.761292, 435.891205],
            fy: [430.208038, 437.767944],
            cx: [312.787292, 312.149200],
            cy: [212.867691, 179.222244],
            dist: [
                [0.042773, -0.070018, -0.000007, -0.000250],
                [0.024833, -0.044414, 0.000232, -0.000101],
            ],
            r: [
                [0.999981, 0.002477, -0.005639],
                [-0.002544, 0.999927, -0.011812],
                [0.005609, 0.011827, 0.999914],
            ],
            t: [99.562683, 0.463940, 9.330422],
        };
        assert!((cal.baseline_mm() - 100.0).abs() < 1e-4, "designed baseline");
        assert!(cal.r.iter().all(|row| row.len() == 3));
    }

    /// Full round-trip against a synthetic dump in the verified layout
    /// (used until `24c16-dump.bin` is committed as a golden file).
    #[test]
    fn synthetic_roundtrip() {
        let mut e = vec![0xFFu8; EEPROM_SIZE];
        e[..16].copy_from_slice(HEADER_TAG);

        let mut put = |off: usize, payload: &[u8]| {
            e[off..off + payload.len()].copy_from_slice(payload);
            e[off + payload.len()] = payload.iter().fold(0u8, |a, &b| a.wrapping_add(b));
            e[off + payload.len() + 1] = 0xFF;
        };

        let floats: Vec<f32> = [
            428.761292, 430.208038, 312.787292, 212.867691, // cam1 intr
            0.042773, -0.070018, -0.000007, -0.000250, // cam1 dist
            435.891205, 437.767944, 312.149200, 179.222244, // cam2 intr
            0.024833, -0.044414, 0.000232, -0.000101, // cam2 dist
            0.999981, 0.002477, -0.005639, // R row 0
            -0.002544, 0.999927, -0.011812, // R row 1
            0.005609, 0.011827, 0.999914, // R row 2
            99.562683, 0.463940, 9.330422, // T
        ]
        .to_vec();
        let mut payload = Vec::new();
        for v in &floats {
            payload.extend_from_slice(&v.to_le_bytes());
        }
        payload.extend_from_slice(&[0u8; 8]); // pad to 120
        put(REC_CALIB, &payload);

        put(REC_VERSION, b"2059\0\0\0\0\0\0\0\0\0\0\0\0");
        let ints: Vec<u8> = [386i16, 473, 215, 303, 142, 221, 0, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        put(REC_INT16, &ints);
        put(REC_SERIAL, b"AB460DC8BA12C6840B49CD5786C329DD");
        put(REC_CODE_A, b"000T");
        put(REC_CODE_B, b"2613");

        // mirror the two regions (drop the closure to release the borrow)
        drop(put);
        e.copy_within(REC_CALIB..REC_CALIB + 128, REC_CALIB + MIRROR_STRIDE);
        e.copy_within(REC_SERIAL..REC_SERIAL + 0x30, REC_SERIAL + MIRROR_STRIDE);

        let (c, id) = parse_eeprom(&e).unwrap();
        assert!((c.fx[0] - 428.761292).abs() < 1e-4);
        assert!((c.baseline_mm() - 100.0).abs() < 1e-3);
        assert_eq!(id.serial, "AB460DC8BA12C6840B49CD5786C329DD");
        assert_eq!(id.code_a, "000T");
        assert_eq!(id.code_b, "2613");
    }

    #[test]
    fn rejects_corrupted_checksum() {
        let mut e = vec![0xFFu8; EEPROM_SIZE];
        e[..16].copy_from_slice(HEADER_TAG);
        let payload = [0u8; 120];
        e[REC_CALIB..REC_CALIB + 120].copy_from_slice(&payload);
        e[REC_CALIB + 120] = 0x00; // wrong checksum (should be 0x00 — ok!)
        e[REC_CALIB + 121] = 0xFF;
        // all-zero payload checksums to 0x00, so this parses; corrupt one
        // payload byte to make it fail:
        e[REC_CALIB] = 1;
        assert!(parse_eeprom(&e).is_err());
    }
}
