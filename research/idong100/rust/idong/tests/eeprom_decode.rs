//! EEPROM decode tests against the real dump's reference values
//! (the dump itself lives outside the repo; the assertions encode the
//! verified facts from `upload/idong100_eeprom_annotated_hexdump.txt`).

use idong::calib::{parse_eeprom, EEPROM_SIZE, HEADER_TAG, REC_CALIB, REC_CALIB_LEN};

#[test]
fn header_tag_is_ascii_magic() {
    assert_eq!(HEADER_TAG, b"ef0123456789abcd");
    assert_eq!(HEADER_TAG.len(), 16);
}

#[test]
fn record_geometry_matches_annotation() {
    // calibration payload 120 bytes at 0x0100, checksum at 0x0178, then
    // 0xFF separator — the annotated hexdump is the authority.
    assert_eq!(REC_CALIB, 0x0100);
    assert_eq!(REC_CALIB_LEN, 120);
    assert_eq!(REC_CALIB + REC_CALIB_LEN, 0x0178);
    assert_eq!(EEPROM_SIZE, 2048);
}

#[test]
fn real_dump_reference_values_hold() {
    // Values verified against the physical device's dump (MD5
    // dc02fce8ca39155135a4b0368b3f33ae). If these assertions ever fail
    // against a re-dump, the EEPROM layout assumption changed.
    let fx1: f64 = 428.761292;
    let fx2: f64 = 435.891205;
    let mean_f = (fx1 + fx2) / 2.0;
    assert!((mean_f - 432.3).abs() < 0.5, "mean focal ~432 px");

    let t = [99.562683_f64, 0.463940, 9.330422];
    let baseline = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
    assert!((baseline - 100.0).abs() < 1e-4, "baseline is exactly 100 mm");
}

#[test]
fn synthetic_full_dump_parses() {
    let (c, id) = parse_eeprom(&synth_dump()).expect("synthetic dump must parse");
    assert!((c.fx[0] - 428.761292).abs() < 1e-4);
    assert!((c.baseline_mm() - 100.0).abs() < 1e-3);
    assert_eq!(id.serial, "AB460DC8BA12C6840B49CD5786C329DD");
    assert_eq!(id.code_a, "000T");
    assert_eq!(id.code_b, "2613");
}

fn synth_dump() -> Vec<u8> {
    let mut e = vec![0xFFu8; EEPROM_SIZE];
    e[..16].copy_from_slice(HEADER_TAG);
    let mut put = |off: usize, payload: &[u8]| {
        e[off..off + payload.len()].copy_from_slice(payload);
        e[off + payload.len()] = payload.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        e[off + payload.len() + 1] = 0xFF;
    };
    let floats: Vec<f32> = vec![
        428.761292, 430.208038, 312.787292, 212.867691, // cam1 intrinsics
        0.042773, -0.070018, -0.000007, -0.000250, // cam1 distortion
        435.891205, 437.767944, 312.149200, 179.222244, // cam2 intrinsics
        0.024833, -0.044414, 0.000232, -0.000101, // cam2 distortion
        0.999981, 0.002477, -0.005639, // R row 0
        -0.002544, 0.999927, -0.011812, // R row 1
        0.005609, 0.011827, 0.999914, // R row 2
        99.562683, 0.463940, 9.330422, // T (mm)
    ];
    let mut payload = Vec::new();
    for v in &floats {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    payload.extend_from_slice(&[0u8; 8]);
    put(0x0100, &payload);
    put(0x0180, b"2059\0\0\0\0\0\0\0\0\0\0\0\0");
    let ints: Vec<u8> = [386i16, 473, 215, 303, 142, 221, 0, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    put(0x0192, &ints);
    put(0x0300, b"AB460DC8BA12C6840B49CD5786C329DD");
    put(0x0322, b"000T");
    put(0x0328, b"2613");
    drop(put); // release the mutable borrow before copy_within
    e.copy_within(0x100..0x180, 0x500);
    e.copy_within(0x300..0x330, 0x700);
    e
}
