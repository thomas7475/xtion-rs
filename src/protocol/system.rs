//! Read-only system/diagnostic commands: firmware log, CPU statistics,
//! BIST, TEC and emitter telemetry, CMOS blanking and register reads
//! (I2C), AHB bus reads, and the READ side of the flash file system
//! (map, file list, file download, raw flash read).
//!
//! Ports of the matching `XnHostProtocol*` functions in
//! `Sensor/XnHostProtocol.cpp`. Everything here is strictly read-only:
//! no file-upload/delete/execute, no calibrations, no I2C/AHB writes —
//! the driver's exclusions from docs/FEATURE_PLAN.md stay in force.
//! Reading flash is safe: the calibration files the updater flashes are
//! exactly what GET_FLASH_MAP / DOWNLOAD_FILE expose.

use std::time::Duration;

use crate::error::{Result, XtionError};
use crate::protocol::command::{CommandLink, ControlTransport};
use crate::protocol::Opcode;

/// Log-entry `log_type` values the C driver special-cases. Firmware
/// >= 3.0 uses 0x1200/0x1201 (XnHostProtocol.cpp:46-53).
const LOG_TEXT_TYPE: u16 = 0x1200;
const LOG_OVERFLOW_TYPE: u16 = 0x1201;

/// Read the firmware log (OPCODE_GET_LOG) until the device reports no
/// more entries, and render it as text. Port of `XnHostProtocolGetLog`
/// (XnHostProtocol.cpp:1213-1332).
///
/// Each exchange returns `XnLogEntryHeader { entry_size: u16 (words),
/// log_type: u16, timestamp: u32 }` followed by `entry_size` words of
/// payload. Text entries are UTF-16 — the C keeps the low byte of each
/// word. Overflow entries mark a wrapped log buffer; anything else is a
/// structured entry `{ line: u16, param: u16 }` rendered as
/// module/error/param/line.
pub fn read_firmware_log<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<String> {
    let mut raw: Vec<u8> = Vec::new();
    for _ in 0..64 {
        let data = link.exchange(transport, Opcode::GetLog, &[], timeout)?;
        if data.is_empty() {
            break;
        }
        raw.extend_from_slice(&data);
    }
    if raw.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::new();
    let mut pos = 0usize;
    while pos + 8 <= raw.len() {
        // XnLogEntryHeader { entry_size: u16 (PAYLOAD words), log_type:
        // u16, timestamp: u32 } — the C advances past the 8-byte header
        // and then by entry_size * 2 payload bytes.
        let entry_size = u16::from_le_bytes([raw[pos], raw[pos + 1]]) as usize; // words
        let log_type = u16::from_le_bytes([raw[pos + 2], raw[pos + 3]]);
        let timestamp =
            u32::from_le_bytes([raw[pos + 4], raw[pos + 5], raw[pos + 6], raw[pos + 7]]);
        pos += 8;
        let payload_len = (entry_size * 2).min(raw.len().saturating_sub(pos));
        let payload = &raw[pos..pos + payload_len];
        pos += payload_len;

        match log_type {
            LOG_TEXT_TYPE => {
                let text: Vec<u8> = payload.chunks_exact(2).map(|w| w[0]).collect();
                out.push_str(&format!(
                    "{}:\t{}\n",
                    timestamp,
                    String::from_utf8_lossy(&text).trim_end()
                ));
            }
            LOG_OVERFLOW_TYPE => {
                out.push_str(&format!("{timestamp}:\tLog Overflow\n"));
            }
            other => {
                let line = payload
                    .chunks_exact(2)
                    .next()
                    .map(|w| u16::from_le_bytes([w[0], w[1]]))
                    .unwrap_or(0);
                let param = payload
                    .chunks_exact(2)
                    .nth(1)
                    .map(|w| u16::from_le_bytes([w[0], w[1]]))
                    .unwrap_or(0);
                out.push_str(&format!(
                    "{}:\tModule: [0x{:X}], Error: [0x{:X}], Param: 0x{:X}, (Line: {})\n",
                    timestamp,
                    other >> 8,
                    other & 0xff,
                    param,
                    line
                ));
            }
        }
    }
    Ok(out)
}

