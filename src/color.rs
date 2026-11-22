//! Color decoding: YUV422 (UYVY) to RGB888, and the PrimeSense image
//! compression decompressor.
//!
//! The Xtion's color pipe offers two YUV422 wire encodings
//! (`XnIOImageFormats`, Include/PS1080.h:253-261):
//!
//! - format 1 "Yuv422" is **PS-compressed** (the
//!   `XnPSCompressedImageProcessor` feeding
//!   `XnStreamUncompressYUVImagePS`); the reference driver's own default
//!   input format on non-Windows hosts is exactly this one, with the
//!   vendor comment *"non win32 works with BULK, so it's better to
//!   compress the image"* (XnSensorImageStream.h:36-41).
//! - format 5 "UncompressedYuv422" is plain UYVY, the Windows/ISO default.
//!
//! Uncompressed decoding: byte order `[U, Y1, V, Y2]` per 4 bytes. Exact
//! port of `Sensor/YUV.cpp` (`YUV444ToRGB888` + `YUV422ToRGB888`, Linux
//! path).

/// Convert one YUV444 triple to RGB888 (integer BT.601, as in the C code).
#[inline]
pub fn yuv444_to_rgb888(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    // C: XnInt32 nC = cY - 16; nC = nC * 298 + 128;
    let c: i32 = (i32::from(y) - 16) * 298 + 128;
    let d: i32 = i32::from(u) - 128;
    let e: i32 = i32::from(v) - 128;

    let r = ((c + 409 * e) >> 8).clamp(0, 255) as u8;
    let g = ((c - 100 * d - 208 * e) >> 8).clamp(0, 255) as u8;
    let b = ((c + 516 * d) >> 8).clamp(0, 255) as u8;
    (r, g, b)
}

/// Convert a YUV422 (UYVY) buffer to an interleaved RGB888 buffer.
///
/// Consumes 4 bytes (2 pixels) at a time; a trailing partial group is
/// ignored, mirroring the reference implementation.
pub fn yuv422_to_rgb888(input: &[u8], output: &mut Vec<u8>) {
    output.reserve(input.len() / 4 * 6);
    for group in input.chunks_exact(4) {
        // byte order: U, Y1, V, Y2
        let (r1, g1, b1) = yuv444_to_rgb888(group[1], group[0], group[2]);
        let (r2, g2, b2) = yuv444_to_rgb888(group[3], group[0], group[2]);
        output.extend_from_slice(&[r1, g1, b1, r2, g2, b2]);
    }
}

