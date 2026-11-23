//! Infrared stream: configuration, frame type and the 10-bit packed
//! pixel decoder. Port of `Sensor/XnSensorIRStream.cpp` (stream setup:
//! PARAM_IR_RESOLUTION / PARAM_IR_FPS / PARAM_IR_MIRROR, STREAM0 mode 3)
//! and `Sensor/XnIRProcessor.cpp::Unpack10to16` (wire format).
//!
//! IR reuses the **image pipe**: the firmware streams IR frames as IMAGE
//! packets (0x8100..0x8500) on the image endpoint, with 4 pixels packed
//! into 5 bytes (4 x 10 bits). The IR CMOS (an MT9M001) transmits
//! `height + 8` wire rows in every resolution except SXGA; frames are
//! validated against the wire size and cropped back to `height` visible
//! rows (see `wire_extra_rows` / `ir_wire_size` / `decode_ir_frame`).
//! The reference driver keeps the firmware IR-mirror off and mirrors
//! host-side; this port returns raw frames.

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
    /// The sub-window this frame represents (`None` = full frame).
    pub crop: Option<crate::device::Crop>,
    /// 10-bit sensor values, left-aligned into u16 (exactly what
    /// `Unpack10to16` produces: value << 2 style shifts, max 0x3FF).
    pub data: Vec<u16>,
    pub timestamp: u32,
    /// Host-referenced microseconds since the session's first frame
    /// (see [`crate::DepthFrame::timestamp_us`]).
    pub timestamp_us: Option<u64>,
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

/// Depth CMOS types as reported in the fixed params (`nDepthCmosType`);
/// enum `XnDepthCMOSType`, Include/PS1080.h:418-423.
pub const DEPTH_CMOS_NONE: i32 = 0;
pub const DEPTH_CMOS_MT9M001: i32 = 1;
pub const DEPTH_CMOS_AR130: i32 = 2;

/// Extra wire rows the IR CMOS transmits beyond the configured Y
/// resolution.
///
/// C reference `XnIRProcessor.cpp:244-271`: "there are additional 8 rows
/// (this is how the CMOS is configured)" — the MT9M001 transmits
/// `height + 8` rows in every resolution except SXGA. At SXGA an AR130
/// loses 64 rows. Any other CMOS type transmits exactly `height` rows.
///
/// Hardware-confirmed (0.2.2 diagnostics run): VGA frames close at
/// 390,400 bytes = 488 rows x 800 B, i.e. 8 extra rows, not 480.
pub fn wire_extra_rows(depth_cmos_type: i32, resolution: Resolution) -> i32 {
    if resolution == Resolution::Sxga {
        if depth_cmos_type == DEPTH_CMOS_AR130 {
            -64
        } else {
            0
        }
    } else if depth_cmos_type == DEPTH_CMOS_MT9M001 {
        8
    } else {
        0
    }
}

/// Expected wire size of one IR frame:
/// `(height + extra CMOS rows) x width` packed-10 pixels.
///
/// The reference does the same accounting host-side: "in IR, in all
/// resolutions except SXGA, we get additional 8 lines"
/// (`XnSensorIRStream::CalcRequiredSize`, XnSensorIRStream.cpp:411-422).
pub fn ir_wire_size(
    width: u32,
    height: u32,
    depth_cmos_type: i32,
    resolution: Resolution,
) -> usize {
    let rows = height as i64 + wire_extra_rows(depth_cmos_type, resolution) as i64;
    expected_wire_size((width as usize) * rows.max(0) as usize)
}

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

/// Decode one assembled IR frame payload into an [`IrFrame`].
///
/// The wire carries `height + wire_extra_rows(...)` rows. The reference
/// validates the (unpacked) frame against the extra-rows height and then
/// crops the trailing rows off the delivered frame — "don't report
/// additional rows out" (XnIRProcessor.cpp:273-282): the FIRST `height`
/// rows are kept, the last `extra` rows are discarded. We mirror that
/// exactly: validate the wire size, then unpack only the first `height`
/// rows.
pub fn decode_ir_frame(
    data: &[u8],
    width: u32,
    height: u32,
    resolution: Resolution,
    depth_cmos_type: i32,
    timestamp: u32,
) -> Result<IrFrame> {
    let wire = ir_wire_size(width, height, depth_cmos_type, resolution);
    if data.len() != wire {
        let rows = height as i32 + wire_extra_rows(depth_cmos_type, resolution);
        return Err(XtionError::BadConfig(format!(
            "ir frame is corrupt: {} bytes != expected {wire} ({width}x{rows} packed-10, depth cmos {depth_cmos_type})",
            data.len()
        )));
    }
    let pixels = (width * height) as usize;
    let visible = expected_wire_size(pixels);
    if wire < visible {
        // Only reachable for the AR130-at-SXGA combination, where the
        // CMOS transmits FEWER rows than the frame reports. The reference
        // would report more pixels than it gathered (a bug); we refuse
        // instead of fabricating rows.
        return Err(XtionError::BadConfig(format!(
            "ir wire ({wire} bytes) smaller than the visible frame ({visible} bytes): \
             depth cmos {depth_cmos_type} cannot serve {width}x{height} IR"
        )));
    }
    let mut values = Vec::with_capacity(pixels);
    unpack_packed10(&data[..visible], &mut values);
    Ok(IrFrame {
        width,
        height,
        crop: None,
        data: values,
        timestamp,
        timestamp_us: None,
    })
}

