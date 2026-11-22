//! PsMonitor command protocol: request framing, exchange and reply
//! validation. Port of `Sensor/XnHostProtocol.cpp` (InitHeader, USBSend,
//! USBReceive, ValidateReplyV25/V26, Execute).

use std::time::{Duration, Instant};

use crate::error::{Nack, ProtocolError, Result, XtionError};

use super::{
    Opcode, CONTROL_SEND_RETRIES, CONTROL_SEND_RETRY_DELAY_MS, FW_MAGIC_V25, FW_MAGIC_V26,
    HOST_MAGIC_V25, HOST_MAGIC_V26,
};

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

    /// Send a command and wait for its reply. Returns the reply payload
    /// (bytes after the reply header and error word).
    pub fn exchange<T: ControlTransport>(
        &mut self,
        transport: &mut T,
        opcode: Opcode,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let request = self.build_request(opcode, payload);
        send_with_retries(transport, &request, timeout)?;

        let deadline = Instant::now() + timeout;
        let mut buf = vec![0u8; 1024];
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(XtionError::ControlTimeout);
            }
            match transport.control_in(&mut buf, deadline - now) {
                Ok(n) => {
                    let data = validate_reply(
                        &buf[..n],
                        opcode.as_u16(),
                        // id of the request we just built:
                        self.last_id(),
                        self.protocol,
                    )?;
                    return Ok(data.to_vec());
                }
                Err(XtionError::Usb(rusb::Error::Timeout)) => continue,
                Err(XtionError::Usb(rusb::Error::Interrupted)) => continue,
                Err(e) => return Err(e),
            }
        }
    }

    fn last_id(&self) -> u16 {
        self.next_id.wrapping_sub(1)
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

/// Validate a reply buffer and return the data payload.
///
/// Port of `ValidateReplyV26`/`ValidateReplyV25`: the firmware magic is
/// searched byte-wise (skipping any garbage), then id, opcode and the ACK
/// word are checked. On success returns the bytes after the reply header
/// and error word.
pub fn validate_reply(
    buf: &[u8],
    expected_opcode: u16,
    expected_id: u16,
    protocol: ProtocolVersion,
) -> Result<&[u8]> {
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
    let data_start = offset + hsize + 2;
    let data_end = (data_start + data_words * 2).min(buf.len());
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

    fn ack_reply(protocol: ProtocolVersion, id: u16, opcode: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        // reply size counts the error word + data words, padded up to a
        // whole word (the firmware always works in 16-bit words)
        v.extend_from_slice(&protocol.fw_magic().to_le_bytes());
        v.extend_from_slice(&((data.len().div_ceil(2) + 1) as u16).to_le_bytes());
        v.extend_from_slice(&opcode.to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        if let ProtocolVersion::V25 = protocol {
            v.extend_from_slice(&0u16.to_le_bytes());
        }
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn request_header_v26_roundtrip() {
        let mut link = CommandLink::new(ProtocolVersion::V26);
        let payload = [0x0102u16.to_le_bytes()[0], 0, 5, 0].to_vec(); // 4 bytes
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
                &[0xaa, 0xbb, 0xcc],
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
        assert_eq!(data, vec![0xaa, 0xbb, 0xcc]);
        assert_eq!(fake.sent.len(), 1);
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
        let mut reply = ack_reply(ProtocolVersion::V26, 0, Opcode::SetParam.as_u16(), &[]);
        // overwrite ACK word with NACK_INVALID_COMMAND
        let ack_off = 8; // v26 header size
        reply[ack_off] = Nack::InvalidCommand as u8;
        reply[ack_off + 1] = 0;
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
    fn wrong_id_is_rejected() {
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
