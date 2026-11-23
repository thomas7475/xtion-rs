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

/// One raw firmware-log entry, decoded from its 8-byte header.
///
/// The header layout is the C's packed `XnLogEntryHeader`
/// (XnHostProtocol.cpp:1196-1211) — **note the field order**:
/// `{ u16 entry_size, u32 timestamp, u16 log_type }`. (The timestamp sits
/// at offset 2, BEFORE the log type; swapping the two is exactly the
/// misparse that made hardware timestamps look like Unix epoch values
/// in 0.2.8: values such as 0x5F00001F were actually
/// `[timestamp_hi16][log_type]`.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareLogEntry {
    /// Entry payload size, in 16-bit words.
    pub entry_size: u16,
    /// Firmware-side timestamp of the entry (a small boot/uptime tick
    /// count; the firmware also embeds it in runtime log texts as the
    /// `%lu` prefix, see `docs/FIRMWARE.md`).
    pub timestamp: u32,
    /// 0x1200 text, 0x1201 overflow, anything else = a structured entry
    /// whose high byte is the module id and low byte the error code.
    pub log_type: u16,
    /// Raw payload: `entry_size * 2` bytes (clamped to what the device
    /// actually sent).
    pub payload: Vec<u8>,
}

/// Read the firmware log (OPCODE_GET_LOG) until the device reports no
/// more entries, decoding every entry's raw fields. Port of the read
/// loop in `XnHostProtocolGetLog` (XnHostProtocol.cpp:1213-1256);
/// [`render_firmware_log`] then turns the entries into text.
pub fn read_firmware_log_entries<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<Vec<FirmwareLogEntry>> {
    let mut raw: Vec<u8> = Vec::new();
    for _ in 0..64 {
        let data = link.exchange(transport, Opcode::GetLog, &[], timeout)?;
        if data.is_empty() {
            break;
        }
        raw.extend_from_slice(&data);
    }

    let mut entries = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= raw.len() {
        // XnLogEntryHeader, packed (XnHostProtocol.cpp:1196-1211):
        // { u16 nEntrySize; u32 nTimeStamp; u16 nLogType }
        let entry_size = u16::from_le_bytes([raw[pos], raw[pos + 1]]) as usize; // words
        let timestamp =
            u32::from_le_bytes([raw[pos + 2], raw[pos + 3], raw[pos + 4], raw[pos + 5]]);
        let log_type = u16::from_le_bytes([raw[pos + 6], raw[pos + 7]]);
        pos += 8;
        let payload_len = (entry_size * 2).min(raw.len().saturating_sub(pos));
        let payload = raw[pos..pos + payload_len].to_vec();
        pos += payload_len;
        entries.push(FirmwareLogEntry {
            entry_size: entry_size as u16,
            timestamp,
            log_type,
            payload,
        });
    }
    Ok(entries)
}

/// Render decoded log entries as text, mirroring the C's three formats
/// (XnHostProtocol.cpp:1258-1332):
/// - text entries (0x1200): UTF-16 payload, low byte of each word kept;
/// - overflow markers (0x1201);
/// - anything else: a structured entry whose payload is the packed
///   `XnLogDefaultData { u16 line; u32 param }` and whose log type is
///   `[module << 8 | error]`.
pub fn render_firmware_log(entries: &[FirmwareLogEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        let payload = &entry.payload;
        match entry.log_type {
            LOG_TEXT_TYPE => {
                let text: Vec<u8> = payload.chunks_exact(2).map(|w| w[0]).collect();
                out.push_str(&format!(
                    "{}:\t{}\n",
                    entry.timestamp,
                    String::from_utf8_lossy(&text).trim_end()
                ));
            }
            LOG_OVERFLOW_TYPE => {
                out.push_str(&format!("{}:\tLog Overflow\n", entry.timestamp));
            }
            _ => {
                // XnLogDefaultData: { u16 nLine; u32 nParam } — the param
                // is 32-bit (0.2.8 wrongly read its low half only).
                let line = payload
                    .chunks_exact(2)
                    .next()
                    .map(|w| u16::from_le_bytes([w[0], w[1]]))
                    .unwrap_or(0);
                let param = payload
                    .get(2..6)
                    .map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
                    .unwrap_or(0);
                out.push_str(&format!(
                    "{}:\tModule: [0x{:X}], Error: [0x{:X}], Param: 0x{:X}, (Line: {})\n",
                    entry.timestamp,
                    entry.log_type >> 8,
                    entry.log_type & 0xff,
                    param,
                    line
                ));
            }
        }
    }
    out
}

