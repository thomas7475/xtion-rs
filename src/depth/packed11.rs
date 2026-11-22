//! "PS compressed" depth: 4 x 11-bit shift values packed into 11 bytes.
//! Exact port of `Sensor/XnPacked11DepthProcessor.cpp::Unpack11to16`.

/// Unpack packed-11 depth data into 16-bit shift values.
///
/// Input is consumed 11 bytes at a time, producing 8 `u16` outputs; a
/// trailing partial group (fewer than 11 bytes) is ignored, mirroring the
/// C implementation (`nElements = nInputSize / 11`, floored).
///
/// Bit layout (from the reference implementation):
/// ```text
/// /// input:    0,  1,  2,3,  4,  5,  6,7,  8,  9,10
/// ///           -,---,---,-,---,---,---,-,---,---,-
/// /// bits:     8,3,5,6,2,8,1,7,4,4,7,1,8,2,6,5,3,8
/// ///           ---,---,-----,---,---,-----,---,---
/// /// output:     0,  1,    2,  3,  4,    5,  6,  7
/// ```
pub fn unpack_packed11(input: &[u8], output: &mut Vec<u16>) {
    let elements = input.len() / 11;
    output.reserve(elements * 8);
    for elem in 0..elements {
        let b = &input[elem * 11..elem * 11 + 11];
        output.push((u16::from(b[0]) << 3) | (b[1] >> 5) as u16);
        output.push(((u16::from(b[1]) & 0x1f) << 6) | u16::from(b[2] >> 2));
        output
            .push(((u16::from(b[2]) & 0x03) << 9) | (u16::from(b[3]) << 1) | u16::from(b[4] >> 7));
        output.push(((u16::from(b[4]) & 0x7f) << 4) | u16::from(b[5] >> 4));
        output.push(((u16::from(b[5]) & 0x0f) << 7) | u16::from(b[6] >> 1));
        output
            .push(((u16::from(b[6]) & 0x01) << 10) | (u16::from(b[7]) << 2) | u16::from(b[8] >> 6));
        output.push(((u16::from(b[8]) & 0x3f) << 5) | u16::from(b[9] >> 3));
        output.push(((u16::from(b[9]) & 0x07) << 8) | u16::from(b[10]));
    }
}

/// Convenience wrapper returning a fresh vector.
pub fn unpack_packed11_vec(input: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    unpack_packed11(input, &mut out);
    out
}

/// Pack 11-bit values back into the wire format (inverse of
/// [`unpack_packed11`]; used by tests to build golden vectors and by the
/// on-hardware validation tooling).
pub fn pack_to_packed11(values: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() / 8 * 11);
    for group in values.chunks(8) {
        let mut group = group.to_vec();
        group.resize(8, 0);
        let v: [u16; 8] = [
            group[0] & 0x7ff,
            group[1] & 0x7ff,
            group[2] & 0x7ff,
            group[3] & 0x7ff,
            group[4] & 0x7ff,
            group[5] & 0x7ff,
            group[6] & 0x7ff,
            group[7] & 0x7ff,
        ];
        out.push((v[0] >> 3) as u8);
        out.push((((v[0] & 0x7) << 5) | (v[1] >> 6)) as u8);
        out.push((((v[1] & 0x3f) << 2) | (v[2] >> 9)) as u8);
        out.push((v[2] >> 1) as u8);
        out.push((((v[2] & 0x1) << 7) | (v[3] >> 4)) as u8);
        out.push((((v[3] & 0xf) << 4) | (v[4] >> 7)) as u8);
        out.push((((v[4] & 0x7f) << 1) | (v[5] >> 10)) as u8);
        out.push((v[5] >> 2) as u8);
        out.push((((v[5] & 0x3) << 6) | (v[6] >> 5)) as u8);
        out.push((((v[6] & 0x1f) << 3) | (v[7] >> 8)) as u8);
        out.push((v[7] & 0xff) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_ones_unpacks_to_max_shift() {
        // 11 bytes of 0xFF -> every 11-bit group is 0x7FF
        let out = unpack_packed11_vec(&[0xff; 11]);
        assert_eq!(out.len(), 8);
        assert!(out.iter().all(|&v| v == 0x7ff), "{:?}", out);
    }

    #[test]
    fn zeros_unpack_to_zero() {
        let out = unpack_packed11_vec(&[0x00; 22]);
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|&v| v == 0));
    }

    #[test]
    fn known_vector_first_group() {
        // Hand-derived from the bit layout:
        // b0=0xFF, b1=0xFF -> a0 = (0xFF<<3)|(0xFF>>5) = 0x7FF
        // a1 = (b1&0x1f)<<6 | b2>>2 with b1=0xFF,b2=0x00 -> 0x7C0
        // a2 = (b2&3)<<9 | b3<<1 | b4>>7 = 0 with b2..b4 = 0
        let out = unpack_packed11_vec(&{
            let mut b = vec![0u8; 11];
            b[0] = 0xFF;
            b[1] = 0xFF;
            b
        });
        assert_eq!(out[0], 0x7ff);
        assert_eq!(out[1], 0x7c0);
        assert_eq!(out[2], 0);
    }

    #[test]
    fn partial_trailing_group_is_ignored() {
        // 11 + 5 bytes: only the first group is decoded
        let mut input = vec![0u8; 16];
        input[0] = 0xff;
        let out = unpack_packed11_vec(&input);
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let values: Vec<u16> = (0..64).map(|i| (i * 131) % 2048).collect();
        let packed = pack_to_packed11(&values);
        assert_eq!(packed.len(), 64 / 8 * 11);
        let unpacked = unpack_packed11_vec(&packed);
        assert_eq!(unpacked, values);
    }

    #[test]
    fn unpacks_typical_vga_frame_size() {
        // 640*480 pixels = 307200 values -> 307200/8*11 = 422400 bytes
        let packed = pack_to_packed11(&vec![1234u16; 307200]);
        let out = unpack_packed11_vec(&packed);
        assert_eq!(out.len(), 307200);
        assert!(out.iter().all(|&v| v == 1234));
    }
}