/// Read bytes from a device flash file by type (OPCODE_DOWNLOAD_FILE),
/// chunk by chunk, until the device returns nothing. Port of
/// `XnHostProtocolFileDownloadChunk` + the download loop
/// (XnHostProtocol.cpp:2511-2602). The request offset is in WORDS (the
/// C divides the byte offset by `sizeof(XnUInt16)`).
pub fn download_file<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    file_type: u16,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut blob = Vec::new();
    let mut offset_words: u32 = 0;
    for _ in 0..4096 {
        let mut payload = Vec::with_capacity(6);
        payload.extend_from_slice(&file_type.to_le_bytes());
        payload.extend_from_slice(&offset_words.to_le_bytes());
        let data = link.exchange(transport, Opcode::DownloadFile, &payload, timeout)?;
        if data.is_empty() {
            break;
        }
        blob.extend_from_slice(&data);
        offset_words += (data.len() / 2) as u32;
    }
    Ok(blob)
}

/// One entry of the device flash map (`XnFlashEntry`, 14 bytes packed).
/// Source: XnHostProtocol.cpp:2313-2326.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashEntry {
    pub file_type: u16,
    pub offset_in_flash: u32,
    pub size_in_words: u32,
    pub version_major: u8,
    pub version_minor: u8,
    pub version_build: u16,
}

/// Read the device flash map (OPCODE_GET_FLASH_MAP). Port of
/// `XnHostProtocolGetFlashMap` (XnHostProtocol.cpp:2283-2361).
pub fn get_flash_map<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<Vec<FlashEntry>> {
    let data = link.exchange(transport, Opcode::GetFlashMap, &[], timeout)?;
    let mut entries = Vec::new();
    let mut pos = 0usize;
    while pos + 14 <= data.len() {
        entries.push(FlashEntry {
            file_type: u16::from_le_bytes([data[pos], data[pos + 1]]),
            offset_in_flash: u32::from_le_bytes([
                data[pos + 2],
                data[pos + 3],
                data[pos + 4],
                data[pos + 5],
            ]),
            size_in_words: u32::from_le_bytes([
                data[pos + 6],
                data[pos + 7],
                data[pos + 8],
                data[pos + 9],
            ]),
            version_major: data[pos + 10],
            version_minor: data[pos + 11],
            version_build: u16::from_le_bytes([data[pos + 12], data[pos + 13]]),
        });
        pos += 14;
    }
    Ok(entries)
}

/// One flash file-system entry (`XnFlashFile`, 24 bytes packed). Source:
/// Include/PS1080.h:600-609.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashFile {
    pub id: u16,
    pub file_type: u16,
    pub version: u32,
    pub offset: u32,
    pub size: u32,
    pub crc: u16,
    pub attributes: u16,
    pub reserved: u16,
}

/// List the files in the device flash file system (OPCODE_GET_FILE_LIST),
/// starting at file id 0 and following the id chain until the device
/// returns nothing. Port of `XnHostProtocolGetFileList`
/// (XnHostProtocol.cpp:2458-2510).
pub fn get_file_list<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<Vec<FlashFile>> {
    let mut files = Vec::new();
    let mut first_id: u16 = 0;
    for _ in 0..64 {
        let data = link.exchange(
            transport,
            Opcode::GetFileList,
            &first_id.to_le_bytes(),
            timeout,
        )?;
        if data.is_empty() {
            break;
        }
        let mut pos = 0usize;
        while pos + 24 <= data.len() {
            files.push(FlashFile {
                id: u16::from_le_bytes([data[pos], data[pos + 1]]),
                file_type: u16::from_le_bytes([data[pos + 2], data[pos + 3]]),
                version: u32::from_le_bytes([
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]),
                offset: u32::from_le_bytes([
                    data[pos + 8],
                    data[pos + 9],
                    data[pos + 10],
                    data[pos + 11],
                ]),
                size: u32::from_le_bytes([
                    data[pos + 12],
                    data[pos + 13],
                    data[pos + 14],
                    data[pos + 15],
                ]),
                crc: u16::from_le_bytes([data[pos + 16], data[pos + 17]]),
                attributes: u16::from_le_bytes([data[pos + 18], data[pos + 19]]),
                reserved: u16::from_le_bytes([data[pos + 20], data[pos + 21]]),
            });
            pos += 24;
        }
        let next = files.last().map(|f| f.id.wrapping_add(1));
        match next {
            Some(id) => first_id = id,
            None => break,
        }
    }
    Ok(files)
}

