//! Golden-byte tests for the frame codec.
//!
//! The vectors below are synthesized from the disassembly-decoded framing
//! (magic + type + payload). Once a USBPcap transcript exists (milestone
//! M3), the real captured bytes replace them in `reference/golden_frames.bin`.

use idong::protocol::frames::{Frame, FrameDecoder, MAGIC_D2H, MAGIC_H2D};

#[test]
fn h2d_magic_bytes_are_pu_7f_7f() {
    let b = Frame::h2d(3, &[]).to_bytes();
    assert_eq!(&b[..4], &[0x50, 0x55, 0x7f, 0x7f]); // 'P','U',0x7f,0x7f
    assert_eq!(MAGIC_H2D, 0x7f7f_5550);
}

#[test]
fn d2h_magic_bytes_are_od_7f_7f() {
    let b = Frame::d2h(2, &[]).to_bytes();
    assert_eq!(&b[..4], &[0x4f, 0x44, 0x7f, 0x7f]); // 'O','D',0x7f,0x7f
    assert_eq!(MAGIC_D2H, 0x7f7f_444f);
}

#[test]
fn ready_to_load_frame_decodes() {
    // device sends type 2 with the ASCII banner
    let payload = b"Ready to load program!";
    let f = Frame::d2h(2, payload.as_slice());
    let mut d = FrameDecoder::new();
    let frames = d.push(&f.to_bytes());
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].kind, 2);
    assert_eq!(frames[0].payload, payload.as_slice());
}

#[test]
fn keepalive_pair_roundtrip() {
    let mut d = FrameDecoder::new();
    let rx = Frame::d2h(0xFFF, &[]).to_bytes();
    let out = Frame::h2d(0xFFE, &[]).to_bytes();
    assert_eq!(d.push(&rx)[0].kind, 0xFFF);
    assert_eq!(&out[4..6], &[0xFE, 0x0F]); // type 0xFFE LE
}

#[test]
fn decoder_survives_split_and_garbage() {
    let mut d = FrameDecoder::new();
    let a = Frame::d2h(4, &[0x11, 0x00, 0x01, 0x00, 0xff, 0xff, 0xff, 0xff]);
    let b = Frame::d2h(7, &[]);
    let mut stream = vec![0xde, 0xad, 0xbe, 0xef]; // leading garbage
    stream.extend_from_slice(&a.to_bytes());
    stream.extend_from_slice(&b.to_bytes());
    // split at an awkward boundary
    let (l, r) = stream.split_at(9);
    let mut got = d.push(l);
    got.extend(d.push(r));
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].kind, 4);
    assert_eq!(got[1].kind, 7);
}