/// PrimeSense image compression: decode one frame of 4-bit elements to
/// raw UYVY. Port of `XnStreamUncompressYUVImagePS`
/// (Sensor/Uncomp.cpp:45-180).
///
/// The stream is a sequence of 4-bit elements, high nibble of a byte
/// first (the C's `bReadByte` toggle):
///
/// - element `0x0..=0xC` is a **diff**: `element - 6` is added to the
///   current channel (wrapping `u8` arithmetic);
/// - element `0xD` is a **dummy** (nothing is produced);
/// - element `0xF` is a **full** byte: two more nibbles follow with the
///   value.
///
/// Four channels run in rotation — output order Y0, U, Y1, V (= UYVY).
/// After U is decoded its value is copied to V (the C's `case 2:`), and
/// after V is decoded U is restored from V (`case 4:`), so one diff
/// updates both chroma samples of the 2-pixel group. Channel state
/// resets at every line boundary (`line_size` output bytes; the C's
/// callers pass `x_res * 2`).
///
/// `input` is one complete frame payload (state never spans frames);
/// unlike the C's chunked streaming call this is a whole-frame one-shot,
/// so the "commit only whole lines unless last part" resync logic is
/// inherent — a trailing partial line is truncated by the caller's
/// output-size check, exactly like the C's `bLastPart` path.
pub fn uncompress_yuv_image_ps(input: &[u8], line_size: usize, output: &mut Vec<u8>) {
    output.reserve(input.len() * 2 + line_size);

    // nLastFullValue: [Y0, U, Y1, V]
    let mut last = [0u8; 4];
    let mut channel = 0usize;
    let mut cur_line_size = 0usize;
    // (pInput, bReadByte) of the C loop
    let mut byte_pos = 0usize;
    let mut high_element = true;

    while byte_pos < input.len() {
        let byte = input[byte_pos];
        let element = if high_element { byte >> 4 } else { byte & 0x0f };

        if high_element {
            // C high branch: bReadByte = FALSE; classify on the whole
            // byte (equivalent to the high nibble); a dummy does NOT
            // advance pInput, a full consumes this byte's low nibble and
            // the next byte's high nibble.
            high_element = false;
            if element < 0xd {
                last[channel] = last[channel].wrapping_add((element as i8 - 6) as u8);
            } else if element < 0xe {
                continue;
            } else {
                byte_pos += 1;
                if byte_pos >= input.len() {
                    break; // C: ++pInput == pInputEnd -> break
                }
                last[channel] = ((byte & 0x0f) << 4) | (input[byte_pos] >> 4);
            }
        } else {
            // C low branch: consume this byte, then classify on the low
            // nibble; a full takes the entire next byte.
            high_element = true;
            byte_pos += 1;
            if element < 0xd {
                last[channel] = last[channel].wrapping_add((element as i8 - 6) as u8);
            } else if element < 0xe {
                continue;
            } else {
                if byte_pos >= input.len() {
                    break; // C: pInput == pInputEnd -> break
                }
                last[channel] = input[byte_pos];
                byte_pos += 1;
            }
        }

        // write output
        output.push(last[channel]);
        channel += 1;
        match channel {
            2 => last[3] = last[1], // V := U
            4 => {
                last[1] = last[3]; // U := V
                channel = 0;
            }
            _ => {}
        }

        cur_line_size += 1;
        if cur_line_size == line_size {
            last = [0; 4];
            cur_line_size = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // PS-compression test helpers: a minimal encoder inverse of the C
    // decompressor. Elements are emitted as a flat nibble list and packed
    // two-per-byte, high nibble first — which matches the decoder's
    // interleaving (a high-position full consumes the current byte's low
    // nibble and the next byte's high nibble).
    // ------------------------------------------------------------------

    fn pack_nibbles(nibbles: &[u8]) -> Vec<u8> {
        assert!(nibbles.len() % 2 == 0, "encoder emits whole bytes");
        nibbles
            .chunks_exact(2)
            .map(|p| (p[0] << 4) | (p[1] & 0x0f))
            .collect()
    }

    fn push_full(value: u8, nib: &mut Vec<u8>) {
        nib.push(0xF);
        nib.push(value >> 4);
        nib.push(value & 0x0f);
    }

    fn push_diff(delta: i8, nib: &mut Vec<u8>) {
        assert!((-6..=6).contains(&delta));
        nib.push((delta + 6) as u8);
    }

    #[test]
    fn decompresses_full_only_stream() {
        // Y0=0x12, U=0x34, Y1=0xAB, V=0xC0 — every byte full
        let mut nib = Vec::new();
        for value in [0x12u8, 0x34, 0xAB, 0xC0] {
            push_full(value, &mut nib);
        }
        let input = pack_nibbles(&nib);
        let mut out = Vec::new();
        uncompress_yuv_image_ps(&input, 8, &mut out);
        assert_eq!(out, vec![0x12, 0x34, 0xAB, 0xC0]);
    }

    #[test]
    fn decompresses_diffs_with_uv_carry() {
        // One UYVY pair + start of the next:
        // Y0=0x80 full, U=0x40 full, Y1=+2 diff (from 0: line start),
        // V=+1 diff (V inherited U=0x40 via the C's case 2),
        // Y0'=-2 diff (0x7E), U'=-2 diff (U restored from V=0x41: 0x3F).
        let mut nib = Vec::new();
        push_full(0x80, &mut nib);
        push_full(0x40, &mut nib);
        push_diff(2, &mut nib);
        push_diff(1, &mut nib);
        push_diff(-2, &mut nib);
        push_diff(-2, &mut nib);
        let input = pack_nibbles(&nib);
        let mut out = Vec::new();
        uncompress_yuv_image_ps(&input, 8, &mut out);
        assert_eq!(out, vec![0x80, 0x40, 0x02, 0x41, 0x7E, 0x3F]);
    }

    #[test]
    fn dummy_elements_produce_no_output() {
        // Y0=0xFF full, U=0x40 full, dummy (no output), V full 0xC0.
        let mut nib = Vec::new();
        push_full(0xFF, &mut nib);
        push_full(0x40, &mut nib);
        nib.push(0xD); // dummy element
        nib.push(0xF); // ...the full marker
        nib.push(0xC);
        nib.push(0x0);
        let input = pack_nibbles(&nib);
        let mut out = Vec::new();
        uncompress_yuv_image_ps(&input, 8, &mut out);
        assert_eq!(out, vec![0xFF, 0x40, 0xC0]);
    }

    #[test]
    fn line_boundary_resets_channel_state() {
        // line_size = 4 output bytes = one UYVY pair.
        // line 1: Y0=0xFF full, U=0x80 full, Y1=+1 diff (0x01),
        //         V=0xC0 full -> line boundary resets channels
        // line 2: Y0=0x11 full (must NOT diff from 0xFF), U=0x22 full
        let mut nib = Vec::new();
        push_full(0xFF, &mut nib);
        push_full(0x80, &mut nib);
        push_diff(1, &mut nib);
        push_full(0xC0, &mut nib);
        push_full(0x11, &mut nib);
        push_full(0x22, &mut nib);
        let input = pack_nibbles(&nib);
        let mut out = Vec::new();
        uncompress_yuv_image_ps(&input, 4, &mut out);
        assert_eq!(out, vec![0xFF, 0x80, 0x01, 0xC0, 0x11, 0x22]);
    }

    #[test]
    fn decompressed_frame_converts_to_rgb() {
        // a neutral-gray line (Y=128, U=V=128) decodes to mid gray
        let mut nib = Vec::new();
        for _ in 0..4 {
            push_full(128, &mut nib);
        }
        let input = pack_nibbles(&nib);
        let mut yuv = Vec::new();
        uncompress_yuv_image_ps(&input, 8, &mut yuv);
        assert_eq!(yuv, vec![128, 128, 128, 128]);
        let mut rgb = Vec::new();
        yuv422_to_rgb888(&yuv, &mut rgb);
        assert_eq!(rgb.len(), 6);
        assert!(rgb[..3].iter().all(|&c| (125..=135).contains(&c)));
    }

    #[test]
    fn truncated_input_stops_cleanly() {
        // a full marker whose value nibbles are cut off must not panic
        // or write a spurious byte
        let mut out = Vec::new();
        uncompress_yuv_image_ps(&[0xF0], 8, &mut out);
        assert!(out.is_empty());
        // a complete full followed by a trailing 0 nibble: the nibble is
        // a -6 diff on the next channel (U), not garbage
        uncompress_yuv_image_ps(&[0xFA, 0x00], 8, &mut out);
        assert_eq!(out, vec![0xA0, 0xFA]);
    }

    #[test]
    fn black_and_white_endpoints() {
        // Y=16 (black), Y=235 (white) with neutral chroma
        let black = yuv444_to_rgb888(16, 128, 128);
        assert_eq!(black, (0, 0, 0));
        let white = yuv444_to_rgb888(235, 128, 128);
        assert_eq!(white, (255, 255, 255));
    }

    #[test]
    fn neutral_gray() {
        let (r, g, b) = yuv444_to_rgb888(128, 128, 128);
        assert_eq!(r, g);
        assert_eq!(g, b, "neutral chroma must give a gray");
        assert!((125..=135).contains(&r), "Y=128 -> mid gray, got {r}");
    }

    #[test]
    fn uvg_order_and_pixel_count() {
        // U=0x10, Y1=0x20, V=0x30, Y2=0x40
        let input = [0x10, 0x20, 0x30, 0x40];
        let mut out = Vec::new();
        yuv422_to_rgb888(&input, &mut out);
        assert_eq!(out.len(), 6);
        // both pixels share chroma, differ only in luma: pixel 2 must be
        // brighter than pixel 1
        let luma1 = out[0] as u32 + out[1] as u32 + out[2] as u32;
        let luma2 = out[3] as u32 + out[4] as u32 + out[5] as u32;
        assert!(luma2 > luma1);
    }

    #[test]
    fn trailing_partial_group_ignored() {
        let mut out = Vec::new();
        yuv422_to_rgb888(&[0x10, 0x20, 0x30, 0x40, 0x50], &mut out);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn vga_frame_size() {
        // 640*480 pixels * 2 bytes per pixel in YUV422
        let input = vec![128u8; 640 * 480 * 2];
        let mut out = Vec::new();
        yuv422_to_rgb888(&input, &mut out);
        assert_eq!(out.len(), 640 * 480 * 3);
    }
}
