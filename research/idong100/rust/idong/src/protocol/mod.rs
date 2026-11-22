//! Protocol constants decoded from the vendor stack.
//!
//! Sources (branch `research/idong100-09-xeyez-analysis`):
//! - `xeyez.inf` — PID family and interface layout
//! - `EasyUSB.dll` disassembly — endpoints, chunking, retries
//! - `xEyeUSB.dll` disassembly — magics, frame types, boot sequence,
//!   timeout constants (`xeyez_boot_protocol.txt`, `xeyez_tx_builder.txt`)
//! - `xEyeClass.dll` `GetADataFromUSB` — record layout (records.rs)

pub mod boot;
pub mod frames;
pub mod records;
pub mod runtime;

/// USB vendor ID (Philips/NXP range — the BF527 boot ROM's default).
pub const VID: u16 = 0x0471;

/// Bootloader PID — the sensor as first plugged in ("TOLTech xeyez
/// Bootloader" in xeyez.inf). This is the state the owner's unit sits in.
pub const PID_BOOTLOADER: u16 = 0x2600;

/// Runtime PID — composite device; the vendor bulk interface is MI_04
/// ("TOLTech xeyez v2.5" in xeyez.inf).
pub const PID_RUNTIME: u16 = 0x2500;

/// Older generations, accepted for identification only.
pub const PID_V21: u16 = 0x2100;
pub const PID_V20: u16 = 0x0668;
pub const PID_V10: u16 = 0x0666;

/// Vendor interface number on the runtime composite device.
pub const IFACE_VENDOR: u8 = 4;

/// Bulk OUT endpoint — commands and firmware upload.
pub const EP_OUT: u8 = 0x02;
/// Bulk IN endpoint — device data (boot replies, marker records).
pub const EP_IN: u8 = 0x82;

/// Bootloader bulk maxpacket (device declares 64 even at high speed —
/// spec violation, logged and accepted; see CONTEXT.md).
pub const BOOT_MAXPACKET: usize = 64;

/// Runtime read chunk: `xeyez.sys` `MaximumTransferSize=4096`;
/// EasyUSB accumulates up to 16 KiB per read burst.
pub const RUNTIME_CHUNK: usize = 4096;
pub const RUNTIME_ACCUM: usize = 16 * 1024;

/// EasyUSB retry policy: 3 attempts, 100 ms apart.
pub const IO_RETRIES: u32 = 3;
pub const IO_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

// ---------------------------------------------------------------------------
// Frame magics (little-endian u32 at frame offset +0).
// ---------------------------------------------------------------------------

/// Host→device magic (LE bytes `50 55 7f 7f` = "PU\x7f\x7f").
pub const MAGIC_H2D: u32 = 0x7f7f_5550;
/// Device→host magic (LE bytes `4f 44 7f 7f` = "OD\x7f\x7f").
pub const MAGIC_D2H: u32 = 0x7f7f_444f;

// ---------------------------------------------------------------------------
// Timeout constants observed in the boot disassembly (GetTickCount deltas).
// ---------------------------------------------------------------------------

/// "Ready to load program!" / CRC-result / device-ID outer timeout.
pub const TIMEOUT_READY: std::time::Duration = std::time::Duration::from_millis(3000);
/// Per firmware-packet timeout.
pub const TIMEOUT_PACKET: std::time::Duration = std::time::Duration::from_millis(6000);
/// DSP-reset ACK and 0xFFF keep-alive timeout.
pub const TIMEOUT_ACK: std::time::Duration = std::time::Duration::from_millis(1000);
/// Device-ID re-poll interval (≤ 10 attempts).
pub const TIMEOUT_ID_POLL: std::time::Duration = std::time::Duration::from_millis(300);
/// Device-ID poll attempts.
pub const ID_POLL_ATTEMPTS: u32 = 10;

// ---------------------------------------------------------------------------
// Frame types (u16 at frame offset +4).
// ---------------------------------------------------------------------------

/// Boot: device ready for program load ("Ready to load program!").
pub const TYPE_READY_TO_LOAD: u16 = 2;
/// Boot: packet-count + CRC announce (TX) / CRC result (RX).
pub const TYPE_COUNT_CRC: u16 = 2;
/// Boot: firmware data packet (TX, 64 B) / CRC verdict (RX).
pub const TYPE_FIRMWARE_PACKET: u16 = 3;
/// Runtime: link status; host replies with word 0x11 (see runtime.rs).
pub const TYPE_LINK_STATUS: u16 = 4;
/// Runtime: DSP reset.
pub const TYPE_DSP_RESET: u16 = 7;
/// Runtime keep-alive pair: device sends 0xFFF, host answers 0xFFE.
pub const TYPE_KEEPALIVE_RX: u16 = 0x0FFF;
pub const TYPE_KEEPALIVE_TX: u16 = 0x0FFE;
/// Link-status reply word (frame payload at +0x06, tx_builder @0x10010291).
pub const LINK_STATUS_REPLY_WORD: u16 = 0x0011;
/// CRC-failure status word at RX frame offset +8 (boot_protocol @0x1000fdee).
pub const CRC_FAIL_WORD: u16 = 0x000F;

// ---------------------------------------------------------------------------
// Streaming / record parameters (xEye.ini, xEyeUsb.ini, chip spec).
// ---------------------------------------------------------------------------

/// Marker output frame rate (xEye.ini `xEyeFrame=50`).
pub const TRACK_FPS: u32 = 50;
/// USB polling rate (xEyeUsb.ini `USBComFPS=35`).
pub const USB_POLL_FPS: u32 = 35;
/// Marker slots per record observed in xEyeClass (loop bound 0xA).
pub const MARKER_SLOTS: usize = 10;
/// Chip-documented target count (idong100: "up to 15 targets").
pub const MARKER_SLOTS_CHIP: usize = 15;
/// Minimum slot quality to pass the vendor filter (GetADataFromUSB).
pub const MARKER_QUALITY_MIN: i32 = 50;
/// Link-quality thresholds (xEyeUsb.ini Green/Yellow/Red).
pub const QUALITY_GREEN: i32 = 20;
pub const QUALITY_YELLOW: i32 = 35;
pub const QUALITY_RED: i32 = 35;
/// Vendor noise-filter ring (xEyeUsb.ini `RingSize=200`).
pub const QUALITY_RING: usize = 200;
