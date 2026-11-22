//! Bulk-stream frame assembly.
//!
//! Port of the mini-packet state machine in
//! `Sensor/XnDeviceSensorProtocol.cpp::XnDeviceSensorProtocolUsbEpCb`:
//! byte stream -> packets (magic scan -> 12-byte header -> payload) ->
//! frames (SOF + data packets + EOF).

use std::collections::VecDeque;

use crate::protocol::{packet_type, PacketHeader, PACKET_HEADER_SIZE};

/// Which stream a completed frame belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Depth,
    Image,
    /// GMC / audio / debug packets seen on the misc endpoint.
    Other(u16),
}

/// A fully assembled frame.
#[derive(Debug, Clone)]
pub struct AssembledFrame {
    pub kind: FrameKind,
    /// Raw frame payload (shift values for depth; YUV422 for image —
    /// exactly what the device sent, before any decoding).
    pub data: Vec<u8>,
    /// Device timestamp of the **last** packet of the frame (the reference
    /// driver uses the EOF packet timestamp as the frame timestamp).
    pub timestamp: u32,
    /// Packet id of the start-of-frame packet.
    pub packet_id: u16,
    /// Padding pixels reported by firmware >= 5.1 in the SOF packet
    /// timestamp field: `(start, end)`.
    pub padding: (u16, u16),
    /// True if a packet was lost or a duplicate SOF appeared mid-frame.
    pub corrupted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    LookingForMagic,
    PacketHeader,
    PacketData,
}

/// Assembles frames from the raw byte stream of one bulk endpoint.
pub struct StreamAssembler {
    fw_magic: u16,
    /// Bytes to skip on this endpoint before parsing (firmware < 5.0
    /// emits one chunk of garbage when streaming starts).
    ignore_bytes: usize,
    state: State,
    /// Bytes still needed for the current state.
    missing: usize,
    /// Partial-magic carry: set when the last byte of the previous chunk
    /// was the first byte of the magic.
    partial_magic: bool,
    header_buf: [u8; PACKET_HEADER_SIZE],
    header: PacketHeader,
    /// Payload bytes of the current packet already dispatched.
    packet_data_offset: usize,
    /// Current frame being assembled.
    frame_data: Vec<u8>,
    frame_kind: FrameKind,
    frame_packet_id: u16,
    frame_padding: (u16, u16),
    frame_corrupted: bool,
    frame_open: bool,
    last_sof_packet_id: u16,
    /// Completed frames, ready for the consumer.
    pub frames: VecDeque<AssembledFrame>,
    /// Count of non-frame packets consumed (GMC, audio, ...).
    pub other_packets: u64,
}

impl StreamAssembler {
    pub fn new(fw_magic: u16, ignore_bytes: usize) -> StreamAssembler {
        StreamAssembler {
            fw_magic,
            ignore_bytes,
            state: State::LookingForMagic,
            missing: 2,
            partial_magic: false,
            header_buf: [0; PACKET_HEADER_SIZE],
            header: PacketHeader {
                magic: 0,
                packet_type: 0,
                packet_id: 0,
                buf_size: 0,
                timestamp: 0,
            },
            packet_data_offset: 0,
            frame_data: Vec::new(),
            frame_kind: FrameKind::Depth,
            frame_packet_id: 0,
            frame_padding: (0, 0),
            frame_corrupted: false,
            frame_open: false,
            last_sof_packet_id: 0,
            frames: VecDeque::new(),
            other_packets: 0,
        }
    }