/// Read raw bytes from device flash (OPCODE_READ_FLASH), chunk by chunk.
/// Port of `XnHostProtocolReadFlashChunk` + `XnHostProtocolReadFlash`
/// (XnHostProtocol.cpp:2603-2678). NOTE the asymmetry with
/// [`download_file`]: this request's offset is a raw BYTE offset.
pub fn read_flash<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    offset: u32,
    size: u32,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut blob = Vec::with_capacity(size as usize);
    let mut loop_offset = offset;
    while (blob.len() as u32) < size {
        let chunk_words: u16 = ((size - blob.len() as u32) / 2).min(0x200) as u16;
        if chunk_words == 0 {
            break;
        }
        let mut payload = Vec::with_capacity(6);
        payload.extend_from_slice(&loop_offset.to_le_bytes());
        payload.extend_from_slice(&chunk_words.to_le_bytes());
        let data = link.exchange(transport, Opcode::ReadFlash, &payload, timeout)?;
        if data.is_empty() {
            return Err(XtionError::BadConfig(format!(
                "flash read at {loop_offset:#x} returned no data"
            )));
        }
        blob.extend_from_slice(&data);
        loop_offset += data.len() as u32;
    }
    blob.truncate(size as usize);
    Ok(blob)
}

/// Read one 32-bit value from the SoC's AHB bus (OPCODE_READ_AHB). Port
/// of `XnHostProtocolReadAHB` (XnHostProtocol.cpp:1521-1545). The WRITE
/// variant is permanently excluded.
pub fn read_ahb<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    address: u32,
    timeout: Duration,
) -> Result<u32> {
    let data = link.exchange(transport, Opcode::ReadAhb, &address.to_le_bytes(), timeout)?;
    if data.len() < 4 {
        return Err(XtionError::BadConfig("AHB read reply too short".into()));
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// CPU utilization per firmware task (`XnTaskCPUInfo`, 8 bytes packed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTaskStats {
    pub times_executed: u32,
    pub time_microseconds: u32,
}

/// Read the firmware CPU statistics (OPCODE_GET_CPU_STATS). Port of
/// `XnHostProtocolGetCPUStats` (XnHostProtocol.cpp:2706-2745).
pub fn get_cpu_stats<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<Vec<CpuTaskStats>> {
    let data = link.exchange(transport, Opcode::GetCpuStats, &[], timeout)?;
    let mut tasks = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        tasks.push(CpuTaskStats {
            times_executed: u32::from_le_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
            ]),
            time_microseconds: u32::from_le_bytes([
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]),
        });
        pos += 8;
    }
    Ok(tasks)
}

/// BIST test masks (Include/PS1080.h:373-379). Only the low 16 bits
/// travel on the wire (`(XnUInt16)nTestsMask`).
pub mod bist {
    pub const IMAGE_CMOS: u16 = 1 << 0;
    pub const IR_CMOS: u16 = 1 << 1;
    pub const POTENTIOMETER: u16 = 1 << 2;
    pub const FLASH: u16 = 1 << 3;
    pub const FULL_FLASH: u16 = 1 << 4;
    pub const PROJECTOR: u16 = 1 << 5;
    pub const TEC: u16 = 1 << 6;
}

