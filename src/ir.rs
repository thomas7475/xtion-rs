//! Infrared stream: configuration, frame type and the 10-bit packed
//! pixel decoder. Port of `Sensor/XnSensorIRStream.cpp` (stream setup:
//! PARAM_IR_RESOLUTION / PARAM_IR_FPS / PARAM_IR_MIRROR, STREAM0 mode 3)
//! and `Sensor/XnIRProcessor.cpp::Unpack10to16` (wire format).
//!
//! IR reuses the **image pipe**: the firmware streams IR frames as IMAGE
//! packets (0x8100..0x8500) on the image endpoint, with 4 pixels packed
//! into 5 bytes (4 x 10 bits). The reference driver keeps the firmware
//! IR-mirror off and mirrors host-side; this port returns raw frames.

use crate::error::{Result, XtionError};
use crate::protocol::Resolution;

/// IR stream configuration.
///
/// Mirrors the reference defaults (`XN_IR_STREAM_DEFAULT_RESOLUTION`,
/// `XN_IR_STREAM_DEFAULT_FPS`): VGA @ 30 fps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrConfig {
    pub resolution: Resolution,
    pub fps: u16,
}

impl Default for IrConfig {
    fn default() -> Self {
        IrConfig {
            resolution: Resolution::Vga,
            fps: 30,
        }
    }
}

impl IrConfig {
    pub fn dimensions(&self) -> (u32, u32) {
        self.resolution.dimensions()
    }
}

/// A decoded infrared frame: 16-bit grayscale per pixel, row-major.
#[derive(Debug, Clone)]
pub struct IrFrame {
    pub width: u32,
    pub height: u32,
    /// 10-bit sensor values, left-aligned into u16 (exactly what
    /// `Unpack10to16` produces: value << 2 style shifts, max 0x3FF).
    pub data: Vec<u16>,
    pub timestamp: u32,
}

impl IrFrame {
    /// Smallest and largest value in the frame (diagnostics).
    pub fn value_range(&self) -> Option<(u16, u16)> {
        let min = self.data.iter().copied().min()?;
        let max = self.data.iter().copied().max()?;
        Some((min, max))
    }
}

/// Bytes per pixel group on the wire: 4 pixels share 5 bytes
/// (`XN_INPUT_ELEMENT_SIZE` / `XN_OUTPUT_ELEMENT_SIZE` in XnIRProcessor).
const INPUT_ELEMENT_SIZE: usize = 5;

/// Expected wire size of one IR frame for `pixels` pixels.
pub fn expected_wire_size(pixels: usize) -> usize {
    pixels * 10 / 8
}

/// Decode a raw IR frame payload (4x10-bit packed) into 16-bit values.
///
/// Instruction-by-instruction port of `XnIRProcessor::Unpack10to16`:
/// per 5-byte group, pixels are assembled MSB-first as
/// `p0 = b0<<2 | b1>>6`, `p1 = (b1&0x3F)<<4 | b2>>4`,
/// `p2 = (b2&0x0F)<<6 | b3>>2`, `p3 = (b3&0x03)<<8 | b4`.
/// The reference floors the input to whole 5-byte groups and errors if
/// the output buffer is too small; we instead validate the total size up
/// front, which is the same contract for well-formed frames.
pub fn unpack_packed10(input: &[u8], out: &mut Vec<u16>) {
    out.reserve(input.len() / INPUT_ELEMENT_SIZE * 4);
    for group in input.chunks_exact(INPUT_ELEMENT_SIZE) {
        let p0 = ((group[0] as u16) << 2) | ((group[1] & 0xC0) as u16 >> 6);
        let p1 = (((group[1] & 0x3F) as u16) << 4) | ((group[2] & 0xF0) as u16 >> 4);
        let p2 = (((group[2] & 0x0F) as u16) << 6) | ((group[3] & 0xFC) as u16 >> 2);
        let p3 = (((group[3] & 0x03) as u16) << 8) | (group[4] as u16);
        out.extend_from_slice(&[p0, p1, p2, p3]);
    }
}

