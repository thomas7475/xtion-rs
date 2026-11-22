//! xEye3Ddata record parsing.
//!
//! Byte-level layout decoded from `xEyeClass.dll` `GetADataFromUSB`
//! (RVA 0x20f0; `analysis/disasm_GetADataFromUSB.txt`). The argument is the
//! in-memory vendor struct; all offsets below are read directly by the
//! disassembled code:
//!
//! ```text
//! +0x000  u32   frame counter (read at [arg+0]; continuity: delta ≤ 1)
//! +0x000  10 ×  slot, 88 bytes each (0x58), stride-addressed directly:
//!    slot+0x18  f64  gate value (compared against a constant — timestamp
//!                     or distance gate; open item #4)
//!    slot+0x24  i32  quality/age (≥ 50 passes the vendor filter)
//! +0x370  72-byte trailer (aggregates — noise/disturbance/timestamps;
//!         fields not yet attributed)
//! +0x3B8  u32   mode flag (== 1 → frame skipped by the pipe writer)
//! ```
//!
//! The 88-byte slots certainly contain the marker 3D position; exact
//! x/y/z offsets are **open item #4** (resolution: disassembly of the
//! record transform at RVA 0x25e0 + `iDongTTCamera.dll`). The parser
//! exposes every slot with the fields we can already read and keeps the
//! raw bytes for offline analysis.

use crate::markers::{Marker, MarkerFrame, Quality};

pub const SLOT_LEN: usize = 0x58;
pub const SLOTS: usize = 10;
/// Slots occupy 0x000..0x370 (10 × 88 = 880).
pub const SLOTS_END: usize = SLOTS * SLOT_LEN; // 0x370
/// 72-byte trailer between the slots and the mode flag.
pub const TRAILER_LEN: usize = 0x48;
pub const MODE_OFFSET: usize = SLOTS_END + TRAILER_LEN; // 0x3B8
/// Minimum record size (mode flag is a u32).
pub const RECORD_MIN: usize = MODE_OFFSET + 4; // 0x3BC

/// One raw 88-byte slot with the fields we can already decode.
#[derive(Debug, Clone)]
pub struct RawSlot {
    pub bytes: [u8; SLOT_LEN],
    /// slot+0x18
    pub gate: f64,
    /// slot+0x24
    pub quality: i32,
}

impl RawSlot {
    fn parse(bytes: &[u8]) -> Option<RawSlot> {
        if bytes.len() < SLOT_LEN {
            return None;
        }
        let mut b = [0u8; SLOT_LEN];
        b.copy_from_slice(&bytes[..SLOT_LEN]);
        let mut g = [0u8; 8];
        g.copy_from_slice(&bytes[0x18..0x20]);
        let mut q = [0u8; 4];
        q.copy_from_slice(&bytes[0x24..0x28]);
        Some(RawSlot {
            bytes: b,
            gate: f64::from_le_bytes(g),
            quality: i32::from_le_bytes(q),
        })
    }
}

/// Parse a device→host marker record payload.
///
/// `prev_counter` enables the vendor's drop check: a counter delta > 1
/// marks the frame as dropped (the "丢帧" path in GetADataFromUSB).
pub fn parse_record(payload: &[u8], prev_counter: Option<u32>) -> Option<MarkerFrame> {
    if payload.len() < RECORD_MIN {
        return None;
    }
    let frame_counter = u32::from_le_bytes([
        payload[0],
        payload[1],
        payload[2],
        payload[3],
    ]);
    let mode = u32::from_le_bytes([
        payload[MODE_OFFSET],
        payload[MODE_OFFSET + 1],
        payload[MODE_OFFSET + 2],
        payload[MODE_OFFSET + 3],
    ]);

    let mut markers = Vec::with_capacity(SLOTS);
    for (i, slot) in payload[..SLOTS_END]
        .chunks(SLOT_LEN)
        .take(SLOTS)
        .enumerate()
    {
        let raw = RawSlot::parse(slot)?;
        if raw.quality >= crate::protocol::MARKER_QUALITY_MIN {
            // Open item #4: x/y/z offsets inside the slot. Until pinned,
            // valid slots surface as tracked markers with placeholder
            // coordinates read from the first three f32-sized words
            // (slot+0x00..0x0C) — the region the transform feeds from.
            let x = f32::from_le_bytes(slot[0x00..0x04].try_into().ok()?);
            let y = f32::from_le_bytes(slot[0x04..0x08].try_into().ok()?);
            let z = f32::from_le_bytes(slot[0x08..0x0C].try_into().ok()?);
            markers.push(Marker {
                slot: i as u8,
                x_mm: x,
                y_mm: y,
                z_mm: z,
                quality: raw.quality,
                gate: raw.gate,
            });
        }
    }

    let dropped = match prev_counter {
        Some(p) => frame_counter.wrapping_sub(p) > 1,
        None => false,
    };

    Some(MarkerFrame {
        frame_counter,
        mode,
        markers,
        link_quality: Quality::from_count(markers.len()),
        dropped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with_slots(counter: u32, qualities: &[i32]) -> Vec<u8> {
        let mut v = vec![0u8; RECORD_MIN];
        v[0..4].copy_from_slice(&counter.to_le_bytes()); // overlays slot 0
        v[MODE_OFFSET..MODE_OFFSET + 4].copy_from_slice(&0u32.to_le_bytes());
        for (i, &q) in qualities.iter().enumerate().take(SLOTS) {
            let off = i * SLOT_LEN + 0x24;
            v[off..off + 4].copy_from_slice(&q.to_le_bytes());
            let gate = i * SLOT_LEN + 0x18;
            v[gate..gate + 8].copy_from_slice(&(i as f64).to_le_bytes());
        }
        v
    }

    #[test]
    fn sizes_match_disassembly() {
        assert_eq!(SLOT_LEN, 0x58);
        assert_eq!(SLOTS_END, 0x370);
        assert_eq!(TRAILER_LEN, 0x48);
        assert_eq!(MODE_OFFSET, 0x3B8);
        assert_eq!(RECORD_MIN, 0x3BC);
        // mode sits exactly 72 bytes past the last slot — the layout the
        // GetADataFromUSB addresses imply (slots at +0, no header).
        assert_eq!(MODE_OFFSET, SLOTS * SLOT_LEN + TRAILER_LEN);
    }

    #[test]
    fn quality_filter_and_drop_detection() {
        let r1 = record_with_slots(100, &[80, 80, 10, 80]);
        let f1 = parse_record(&r1, None).unwrap();
        assert_eq!(f1.frame_counter, 100);
        assert_eq!(f1.markers.len(), 3); // the 10-quality slot is filtered

        let r2 = record_with_slots(103, &[80, 80, 80, 80]);
        let f2 = parse_record(&r2, Some(100)).unwrap();
        assert!(f2.dropped, "counter jumped 100 -> 103");
    }
}