/// Run the built-in self-test for the selected modules
/// (OPCODE_BIST). Port of `XnHostProtocolRunBIST`
/// (XnHostProtocol.cpp:2679-2705). Returns the failure bitmask (0 = all
/// green). The C uses a 300 s timeout for this opcode
/// (XN_USB_HOST_PROTOCOL_TIMEOUT_BIST) — pass a matching `timeout`.
pub fn run_bist<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    tests_mask: u16,
    timeout: Duration,
) -> Result<u32> {
    let data = link.exchange(transport, Opcode::Bist, &tests_mask.to_le_bytes(), timeout)?;
    if data.len() < 4 {
        return Err(XtionError::BadConfig("BIST reply too short".into()));
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// TEC controller telemetry, the post-5.4 `XnTecFastConvergenceData`
/// layout (Include/PS1080.h:549-562; 24 bytes packed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TecFastConvergenceData {
    /// Set point in degrees celsius, scaled by 100.
    pub set_point_temperature: i16,
    /// Measured temperature in degrees celsius, scaled by 100.
    pub measured_temperature: i16,
    pub proportional_error: i32,
    pub integral_error: i32,
    pub derivative_error: i32,
    /// 0 initial, 1 crude, 2 precise.
    pub scan_mode: u16,
    /// 0 idle, 1 heat, 2 cool.
    pub heat_mode: u16,
    /// Duty cycle in percent.
    pub tec_duty_cycle: u16,
    /// 0 cool, 1 room, 2 warm.
    pub temperature_range: u16,
}

/// Read the TEC fast-convergence data (OPCODE_GET_FAST_CONVERGENCE_TEC).
/// Port of `XnHostProtocolGetTecFastConvergenceData`
/// (XnHostProtocol.cpp:2946-2982). This is what firmware >= 5.4 answers
/// for TEC reads; this camera's 5.8.22 always takes this path.
pub fn get_tec_fast_convergence_data<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<TecFastConvergenceData> {
    let data = link.exchange(transport, Opcode::GetFastConvergenceTec, &[], timeout)?;
    if data.len() < 24 {
        return Err(XtionError::BadConfig(format!(
            "TEC fast-convergence reply too short: {} bytes",
            data.len()
        )));
    }
    let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    let i16_at = |i: usize| i16::from_le_bytes([data[i], data[i + 1]]);
    let i32_at = |i: usize| i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    Ok(TecFastConvergenceData {
        set_point_temperature: i16_at(0),
        measured_temperature: i16_at(2),
        proportional_error: i32_at(4),
        integral_error: i32_at(8),
        derivative_error: i32_at(12),
        scan_mode: u16_at(16),
        heat_mode: u16_at(18),
        tec_duty_cycle: u16_at(20),
        temperature_range: u16_at(22),
    })
}

/// IR emitter telemetry (`XnEmitterData`, 12 x u16 = 24 bytes packed).
/// Source: Include/PS1080.h:564-577.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitterData {
    /// idle / calibrating.
    pub state: u16,
    /// The set point voltage (as the XML calibration stores it).
    pub set_point_voltage: u16,
    /// Target cross duty cycle in system clocks.
    pub set_point_clocks: u16,
    /// Current cross duty cycle in system clocks (high time).
    pub pd_reading: u16,
    /// Emitter set duty cycle in system clocks (high time).
    pub emitter_set: u16,
    /// TRUE = positive logic.
    pub emitter_setting_logic: u16,
    /// TRUE = positive logic.
    pub light_measure_logic: u16,
    pub is_apc_enabled: u16,
    /// MilliVolts (fw >= 5.1).
    pub emitter_set_step_size: u16,
    /// System clocks (fw >= 5.1).
    pub apc_tolerance: u16,
    /// System clocks (fw >= 5.3).
    pub sub_clocking: u16,
    /// fw >= 5.3.
    pub precision: u16,
}

/// Read the IR emitter telemetry (OPCODE_GET_EMITTER_DATA). Port of
/// `XnHostProtocolGetEmitterData` (XnHostProtocol.cpp:3008-3060). All
/// 12 fields are read when present; short replies zero the tail exactly
/// like the C's partially-filled struct reads.
pub fn get_emitter_data<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<EmitterData> {
    let data = link.exchange(transport, Opcode::GetEmitterData, &[], timeout)?;
    if data.len() < 20 {
        return Err(XtionError::BadConfig(format!(
            "emitter data reply too short: {} bytes",
            data.len()
        )));
    }
    let u16_at = |i: usize| {
        data.get(i..i + 2)
            .map(|s| u16::from_le_bytes([s[0], s[1]]))
            .unwrap_or(0)
    };
    Ok(EmitterData {
        state: u16_at(0),
        set_point_voltage: u16_at(2),
        set_point_clocks: u16_at(4),
        pd_reading: u16_at(6),
        emitter_set: u16_at(8),
        emitter_setting_logic: u16_at(10),
        light_measure_logic: u16_at(12),
        is_apc_enabled: u16_at(14),
        emitter_set_step_size: u16_at(16),
        apc_tolerance: u16_at(18),
        sub_clocking: u16_at(20),
        precision: u16_at(22),
    })
}