/// Read the firmware log and render it as text (the convenience form of
/// [`read_firmware_log_entries`] + [`render_firmware_log`]).
pub fn read_firmware_log<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<String> {
    Ok(render_firmware_log(&read_firmware_log_entries(
        link, transport, timeout,
    )?))
}

/// Read a flash file by type (OPCODE_DOWNLOAD_FILE), chunk by chunk,
/// until the device returns nothing. Port of
/// `XnHostProtocolFileDownloadChunk` + the download loop
/// (XnHostProtocol.cpp:2511-2602). The request offset is in WORDS (the
/// C divides the byte offset by `sizeof(XnUInt16)`).
///
/// The reply is the file's `.fla` image: a 6-byte
/// `[type u16][build u16][version u16]` header followed by `size` words
/// of payload, so the payload is `&blob[6..]` and its integrity check is
/// [`flash_file_crc`] (verified on-camera for type 0x0A, ImageCmos:
/// 2278 B = 6 + 2 * 1136 words).
///
/// **fw 5.8.22 caveat (hardware-verified):** the firmware's type lookup
/// walks the packed 22-byte on-flash file table with its unpacked
/// 24-byte struct — the same packed/unpacked mismatch that corrupts
/// [`get_file_list`] — so it can serve the WRONG file: a type-0x12
/// request (GainControl, 57 words) returned the 548-byte
/// RegistrationParams image, while type 0x0A returned the correct
/// ImageCmos image. Prefer [`read_file_table`] + [`read_flash`] at the
/// record's offset when the exact bytes matter.
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
/// Source: XnHostProtocol.cpp:2313-2326. `offset_in_flash` is a
/// 16-bit-word address like every flash offset on this sensor (byte
/// address = value * 2).
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

/// One flash file-system entry (`XnFlashFile`, **22 bytes packed**,
/// `#pragma pack(1)`). Source: Include/PS1080.h:600-609.
///
/// Field notes (hardware-verified on fw 5.8.22):
/// - [`offset`](Self::offset) is a **16-bit-word address** — like every
///   flash address on this sensor: READ_FLASH and DOWNLOAD_FILE wire
///   offsets, the file-table records and the updater's
///   `UsbUpdate_RD108.ini` all use words (byte address =
///   [`offset_bytes`](Self::offset_bytes)).
/// - [`size`](Self::size) is in **16-bit words**, like the vendor
///   console's "SizeInWords" column (PS1080Console.cpp:503). Use
///   [`size_bytes`](Self::size_bytes) for bytes.
/// - [`crc`](Self::crc) is [`flash_file_crc`](fn@flash_file_crc) of the
///   payload words (identified against the 5.8.22 factory image).
/// - [`attributes`](Self::attributes) is a bit mask;
///   [`FILE_ATTRIBUTE_READ_ONLY`] (0x8000) is the only bit the
///   vendor tools render.
///
/// The **packed** struct is 22 bytes; the natural-alignment (unpacked)
/// host layout is 24. Firmware 5.8.22 walks the on-flash table with the
/// unpacked 24-byte stride, which is why its GET_FILE_LIST replies and
/// DOWNLOAD_FILE lookups are shifted — parse the raw table with
/// [`parse_flash_file_table`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashFile {
    pub id: u16,
    pub file_type: u16,
    pub version: u32,
    pub offset: u32,
    /// File size in **16-bit words** (not bytes).
    pub size: u32,
    pub crc: u16,
    pub attributes: u16,
    pub reserved: u16,
}

