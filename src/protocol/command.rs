//! PsMonitor command protocol: request framing, exchange and reply
//! validation. Port of `Sensor/XnHostProtocol.cpp` (InitHeader, USBSend,
//! USBReceive, ReceiveReply, Execute, ValidateReplyV25/V26).
//!
//! Reply transport semantics (all verified against the reference):
//! - A reply may arrive **split across several vendor control-IN
//!   transfers**. The first transfer carries the reply header + the first
//!   data chunk; `Execute` keeps issuing control reads and appending until
//!   the promised `size * 2` bytes are collected (the "Get rest of data"
//!   loop).
//! - Replies with a **wrong request id** (stale answers to earlier
//!   commands) are discarded and the receive is repeated.
//! - **Bad magic** (e.g. a stray packet) is retried every 10 ms until the
//!   receive budget expires.
//! - A **NACK_NOT_READY** reply causes the whole request to be re-sent
//!   (same request id), up to 3 times, 1 s apart.
//! - The device pads odd data lengths up to whole 16-bit words.

use std::time::{Duration, Instant};

use crate::error::{Nack, ProtocolError, Result, XtionError};

use super::{
    Opcode, CONTROL_SEND_RETRIES, CONTROL_SEND_RETRY_DELAY_MS, FW_MAGIC_V25, FW_MAGIC_V26,
    HOST_MAGIC_V25, HOST_MAGIC_V26,
};

/// Read size for control-IN transfers (protocol max packet size for
/// firmware >= 5.0, `XN_PROTOCOL_MAX_PACKET_SIZE_V5_0`).
const RECEIVE_READ_SIZE: usize = 512;
/// Delay before sending a command (`FWInfo.nUSBDelayExecutePreSend`).
const PRE_SEND_DELAY: Duration = Duration::from_millis(1);
/// Delay between sending and reading the reply
/// (`FWInfo.nUSBDelayExecutePostSend`).
const POST_SEND_DELAY: Duration = Duration::from_millis(10);
/// Retry delay when the reply buffer holds no firmware magic
/// (`ReceiveReply`: xnOSSleep(10) on BAD_MAGIC).
const BAD_MAGIC_RETRY_DELAY: Duration = Duration::from_millis(10);
/// Re-send attempts when the device answers NACK_NOT_READY
/// (`XN_HOST_PROTOCOL_NOT_READY_RETRIES`).
const NOT_READY_RETRIES: usize = 3;
/// Delay between NOT_READY re-sends (Execute: xnOSSleep(1000)).
const NOT_READY_RETRY_DELAY: Duration = Duration::from_millis(1000);

/// Transport abstraction over the USB control endpoint. Implemented by
/// [`crate::usb::UsbDevice`] and by test doubles.
pub trait ControlTransport {
    /// Vendor control OUT transfer (bmRequestType 0x40, bRequest 0).
    fn control_out(&mut self, data: &[u8], timeout: Duration) -> Result<usize>;
    /// Vendor control IN transfer (bmRequestType 0xC0, bRequest 0).
    fn control_in(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize>;
}

/// Which protocol dialect the firmware speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolVersion {
    /// 8-byte header, magic "MG"/"BR", firmware >= 1.2.
    V26,
    /// 10-byte header (extra CRC16 field, always 0), magic "PS"/"SP".
    V25,
}

impl ProtocolVersion {
    pub fn header_size(self) -> usize {
        match self {
            ProtocolVersion::V26 => 8,
            ProtocolVersion::V25 => 10,
        }
    }

    pub fn host_magic(self) -> u16 {
        match self {
            ProtocolVersion::V26 => HOST_MAGIC_V26,
            ProtocolVersion::V25 => HOST_MAGIC_V25,
        }
    }

    pub fn fw_magic(self) -> u16 {
        match self {
            ProtocolVersion::V26 => FW_MAGIC_V26,
            ProtocolVersion::V25 => FW_MAGIC_V25,
        }
    }
}

/// Stateful command link: tracks the request id counter and the protocol
/// dialect, and performs request/reply exchanges over a [`ControlTransport`].
pub struct CommandLink {
    protocol: ProtocolVersion,
    next_id: u16,
}

impl CommandLink {
    pub fn new(protocol: ProtocolVersion) -> CommandLink {
        CommandLink {
            protocol,
            next_id: 0,
        }
    }

    pub fn protocol(&self) -> ProtocolVersion {
        self.protocol
    }