/// Read the current vertical blanking of one CMOS
/// (OPCODE_GET_CMOS_BLANKING; `XnGetCmosBlankingRequest/Reply`,
/// XnHostProtocol.cpp:3211-3245). This is the direct firmware command —
/// distinct from the ALGORITHM_PARAMS(0x06) blanking read the stream
/// starts perform.
pub fn get_cmos_blanking_lines<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    cmos_type: u16,
    timeout: Duration,
) -> Result<u32> {
    let data = link.exchange(
        transport,
        Opcode::GetCmosBlanking,
        &cmos_type.to_le_bytes(),
        timeout,
    )?;
    if data.len() < 4 {
        return Err(XtionError::BadConfig(
            "cmos blanking reply too short".into(),
        ));
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read one 16-bit word from an I2C bus (OPCODE_I2C_READ). Port of
/// `XnHostProtocolReadI2C` (XnHostProtocol.cpp:1901-1932): the request
/// carries `{ bus u16, slave u16, read_size u16, write words... }` —
/// the write words are sent first on the bus (typically the register
/// address), and the reply carries `read_size` u16 words.
pub fn read_i2c<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    bus: u16,
    slave_address: u16,
    write_words: &[u16],
    read_words: u16,
    timeout: Duration,
) -> Result<Vec<u16>> {
    let mut payload = Vec::with_capacity(6 + write_words.len() * 2);
    payload.extend_from_slice(&bus.to_le_bytes());
    payload.extend_from_slice(&slave_address.to_le_bytes());
    payload.extend_from_slice(&read_words.to_le_bytes());
    for word in write_words {
        payload.extend_from_slice(&word.to_le_bytes());
    }
    let data = link.exchange(transport, Opcode::I2cRead, &payload, timeout)?;
    if data.len() < read_words as usize * 2 {
        return Err(XtionError::BadConfig(format!(
            "I2C read reply too short: {} bytes (need {})",
            data.len(),
            read_words * 2
        )));
    }
    Ok(data[..read_words as usize * 2]
        .chunks_exact(2)
        .map(|w| u16::from_le_bytes([w[0], w[1]]))
        .collect())
}

/// Read one CMOS register through the I2C path. Port of
/// `XnHostProtocolGetCMOSRegisterI2C` (XnHostProtocol.cpp:1959-1995):
/// write the 16-bit register address, read 2 bytes back, and combine
/// them big-endian (`(buf[0] << 8) + buf[1]`).
///
/// `bus` / `slave_address` come from the fixed-params block
/// (`GetImageCmosI2CBus` etc.).
pub fn read_cmos_register<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    bus: u16,
    slave_address: u16,
    address: u16,
    timeout: Duration,
) -> Result<u16> {
    let words = read_i2c(link, transport, bus, slave_address, &[address], 2, timeout)?;
    // the C adds the two byte-words as ints and truncates to 16 bits
    Ok(words[0].wrapping_shl(8).wrapping_add(words[1]))
}

