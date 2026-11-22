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

/// `XnCmosBlankingInformation`: two {a, b} float pairs, one per CMOS
/// (index 0 = depth, 1 = image; Sensor/XnDeviceSensor.h:171-180).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CmosBlanking {
    pub coefficients: [(f32, f32); 2],
}

/// Read the CMOS blanking information (algorithm 0x06, 16 bytes) for one
/// (resolution, fps) pair.
///
/// Port of the `XnCmosInfo::SetCmosConfig` call that the reference driver
/// makes on EVERY stream start: the depth, image and IR
/// `ConfigureStreamImpl`s all fetch the blanking data for the requested
/// resolution/fps (XnSensorIRStream.cpp:147-152, XnSensorDepthStream.cpp,
/// XnSensorImageStream.cpp) before the stream mode is written. The
/// firmware computes the sensor timing for that mode as part of answering
/// the read; without this step real hardware never produces IR packets
/// on the image pipe (observed on fw 5.8.22, 2026-11-22).
pub fn read_cmos_blanking<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    resolution: Resolution,
    fps: u16,
    timeout: Duration,
) -> Result<CmosBlanking> {
    let blob = read_algorithm_params(
        link,
        transport,
        super::algorithm_ids::BLANKING,
        resolution,
        fps,
        16, // sizeof(XnCmosBlankingInformation) = 2 * 2 * f32
        timeout,
    )?;
    Ok(parse_blanking(blob))
}

/// Parse the 16-byte `XnCmosBlankingInformation` blob into two {a, b}
/// coefficient pairs.
fn parse_blanking(blob: Vec<u8>) -> CmosBlanking {
    let pair = |i: usize| {
        (
            f32::from_le_bytes([blob[i], blob[i + 1], blob[i + 2], blob[i + 3]]),
            f32::from_le_bytes([blob[i + 4], blob[i + 5], blob[i + 6], blob[i + 7]]),
        )
    };
    CmosBlanking {
        coefficients: [pair(0), pair(8)],
    }
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

    #[test]
    fn blanking_request_layout_and_parse() {
        // the request must be algorithm 0x06 for the given mode
        let payload = build_request(
            super::super::algorithm_ids::BLANKING,
            Resolution::Vga,
            30,
            0,
        );
        assert_eq!(payload.len(), 10);
        assert_eq!(u16::from_le_bytes([payload[0], payload[1]]), 0x06);
        assert_eq!(u16::from_le_bytes([payload[6], payload[7]]), 30);

        // the reply parses into two float pairs (little-endian floats on
        // the wire): [a0, b0, a1, b1] = 16 bytes
        let mut blob = Vec::new();
        for v in [1.25f32, -2.5, 3.75, 4.0] {
            blob.extend_from_slice(&v.to_le_bytes());
        }
        let parsed = parse_blanking(blob);
        assert_eq!(parsed.coefficients[0], (1.25, -2.5));
        assert_eq!(parsed.coefficients[1], (3.75, 4.0));
    }
}
