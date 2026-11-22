//! Frame codec: `[u32 magic LE][u16 type LE][payload…]`.
//!
//! Host→device magic `0x7f7f_5550` (`"PU\x7f\x7f"`), device→host
//! `0x7f7f_444f` (`"OD\x7f\x7f"`). There is **no explicit length field**:
//! frames are delimited by bulk-packet boundaries and the known record
//! types; the [`FrameDecoder`] therefore scans for magic and deframing is
//! completed by the caller (see `stream.rs` and `records.rs`).

use super::{MAGIC_D2H, MAGIC_H2D};

/// A single protocol frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub magic: u32,
    pub kind: u16,
    pub payload: Vec<u8>,
}

impl Frame {
    /// New host→device frame.
    pub fn h2d(kind: u16, payload: &[u8]) -> Self {
        Frame {
            magic: MAGIC_H2D,
            kind,
            payload: payload.to_vec(),
        }
    }

    /// New device→host frame (for tests / replay).
    pub fn d2h(kind: u16, payload: &[u8]) -> Self {
        Frame {
            magic: MAGIC_D2H,
            kind,
            payload: payload.to_vec(),
        }
    }

    /// Header size (magic + type).
    pub const HEADER: usize = 6;

    /// Encode into `out`.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.reserve(Self::HEADER + self.payload.len());
        out.extend_from_slice(&self.magic.to_le_bytes());
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&self.payload);
    }

    /// Encode into a fresh buffer.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(Self::HEADER + self.payload.len());
        self.encode(&mut v);
        v
    }

    /// Parse one frame from the front of `buf`. Returns the frame and the
    /// number of bytes consumed. `None` if more data is needed or no magic
    /// is present.
    pub fn parse(buf: &[u8]) -> Option<(Frame, usize)> {
        if buf.len() < Self::HEADER {
            return None;
        }
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC_H2D && magic != MAGIC_D2H {
            return None;
        }
        let kind = u16::from_le_bytes([buf[4], buf[5]]);
        // Frames are bulk-packet delimited; a full bulk packet IS a frame
        // in the boot phase (64 B maxpacket). For the runtime stream the
        // record length is derived by the record parser; here we consume
        // everything available, the caller re-slices.
        let payload = buf[Self::HEADER..].to_vec();
        let n = buf.len();
        Some((Frame { magic, kind, payload }, n))
    }
}

/// Incremental decoder for a byte stream: scans for either magic, then
/// emits frames once a bulk boundary provides the payload. Used by the
/// read loops; unit-tested against golden byte streams.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk; returns all complete frames found at buffer
    /// boundaries. Any leading garbage before a magic is dropped (and
    /// reported through the stream stats by the caller).
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            // find a magic prefix
            let idx = match find_magic(&self.buf) {
                Some(i) => i,
                None => {
                    // keep the last 3 bytes (partial magic)
                    let keep = self.buf.len().saturating_sub(3);
                    self.buf.drain(..keep);
                    break;
                }
            };
            self.buf.drain(..idx);
            if self.buf.len() < Frame::HEADER {
                break;
            }
            if let Some((f, n)) = Frame::parse(&self.buf) {
                self.buf.drain(..n);
                frames.push(f);
            } else {
                // magic parse refused (mixed magic) — skip 1 byte
                self.buf.drain(..1);
            }
        }
        frames
    }
}

fn find_magic(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    for i in 0..=buf.len() - 4 {
        let m = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        if m == MAGIC_H2D || m == MAGIC_D2H {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_h2d() {
        let f = Frame::h2d(3, &[0xAA; 64]);
        let b = f.to_bytes();
        assert_eq!(&b[..4], &[0x50, 0x55, 0x7f, 0x7f]);
        let (g, n) = Frame::parse(&b).unwrap();
        assert_eq!(n, b.len());
        assert_eq!(g, f);
    }

    #[test]
    fn roundtrip_d2h() {
        let f = Frame::d2h(0xFFF, &[0x01]);
        let b = f.to_bytes();
        assert_eq!(&b[..4], &[0x4f, 0x44, 0x7f, 0x7f]);
        let (g, _) = Frame::parse(&b).unwrap();
        assert_eq!(g.kind, 0xFFF);
    }

    #[test]
    fn scanner_resync_over_garbage() {
        let mut d = FrameDecoder::new();
        let mut stream = vec![0u8; 37];
        let f = Frame::d2h(2, b"Ready to load program!");
        let b = f.to_bytes();
        stream.extend_from_slice(&b);
        let frames = d.push(&stream);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, 2);
        assert_eq!(&frames[0].payload, b"Ready to load program!");
    }

    #[test]
    fn split_chunks_reassemble() {
        let mut d = FrameDecoder::new();
        let f = Frame::d2h(4, &[0x11, 0x00, 0x01]);
        let b = f.to_bytes();
        assert!(d.push(&b[..3]).is_empty());
        let frames = d.push(&b[3..]);
        assert_eq!(frames.len(), 1);
    }
}
