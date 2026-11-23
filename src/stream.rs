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

/// Asynchronous device events, delivered inside the bulk packet stream.
///
/// The reference driver maps both to the device error state
/// (`XnFirmwareStreams.cpp`: OVERHEAT -> XN_STATUS_DEVICE_OVERHEAT,
/// PROJECTOR_FAULT_EVENT -> XN_STATUS_DEVICE_PROJECTOR_FAULT; a following
/// good stream packet clears the state again). They are informational:
/// the firmware keeps streaming unless the fault persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    /// Packet 0xf31f: the sensor is overheating.
    Overheat,
    /// Packet 0xdead: the projector-fault detector tripped.
    ProjectorFault,
}

/// `XN_SENSOR_PROTOCOL_GMC_MAX_POINTS_IN_PACKET`
/// (Sensor/XnDeviceSensorProtocol.h:75) — the most motion points one
/// GMC packet can carry.
pub const GMC_MAX_POINTS_IN_PACKET: usize = 100;

/// Wire size of one [`GmcPoint`] (pack(1), five 16-bit fields).
pub const GMC_POINT_WIRE_SIZE: usize = 10;

/// Wire size of the packed `XnHostProtocolGMCLastPacketData`
/// (XnHostProtocol.h:267-280).
pub const GMC_LAST_PACKET_WIRE_SIZE: usize = 38;

/// One tracked point of a GMC motion-data packet — a field-faithful
/// port of `XnDeviceSensorGMCPoint`
/// (Sensor/XnDeviceSensor.h:162-169, `pack(1)`): five 16-bit fields,
/// 10 bytes on the wire, up to [`GMC_MAX_POINTS_IN_PACKET`] per packet.
///
/// GMC (Global Motion Compensation) tracks projected-pattern features
/// frame to frame; the resulting motion vectors describe how much the
/// camera/scene moved between two depth frames and let a consumer
/// stabilise or reject shaken frames. The reference driver never
/// decodes the plain packet — its `XnGMCDebugProcessor` only dumps the
/// debug variant to `GMC_Points` files as "X,Y,DX,DY".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GmcPoint {
    /// Pattern X coordinate the tracker locked onto.
    pub x: u16,
    /// Pattern Y coordinate.
    pub y: u16,
    /// Horizontal displacement since the previous frame. The C's field
    /// type is `XnUInt16`, so a leftward motion reads as two's
    /// complement here too (`dx as i16`).
    pub dx: u16,
    /// Vertical displacement (the C's field is `XnInt16` — signed).
    pub dy: i16,
    /// Tracker confidence of the match.
    pub score: u16,
}

impl GmcPoint {
    fn parse(chunk: &[u8]) -> GmcPoint {
        GmcPoint {
            x: u16::from_le_bytes([chunk[0], chunk[1]]),
            y: u16::from_le_bytes([chunk[2], chunk[3]]),
            dx: u16::from_le_bytes([chunk[4], chunk[5]]),
            dy: i16::from_le_bytes([chunk[6], chunk[7]]),
            score: u16::from_le_bytes([chunk[8], chunk[9]]),
        }
    }
}

/// The firmware's correlation-configuration state, first half:
/// `XnHostProtocolGMCLastConfData` (XnHostProtocol.h:254-259, pack(1)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GmcLastConfData {
    pub last: i16,
    pub ricc_last: u16,
    pub ricc_iir: f32,
}

/// The 0xb500 end-of-scan summary — `XnHostProtocolGMCLastPacketData`
/// (XnHostProtocol.h:267-280, pack(1), 38 bytes). The reference's
/// `XnGMCDebugProcessor` renders this as "Last Configuration" /
/// "New Configuration" (N, RICC, RICC IIR, A, B, C, StartB, DeltaB)
/// plus whether the flash reference offset was updated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GmcLastPacketData {
    /// GMC mode the scan ran in (0 = normal, 1 = scan).
    pub mode: u16,
    pub coverage_pass: u16,
    pub last_conf: GmcLastConfData,
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub n: i16,
    pub ricc: u16,
    pub start_b: u32,
    pub delta_b: u32,
    /// Reference offset the firmware stored to flash this scan; -1000
    /// means "not written" (the C fills that in for fw < 5.2).
    pub flash_stored_ref_offset: i16,
}