impl FlashFile {
    /// File size in bytes (`size * 2`).
    pub fn size_bytes(&self) -> u64 {
        self.size as u64 * 2
    }

    /// Byte offset in flash (`offset * 2` — the record stores a
    /// 16-bit-word address).
    pub fn offset_bytes(&self) -> u64 {
        self.offset as u64 * 2
    }

    /// True when the read-only attribute bit is set.
    pub fn is_read_only(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_READ_ONLY != 0
    }
}

/// `XnFileAttributeReadOnly` — the read-only attribute bit of
/// [`FlashFile::attributes`] (rendered as "[R]" by the vendor console,
/// PS1080Console.cpp:511-515).
pub const FILE_ATTRIBUTE_READ_ONLY: u16 = 0x8000;

/// File type of the on-flash file table itself (the `FileTable`).
pub const FILE_TYPE_FILE_TABLE: u16 = 0x00;

/// **Word** offset of the on-flash file table: record 0 of the table
/// describes itself (offset 0x1000 words = byte 0x2000, ending exactly
/// where the BootManager at word 0x2000 begins).
pub const FILE_TABLE_OFFSET_WORDS: u32 = 0x1000;
/// Size of the on-flash file table in **words** (record 0's own size
/// field): 0x1000 words = 8192 bytes.
pub const FILE_TABLE_SIZE_WORDS: u32 = 0x1000;
/// Stride of the packed on-flash `XnFlashFile` records
/// ([`parse_flash_file_table`]).
pub const FLASH_FILE_RECORD_SIZE: usize = 22;

/// List the files in the device flash file system (OPCODE_GET_FILE_LIST),
/// starting at file id 0 and following the id chain until the device
/// returns nothing. Port of `XnHostProtocolGetFileList`
/// (XnHostProtocol.cpp:2458-2510).
///
/// **fw 5.8.22 caveat (hardware-verified):** the firmware reads the
/// packed 22-byte on-flash records ([`parse_flash_file_table`]) with
/// its unpacked 24-byte struct layout, so every reply row past the
/// first drifts +2 bytes per record — only the rows where both grids
/// resynchronise (every 11th, lcm(22, 24) = 264) are true records.
/// This function reproduces the vendor C driver's behaviour byte for
/// byte, corruption included. For a correct enumeration use
/// [`read_file_table`].
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
/// (XnHostProtocol.cpp:2603-2678).
///
/// **All flash addresses on this sensor are 16-bit words**: the wire
/// offset here, the chunk size (the vendor console multiplies the word
/// count by `sizeof(XnUInt16)` for its buffer, PS1080Console.cpp:1164),
/// the file-table records ([`FlashFile::offset`]) and the updater ini
/// all agree on words. Hence `offset_words` (byte address = value * 2)
/// and a `size_bytes` convenience length — an odd trailing byte cannot
/// be requested and is dropped.
///
/// 0.3.2 bug fix: the chunk loop advanced the wire offset by BYTES
/// instead of words, silently skipping every second 1 KiB block on
/// reads larger than 1024 B (the 0.3.1 "4096-byte" table dump was four
/// gapped blocks; all records happened to live in the first one).
pub fn read_flash<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    offset_words: u32,
    size_bytes: u32,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let total_bytes = size_bytes & !1; // whole words only
    let mut blob = Vec::with_capacity(total_bytes as usize);
    let mut offset = offset_words;
    while (blob.len() as u32) < total_bytes {
        let remaining_words = (total_bytes - blob.len() as u32) / 2;
        let chunk_words: u16 = remaining_words.min(0x200) as u16;
        let mut payload = Vec::with_capacity(6);
        payload.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(&chunk_words.to_le_bytes());
        let data = link.exchange(transport, Opcode::ReadFlash, &payload, timeout)?;
        if data.is_empty() {
            return Err(XtionError::BadConfig(format!(
                "flash read at word {offset:#x} returned no data"
            )));
        }
        blob.extend_from_slice(&data);
        // advance by the words actually received (0.3.2 fix: was bytes)
        offset += (data.len() / 2) as u32;
    }
    blob.truncate(total_bytes as usize);
    Ok(blob)
}

