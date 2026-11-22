//! Color decoding: YUV422 (UYVY) to RGB888.
//!
//! The Xtion's default image input format is uncompressed YUV422, byte
//! order `[U, Y1, V, Y2]` per 4 bytes. Exact port of `Sensor/YUV.cpp`
//! (`YUV444ToRGB888` + `YUV422ToRGB888`, Linux path).

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

#[cfg(test)]
mod tests {
    use super::*;

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