/// A decoded GMC packet from the misc endpoint (all GMC traffic lives
/// on EP 0x83 — `docs/FIRMWARE.md` §3).
#[derive(Debug, Clone, PartialEq)]
pub enum GmcPacket {
    /// 0xa200 plain motion data, or 0xb200 debug points (identical
    /// layout — the debug processor treats them the same).
    Points(GmcPoints),
    /// 0xb500 debug end-of-scan configuration summary.
    DebugEnd(GmcDebugEnd),
}

/// The point-array payload of one GMC packet.
#[derive(Debug, Clone, PartialEq)]
pub struct GmcPoints {
    /// Packet timestamp (device ticks, 60 ticks/us).
    pub timestamp: u32,
    /// True when carried by the debug channel (0xb200) rather than the
    /// plain GMC packet (0xa200).
    pub debug: bool,
    /// Motion points; never more than [`GMC_MAX_POINTS_IN_PACKET`].
    pub points: Vec<GmcPoint>,
}

/// The 0xb500 summary payload.
#[derive(Debug, Clone, PartialEq)]
pub struct GmcDebugEnd {
    pub timestamp: u32,
    pub data: GmcLastPacketData,
}

/// Parse a point-array GMC payload (0xa200 / 0xb200). Returns `None`
/// when the payload is not a whole number of 10-byte records.
fn parse_gmc_points(raw: &[u8], timestamp: u32, debug: bool) -> Option<GmcPoints> {
    if raw.is_empty() || raw.len() % GMC_POINT_WIRE_SIZE != 0 {
        return None;
    }
    let mut points = Vec::with_capacity(raw.len() / GMC_POINT_WIRE_SIZE);
    for chunk in raw.chunks_exact(GMC_POINT_WIRE_SIZE) {
        if points.len() == GMC_MAX_POINTS_IN_PACKET {
            log::warn!(
                "GMC packet advertises more than {GMC_MAX_POINTS_IN_PACKET} points; truncating"
            );
            break;
        }
        points.push(GmcPoint::parse(chunk));
    }
    Some(GmcPoints {
        timestamp,
        debug,
        points,
    })
}

/// Parse the packed `XnHostProtocolGMCLastPacketData` summary
/// (field offsets follow XnHostProtocol.h:267-280 with pack(1)).
fn parse_gmc_last_packet(raw: &[u8], timestamp: u32) -> Option<GmcDebugEnd> {
    if raw.len() < GMC_LAST_PACKET_WIRE_SIZE {
        return None;
    }
    let le16 = |o: usize| u16::from_le_bytes([raw[o], raw[o + 1]]);
    let le16s = |o: usize| i16::from_le_bytes([raw[o], raw[o + 1]]);
    let le32 = |o: usize| {
        u32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]])
    };
    let lef32 = |o: usize| {
        f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]])
    };
    Some(GmcDebugEnd {
        timestamp,
        data: GmcLastPacketData {
            mode: le16(0),
            coverage_pass: le16(2),
            last_conf: GmcLastConfData {
                last: le16s(4),
                ricc_last: le16(6),
                ricc_iir: lef32(8),
            },
            a: lef32(12),
            b: lef32(16),
            c: lef32(20),
            n: le16s(24),
            ricc: le16(26),
            start_b: le32(28),
            delta_b: le32(32),
            flash_stored_ref_offset: le16s(36),
        },
    })
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
    /// Completed device events (OVERHEAT / PROJECTOR_FAULT), ready for
    /// the consumer.
    events: VecDeque<DeviceEvent>,
    /// Decoded GMC packets (plain 0xa200 motion data, debug 0xb200/0xb500).
    gmc_packets: VecDeque<GmcPacket>,
    /// Payload accumulator of the in-flight GMC packet (misc packets can
    /// span several bulk transfers; the family is only parseable whole).
    gmc_accum: Vec<u8>,
    /// True while the in-flight packet is a non-frame packet, so it is
    /// counted exactly once (at completion) in `other_packets`.
    packet_is_misc: bool,
    /// Count of non-frame packets consumed (GMC, audio, ...).
    pub other_packets: u64,
    /// Diagnostic counters, surfaced when a stream misbehaves: a single
    /// camera-side log line then pinpoints the framing problem (starts
    /// seen but no ends = framing never completes, etc.).
    pub stats: AssemblerStats,
}