/// Decode one CROPPED IR frame payload into an [`IrFrame`].
///
/// While a firmware crop window is active the IR CMOS transmits
/// exactly `crop.width x crop.height` pixels: the MT9M001's extra-rows
/// quirk applies to FULL frames only — "when cropping is turned on,
/// actual depth size is smaller ... `actualHeight = height`"
/// (XnIRProcessor.cpp:249-255). The wire size is therefore exactly
/// `width * height` packed-10 pixels, with no trailing rows to strip.
pub fn decode_ir_frame_cropped(
    data: &[u8],
    crop: crate::device::Crop,
    timestamp: u32,
) -> Result<IrFrame> {
    let width = crop.width as u32;
    let height = crop.height as u32;
    let pixels = (width * height) as usize;
    let wire = expected_wire_size(pixels);
    if data.len() != wire {
        return Err(XtionError::BadConfig(format!(
            "cropped ir frame is corrupt: {} bytes != expected {wire} \
             ({width}x{height} packed-10, no extra CMOS rows while cropped)",
            data.len()
        )));
    }
    let mut values = Vec::with_capacity(pixels);
    unpack_packed10(data, &mut values);
    Ok(IrFrame {
        width,
        height,
        crop: Some(crop),
        data: values,
        timestamp,
        timestamp_us: None,
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

    /// Inverse of `unpack_packed10` for one row of 10-bit values
    /// (width must be a multiple of 4): used to build synthetic wires.
    fn pack_row(width: u32, value: u16) -> Vec<u8> {
        debug_assert!(value < 0x400);
        let mut out = Vec::with_capacity(width as usize * 10 / 8);
        for _ in (0..width).step_by(4) {
            let p0 = value;
            let p1 = value;
            let p2 = value;
            let p3 = value;
            out.push((p0 >> 2) as u8);
            out.push(((((p0 & 0x3) << 6) | (p1 >> 4)) & 0xFF) as u8);
            out.push(((((p1 & 0xF) << 4) | (p2 >> 6)) & 0xFF) as u8);
            out.push(((((p2 & 0x3F) << 2) | (p3 >> 8)) & 0xFF) as u8);
            out.push((p3 & 0xFF) as u8);
        }
        out
    }

    #[test]
    fn extra_rows_follow_cmos_type_matrix() {
        // MT9M001: +8 rows in every resolution except SXGA
        // (XnIRProcessor.cpp:256-263).
        assert_eq!(wire_extra_rows(DEPTH_CMOS_MT9M001, Resolution::Vga), 8);
        assert_eq!(wire_extra_rows(DEPTH_CMOS_MT9M001, Resolution::Qvga), 8);
        assert_eq!(wire_extra_rows(DEPTH_CMOS_MT9M001, Resolution::Qqvga), 8);
        assert_eq!(wire_extra_rows(DEPTH_CMOS_MT9M001, Resolution::Sxga), 0);
        // AR130: only at SXGA, and it LOSES 64 rows (lines 264-271).
        assert_eq!(wire_extra_rows(DEPTH_CMOS_AR130, Resolution::Sxga), -64);
        assert_eq!(wire_extra_rows(DEPTH_CMOS_AR130, Resolution::Vga), 0);
        // Unknown/none: no adjustment (C checks the exact type).
        assert_eq!(wire_extra_rows(DEPTH_CMOS_NONE, Resolution::Vga), 0);
    }

    #[test]
    fn vga_mt9m001_wire_frame_is_488_rows() {
        // Hardware-observed in the 0.2.2 diagnostics run: frames close
        // at exactly 390,400 bytes (488 rows x 800 B packed-10).
        let wire = ir_wire_size(640, 480, DEPTH_CMOS_MT9M001, Resolution::Vga);
        assert_eq!(wire, 390_400);
        assert_eq!(wire - expected_wire_size(640 * 480), 6_400); // 8 rows
                                                                 // Without an MT9M001 the old 384,000-byte expectation applies.
        assert_eq!(
            ir_wire_size(640, 480, DEPTH_CMOS_NONE, Resolution::Vga),
            384_000
        );
        // QVGA MT9M001: 248 rows x 400 B.
        assert_eq!(
            ir_wire_size(320, 240, DEPTH_CMOS_MT9M001, Resolution::Qvga),
            99_200
        );
    }

    #[test]
    fn decode_validates_size_and_decodes() {
        // 8x2 frame = 16 pixels = 20 wire bytes (no extra CMOS rows)
        let wire: Vec<u8> = (0..4).flat_map(|_| vec![0xFFu8; 5]).collect();
        let frame = decode_ir_frame(&wire, 8, 2, Resolution::Vga, DEPTH_CMOS_NONE, 77).unwrap();
        assert_eq!(frame.width, 8);
        assert_eq!(frame.data.len(), 16);
        assert!(frame.data.iter().all(|&v| v == 0x3FF));
        assert_eq!(frame.timestamp, 77);
        assert_eq!(frame.value_range(), Some((0x3FF, 0x3FF)));
    }

    /// The 0.2.2 on-camera failure, as a regression test: an MT9M001
    /// transmits `height + 8` rows; frames closed at 390,400 bytes and
    /// the old 384,000-byte expectation rejected every frame.
    #[test]
    fn decode_accepts_hardware_wire_and_crops_extra_rows() {
        let (width, height) = (8u32, 2u32);
        // Wire: 10 rows (2 visible + 8 CMOS rows), row r carries 0x100+r.
        let mut wire = Vec::new();
        for row in 0..10u16 {
            wire.extend(pack_row(width, 0x100 + row));
        }
        assert_eq!(
            wire.len(),
            ir_wire_size(width, height, DEPTH_CMOS_MT9M001, Resolution::Vga)
        );
        let frame =
            decode_ir_frame(&wire, width, height, Resolution::Vga, DEPTH_CMOS_MT9M001, 5).unwrap();
        // Only the FIRST `height` rows survive (XnIRProcessor.cpp:281-282
        // "don't report additional rows out").
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.data.len(), (width * height) as usize);
        assert!(frame.data[..8].iter().all(|&v| v == 0x100));
        assert!(frame.data[8..16].iter().all(|&v| v == 0x101));
        assert!(frame.data.iter().all(|&v| v < 0x102), "extra rows leaked");
    }

    #[test]
    fn decode_rejects_wrong_size() {
        let err =
            decode_ir_frame(&[0xFF; 15], 8, 2, Resolution::Vga, DEPTH_CMOS_NONE, 0).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("expected 20"), "{msg}");
        // The hardware failure signature: 390,400 bytes against the old
        // 384,000-byte expectation must now be VALID for an MT9M001.
        let ok = decode_ir_frame(
            &vec![0x7Fu8; 390_400],
            640,
            480,
            Resolution::Vga,
            DEPTH_CMOS_MT9M001,
            0,
        )
        .unwrap();
        assert_eq!(ok.data.len(), 640 * 480);
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

    /// Cropped IR frames: exactly `crop.width x crop.height` packed-10
    /// pixels, NO extra CMOS rows (the MT9M001 `height + 8` quirk is a
    /// full-frame property only — XnIRProcessor.cpp:249-255), and the
    /// frame reports the crop window.
    #[test]
    fn decode_cropped_frame_has_no_extra_rows() {
        use crate::device::Crop;
        let crop = Crop {
            x: 160,
            y: 120,
            width: 320,
            height: 240,
        };
        // 320x240 = 76,800 pixels = 96,000 wire bytes — the FULL-frame
        // MT9M001 wire size would be 320 * 248 * 10/8 = 99,200 bytes.
        let wire = vec![0x55u8; 96_000];
        let frame = decode_ir_frame_cropped(&wire, crop, 42).unwrap();
        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);
        assert_eq!(frame.crop, Some(crop));
        assert_eq!(frame.data.len(), 76_800);
        assert_eq!(frame.timestamp, 42);

        // wrong size (e.g. a full 248-row frame) must be rejected
        let err = decode_ir_frame_cropped(&vec![0u8; 99_200], crop, 0).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("expected 96000"), "{msg}");
        assert!(msg.contains("no extra CMOS rows"), "{msg}");
    }
}
