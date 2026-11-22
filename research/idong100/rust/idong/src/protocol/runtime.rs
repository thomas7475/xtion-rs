//! Runtime protocol client: keep-alive, link status, handle match.
//!
//! Decoded from `xeyez_tx_builder.txt` / `xeyez_command_protocol.txt`
//! (xEyeUSB.dll):
//! - RX type `0xFFF` → host answers TX type `0xFFE` within 1 s.
//! - RX type 4 (link status): word at payload offset +2 (frame +8) selects
//!   the subcode — `1` or `0xF` → reply type 4 with word `0x11`;
//!   dwords at frame +0xC/+0x10 equal to −1 select internal state 3 /
//!   command 2, else state 4 / command 3.
//! - Handle match (`NeedHandleMatch=1`, `MonitorList=10`):
//!   TSOL_DEVICE_HANDLE_FOUND / NOT_FOUND / TIMEOUT / FOUND_OVER.

use super::frames::Frame;
use super::{
    MAGIC_D2H, LINK_STATUS_REPLY_WORD, TIMEOUT_ACK, TYPE_KEEPALIVE_RX, TYPE_KEEPALIVE_TX,
    TYPE_LINK_STATUS,
};
use crate::error::{IdongError, ProtocolError, Result};
use crate::handle::{HandleConfig, HandleEvent};
use crate::usb::Transport;

/// Runtime link status decoded from a type-4 frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkStatus {
    /// Subcode 1: nominal.
    Nominal,
    /// Subcode 0xF: disturbance reported.
    Disturbed,
}

/// Runtime client operating on an open runtime [`Transport`].
pub struct RuntimeClient<'a> {
    transport: &'a Transport,
}

impl<'a> RuntimeClient<'a> {
    pub fn new(transport: &'a Transport) -> Self {
        RuntimeClient { transport }
    }

    /// Answer an RX 0xFFF keep-alive with TX 0xFFE (1 s budget).
    pub fn answer_keepalive(&self) -> Result<()> {
        let f = Frame::h2d(TYPE_KEEPALIVE_TX, &[]);
        self.transport.write_frame(&f, super::TIMEOUT_ACK)
    }

    /// Parse a link-status (type 4) frame and produce the mandated reply.
    /// Returns the decoded status.
    pub fn handle_link_status(&self, f: &Frame) -> Result<LinkStatus> {
        if f.kind != TYPE_LINK_STATUS {
            return Err(ProtocolError::BadType {
                kind: f.kind,
                expected: TYPE_LINK_STATUS,
            }.into());
        }
        // payload starts at frame +6; the disassembly reads word @ +8 and
        // dwords @ +0xC/+0x10.
        if f.payload.len() < 4 {
            return Err(IdongError::Protocol(ProtocolError::Short {
                len: f.payload.len(),
                need: 4,
            }));
        }
        let sub = u16::from_le_bytes([f.payload[2], f.payload[3]]);
        let status = match sub {
            1 => LinkStatus::Nominal,
            0xF => LinkStatus::Disturbed,
            other => {
                return Err(IdongError::Protocol(ProtocolError::BadType {
                    kind: other,
                    expected: TYPE_LINK_STATUS,
                }))
            }
        };
        // Reply: type 4, word 0x11 (tx_builder @0x1001028e/0x10010327).
        let mut p = vec![0u8; 2];
        p.copy_from_slice(&LINK_STATUS_REPLY_WORD.to_le_bytes());
        let reply = Frame::h2d(TYPE_LINK_STATUS, &p);
        self.transport.write_frame(&reply, super::TIMEOUT_ACK)?;
        Ok(status)
    }

    /// Is `f` a keep-alive poll from the device?
    pub fn is_keepalive(f: &Frame) -> bool {
        f.kind == TYPE_KEEPALIVE_RX && f.magic == super::MAGIC_D2H
    }

    /// Start a handle-match round (airtouch wristband pairing).
    /// The exact handle-config blob layout is open item #6; the config is
    /// serialized verbatim from a captured session until then.
    pub fn handle_match(&self, cfg: &HandleConfig) -> Result<HandleEvent> {
        let f = Frame::h2d(TYPE_LINK_STATUS, &cfg.raw);
        self.transport.write_frame(&f, super::TIMEOUT_ACK)?;
        // The event arrives asynchronously; the streamer surfaces it via
        // `Event::Handle`. Here we only confirm the request was sent.
        Ok(HandleEvent::Requested)
    }
}
