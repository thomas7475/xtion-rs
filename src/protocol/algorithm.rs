//! ALGORITHM_PARAMS: per-resolution algorithm data (depth calibration,
//! registration tables). Port of `XnHostProtocolAlgorithmParams` and
//! `XnAlgorithmParamRequest` (Sensor/XnHostProtocol.cpp lines 2344-2430).

use crate::error::{Result, XtionError};

use super::{command::ControlTransport, CommandLink, Opcode, Resolution};
use std::time::Duration;

/// `XnDepthInformation`: the only field the driver consumes from
/// ALGORITHM_DEPTH_INFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthInformation {
    pub const_shift: u16,
}

/// Build the 10-byte V5.1+ algorithm-param request payload:
/// `{ u16 param_id, u16 format, u16 resolution, u16 fps, u16 offset_words }`.
fn build_request(
    algorithm_id: u16,
    resolution: Resolution,
    fps: u16,
    offset_words: u16,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(10);
    payload.extend_from_slice(&algorithm_id.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes()); // format
    payload.extend_from_slice(&resolution.as_u16().to_le_bytes());
    payload.extend_from_slice(&fps.to_le_bytes());
    payload.extend_from_slice(&offset_words.to_le_bytes());
    payload
}

/// Read `total_size` bytes of algorithm data for one algorithm/resolution.
fn read_algorithm_params<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    algorithm_id: u16,
    resolution: Resolution,
    fps: u16,
    total_size: usize,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut blob = Vec::with_capacity(total_size);
    while blob.len() < total_size {
        let offset_words = (blob.len() / 2) as u16;
        let payload = build_request(algorithm_id, resolution, fps, offset_words);
        let data = link.exchange(transport, Opcode::AlgorithmParams, &payload, timeout)?;
        if data.is_empty() {
            return Err(XtionError::AlgorithmParamsEmpty);
        }
        blob.extend_from_slice(&data);
    }
    Ok(blob)
}

/// Read the depth `ConstShift` calibration constant (algorithm 0x00,
/// requested at VGA/30 like the reference driver does).
pub fn read_depth_information<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<DepthInformation> {
    let blob = read_algorithm_params(
        link,
        transport,
        super::algorithm_ids::DEPTH_INFO,
        Resolution::Vga,
        30,
        2, // sizeof(XnDepthInformation)
        timeout,
    )?;
    Ok(DepthInformation {
        const_shift: u16::from_le_bytes([blob[0], blob[1]]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_payload_layout() {
        let payload = build_request(
            super::super::algorithm_ids::DEPTH_INFO,
            Resolution::Vga,
            30,
            4,
        );
        assert_eq!(payload.len(), 10);
        assert_eq!(u16::from_le_bytes([payload[0], payload[1]]), 0x00);
        assert_eq!(u16::from_le_bytes([payload[4], payload[5]]), 1); // VGA
        assert_eq!(u16::from_le_bytes([payload[6], payload[7]]), 30);
        assert_eq!(u16::from_le_bytes([payload[8], payload[9]]), 4);
    }
}