    /// Build a request packet: header + payload.
    ///
    /// The `size` header field counts **payload words** (the 2/4-byte
    /// command payload, not the header) — see `XnHostProtocolInitHeader`.
    pub fn build_request(&mut self, opcode: Opcode, payload: &[u8]) -> Vec<u8> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let mut buf = Vec::with_capacity(self.protocol.header_size() + payload.len());
        let size_words = (payload.len() / 2) as u16;
        buf.extend_from_slice(&self.protocol.host_magic().to_le_bytes());
        buf.extend_from_slice(&size_words.to_le_bytes());
        buf.extend_from_slice(&opcode.as_u16().to_le_bytes());
        buf.extend_from_slice(&id.to_le_bytes());
        if let ProtocolVersion::V25 = self.protocol {
            // CRC16 field exists but is always sent as 0 by the reference
            // implementation.
            buf.extend_from_slice(&0u16.to_le_bytes());
        }
        buf.extend_from_slice(payload);
        buf
    }

    fn last_id(&self) -> u16 {
        self.next_id.wrapping_sub(1)
    }

    /// Send a command and wait for its complete reply. Returns the reply
    /// payload (bytes after the reply header and error word, padded to
    /// whole words as the device sends them).
    pub fn exchange<T: ControlTransport>(
        &mut self,
        transport: &mut T,
        opcode: Opcode,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        // The request (and its id) is built once and re-sent verbatim on
        // NOT_READY retries, exactly like Execute's stored request buffer.
        let request = self.build_request(opcode, payload);
        let request_id = self.last_id();

        for attempt in 0..=NOT_READY_RETRIES {
            std::thread::sleep(PRE_SEND_DELAY);
            send_with_retries(transport, &request, timeout)?;
            std::thread::sleep(POST_SEND_DELAY);

            match self.receive_reply(transport, opcode.as_u16(), request_id, timeout) {
                Ok(data) => return Ok(data),
                Err(XtionError::Nack(Nack::NotReady)) if attempt < NOT_READY_RETRIES => {
                    std::thread::sleep(NOT_READY_RETRY_DELAY);
                }
                Err(e) => return Err(e),
            }
        }
        Err(XtionError::ControlTimeout)
    }

    /// Receive and validate one reply. Port of `XnHostProtocolReceiveReply`
    /// plus Execute's "Get rest of data" accumulation loop.
    fn receive_reply<T: ControlTransport>(
        &self,
        transport: &mut T,
        expected_opcode: u16,
        expected_id: u16,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut recv: Vec<u8> = Vec::with_capacity(RECEIVE_READ_SIZE);

        loop {
            // One control read (retried on USB timeouts until the budget
            // expires). Each new attempt overwrites the buffer, like the
            // reference passing the same pBuffer again.
            let n = read_control(transport, &mut recv, deadline)?;
            if n == 0 {
                continue;
            }

            match validate_reply_parts(&recv, expected_opcode, expected_id, self.protocol) {
                Ok((data_start, expected_len)) => {
                    // "Get rest of data": the reply may arrive split across
                    // several control transfers; keep reading and appending
                    // until the promised byte count is complete.
                    while recv.len() < data_start + expected_len {
                        let now = Instant::now();
                        if now >= deadline {
                            return Err(XtionError::ControlTimeout);
                        }
                        let mut chunk = [0u8; RECEIVE_READ_SIZE];
                        let n = transport.control_in(&mut chunk, deadline - now)?;
                        if n == 0 {
                            continue;
                        }
                        recv.extend_from_slice(&chunk[..n]);
                    }
                    return Ok(recv[data_start..data_start + expected_len].to_vec());
                }
                // stale answer to an earlier command: discard, receive again
                Err(XtionError::Protocol(ProtocolError::WrongId { .. })) => continue,
                // no magic or a too-short partial first read: retry after a
                // short delay
                Err(XtionError::Protocol(ProtocolError::BadMagic))
                | Err(XtionError::Protocol(ProtocolError::ReplyTooShort)) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(XtionError::Protocol(ProtocolError::BadMagic));
                    }
                    std::thread::sleep(BAD_MAGIC_RETRY_DELAY);
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// One control-IN read of up to [`RECEIVE_READ_SIZE`] bytes into `recv`
/// (clearing it first), retrying USB timeouts until `deadline`.
fn read_control<T: ControlTransport>(
    transport: &mut T,
    recv: &mut Vec<u8>,
    deadline: Instant,
) -> Result<usize> {
    let mut chunk = [0u8; RECEIVE_READ_SIZE];
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(XtionError::ControlTimeout);
        }
        match transport.control_in(&mut chunk, deadline - now) {
            Ok(n) => {
                recv.clear();
                recv.extend_from_slice(&chunk[..n]);
                return Ok(n);
            }
            Err(XtionError::Usb(rusb::Error::Timeout))
            | Err(XtionError::Usb(rusb::Error::Interrupted)) => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Send with the reference retry semantics: retry on timeout/stall,
/// sleeping between attempts (`XnHostProtocolUSBSend`).
fn send_with_retries<T: ControlTransport>(
    transport: &mut T,
    request: &[u8],
    timeout: Duration,
) -> Result<()> {
    for _ in 0..CONTROL_SEND_RETRIES {
        match transport.control_out(request, timeout) {
            Ok(_) => return Ok(()),
            Err(XtionError::Usb(rusb::Error::Timeout))
            | Err(XtionError::Usb(rusb::Error::Pipe)) => {
                std::thread::sleep(Duration::from_millis(CONTROL_SEND_RETRY_DELAY_MS));
            }
            Err(e) => return Err(e),
        }
    }
    Err(XtionError::ControlTimeout)
}

/// Locate and validate the reply inside `buf`; on success return the byte
/// offset where the data starts and the **promised** data length in bytes
/// (`size_words - 1` words, padded up to whole words by the device).
///
/// Port of `ValidateReplyV26`/`ValidateReplyV25`: the firmware magic is
/// searched byte-wise (skipping any garbage), then id, opcode and the ACK
/// word are checked.
pub(crate) fn validate_reply_parts(
    buf: &[u8],
    expected_opcode: u16,
    expected_id: u16,
    protocol: ProtocolVersion,
) -> Result<(usize, usize)> {
    let hsize = protocol.header_size();
    let fw_magic = protocol.fw_magic();
    // A valid reply needs: header + error word (+ data).
    if buf.len() < hsize + 2 {
        return Err(ProtocolError::ReplyTooShort.into());
    }

    // Byte-wise scan for the firmware magic.
    let mut offset = 0usize;
    let last_possible = buf.len() - hsize - 2;
    while offset <= last_possible {
        if u16::from_le_bytes([buf[offset], buf[offset + 1]]) == fw_magic {
            break;
        }
        offset += 1;
    }
    if offset > last_possible {
        return Err(ProtocolError::BadMagic.into());
    }

    let read_u16 = |i: usize| -> u16 { u16::from_le_bytes([buf[i], buf[i + 1]]) };

    let id = read_u16(offset + 6);
    let opcode = read_u16(offset + 4);
    let size_words = read_u16(offset + 2);

    if id != expected_id {
        return Err(ProtocolError::WrongId {
            expected: expected_id,
            actual: id,
        }
        .into());
    }
    if opcode != expected_opcode {
        return Err(ProtocolError::WrongOpcode {
            expected: expected_opcode,
            actual: opcode,
        }
        .into());
    }

    let error_code = read_u16(offset + hsize);
    if error_code != 0 {
        return Err(Nack::from_u16(error_code).into());
    }

    // Reply `size` counts the error word + data words.
    let data_words = size_words.saturating_sub(1) as usize;
    Ok((offset + hsize + 2, data_words * 2))
}

/// Validate a reply buffer and return the data payload (clamped to the
/// bytes actually received; see [`CommandLink::exchange`] for the full
/// accumulation semantics).
pub fn validate_reply(
    buf: &[u8],
    expected_opcode: u16,
    expected_id: u16,
    protocol: ProtocolVersion,
) -> Result<&[u8]> {
    let (data_start, expected_len) =
        validate_reply_parts(buf, expected_opcode, expected_id, protocol)?;
    let data_end = (data_start + expected_len).min(buf.len());
    if data_start > data_end {
        return Ok(&[]);
    }
    Ok(&buf[data_start..data_end])
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeTransport {
        replies: Vec<Vec<u8>>,
        sent: Vec<Vec<u8>>,
    }

    impl ControlTransport for FakeTransport {
        fn control_out(&mut self, data: &[u8], _t: Duration) -> Result<usize> {
            self.sent.push(data.to_vec());
            Ok(data.len())
        }
        fn control_in(&mut self, buf: &mut [u8], _t: Duration) -> Result<usize> {
            let reply = self.replies.remove(0);
            buf[..reply.len()].copy_from_slice(&reply);
            Ok(reply.len())
        }
    }

    /// Build an ACK reply (word-aligned data, as the device sends it).
    fn ack_reply(protocol: ProtocolVersion, id: u16, opcode: u16, data: &[u8]) -> Vec<u8> {
        assert_eq!(data.len() % 2, 0, "test helper wants word-aligned data");
        let mut v = Vec::new();
        v.extend_from_slice(&protocol.fw_magic().to_le_bytes());
        v.extend_from_slice(&((data.len() / 2 + 1) as u16).to_le_bytes());
        v.extend_from_slice(&opcode.to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        if let ProtocolVersion::V25 = protocol {
            v.extend_from_slice(&0u16.to_le_bytes());
        }
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(data);
        v
    }

    fn nack_reply(protocol: ProtocolVersion, id: u16, opcode: u16, code: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&protocol.fw_magic().to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&opcode.to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        if let ProtocolVersion::V25 = protocol {
            v.extend_from_slice(&0u16.to_le_bytes());
        }
        v.extend_from_slice(&code.to_le_bytes());
        v
    }

    #[test]
    fn request_header_v26_roundtrip() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let payload = [0x01, 0x02, 0x00, 0x05]; // 4 bytes
        let req = link.build_request(Opcode::SetParam, &payload);
        assert_eq!(req.len(), 8 + 4);
        assert_eq!(u16::from_le_bytes([req[0], req[1]]), HOST_MAGIC_V26);
        assert_eq!(u16::from_le_bytes([req[2], req[3]]), 2); // 4 payload bytes = 2 words
        assert_eq!(
            u16::from_le_bytes([req[4], req[5]]),
            Opcode::SetParam.as_u16()
        );
        assert_eq!(u16::from_le_bytes([req[6], req[7]]), 0); // first id
    }

    #[test]
    fn request_header_v25_has_crc_field() {
        let mut link = CommandLink::new(ProtocolVersion::V25);
        let req = link.build_request(Opcode::GetVersion, &[]);
        assert_eq!(req.len(), 10);
        assert_eq!(u16::from_le_bytes([req[0], req[1]]), HOST_MAGIC_V25);
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 0);
    }

    #[test]
    fn ids_increment() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let r0 = link.build_request(Opcode::KeepAlive, &[]);
        let r1 = link.build_request(Opcode::KeepAlive, &[]);
        assert_eq!(u16::from_le_bytes([r0[6], r0[7]]), 0);
        assert_eq!(u16::from_le_bytes([r1[6], r1[7]]), 1);
    }

    #[test]
    fn exchange_validates_and_returns_data() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let mut fake = FakeTransport {
            replies: vec![ack_reply(
                ProtocolVersion::V26,
                0,
                Opcode::GetVersion.as_u16(),
                &[0xaa, 0xbb, 0xcc, 0x00],
            )],
            sent: vec![],
        };
        let data = link
            .exchange(
                &mut fake,
                Opcode::GetVersion,
                &[],
                Duration::from_millis(50),
            )
            .unwrap();
        assert_eq!(data, vec![0xaa, 0xbb, 0xcc, 0x00]);
        assert_eq!(fake.sent.len(), 1);
    }

    /// The exact on-hardware failure: the reply arrives SPLIT across two
    /// control transfers, the first carrying only 12 of 34 data bytes.
    /// Reported by the first hardware run ("version reply too short: 12
    /// bytes"); regression test for the rest-of-data accumulation.
    #[test]
    fn exchange_accumulates_split_reply() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let full = ack_reply(
            ProtocolVersion::V26,
            0,
            Opcode::GetVersion.as_u16(),
            &[0x77; 34],
        );
        // first control read: header + ACK + 12 data bytes
        let first = full[..8 + 2 + 12].to_vec();
        // second control read: the remaining 22 data bytes
        let second = full[8 + 2 + 12..].to_vec();
        let mut fake = FakeTransport {
            replies: vec![first, second],
            sent: vec![],
        };
        let data = link
            .exchange(
                &mut fake,
                Opcode::GetVersion,
                &[],
                Duration::from_millis(200),
            )
            .unwrap();
        assert_eq!(data, vec![0x77; 34]);
    }

    #[test]
    fn exchange_accumulates_across_many_small_reads() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let payload: Vec<u8> = (0..32u16).flat_map(|v| v.to_le_bytes()).collect();
        let full = ack_reply(
            ProtocolVersion::V26,
            0,
            Opcode::GetFixedParams.as_u16(),
            &payload,
        );
        // the first control read must carry the full header (as real
        // transfers do); the rest can arrive in tiny pieces
        let mut replies = vec![full[..12].to_vec()];
        for chunk in full[12..].chunks(7) {
            replies.push(chunk.to_vec());
        }
        let mut fake = FakeTransport {
            replies,
            sent: vec![],
        };
        let data = link
            .exchange(
                &mut fake,
                Opcode::GetFixedParams,
                &[0, 0],
                Duration::from_millis(500),
            )
            .unwrap();
        assert_eq!(data.len(), 64);
        // word 1 of the payload is value 1
        assert_eq!(u16::from_le_bytes([data[2], data[3]]), 1);
    }

    #[test]
    fn stale_reply_with_wrong_id_is_discarded() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let stale = ack_reply(ProtocolVersion::V26, 99, Opcode::GetParam.as_u16(), &[0, 0]);
        let good = ack_reply(
            ProtocolVersion::V26,
            0,
            Opcode::GetParam.as_u16(),
            &[0x11, 0x22],
        );
        let mut fake = FakeTransport {
            replies: vec![stale, good],
            sent: vec![],
        };
        let data = link
            .exchange(
                &mut fake,
                Opcode::GetParam,
                &[0x09, 0x00],
                Duration::from_millis(200),
            )
            .unwrap();
        assert_eq!(data, vec![0x11, 0x22]);
    }

    #[test]
    fn not_ready_nack_is_retried() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let busy = nack_reply(
            ProtocolVersion::V26,
            0,
            Opcode::SetParam.as_u16(),
            Nack::NotReady as u16,
        );
        // the retry re-sends the SAME request id (0)
        let good = ack_reply(ProtocolVersion::V26, 0, Opcode::SetParam.as_u16(), &[]);
        let mut fake = FakeTransport {
            replies: vec![busy, good],
            sent: vec![],
        };
        link.exchange(
            &mut fake,
            Opcode::SetParam,
            &[0, 0, 0, 0],
            Duration::from_millis(200),
        )
        .unwrap();
        // two sends, both carrying request id 0
        assert_eq!(fake.sent.len(), 2);
        assert_eq!(u16::from_le_bytes([fake.sent[0][6], fake.sent[0][7]]), 0);
        assert_eq!(u16::from_le_bytes([fake.sent[1][6], fake.sent[1][7]]), 0);
    }

    #[test]
    fn exchange_skips_garbage_before_magic() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let mut reply = vec![0xde, 0xad, 0xbe, 0xef];
        reply.extend(ack_reply(
            ProtocolVersion::V26,
            0,
            Opcode::GetParam.as_u16(),
            &[0x11, 0x22],
        ));
        let mut fake = FakeTransport {
            replies: vec![reply],
            sent: vec![],
        };
        let data = link
            .exchange(
                &mut fake,
                Opcode::GetParam,
                &[0x09, 0x00],
                Duration::from_millis(50),
            )
            .unwrap();
        assert_eq!(data, vec![0x11, 0x22]);
    }

    #[test]
    fn exchange_maps_nack() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let reply = nack_reply(
            ProtocolVersion::V26,
            0,
            Opcode::SetParam.as_u16(),
            Nack::InvalidCommand as u16,
        );
        let mut fake = FakeTransport {
            replies: vec![reply],
            sent: vec![],
        };
        let err = link
            .exchange(
                &mut fake,
                Opcode::SetParam,
                &[0, 0, 0, 0],
                Duration::from_millis(50),
            )
            .unwrap_err();
        assert!(matches!(err, XtionError::Nack(Nack::InvalidCommand)));
    }

    #[test]
    fn wrong_id_is_rejected_by_validate() {
        let reply = ack_reply(ProtocolVersion::V26, 7, Opcode::GetParam.as_u16(), &[0, 0]);
        let err =
            validate_reply(&reply, Opcode::GetParam.as_u16(), 3, ProtocolVersion::V26).unwrap_err();
        assert!(matches!(
            err,
            XtionError::Protocol(ProtocolError::WrongId {
                expected: 3,
                actual: 7
            })
        ));
    }
}