    /// Feed raw bytes from one bulk read into the assembler.
    pub fn feed(&mut self, mut chunk: &[u8]) {
        // 1. firmware garbage skip
        if self.ignore_bytes > 0 {
            let skip = self.ignore_bytes.min(chunk.len());
            log::debug!("ignoring {skip} garbage bytes");
            chunk = &chunk[skip..];
            self.ignore_bytes -= skip;
            if chunk.is_empty() {
                return;
            }
        }

        while !chunk.is_empty() {
            match self.state {
                State::LookingForMagic => {
                    chunk = self.scan_magic(chunk);
                }
                State::PacketHeader => {
                    let take = self.missing.min(chunk.len());
                    let start = PACKET_HEADER_SIZE - self.missing;
                    self.header_buf[start..start + take].copy_from_slice(&chunk[..take]);
                    self.missing -= take;
                    chunk = &chunk[take..];
                    if self.missing == 0 {
                        let header =
                            PacketHeader::parse(&self.header_buf).expect("12 bytes always parse");
                        self.header = header;
                        self.state = State::PacketData;
                        self.missing = header.payload_size();
                        self.packet_data_offset = 0;
                        if self.missing == 0 {
                            // Zero-payload packet (a bare SOF/EOF header is
                            // exactly 12 bytes): dispatch it now. The loop
                            // below would otherwise not complete it until
                            // the next chunk arrives, delaying every frame
                            // close and losing the last frame on stream
                            // stop.
                            self.dispatch_packet(&[], true);
                            self.close_if_eof();
                            self.state = State::LookingForMagic;
                            self.missing = 2;
                        }
                    }
                }
                State::PacketData => {
                    let take = self.missing.min(chunk.len());
                    let data = &chunk[..take];
                    self.missing -= take;
                    chunk = &chunk[take..];
                    let is_packet_start = self.packet_data_offset == 0;
                    self.packet_data_offset += take;
                    self.dispatch_packet(data, is_packet_start);
                    if self.missing == 0 {
                        // the packet is fully consumed; an EOF packet
                        // completes the frame (C: nDataOffset + nDataSize
                        // == nBufSize -> OnEndOfFrame)
                        self.close_if_eof();
                        self.state = State::LookingForMagic;
                        self.missing = 2;
                    }
                }
            }
        }
    }

