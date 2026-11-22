//! GET_FIXED_PARAMS: device calibration and hardware constants.
//!
//! The firmware returns a 168-byte (V3.0+ layout) packed struct of i32/f32
//! fields, read in chunks addressed by a 32-bit-word offset. Port of
//! `XnHostProtocolGetFixedParams` and `XnFixedParams` (XnDeviceSensor.h).

use crate::error::{Result, XtionError};

use super::{CommandLink, Opcode};
use std::time::Duration;

/// Fixed parameters of a PS1080 device (V3.0+ layout, 168 bytes on the
/// wire, all little-endian). Only fields relevant to the driver are kept;
/// see docs/PROTOCOL.md for the complete layout with offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedParams {
    /// Flash serial number of the sensor.
    pub serial_number: i32,
    /// Depth CMOS type (1 = MT9M001, 2 = AR130).
    pub depth_cmos_type: i32,
    /// Image CMOS type (1 = MT9M112, 2 = MT9D131, 3 = MT9M114).
    pub image_cmos_type: i32,
    /// Emitter-to-depth-CMOS distance (LDDIS), in centimeters.
    pub d_cmos_emitter_distance: f32,
    /// Depth-CMOS-to-RGB-CMOS distance, in centimeters.
    pub d_cmos_r_cmos_distance: f32,
    /// Zero plane distance (ZPD), in depth units (millimeters).
    pub reference_distance: f32,
    /// Zero plane pixel size (ZPPS), in millimeters.
    pub reference_pixel_size: f32,
    /// PLL value used for clock generation.
    pub pll_value: i32,
    /// Sensor type.
    pub sensor_type: i32,
}

/// Total size of the V3.0+ fixed-params struct the firmware streams out.
pub const FIXED_PARAMS_V3_SIZE: usize = 168;

impl FixedParams {
    /// Parse the 168-byte V3.0+ layout.
    ///
    /// Field order mirrors `XnFixedParams` (Source/Drivers/PS1080/Sensor/
    /// XnDeviceSensor.h): 11 misc/flash words, audio, 5 depth words,
    /// 5 image words, geometry (1 i32 + 4 f32), 7 clock words, 8 tail words.
    pub fn parse_v3(buf: &[u8]) -> Result<FixedParams> {
        if buf.len() < FIXED_PARAMS_V3_SIZE {
            return Err(XtionError::BadConfig(format!(
                "fixed params blob too short: {} bytes (need {})",
                buf.len(),
                FIXED_PARAMS_V3_SIZE
            )));
        }
        let i32_at = |off: usize| -> i32 {
            i32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
        };
        let f32_at = |off: usize| -> f32 {
            f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
        };

        Ok(FixedParams {
            // offset 0
            serial_number: i32_at(0),
            // depth CMOS block starts at 48 (11 misc/flash + 1 audio, in words)
            depth_cmos_type: i32_at(48 + 2 * 4),
            // image CMOS block starts at 68
            image_cmos_type: i32_at(68 + 2 * 4),
            // geometry block starts at 88: ir_cmos_close_to_projector, then 4 floats
            d_cmos_emitter_distance: f32_at(88 + 4),
            d_cmos_r_cmos_distance: f32_at(88 + 8),
            reference_distance: f32_at(88 + 12),
            reference_pixel_size: f32_at(88 + 16),
            // clocks block starts at 108
            pll_value: i32_at(108),
            // tail block starts at 136: i2c_hold_fix, sensor_type, ...
            sensor_type: i32_at(136 + 4),
        })
    }
}

/// Read the full fixed-params blob from the device, chunk by chunk.
///
/// Each request carries a word offset (`nDataRead / 4`); each reply
/// contributes `nDataSize * 2` bytes.
pub fn read_fixed_params<T: super::command::ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<FixedParams> {
    let mut blob = Vec::with_capacity(FIXED_PARAMS_V3_SIZE);
    while blob.len() < FIXED_PARAMS_V3_SIZE {
        let offset_words = (blob.len() / 4) as u16;
        let data = link.exchange(
            transport,
            Opcode::GetFixedParams,
            &offset_words.to_le_bytes(),
            timeout,
        )?;
        if data.is_empty() {
            break;
        }
        blob.extend_from_slice(&data);
    }
    FixedParams::parse_v3(&blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_fields_at_documented_offsets() {
        let mut buf = vec![0u8; FIXED_PARAMS_V3_SIZE];
        // serial at 0
        buf[0..4].copy_from_slice(&0x00AB_i32.to_le_bytes());
        // depth_cmos_type at 56
        buf[56..60].copy_from_slice(&2_i32.to_le_bytes());
        // image_cmos_type at 76
        buf[76..80].copy_from_slice(&1_i32.to_le_bytes());
        // geometry floats at 92..108
        buf[92..96].copy_from_slice(&7.5f32.to_le_bytes());
        buf[96..100].copy_from_slice(&(-0.25f32).to_le_bytes());
        buf[100..104].copy_from_slice(&120.0f32.to_le_bytes());
        buf[104..108].copy_from_slice(&0.1052f32.to_le_bytes());
        buf[108..112].copy_from_slice(&42_i32.to_le_bytes());
        buf[140..144].copy_from_slice(&5_i32.to_le_bytes());

        let fp = FixedParams::parse_v3(&buf).unwrap();
        assert_eq!(fp.serial_number, 0xAB);
        assert_eq!(fp.depth_cmos_type, 2);
        assert_eq!(fp.image_cmos_type, 1);
        assert_eq!(fp.d_cmos_emitter_distance, 7.5);
        assert_eq!(fp.d_cmos_r_cmos_distance, -0.25);
        assert_eq!(fp.reference_distance, 120.0);
        assert_eq!(fp.reference_pixel_size, 0.1052);
        assert_eq!(fp.pll_value, 42);
        assert_eq!(fp.sensor_type, 5);
    }

    #[test]
    fn rejects_short_blob() {
        let err = FixedParams::parse_v3(&[0u8; 100]).unwrap_err();
        assert!(matches!(err, XtionError::BadConfig(_)));
    }
}
