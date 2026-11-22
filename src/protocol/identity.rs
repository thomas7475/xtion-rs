//! Identity and actuator commands: platform string, USB core type and
//! IR-emitter toggle. Port of the matching functions in
//! `Sensor/XnHostProtocol.cpp` (XnHostProtocolGetPlatformString,
//! XnHostProtocolGetUsbCoreType, XnHostProtocolSetEmitterState).
//!
//! All three are read-only or runtime-state commands — nothing here can
//! touch calibration data or persistent configuration.

use std::time::Duration;

use crate::error::{Result, XtionError};
use crate::protocol::command::{CommandLink, ControlTransport};
use crate::protocol::{Opcode, UsbCore};

/// Read the sensor platform string (OPCODE_GET_PLATFORM_STRING).
///
/// The reply stores one character per 16-bit word — the C driver copies
/// `platformStringBuffer[i * 2]` for each output character — so we take
/// every second byte until the end of the data. Firmware that does not
/// support the opcode answers with a NACK, which the C maps to an empty
/// string; we surface the error instead so callers can decide.
pub fn read_platform_string<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<String> {
    let data = link.exchange(transport, Opcode::GetPlatformString, &[], timeout)?;
    let chars: Vec<u8> = data
        .chunks_exact(2)
        .map(|c| c[0])
        .take_while(|&b| b != 0)
        .collect();
    Ok(String::from_utf8_lossy(&chars).trim_end().to_string())
}

/// Read the USB core type (OPCODE_GET_USB_CORE_TYPE): a single u16,
/// `XN_USB_CORE_JANGO = 0` or `XN_USB_CORE_GADGETFS = 1`.
pub fn read_usb_core_type<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<UsbCore> {
    let data = link.exchange(transport, Opcode::GetUsbCoreType, &[], timeout)?;
    if data.len() < 2 {
        return Err(XtionError::BadConfig(
            "usb core type reply too short".into(),
        ));
    }
    let value = u16::from_le_bytes([data[0], data[1]]);
    UsbCore::from_u16(value)
        .ok_or_else(|| XtionError::BadConfig(format!("unknown usb core type {value}")))
}

/// Enable or disable the IR projector (OPCODE_ENABLE_EMITTER).
///
/// Request payload is a single u16 (0/1); this is the runtime toggle the
/// reference driver issues for `XN_MODULE_PROPERTY_EMITTER_STATE`. It is
/// volatile firmware state — power-cycling the camera restores the emitter.
pub fn set_emitter_state<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    active: bool,
    timeout: Duration,
) -> Result<()> {
    let payload = (active as u16).to_le_bytes();
    link.exchange(transport, Opcode::EnableEmitter, &payload, timeout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::command::{ControlTransport, ProtocolVersion};
    use crate::protocol::{Opcode, FW_MAGIC_V26};
    use std::time::Duration;

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

    fn ack_reply(id: u16, opcode: Opcode, data: &[u8]) -> Vec<u8> {
        let mut data = data.to_vec();
        if data.len() % 2 == 1 {
            data.push(0); // the device pads to whole words
        }
        let mut v = Vec::new();
        v.extend_from_slice(&FW_MAGIC_V26.to_le_bytes());
        v.extend_from_slice(&((data.len() / 2 + 1) as u16).to_le_bytes());
        v.extend_from_slice(&opcode.as_u16().to_le_bytes());
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // ACK
        v.extend_from_slice(&data);
        v
    }

    fn link_and(replies: Vec<Vec<u8>>) -> (CommandLink, FakeTransport) {
        (
            CommandLink::new(ProtocolVersion::V26),
            FakeTransport {
                replies,
                sent: vec![],
            },
        )
    }

    #[test]
    fn platform_string_takes_every_second_byte() {
        // "RD108x-112" as one char per u16 word, as the firmware sends it
        let text = "RD108x-112";
        let mut data = Vec::new();
        for b in text.bytes() {
            data.extend_from_slice(&[b, 0]);
        }
        data.extend_from_slice(&[0, 0]); // NUL terminator
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetPlatformString, &data)]);
        let s = read_platform_string(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(s, "RD108x-112");
    }

    #[test]
    fn platform_string_stops_at_nul() {
        // trailing garbage after the NUL must not leak into the string
        let data: Vec<u8> = "AB".bytes().flat_map(|b| vec![b, 0]).collect();
        let mut data = data;
        data.extend_from_slice(&[0, 0]); // NUL
        data.extend_from_slice(b"\xff\xff"); // garbage word
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetPlatformString, &data)]);
        let s = read_platform_string(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(s, "AB");
    }

    #[test]
    fn usb_core_type_parses_jango() {
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetUsbCoreType, &[0, 0])]);
        let core = read_usb_core_type(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(core, UsbCore::Jango);
    }

    #[test]
    fn usb_core_type_parses_gadgetfs() {
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetUsbCoreType, &[1, 0])]);
        let core = read_usb_core_type(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(core, UsbCore::Gadgetfs);
    }

    #[test]
    fn usb_core_type_rejects_unknown() {
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetUsbCoreType, &[9, 0])]);
        let err = read_usb_core_type(&mut link, &mut fake, Duration::from_millis(50))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown usb core"), "{err}");
    }

    #[test]
    fn emitter_request_carries_single_u16() {
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::EnableEmitter, &[])]);
        set_emitter_state(&mut link, &mut fake, true, Duration::from_millis(50)).unwrap();
        assert_eq!(fake.sent.len(), 1);
        let req = &fake.sent[0];
        // header (8) + 1 word payload
        assert_eq!(req.len(), 10);
        assert_eq!(u16::from_le_bytes([req[2], req[3]]), 1); // size_words
        assert_eq!(
            u16::from_le_bytes([req[4], req[5]]),
            Opcode::EnableEmitter.as_u16()
        );
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 1); // active
    }

    #[test]
    fn emitter_off_sends_zero() {
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::EnableEmitter, &[])]);
        set_emitter_state(&mut link, &mut fake, false, Duration::from_millis(50)).unwrap();
        let req = &fake.sent[0];
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 0);
    }
}