/// Human-readable flash file-type names (Include/PS1080.h:337-355 and
/// the updater's per-sensor calibration images).
pub fn flash_file_type_name(file_type: u16) -> &'static str {
    match file_type {
        0x00 => "FileTable",
        0x01 => "ScratchFile",
        0x02 => "BootSector",
        0x03 => "BootManager",
        0x04 => "CodeDownloader",
        0x05 => "Monitor",
        0x06 => "Application",
        0x07 => "FixedParams",
        0x08 => "Descriptors",
        0x09 => "DefaultParams",
        0x0A => "ImageCmos",
        0x0B => "DepthCmos",
        0x0C => "AlgorithmParams",
        0x0D => "ReferenceQVGA",
        0x0E => "ReferenceVGA",
        0x0F => "Maintenance",
        0x10 => "DebugParams",
        0x11 => "PrimeProcessor",
        0x12 => "GainControl",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::command::{ControlTransport, ProtocolVersion};
    use crate::protocol::FW_MAGIC_V26;

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
            data.push(0);
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
    fn firmware_log_renders_text_and_overflow_entries() {
        // two entries: a text entry ("AB", 2 words payload incl. header
        // words? entry_size counts the entry's words AFTER the 4-byte
        // header the C reads) and an overflow marker
        let mut data = Vec::new();
        // entry 1: entry_size = 2 words (payload 0 bytes -> hmm), use a
        // text entry with 1 word payload: entry_size words includes the
        // 4-byte header -> payload words = entry_size - 2... the C reads
        // the 8-byte header then advances entry_size*2 total.
        let mut entry = Vec::new();
        entry.extend_from_slice(&4u16.to_le_bytes()); // entry_size words
        entry.extend_from_slice(&LOG_TEXT_TYPE.to_le_bytes());
        entry.extend_from_slice(&1234u32.to_le_bytes());
        // payload: 4 words of UTF-16 "TEST"
        for b in b"TEST" {
            entry.extend_from_slice(&[*b, 0]);
        }
        data.extend_from_slice(&entry);
        // entry 2: overflow, entry_size = 2 words (4 payload bytes that
        // the overflow renderer ignores)
        let mut entry = Vec::new();
        entry.extend_from_slice(&2u16.to_le_bytes());
        entry.extend_from_slice(&LOG_OVERFLOW_TYPE.to_le_bytes());
        entry.extend_from_slice(&99u32.to_le_bytes());
        entry.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&entry);
        // empty reply ends the loop
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::GetLog, &data),
            ack_reply(1, Opcode::GetLog, &[]),
        ]);
        let log = read_firmware_log(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert!(log.contains("1234:\tTEST"), "{log}");
        assert!(log.contains("99:\tLog Overflow"), "{log}");
    }

    #[test]
    fn download_file_loops_with_word_offsets() {
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::DownloadFile, &[0xAA, 0xBB, 0xCC, 0xDD]),
            ack_reply(1, Opcode::DownloadFile, &[]),
        ]);
        let blob = download_file(&mut link, &mut fake, 0x0A, Duration::from_millis(50)).unwrap();
        assert_eq!(blob, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        // request 1: file type 0x0A, offset 0 words
        let req = &fake.sent[0];
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 0x0A);
        assert_eq!(u32::from_le_bytes([req[10], req[11], req[12], req[13]]), 0);
    }

    #[test]
    fn flash_map_parses_14_byte_entries() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0Au16.to_le_bytes()); // ImageCmos
        data.extend_from_slice(&0x28000u32.to_le_bytes());
        data.extend_from_slice(&1139u32.to_le_bytes()); // words
        data.push(5);
        data.push(8);
        data.extend_from_slice(&22u16.to_le_bytes());
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetFlashMap, &data)]);
        let map = get_flash_map(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].file_type, 0x0A);
        assert_eq!(map[0].offset_in_flash, 0x28000);
        assert_eq!(map[0].size_in_words, 1139);
        assert_eq!(map[0].version_major, 5);
        assert_eq!(map[0].version_build, 22);
        assert_eq!(flash_file_type_name(0x0A), "ImageCmos");
    }

    #[test]
    fn file_list_follows_the_id_chain() {
        // file id 3, then an empty reply ends the loop
        let mut data = Vec::new();
        data.extend_from_slice(&3u16.to_le_bytes()); // id
        data.extend_from_slice(&0x07u16.to_le_bytes()); // FixedParams
        data.extend_from_slice(&0x00050822u32.to_le_bytes());
        data.extend_from_slice(&0x28000u32.to_le_bytes());
        data.extend_from_slice(&1000u32.to_le_bytes());
        data.extend_from_slice(&0x1234u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        // XnFlashFile is 24 bytes: pad to it
        while data.len() % 24 != 0 {
            data.push(0);
        }
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::GetFileList, &data),
            ack_reply(1, Opcode::GetFileList, &[]),
        ]);
        let files = get_file_list(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, 3);
        assert_eq!(files[0].file_type, 0x07);
        // second request must start at id + 1
        let req = &fake.sent[1];
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 4);
    }

    #[test]
    fn read_flash_chunks_by_byte_offset() {
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::ReadFlash, &[0x11, 0x22, 0x33, 0x44]),
            ack_reply(1, Opcode::ReadFlash, &[0x55, 0x66]),
        ]);
        let blob = read_flash(&mut link, &mut fake, 0x28000, 6, Duration::from_millis(50)).unwrap();
        assert_eq!(blob, vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        // second request carries the byte offset advanced by the reply
        let req = &fake.sent[1];
        assert_eq!(
            u32::from_le_bytes([req[8], req[9], req[10], req[11]]),
            0x28004
        );
    }

    #[test]
    fn ahb_read_roundtrips_u32() {
        let (mut link, mut fake) = link_and(vec![ack_reply(
            0,
            Opcode::ReadAhb,
            &[0x78, 0x56, 0x34, 0x12],
        )]);
        let value = read_ahb(&mut link, &mut fake, 0x1234, Duration::from_millis(50)).unwrap();
        assert_eq!(value, 0x12345678);
        let req = &fake.sent[0];
        assert_eq!(
            u32::from_le_bytes([req[8], req[9], req[10], req[11]]),
            0x1234
        );
    }

    #[test]
    fn cpu_stats_parse_8_byte_entries() {
        let mut data = Vec::new();
        for (t, us) in [(100u32, 5u32), (7, 999_999)] {
            data.extend_from_slice(&t.to_le_bytes());
            data.extend_from_slice(&us.to_le_bytes());
        }
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetCpuStats, &data)]);
        let stats = get_cpu_stats(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[1].time_microseconds, 999_999);
    }

    #[test]
    fn bist_sends_mask_and_returns_failures() {
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::Bist, &[0, 0, 0, 0])]);
        let failures = run_bist(
            &mut link,
            &mut fake,
            bist::IMAGE_CMOS | bist::IR_CMOS,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(failures, 0);
        let req = &fake.sent[0];
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 0b0000_0011);
    }

    #[test]
    fn tec_fast_convergence_parses_signed_fields() {
        let mut data = Vec::new();
        data.extend_from_slice(&(-1500i16).to_le_bytes()); // 15.00 C
        data.extend_from_slice(&(-1495i16).to_le_bytes());
        data.extend_from_slice(&(-12i32).to_le_bytes());
        data.extend_from_slice(&34567i32.to_le_bytes());
        data.extend_from_slice(&(-8i32).to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes()); // precise
        data.extend_from_slice(&2u16.to_le_bytes()); // cool
        data.extend_from_slice(&55u16.to_le_bytes()); // duty
        data.extend_from_slice(&1u16.to_le_bytes()); // room
        let (mut link, mut fake) =
            link_and(vec![ack_reply(0, Opcode::GetFastConvergenceTec, &data)]);
        let tec =
            get_tec_fast_convergence_data(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(tec.set_point_temperature, -1500);
        assert_eq!(tec.measured_temperature, -1495);
        assert_eq!(tec.scan_mode, 2);
        assert_eq!(tec.tec_duty_cycle, 55);
        assert_eq!(tec.temperature_range, 1);
    }

    #[test]
    fn emitter_data_parses_twelve_fields() {
        let mut data = Vec::new();
        for i in 0..12u16 {
            data.extend_from_slice(&(0x100 * i).to_le_bytes());
        }
        let (mut link, mut fake) = link_and(vec![ack_reply(0, Opcode::GetEmitterData, &data)]);
        let emitter = get_emitter_data(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(emitter.state, 0);
        assert_eq!(emitter.is_apc_enabled, 0x700);
        assert_eq!(emitter.sub_clocking, 0xA00);
        assert_eq!(emitter.precision, 0xB00);
    }

    #[test]
    fn cmos_blanking_direct_returns_u32() {
        let (mut link, mut fake) =
            link_and(vec![ack_reply(0, Opcode::GetCmosBlanking, &[58, 0, 0, 0])]);
        let lines = get_cmos_blanking_lines(
            &mut link,
            &mut fake,
            crate::protocol::presets::CMOS_TYPE_IMAGE,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(lines, 58);
        let req = &fake.sent[0];
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 0);
    }

    #[test]
    fn i2c_read_builds_the_c_request_layout() {
        let (mut link, mut fake) = link_and(vec![ack_reply(
            0,
            Opcode::I2cRead,
            &[0x06, 0x40, 0x00, 0x00],
        )]);
        let words = read_i2c(
            &mut link,
            &mut fake,
            2,
            0xBA,
            &[0x00],
            2,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(words, vec![0x4006, 0x0000]);
        let req = &fake.sent[0];
        // bus, slave, read_size, write words
        assert_eq!(u16::from_le_bytes([req[8], req[9]]), 2);
        assert_eq!(u16::from_le_bytes([req[10], req[11]]), 0xBA);
        assert_eq!(u16::from_le_bytes([req[12], req[13]]), 2);
        assert_eq!(u16::from_le_bytes([req[14], req[15]]), 0x00);
    }

    #[test]
    fn cmos_register_combines_bytes_big_endian() {
        // each I2C byte travels as its own u16 word: bytes 0x12, 0x34
        // arrive as words [0x0012, 0x0034]
        let (mut link, mut fake) = link_and(vec![ack_reply(
            0,
            Opcode::I2cRead,
            &[0x12, 0x00, 0x34, 0x00],
        )]);
        let value = read_cmos_register(
            &mut link,
            &mut fake,
            2,
            0xBA,
            0x00,
            Duration::from_millis(50),
        )
        .unwrap();
        assert_eq!(value, 0x1234);
    }
}