/// Parse the raw on-flash file table into [`FlashFile`] records.
///
/// The table (read with [`read_flash`] at
/// [`FILE_TABLE_OFFSET_WORDS`]) is a flat array of packed 22-byte
/// `XnFlashFile` records ([`FLASH_FILE_RECORD_SIZE`]) starting at byte
/// 0 of the region. Record 0 describes the table itself and carries the
/// magic `0x12345678` in [`FlashFile::version`]; the remaining ids
/// follow. Parsing stops at the first slot that is erased (all `0xFF`),
/// zeroed, or fails the plausibility bounds (id <= 0x0FFF, type <= 0xFF,
/// offset <= 4 MiB in words, size <= 1 Mi words) — the records end well
/// before the 8 KiB region does.
///
/// This is the ground truth on fw 5.8.22; the firmware's own
/// GET_FILE_LIST walk is shifted (see [`get_file_list`]).
pub fn parse_flash_file_table(table: &[u8]) -> Vec<FlashFile> {
    let mut files = Vec::new();
    let mut pos = 0usize;
    while pos + FLASH_FILE_RECORD_SIZE <= table.len() {
        let r = &table[pos..pos + FLASH_FILE_RECORD_SIZE];
        if r.iter().all(|&b| b == 0x00) || r.iter().all(|&b| b == 0xFF) {
            break;
        }
        let u16_at = |i: usize| u16::from_le_bytes([r[i], r[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([r[i], r[i + 1], r[i + 2], r[i + 3]]);
        let rec = FlashFile {
            id: u16_at(0),
            file_type: u16_at(2),
            version: u32_at(4),
            offset: u32_at(8),
            size: u32_at(12),
            crc: u16_at(16),
            attributes: u16_at(18),
            reserved: u16_at(20),
        };
        let plausible = rec.id <= 0x0FFF
            && rec.file_type <= 0x00FF
            && rec.offset <= 0x20_0000
            && rec.size <= 0x10_0000;
        if !plausible {
            break;
        }
        files.push(rec);
        pos += FLASH_FILE_RECORD_SIZE;
    }
    files
}

/// Read and parse the on-flash file table — the ground-truth file list.
///
/// Reads [`FILE_TABLE_SIZE_WORDS`] words at [`FILE_TABLE_OFFSET_WORDS`]
/// with [`read_flash`] and parses them with [`parse_flash_file_table`].
/// On fw 5.8.22 this is the reliable enumeration; the firmware's own
/// GET_FILE_LIST serialization drifts (see [`get_file_list`]).
pub fn read_file_table<T: ControlTransport>(
    link: &mut CommandLink,
    transport: &mut T,
    timeout: Duration,
) -> Result<Vec<FlashFile>> {
    let bytes = read_flash(
        link,
        transport,
        FILE_TABLE_OFFSET_WORDS,
        FILE_TABLE_SIZE_WORDS * 2,
        timeout,
    )?;
    Ok(parse_flash_file_table(&bytes))
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
    /// The TEC test — the module whose run makes the firmware emit the
    /// TEC debug channel (0xd200) on the misc endpoint.
    pub const TEC: u16 = 1 << 6;
    /// The NESA test (manual, bounded) — makes the firmware emit the
    /// NESA debug channel (0xd201).
    pub const NESA: u16 = 1 << 7;
    /// The NESA test in "unlimited" mode — the variant the official
    /// updater runs as its `RunBISTNESAUnlimited` finish function.
    /// It does NOT terminate on its own: do not select it through
    /// [`crate::Xtion::run_bist`] unless you intend to power-cycle.
    pub const NESA_UNLIMITED: u16 = 1 << 8;
    /// Mask of all the AUTO tests (`XN_BIST_ALL`) — every bit except
    /// the two manual NESA ones.
    pub const ALL: u16 = 0x7f;

    /// One failed BIST module, decoded from the reply's failure
    /// bitmask (`XnBistError`, Include/PS1080.h:391-414).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BistFailure {
        /// The bit position in the reply word.
        pub bit: u32,
        /// The reference's name for the failure
        /// (`XN_BIST_*_FAILURE`, `XN_TEC_TEST_*`).
        pub name: &'static str,
    }

    /// Decode every set bit of a BIST failure word (0 = all green) into
    /// its named failure. Unknown bits (newer firmware) keep their bit
    /// number with an `unknown` name.
    pub fn failure_bits(failures: u32) -> Vec<BistFailure> {
        const NAMES: [&str; 24] = [
            "RAM_TEST",
            "IR_CMOS_CONTROL_BUS",
            "IR_CMOS_DATA_BUS",
            "IR_CMOS_BAD_VERSION",
            "IR_CMOS_RESET",
            "IR_CMOS_TRIGGER",
            "IR_CMOS_STROBE",
            "COLOR_CMOS_CONTROL_BUS",
            "COLOR_CMOS_DATA_BUS",
            "COLOR_CMOS_BAD_VERSION",
            "COLOR_CMOS_RESET",
            "FLASH_WRITE_LINE",
            "FLASH_TEST",
            "POTENTIOMETER_CONTROL_BUS",
            "POTENTIOMETER",
            "AUDIO_TEST",
            "PROJECTOR_LD",
            "PROJECTOR_LD_FAILSAFE_TRIG",
            "PROJECTOR_FAILSAFE_HIGH",
            "PROJECTOR_FAILSAFE_LOW",
            "TEC_HEATER_CROSSED",
            "TEC_HEATER_DISCONNECTED",
            "TEC_CROSSED",
            "TEC_FAULT",
        ];
        let mut out = Vec::new();
        for (bit, name) in NAMES.iter().enumerate() {
            if failures & (1 << bit) != 0 {
                out.push(BistFailure {
                    bit: bit as u32,
                    name,
                });
            }
        }
        // bits beyond the named 24 (the reply is a u32)
        for bit in NAMES.len()..32 {
            if failures & (1 << bit) != 0 {
                out.push(BistFailure {
                    bit: bit as u32,
                    name: "unknown",
                });
            }
        }
        out
    }
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

/// Human-readable flash file-type names — the complete
/// `XnFlashFileType` enum (Include/PS1080.h:335-368). The 0.3.2 camera
/// run revealed the per-device files 0x13-0x1C live on this unit
/// (ids 14-21 of the on-flash table), so every enum value now has a
/// name.
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
        // "RegistartionParams" in the vendor header (sic)
        0x13 => "RegistrationParams",
        0x14 => "IDParams",
        0x15 => "SensorTECParams",
        0x16 => "SensorAPCParams",
        0x17 => "SensorProjectorFaultParams",
        0x18 => "ProductionFile",
        0x19 => "UpgradeInProgress",
        0x1A => "WavelengthCorrection",
        0x1B => "GMCReferenceOffset",
        0x1C => "SensorNESAParams",
        0x1D => "SensorFault",
        0x1E => "VendorData",
        _ => "Unknown",
    }
}

/// CRC-16/CCITT-FALSE over `data` (poly 0x1021, init 0xFFFF, MSB-first,
/// no reflection, no xorout). Catalog check value over `"123456789"`:
/// 0x29B1.
fn crc16_ccitt_false(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// The on-flash file CRC — the [`FlashFile::crc`] column.
///
/// Identified in the 0.3.2 session against the factory image bundled in
/// `firmware/5.8.22` (every file's on-flash payload equals its `.fla`
/// minus the 6-byte `[type][build][version]` header, so the payloads
/// are known offline): the stored value is **CRC-16/CCITT-FALSE over
/// the payload words, each 16-bit word fed most-significant byte
/// first** — i.e. CCITT-FALSE of the payload serialized as a
/// big-endian u16 array (the flash stores little-endian words).
/// Verified 8/8 against the table records decoded from the raw sector:
/// GainControl 0xC44B, FixedParams 0x233D, AlgorithmParams 0xA7F1,
/// DefaultParams 0xE4EC, PrimeProcessor 0x9A35, ImageCmos 0x1CFA,
/// BootManager 0x9F64, Application 0xDF96
/// (`scripts/flash_crc_sweep.py` in the workspace).
pub fn flash_file_crc(payload: &[u8]) -> u16 {
    // per-word big-endian feed == CCITT-FALSE over the pair-swapped bytes
    let mut swapped = Vec::with_capacity(payload.len());
    for word in payload.chunks(2) {
        match word {
            [lo, hi] => {
                swapped.push(*hi);
                swapped.push(*lo);
            }
            [b] => swapped.push(*b),
            _ => unreachable!("chunks(2) yields 1-2 bytes"),
        }
    }
    crc16_ccitt_false(&swapped)
}

#[cfg(test)]
mod crc_tests {
    use super::{crc16_ccitt_false, flash_file_crc, parse_flash_file_table, FlashFile};

    #[test]
    fn ccitt_false_engine_matches_catalog_check() {
        assert_eq!(crc16_ccitt_false(b"123456789"), 0x29B1);
        assert_eq!(crc16_ccitt_false(b""), 0xFFFF);
    }

    #[test]
    fn flash_file_crc_feeds_words_big_endian() {
        // one stored word 0x3412 (bytes 12 34) is fed as 34 12
        assert_eq!(
            flash_file_crc(&[0x12, 0x34]),
            crc16_ccitt_false(&[0x34, 0x12])
        );
        // empty payload keeps the init value
        assert_eq!(flash_file_crc(&[]), 0xFFFF);
    }

    #[test]
    fn file_table_parser_stops_at_erased_flash() {
        // record 0 (the FileTable self-entry, magic version) followed by
        // erased flash
        let mut table = Vec::new();
        table.extend_from_slice(&0u16.to_le_bytes()); // id
        table.extend_from_slice(&0u16.to_le_bytes()); // type FileTable
        table.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // magic
        table.extend_from_slice(&0x1000u32.to_le_bytes()); // offset (words)
        table.extend_from_slice(&0x1000u32.to_le_bytes()); // size (words)
        table.extend_from_slice(&0xFFFCu16.to_le_bytes()); // crc
        table.extend_from_slice(&0u16.to_le_bytes()); // attributes
        table.extend_from_slice(&0u16.to_le_bytes()); // reserved
        table.extend_from_slice(&[0xFF; 64]);
        let files = parse_flash_file_table(&table);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].version, 0x1234_5678);
        assert_eq!(files[0].crc, 0xFFFC);
    }

    #[test]
    fn file_table_parser_rejects_implausible_records() {
        // a record with a huge offset is a terminator, not a file
        let mut table = vec![0u8; 22];
        table[10] = 0xFF; // offset high byte
        assert!(parse_flash_file_table(&table).is_empty());
        // all-zero slots terminate as well
        assert!(parse_flash_file_table(&[0u8; 64]).is_empty());
    }

    #[test]
    fn file_table_parser_decodes_a_regular_record() {
        let mut table = Vec::new();
        // record 0: the FileTable self-entry
        table.extend_from_slice(&0u16.to_le_bytes()); // id
        table.extend_from_slice(&0u16.to_le_bytes()); // type FileTable
        table.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // magic
        table.extend_from_slice(&0x1000u32.to_le_bytes()); // offset
        table.extend_from_slice(&0x1000u32.to_le_bytes()); // size
        table.extend_from_slice(&0xFFFCu16.to_le_bytes()); // crc
        table.extend_from_slice(&0u16.to_le_bytes());
        table.extend_from_slice(&0u16.to_le_bytes());
        // record 1: GainControl-like entry
        table.extend_from_slice(&12u16.to_le_bytes()); // id
        table.extend_from_slice(&0x12u16.to_le_bytes()); // GainControl
        table.extend_from_slice(&0x0508_0022u32.to_le_bytes()); // 5.8, build 34
        table.extend_from_slice(&0x550u32.to_le_bytes()); // offset words
        table.extend_from_slice(&57u32.to_le_bytes()); // 57 words
        table.extend_from_slice(&0xC44Bu16.to_le_bytes()); // crc
        table.extend_from_slice(&0u16.to_le_bytes());
        table.extend_from_slice(&0u16.to_le_bytes());
        table.extend_from_slice(&[0xFF; 22]);
        let files = parse_flash_file_table(&table);
        assert_eq!(files.len(), 2);
        assert_eq!(
            files[1],
            FlashFile {
                id: 12,
                file_type: 0x12,
                version: 0x0508_0022,
                offset: 0x550,
                size: 57,
                crc: 0xC44B,
                attributes: 0,
                reserved: 0,
            }
        );
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
        // Two entries in the CORRECT packed XnLogEntryHeader layout
        // {u16 entry_size, u32 timestamp, u16 log_type} (the 0.2.8 parser
        // wrongly read {entry_size, log_type, timestamp} — that is what
        // turned hardware timestamps into 0x5F00001F-style values).
        let mut data = Vec::new();
        // entry 1: text "TEST", 4 payload words, timestamp 1234
        data.extend_from_slice(&4u16.to_le_bytes()); // entry_size words
        data.extend_from_slice(&1234u32.to_le_bytes()); // timestamp
        data.extend_from_slice(&LOG_TEXT_TYPE.to_le_bytes()); // log type
        for b in b"TEST" {
            data.extend_from_slice(&[*b, 0]); // UTF-16 payload
        }
        // entry 2: overflow marker, 2 payload words (ignored by renderer)
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&99u32.to_le_bytes());
        data.extend_from_slice(&LOG_OVERFLOW_TYPE.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        // empty reply ends the loop
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::GetLog, &data),
            ack_reply(1, Opcode::GetLog, &[]),
        ]);
        let entries =
            read_firmware_log_entries(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp, 1234);
        assert_eq!(entries[0].log_type, LOG_TEXT_TYPE);
        assert_eq!(entries[0].payload, b"T\0E\0S\0T\0".to_vec());
        assert_eq!(entries[1].log_type, LOG_OVERFLOW_TYPE);
        let log = render_firmware_log(&entries);
        assert!(log.contains("1234:\tTEST"), "{log}");
        assert!(log.contains("99:\tLog Overflow"), "{log}");
    }

    #[test]
    fn firmware_log_structured_entry_decodes_module_error_and_u32_param() {
        // A structured (non-text) entry: log type = module<<8 | error,
        // payload = packed XnLogDefaultData { u16 line; u32 param }.
        // This is the entry shape behind the 0.2.8 "timestamp" red
        // herrings 0x5F00001F / 0x5A02001F: reading the header in the
        // wrong order makes [timestamp_hi16 | log_type] look like the
        // timestamp.
        let mut data = Vec::new();
        data.extend_from_slice(&3u16.to_le_bytes()); // 3 payload words
        data.extend_from_slice(&0x001F1200u32.to_le_bytes()); // a boot tick
        data.extend_from_slice(&0x5F00u16.to_le_bytes()); // module 0x5F, error 0
                                                          // payload: line = 0x002A, param = 0x0000BEEF (u32!)
        data.extend_from_slice(&0x002Au16.to_le_bytes());
        data.extend_from_slice(&0xBEEFu32.to_le_bytes());
        data.extend_from_slice(&0x0000u16.to_le_bytes()); // padding word
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::GetLog, &data),
            ack_reply(1, Opcode::GetLog, &[]),
        ]);
        let entries =
            read_firmware_log_entries(&mut link, &mut fake, Duration::from_millis(50)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].timestamp, 0x001F1200);
        assert_eq!(entries[0].log_type, 0x5F00);
        let log = render_firmware_log(&entries);
        // the C's structured format: the u32 param prints in full
        assert!(
            log.contains("Module: [0x5F], Error: [0x0], Param: 0xBEEF, (Line: 42)"),
            "{log}"
        );
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

    /// The 0.3.2 camera run revealed the per-device file types 0x13-0x1C
    /// on real silicon (table ids 14-21); every enum value of
    /// XnFlashFileType (PS1080.h:335-368) must have its name.
    #[test]
    fn flash_type_names_cover_the_full_vendor_enum() {
        assert_eq!(flash_file_type_name(0x13), "RegistrationParams");
        assert_eq!(flash_file_type_name(0x14), "IDParams");
        assert_eq!(flash_file_type_name(0x15), "SensorTECParams");
        assert_eq!(flash_file_type_name(0x16), "SensorAPCParams");
        assert_eq!(flash_file_type_name(0x17), "SensorProjectorFaultParams");
        assert_eq!(flash_file_type_name(0x18), "ProductionFile");
        assert_eq!(flash_file_type_name(0x19), "UpgradeInProgress");
        assert_eq!(flash_file_type_name(0x1A), "WavelengthCorrection");
        assert_eq!(flash_file_type_name(0x1B), "GMCReferenceOffset");
        assert_eq!(flash_file_type_name(0x1C), "SensorNESAParams");
        assert_eq!(flash_file_type_name(0x1D), "SensorFault");
        assert_eq!(flash_file_type_name(0x1E), "VendorData");
        // beyond the vendor enum
        assert_eq!(flash_file_type_name(0x1F), "Unknown");
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
    fn read_flash_advances_the_wire_offset_in_words() {
        let (mut link, mut fake) = link_and(vec![
            ack_reply(0, Opcode::ReadFlash, &[0x11, 0x22, 0x33, 0x44]),
            ack_reply(1, Opcode::ReadFlash, &[0x55, 0x66]),
        ]);
        let blob = read_flash(&mut link, &mut fake, 0x28000, 6, Duration::from_millis(50)).unwrap();
        assert_eq!(blob, vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        // second request advances the WORD offset by the 2 words received
        // (0.3.2 fix: this used to advance by bytes — 0x28004 — skipping
        // every second 1 KiB block on large reads)
        let req = &fake.sent[1];
        assert_eq!(
            u32::from_le_bytes([req[8], req[9], req[10], req[11]]),
            0x28002
        );
        // and it asks for the remaining word count
        assert_eq!(u16::from_le_bytes([req[12], req[13]]), 1);
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

    /// The full request-mask set matches `XnBistType`
    /// (Include/PS1080.h:373-379) and `ALL` excludes the manual NESA
    /// bits exactly like `XN_BIST_ALL`.
    #[test]
    fn bist_masks_match_the_c_header() {
        assert_eq!(bist::IMAGE_CMOS, 1 << 0);
        assert_eq!(bist::IR_CMOS, 1 << 1);
        assert_eq!(bist::POTENTIOMETER, 1 << 2);
        assert_eq!(bist::FLASH, 1 << 3);
        assert_eq!(bist::FULL_FLASH, 1 << 4);
        assert_eq!(bist::PROJECTOR, 1 << 5);
        assert_eq!(bist::TEC, 1 << 6);
        assert_eq!(bist::NESA, 1 << 7);
        assert_eq!(bist::NESA_UNLIMITED, 1 << 8);
        assert_eq!(bist::ALL, 0x7f, "XN_BIST_ALL = every bit but the NESA ones");
    }

    /// Failure bits decode to their `XnBistError` names, including the
    /// TEC block (bits 20-23) and unknown high bits.
    #[test]
    fn bist_failure_bits_decode_names() {
        assert!(bist::failure_bits(0).is_empty());
        // FLASH_TEST (bit 12) + TEC_CROSSED (bit 22)
        let failures = bist::failure_bits((1 << 12) | (1 << 22));
        assert_eq!(
            failures,
            vec![
                bist::BistFailure {
                    bit: 12,
                    name: "FLASH_TEST"
                },
                bist::BistFailure {
                    bit: 22,
                    name: "TEC_CROSSED"
                },
            ]
        );
        // a bit past the named 24 keeps its number
        let unknown = bist::failure_bits(1 << 30);
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].bit, 30);
        assert_eq!(unknown[0].name, "unknown");
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
