//! Public marker data model — the driver's main data product.
//!
//! Semantics follow the vendor stack: a record per 50 fps tick, up to 10
//! tracked slots (chip documents 15 on idong100 / 20+ on idong200),
//! per-slot quality with a ≥ 50 tracking threshold, and a link-quality
//! classification driven by the disturbance counters (xEyeUsb.ini
//! thresholds Green=20 / Yellow=35 / Red=35).

use crate::protocol::{QUALITY_GREEN, QUALITY_RED, QUALITY_YELLOW};

/// One tracked reflective marker in the device coordinate frame.
///
/// Coordinate system: X right, Y up, Z along the optical axis away from
/// the sensor (right-handed, device frame; confirmed orientation pending
/// open item #4 — the values are the device's millimetre output).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marker {
    /// Slot index inside the record (0..=9 observed).
    pub slot: u8,
    /// Lateral position, millimetres.
    pub x_mm: f32,
    /// Vertical position, millimetres.
    pub y_mm: f32,
    /// Depth from the sensor bar, millimetres.
    pub z_mm: f32,
    /// Vendor quality/age value (slot+0x24); ≥ 50 = tracked.
    pub quality: i32,
    /// Gate value from slot+0x18 (timestamp or distance gate — item #4).
    pub gate: f64,
}

/// Link-quality classification (vendor traffic-light semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Green,
    Yellow,
    Red,
}

impl Quality {
    /// Classify from the vendor disturbance counters (placeholder mapping
    /// on tracked-marker count until the counter payload layout is pinned
    /// by open item #3).
    pub fn from_count(tracked: usize) -> Quality {
        match tracked {
            t if t as i32 >= QUALITY_YELLOW => Quality::Green,
            t if t as i32 >= QUALITY_GREEN => Quality::Yellow,
            _ => Quality::Red,
        }
    }

    pub fn from_disturbance(d: i32) -> Quality {
        if d >= QUALITY_RED {
            Quality::Red
        } else if d >= QUALITY_YELLOW {
            Quality::Yellow
        } else {
            Quality::Green
        }
    }
}

/// One decoded marker record (50 fps).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerFrame {
    /// Record +0x00: vendor frame counter (continuity-checked).
    pub frame_counter: u32,
    /// Record +0x3B8: vendor mode flag (1 = skipped by the pipe writer).
    pub mode: u32,
    /// Slots that passed the ≥ 50 quality filter.
    pub markers: Vec<Marker>,
    /// Traffic-light link quality for this frame.
    pub link_quality: Quality,
    /// True when the frame counter jumped (vendor drop check).
    pub dropped: bool,
}

impl MarkerFrame {
    /// Number of tracked markers.
    pub fn tracked(&self) -> usize {
        self.markers.len()
    }
}