/// Decode one assembled IR frame payload into an [`IrFrame`], validating
/// the wire size the same way the depth path does.
pub fn decode_ir_frame(data: &[u8], width: u32, height: u32, timestamp: u32) -> Result<IrFrame> {
    let pixels = (width * height) as usize;
    let expected = expected_wire_size(pixels);
    if data.len() != expected {
        return Err(XtionError::BadConfig(format!(
            "ir frame is corrupt: {} bytes != expected {expected}",
            data.len()
        )));
    }
    let mut values = Vec::with_capacity(pixels);
    unpack_packed10(data, &mut values);
    Ok(IrFrame {
        width,
        height,
        data: values,
        timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_size_is_five_bytes_per_four_pixels() {
        assert_eq!(expected_wire_size(4), 5);
        assert_eq!(expected_wire_size(640 * 480), 384_000);
        assert_eq!(expected_wire_size(320 * 240), 96_000);
    }

    #[test]
    fn unpack_all_ones_is_max_10bit() {
        let mut out = Vec::new();
        unpack_packed10(&[0xFF; 5], &mut out);
        assert_eq!(out, vec![0x3FF, 0x3FF, 0x3FF, 0x3FF]);
    }

    #[test]
    fn unpack_all_zeros_is_zero() {
        let mut out = Vec::new();
        unpack_packed10(&[0x00; 5], &mut out);
        assert_eq!(out, vec![0, 0, 0, 0]);
    }

    /// Hand-computed golden vector, machine-verified against the C bit
    /// operations (`group = 0x69 0xCA 0x34 0xB6 0xE1`):
    ///   p0 = 0x69<<2 | (0xCA & 0xC0)>>6 = 0x1A4 | 0x3 = 0x1A7
    ///   p1 = (0xCA & 0x3F)<<4 | (0x34 & 0xF0)>>4 = 0xA0 | 0x3 = 0xA3
    ///   p2 = (0x34 & 0x0F)<<6 | (0xB6 & 0xFC)>>2 = 0x100 | 0x2D = 0x12D
    ///   p3 = (0xB6 & 0x03)<<8 | 0xE1 = 0x200 | 0xE1 = 0x2E1
    #[test]
    fn unpack_mixed_golden_vector() {
        let group = [0x69, 0xCA, 0x34, 0xB6, 0xE1];
        let mut out = Vec::new();
        unpack_packed10(&group, &mut out);
        assert_eq!(out, vec![0x1A7, 0xA3, 0x12D, 0x2E1]);
    }

    /// A second hand-checked vector exercising every mask boundary:
    /// group = 0x3F 0xFF 0x00 0x00 0x00
    ///   p0 = 0x3F<<2 | 0x3 = 0xFF
    ///   p1 = 0x3F<<4 | 0x0 = 0x3F0
    ///   p2 = 0x0<<6 | 0x0  = 0
    ///   p3 = 0x0<<8 | 0x0  = 0
    #[test]
    fn unpack_mask_boundaries() {
        let group = [0x3F, 0xFF, 0x00, 0x00, 0x00];
        let mut out = Vec::new();
        unpack_packed10(&group, &mut out);
        assert_eq!(out, vec![0xFF, 0x3F0, 0x0, 0x0]);
    }

    #[test]
    fn unpack_handles_multiple_groups_and_trailing_padding() {
        // two full groups + 3 trailing pad bytes (chunks_exact floors,
        // exactly like the C's nInputSize / XN_INPUT_ELEMENT_SIZE)
        let mut input = vec![0xFF; 5];
        input.extend_from_slice(&[0x00; 5]);
        input.extend_from_slice(&[0xAA; 3]);
        let mut out = Vec::new();
        unpack_packed10(&input, &mut out);
        assert_eq!(out, vec![0x3FF, 0x3FF, 0x3FF, 0x3FF, 0, 0, 0, 0]);
    }

    #[test]
    fn decode_validates_size_and_decodes() {
        // 8x2 frame = 16 pixels = 20 wire bytes
        let wire: Vec<u8> = (0..4).flat_map(|_| vec![0xFFu8; 5]).collect();
        let frame = decode_ir_frame(&wire, 8, 2, 77).unwrap();
        assert_eq!(frame.width, 8);
        assert_eq!(frame.data.len(), 16);
        assert!(frame.data.iter().all(|&v| v == 0x3FF));
        assert_eq!(frame.timestamp, 77);
        assert_eq!(frame.value_range(), Some((0x3FF, 0x3FF)));
    }

    #[test]
    fn decode_rejects_wrong_size() {
        let err = decode_ir_frame(&[0xFF; 15], 8, 2, 0).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("expected 20"), "{msg}");
    }

    #[test]
    fn default_config_is_vga30() {
        assert_eq!(
            IrConfig::default(),
            IrConfig {
                resolution: Resolution::Vga,
                fps: 30,
            }
        );
        assert_eq!(IrConfig::default().dimensions(), (640, 480));
    }
}