/// Per-kind packet and frame counters of one assembler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssemblerStats {
    /// DEPTH_START packets seen.
    pub depth_start: u64,
    /// IMAGE_START packets seen.
    pub image_start: u64,
    /// *_BUFFER packets seen.
    pub buffer: u64,
    /// *_END packets seen.
    pub end: u64,
    /// Frames closed and pushed to the frame queue.
    pub frames_closed: u64,
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
            events: VecDeque::new(),
            gmc_packets: VecDeque::new(),
            gmc_accum: Vec::new(),
            packet_is_misc: false,
            other_packets: 0,
            stats: AssemblerStats::default(),
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
                            self.on_packet_complete();
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
                        self.on_packet_complete();
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

    /// Called when the current packet's payload is fully consumed:
    /// counts a misc packet exactly once, and decodes the GMC family
    /// (0xa200/0xb200 points, 0xb500 summary) now that its payload is
    /// whole — the C only ever parsed them as
    /// `XnWholePacketProcessor`s too.
    fn on_packet_complete(&mut self) {
        if self.packet_is_misc {
            self.packet_is_misc = false;
            self.other_packets += 1;
        }
        let kind = self.header.packet_type;
        if matches!(
            kind,
            packet_type::GMC | packet_type::GMC_DEBUG | packet_type::GMC_DEBUG_END
        ) {
            let raw = std::mem::take(&mut self.gmc_accum);
            let timestamp = self.header.timestamp;
            let parsed = if kind == packet_type::GMC_DEBUG_END {
                parse_gmc_last_packet(&raw, timestamp).map(GmcPacket::DebugEnd)
            } else {
                parse_gmc_points(&raw, timestamp, kind == packet_type::GMC_DEBUG)
                    .map(GmcPacket::Points)
            };
            match parsed {
                Some(packet) => self.gmc_packets.push_back(packet),
                None => log::warn!(
                    "GMC packet {kind:#06x} payload is {} bytes — not whole GMC records, dropped",
                    raw.len()
                ),
            }
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
                    if h.packet_type == packet_type::DEPTH_START {
                        self.stats.depth_start += 1;
                    } else {
                        self.stats.image_start += 1;
                    }
                }
                self.frame_data.extend_from_slice(data);
            }
            packet_type::DEPTH_BUFFER
            | packet_type::IMAGE_BUFFER
            | packet_type::DEPTH_END
            | packet_type::IMAGE_END => {
                // data is appended; frame completion is handled in feed()
                // when the packet's last byte has been consumed
                if h.packet_type == packet_type::DEPTH_BUFFER
                    || h.packet_type == packet_type::IMAGE_BUFFER
                {
                    self.stats.buffer += 1;
                } else {
                    self.stats.end += 1;
                }
                self.frame_data.extend_from_slice(data);
            }
            other => {
                // Error-state packets become events instead of being
                // dropped on the floor (the reference driver's
                // XnFirmwareStreams.cpp sets the device error state for
                // these two; everything else is just counted once at
                // completion). GMC-family payloads additionally collect
                // into `gmc_accum` so they can be decoded whole.
                match other {
                    packet_type::OVERHEAT => {
                        log::warn!("device event: OVERHEAT");
                        self.events.push_back(DeviceEvent::Overheat);
                    }
                    packet_type::PROJECTOR_FAULT_EVENT => {
                        log::warn!("device event: PROJECTOR_FAULT");
                        self.events.push_back(DeviceEvent::ProjectorFault);
                    }
                    packet_type::GMC | packet_type::GMC_DEBUG | packet_type::GMC_DEBUG_END => {
                        self.gmc_accum.extend_from_slice(data);
                    }
                    _ => {}
                }
                self.packet_is_misc = true;
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
        self.stats.frames_closed += 1;
        self.frames.push_back(frame);
    }

    /// Pop the next completed frame, if any.
    pub fn pop_frame(&mut self) -> Option<AssembledFrame> {
        self.frames.pop_front()
    }

    /// Pop the next device event (OVERHEAT / PROJECTOR_FAULT), if any.
    pub fn pop_event(&mut self) -> Option<DeviceEvent> {
        self.events.pop_front()
    }

    /// Drain all pending device events.
    pub fn drain_events(&mut self) -> Vec<DeviceEvent> {
        self.events.drain(..).collect()
    }

    /// Pop the next decoded GMC packet, if any.
    pub fn pop_gmc_packet(&mut self) -> Option<GmcPacket> {
        self.gmc_packets.pop_front()
    }

    /// Drain all pending GMC packets.
    pub fn drain_gmc_packets(&mut self) -> Vec<GmcPacket> {
        self.gmc_packets.drain(..).collect()
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
    fn assembler_counters_track_packet_kinds() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::IMAGE_START, 1, b"ab", 1));
        stream.extend(packet(packet_type::IMAGE_BUFFER, 2, b"cd", 2));
        stream.extend(packet(packet_type::IMAGE_END, 3, b"!", 3));
        stream.extend(packet(packet_type::GMC, 4, b"x", 4));
        asm.feed(&stream);
        assert_eq!(
            asm.stats,
            AssemblerStats {
                depth_start: 0,
                image_start: 1,
                buffer: 1,
                end: 1,
                frames_closed: 1,
            }
        );
        assert_eq!(asm.pop_frame().unwrap().data, b"abcd!".to_vec());
    }

    #[test]
    fn error_state_packets_become_events() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::OVERHEAT, 1, b"hot", 1));
        stream.extend(packet(packet_type::GMC, 2, b"gmc", 2));
        stream.extend(packet(packet_type::PROJECTOR_FAULT_EVENT, 3, b"", 3));
        asm.feed(&stream);
        assert_eq!(
            asm.drain_events(),
            vec![DeviceEvent::Overheat, DeviceEvent::ProjectorFault]
        );
        // and the queue is empty afterwards
        assert!(asm.pop_event().is_none());
        // still counted as misc packets
        assert_eq!(asm.other_packets, 3);
    }

    /// The plain GMC packet (0xa200) decodes into motion points: the
    /// 10-byte `XnDeviceSensorGMCPoint` records, little-endian.
    #[test]
    fn plain_gmc_points_are_decoded() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        // two points: (x=100, y=200, dx=0xffff [-1 as i16], dy=-3, score=90)
        //             (x=101, y=201, dx=2, dy=0x8000 [-32768], score=0)
        let mut payload = Vec::new();
        payload.extend_from_slice(&100u16.to_le_bytes());
        payload.extend_from_slice(&200u16.to_le_bytes());
        payload.extend_from_slice(&0xffffu16.to_le_bytes());
        payload.extend_from_slice(&(-3i16).to_le_bytes());
        payload.extend_from_slice(&90u16.to_le_bytes());
        payload.extend_from_slice(&101u16.to_le_bytes());
        payload.extend_from_slice(&201u16.to_le_bytes());
        payload.extend_from_slice(&2u16.to_le_bytes());
        payload.extend_from_slice(&0x8000u16.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());
        let stream = packet(packet_type::GMC, 9, &payload, 1234);
        // the packet is deliberately split across three feeds: misc
        // packets may span bulk transfers
        let (a, rest) = stream.split_at(15);
        asm.feed(a);
        let (b, c) = rest.split_at(17);
        asm.feed(b);
        asm.feed(c);
        match asm.pop_gmc_packet() {
            Some(GmcPacket::Points(p)) => {
                assert_eq!(p.timestamp, 1234);
                assert!(!p.debug);
                assert_eq!(
                    p.points,
                    vec![
                        GmcPoint {
                            x: 100,
                            y: 200,
                            dx: 0xffff,
                            dy: -3,
                            score: 90
                        },
                        GmcPoint {
                            x: 101,
                            y: 201,
                            dx: 2,
                            dy: i16::MIN,
                            score: 0
                        },
                    ]
                );
            }
            other => panic!("expected plain points packet, got {other:?}"),
        }
        // still counted as a (single) misc packet despite the split
        assert_eq!(asm.other_packets, 1);
    }

    /// The debug points packet (0xb200) decodes identically and is
    /// flagged as debug.
    #[test]
    fn gmc_debug_points_are_flagged() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut payload = Vec::new();
        payload.extend_from_slice(&7u16.to_le_bytes());
        payload.extend_from_slice(&8u16.to_le_bytes());
        payload.extend_from_slice(&9u16.to_le_bytes());
        payload.extend_from_slice(&10u16.to_le_bytes());
        payload.extend_from_slice(&11u16.to_le_bytes());
        asm.feed(&packet(packet_type::GMC_DEBUG, 1, &payload, 99));
        match asm.pop_gmc_packet() {
            Some(GmcPacket::Points(p)) => {
                assert!(p.debug);
                assert_eq!(p.points.len(), 1);
                assert_eq!(p.points[0], GmcPoint { x: 7, y: 8, dx: 9, dy: 10, score: 11 });
            }
            other => panic!("expected debug points packet, got {other:?}"),
        }
    }

    /// The 0xb500 debug end-of-scan summary parses as the packed
    /// `XnHostProtocolGMCLastPacketData` (38 bytes).
    #[test]
    fn gmc_debug_end_summary_is_decoded() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&1u16.to_le_bytes()); // mode = scan
        raw.extend_from_slice(&2u16.to_le_bytes()); // coverage pass
        raw.extend_from_slice(&(-7i16).to_le_bytes()); // last conf: last
        raw.extend_from_slice(&300u16.to_le_bytes()); // last conf: ricc last
        raw.extend_from_slice(&1.5f32.to_le_bytes()); // last conf: ricc iir
        raw.extend_from_slice(&2.25f32.to_le_bytes()); // A
        raw.extend_from_slice(&0.5f32.to_le_bytes()); // B
        raw.extend_from_slice(&(-1.0f32).to_le_bytes()); // C
        raw.extend_from_slice(&(-42i16).to_le_bytes()); // N
        raw.extend_from_slice(&1024u16.to_le_bytes()); // RICC
        raw.extend_from_slice(&100_000u32.to_le_bytes()); // StartB
        raw.extend_from_slice(&4_000u32.to_le_bytes()); // DeltaB
        raw.extend_from_slice(&(-1000i16).to_le_bytes()); // ref offset: not written
        assert_eq!(raw.len(), GMC_LAST_PACKET_WIRE_SIZE);

        let mut asm = StreamAssembler::new(MAGIC, 0);
        asm.feed(&packet(packet_type::GMC_DEBUG_END, 3, &raw, 55));
        match asm.pop_gmc_packet() {
            Some(GmcPacket::DebugEnd(end)) => {
                assert_eq!(end.timestamp, 55);
                let d = end.data;
                assert_eq!(d.mode, 1);
                assert_eq!(d.coverage_pass, 2);
                assert_eq!(d.last_conf.last, -7);
                assert_eq!(d.last_conf.ricc_last, 300);
                assert_eq!(d.last_conf.ricc_iir, 1.5);
                assert_eq!(d.a, 2.25);
                assert_eq!(d.b, 0.5);
                assert_eq!(d.c, -1.0);
                assert_eq!(d.n, -42);
                assert_eq!(d.ricc, 1024);
                assert_eq!(d.start_b, 100_000);
                assert_eq!(d.delta_b, 4_000);
                assert_eq!(d.flash_stored_ref_offset, -1000);
            }
            other => panic!("expected debug end packet, got {other:?}"),
        }
    }

    /// A GMC payload that is not whole 10-byte records is counted as
    /// misc but not decoded (keeps the pre-0.3.5 counting behaviour).
    #[test]
    fn non_record_gmc_payload_is_counted_not_decoded() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        asm.feed(&packet(packet_type::GMC, 1, b"gmc-data", 1));
        asm.feed(&packet(packet_type::OVERHEAT, 2, b"hot", 2));
        assert_eq!(asm.other_packets, 2);
        assert!(asm.pop_gmc_packet().is_none());
    }

    /// More than 100 points in one packet are truncated to the C's
    /// `XN_SENSOR_PROTOCOL_GMC_MAX_POINTS_IN_PACKET` instead of growing
    /// the allocation with the advertised size.
    #[test]
    fn oversized_gmc_packets_are_truncated() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut payload = Vec::new();
        for i in 0..150u16 {
            payload.extend_from_slice(&i.to_le_bytes());
            payload.extend_from_slice(&0u16.to_le_bytes());
            payload.extend_from_slice(&0u16.to_le_bytes());
            payload.extend_from_slice(&0i16.to_le_bytes());
            payload.extend_from_slice(&0u16.to_le_bytes());
        }
        asm.feed(&packet(packet_type::GMC, 1, &payload, 1));
        match asm.pop_gmc_packet() {
            Some(GmcPacket::Points(p)) => {
                assert_eq!(p.points.len(), GMC_MAX_POINTS_IN_PACKET);
                assert_eq!(p.points[0].x, 0);
                assert_eq!(p.points[99].x, 99);
            }
            other => panic!("expected points packet, got {other:?}"),
        }
    }

    /// GMC packets interleaved with depth frames must not disturb frame
    /// assembly, and both queues must deliver.
    #[test]
    fn gmc_interleaved_with_frames_does_not_disturb_assembly() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut points = Vec::new();
        points.extend_from_slice(&1u16.to_le_bytes());
        points.extend_from_slice(&2u16.to_le_bytes());
        points.extend_from_slice(&0u16.to_le_bytes());
        points.extend_from_slice(&0i16.to_le_bytes());
        points.extend_from_slice(&50u16.to_le_bytes());
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::DEPTH_START, 1, b"SOF", 1));
        stream.extend(packet(packet_type::GMC, 2, &points, 2));
        stream.extend(packet(packet_type::DEPTH_BUFFER, 3, b"MID", 3));
        stream.extend(packet(packet_type::DEPTH_END, 4, b"!", 4));
        asm.feed(&stream);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"SOFMID!".to_vec());
        assert_eq!(
            asm.pop_gmc_packet(),
            Some(GmcPacket::Points(GmcPoints {
                timestamp: 2,
                debug: false,
                points: vec![GmcPoint {
                    x: 1,
                    y: 2,
                    dx: 0,
                    dy: 0,
                    score: 50
                }],
            }))
        );
    }

    /// Regression guard: event packets interleaved into a frame must not
    /// disturb frame assembly (the magic-resync path must not eat frame
    /// data), and the event must still surface.
    #[test]
    fn events_interleaved_with_frames_do_not_disturb_assembly() {
        let mut asm = StreamAssembler::new(MAGIC, 0);
        let mut stream = Vec::new();
        stream.extend(packet(packet_type::DEPTH_START, 1, b"SOF", 1));
        stream.extend(packet(packet_type::OVERHEAT, 2, b"hot", 2));
        stream.extend(packet(packet_type::DEPTH_BUFFER, 3, b"MID", 3));
        stream.extend(packet(packet_type::PROJECTOR_FAULT_EVENT, 4, b"", 4));
        stream.extend(packet(packet_type::DEPTH_END, 5, b"!", 5));
        asm.feed(&stream);
        let f = asm.pop_frame().unwrap();
        assert_eq!(f.data, b"SOFMID!".to_vec());
        assert!(!f.corrupted);
        assert_eq!(
            asm.drain_events(),
            vec![DeviceEvent::Overheat, DeviceEvent::ProjectorFault]
        );
    }

    #[test]
    fn host_magic_is_not_fw_magic() {
        // sanity: the stream magic is BR (0x4252), not MG (0x4d47)
        assert_ne!(MAGIC, HOST_MAGIC_V26);
    }
}