    /// Advance through the buffer while looking for the fw magic u16.
    /// Returns the unconsumed remainder. The lifetime is tied only to the
    /// input chunk, not to `self`.
    fn scan_magic<'c>(&mut self, chunk: &'c [u8]) -> &'c [u8] {
        let magic_lo = (self.fw_magic & 0xff) as u8;
        let magic_hi = (self.fw_magic >> 8) as u8;
        let mut i = 0usize;

        // complete a partial magic carried over from the previous chunk
        if self.partial_magic {
            if chunk[0] == magic_hi {
                self.partial_magic = false;
                // The 12-byte header INCLUDES the two magic bytes. The low
                // byte arrived at the end of the previous chunk and the
                // high byte is chunk[0]; pre-seed both and read the
                // remaining 10 header bytes.
                self.state = State::PacketHeader;
                self.header_buf[0] = magic_lo;
                self.header_buf[1] = magic_hi;
                self.missing = PACKET_HEADER_SIZE - 2;
                return &chunk[1..];
            }
            self.partial_magic = false;
        }

        while i + 1 < chunk.len() {
            if chunk[i] == magic_lo && chunk[i + 1] == magic_hi {
                self.enter_header_state();
                return &chunk[i..];
            }
            i += 1;
        }
        // maybe the last byte starts a magic that completes next chunk
        if chunk[chunk.len() - 1] == magic_lo {
            self.partial_magic = true;
        }
        &[]
    }

    fn enter_header_state(&mut self) {
        self.state = State::PacketHeader;
        self.missing = PACKET_HEADER_SIZE;
    }

    /// Close the open frame if the packet just consumed was an EOF packet
    /// (the frame timestamp is the EOF packet's header timestamp, matching
    /// the reference driver's `CreateTimestampFromDevice(pHeader->nTimeStamp)`
    /// in `OnEndOfFrame`).
    fn close_if_eof(&mut self) {
        if self.header.packet_type == packet_type::DEPTH_END
            || self.header.packet_type == packet_type::IMAGE_END
        {
            self.close_frame(self.header.timestamp);
        }
    }

    /// Handle one chunk of a complete packet's payload. `is_packet_start`
    /// is true for the first bytes of this packet (C: `nDataOffset == 0`).
    fn dispatch_packet(&mut self, data: &[u8], is_packet_start: bool) {
        let h = self.header;
        match h.packet_type {
            packet_type::DEPTH_START | packet_type::IMAGE_START => {
                // A new frame begins with the first data of an SOF packet
                // (the reference driver uses nDataOffset == 0; the
                // double-SOF packet-id logic applies to low-bandwidth ISO
                // mode only and is not ported).
                if is_packet_start {
                    self.last_sof_packet_id = h.packet_id;
                    // a previous unterminated frame is dropped: the
                    // reference driver marks the buffer corrupt and resets
                    if self.frame_open {
                        log::warn!("dropping unterminated frame before new SOF");
                    }
                    self.frame_open = true;
                    self.frame_corrupted = false;
                    self.frame_data.clear();
                    self.frame_packet_id = h.packet_id;
                    self.frame_kind = if h.packet_type == packet_type::DEPTH_START {
                        FrameKind::Depth
                    } else {
                        FrameKind::Image
                    };
                    // firmware >= 5.1: the SOF timestamp field carries
                    // padding pixel counts, not a timestamp
                    self.frame_padding =
                        ((h.timestamp >> 16) as u16, (h.timestamp & 0xffff) as u16);
                }
                self.frame_data.extend_from_slice(data);
            }
            packet_type::DEPTH_BUFFER
            | packet_type::IMAGE_BUFFER
            | packet_type::DEPTH_END
            | packet_type::IMAGE_END => {
                // data is appended; frame completion is handled in feed()
                // when the packet's last byte has been consumed
                self.frame_data.extend_from_slice(data);
            }
            other => {
                self.other_packets += 1;
                let _ = other;
            }
        }
    }

    fn close_frame(&mut self, last_timestamp: u32) {
        let frame = AssembledFrame {
            kind: self.frame_kind,
            data: std::mem::take(&mut self.frame_data),
            timestamp: last_timestamp,
            packet_id: self.frame_packet_id,
            padding: self.frame_padding,
            corrupted: self.frame_corrupted,
        };
        self.frame_open = false;
        self.frames.push_back(frame);
    }

    /// Pop the next completed frame, if any.
    pub fn pop_frame(&mut self) -> Option<AssembledFrame> {
        self.frames.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FW_MAGIC_V26, HOST_MAGIC_V26};

    const MAGIC: u16 = FW_MAGIC_V26;

    fn packet(pkt_type: u16, packet_id: u16, payload: &[u8], timestamp: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&MAGIC.to_le_bytes());
        v.extend_from_slice(&pkt_type.to_le_bytes());
        v.extend_from_slice(&packet_id.to_le_bytes());
        // buf_size is the one big-endian field (see PacketHeader).
        v.extend_from_slice(&((payload.len() + PACKET_HEADER_SIZE) as u16).to_be_bytes());
        v.extend_from_slice(&timestamp.to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn buf_size_field_is_big_endian_like_the_reference_driver() {
        // Regression for the 0.1.3 on-hardware failure: a real 512-byte
        // bulk packet (fw 5.8.22, mps 512) carries buf_size 0x0200 as the
        // byte pair [02, 00]. Read little-endian this is 2 -> payload 0,
        // so every data packet silently contributed nothing and frames
        // assembled from false-magic junk (24560 of 307200 pixels).
        let mut wire = Vec::new();
        wire.extend_from_slice(&MAGIC.to_le_bytes());
        wire.extend_from_slice(&packet_type::DEPTH_BUFFER.to_le_bytes());
        wire.extend_from_slice(&7u16.to_le_bytes());
        wire.extend_from_slice(&512u16.to_be_bytes()); // [02, 00]
        wire.extend_from_slice(&0u32.to_le_bytes());
        wire.extend_from_slice(&[0xABu8; 500]);

        let header = PacketHeader::parse(&wire).unwrap();
        assert_eq!(header.buf_size, 512);
        assert_eq!(header.payload_size(), 500);

        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = packet(packet_type::DEPTH_START, 1, b"", 0);
        stream.extend(wire);
        stream.extend(packet(packet_type::DEPTH_END, 2, b"", 0));
        asm.feed(&stream);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data.len(), 500);
    }

    #[test]
    fn simple_frame_across_three_packets() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::DEPTH_START, 1, b"SOF-data", 111));
        stream.extend(packet(packet_type::DEPTH_BUFFER, 2, b"-mid-", 222));
        stream.extend(packet(packet_type::DEPTH_END, 3, b"-end", 333));
        asm.feed(&stream);

        let f = asm.pop_frame().unwrap();
        assert_eq!(f.kind, FrameKind::Depth);
        assert_eq!(f.data, b"SOF-data-mid--end".to_vec());
        assert_eq!(f.timestamp, 333); // EOF timestamp is the frame timestamp
        assert_eq!(f.packet_id, 1);
        assert!(!f.corrupted);
        assert!(asm.pop_frame().is_none());
    }

    #[test]
    fn image_and_depth_are_separated_by_kind() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::IMAGE_START, 5, b"img", 1));
        stream.extend(packet(packet_type::IMAGE_END, 6, b"!", 2));
        stream.extend(packet(packet_type::DEPTH_START, 7, b"dep", 3));
        stream.extend(packet(packet_type::DEPTH_END, 8, b"th", 4));
        asm.feed(&stream);

        let img = asm.pop_frame().unwrap();
        assert_eq!(img.kind, FrameKind::Image);
        assert_eq!(img.data, b"img!".to_vec());
        let dep = asm.pop_frame().unwrap();
        assert_eq!(dep.kind, FrameKind::Depth);
        assert_eq!(dep.data, b"depth".to_vec());
    }

    #[test]
    fn garbage_before_magic_is_skipped() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = vec![0xaa, 0xbb, 0xcc, 0xdd];
        stream.extend(packet(packet_type::DEPTH_START, 1, b"x", 9));
        stream.extend(packet(packet_type::DEPTH_END, 2, b"y", 10));
        asm.feed(&stream);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"xy".to_vec());
    }

    #[test]
    fn partial_magic_at_chunk_boundary_is_stitched() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::DEPTH_START, 1, b"AB", 1));
        stream.extend(packet(packet_type::DEPTH_END, 2, b"C", 2));
        // split exactly between the two bytes of the second packet's magic
        let first_packet_len = 12 + 2; // header + "AB"
        let split = first_packet_len + 1;
        assert_eq!(
            u16::from_le_bytes([stream[split - 1], stream[split]]),
            MAGIC,
            "split must fall inside the magic"
        );
        asm.feed(&stream[..split]);
        asm.feed(&stream[split..]);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"ABC".to_vec());
    }

    #[test]
    fn chunks_can_split_anywhere() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::DEPTH_START, 1, b"hello", 1));
        stream.extend(packet(packet_type::DEPTH_END, 2, b"world", 2));
        for b in stream.chunks(3) {
            asm.feed(b);
        }
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"helloworld".to_vec());
    }

    #[test]
    fn firmware_garbage_is_ignored_first() {
        let mut asm = StreamAssembler::new(MAGIC, 16);
        let mut stream = vec![0xff; 16];
        stream.extend(packet(packet_type::DEPTH_START, 1, b"ok", 1));
        stream.extend(packet(packet_type::DEPTH_END, 2, b"!", 2));
        asm.feed(&stream);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"ok!".to_vec());
    }

    #[test]
    fn sof_padding_quirk_is_decoded() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        // SOF timestamp = 0x00030002 -> (start=3, end=2)
        let mut stream = packet(packet_type::DEPTH_START, 1, b"z", 0x0003_0002);
        stream.extend(packet(packet_type::DEPTH_END, 2, b"!", 5));
        asm.feed(&stream);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.padding, (3, 2));
    }

    #[test]
    fn unterminated_frame_is_dropped_when_next_sof_arrives() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::DEPTH_START, 1, b"orphan", 1));
        stream.extend(packet(packet_type::DEPTH_START, 5, b"fresh", 2));
        stream.extend(packet(packet_type::DEPTH_END, 6, b"!", 3));
        asm.feed(&stream);
        // the orphan frame is dropped; the fresh one completes
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"fresh!".to_vec());
        assert!(!f.corrupted);
    }

    #[test]
    fn misc_packets_are_counted_not_framed() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::GMC, 1, b"gmc-data", 1));
        stream.extend(packet(packet_type::OVERHEAT, 2, b"hot", 2));
        asm.feed(&stream);
        assert_eq!(asm.other_packets, 2);
        assert!(asm.pop_frame().is_none());
    }

    #[test]
    fn host_magic_is_not_fw_magic() {
        // sanity: the stream magic is BR (0x4252), not MG (0x4d47)
        assert_ne!(MAGIC, HOST_MAGIC_V26);
    }
}
