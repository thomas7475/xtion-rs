//! Device-to-host timestamp conversion for stream frames.
//!
//! Frame timestamps on the wire are raw 32-bit ticks of the firmware's
//! ~60 MHz clock — the value wraps every `2^32 / 60e6` = **71.6 s**.
//! Port of `XnDataProcessor::CreateTimestampFromDevice`
//! (Source/Drivers/PS1080/Sensor/XnDataProcessor.cpp:84-226), including
//! the first-frame wrap estimation and the 10 s sanity re-sync.
//!
//! The converter answers in **microseconds since the session anchor**
//! (the arrival of the first frame), exactly like the reference driver:
//!
//! ```text
//! anchor:      first frame -> anchor_device_ts + anchor host time
//! normal case: ticks = total_ticks + ts - reference_ts
//! wrap (ts <= last):  total_ticks += 2^32 + ts - reference_ts
//!                      reference_ts = ts
//! result:      microseconds = ticks / device_frequency
//! ```
//!
//! `device_frequency` is the ALGORITHM_PARAMS(0x80) value the reference
//! driver feeds the same math with (`fDeviceFrequency`): ticks per
//! microsecond (the ~60.0 of the PS1080 clock). The sanity check ports
//! `XN_SENSOR_TIMESTAMP_SANITY_DIFF` (10 ms) * 1000: a jump of more
//! than 10 s between consecutive results re-runs the first-frame
//! estimation.

use std::time::Instant;

/// `XN_SENSOR_TIMESTAMP_SANITY_DIFF` (10 ms, XnDeviceSensor.h) times the
/// ms-to-µs factor the C applies when comparing consecutive results
/// (XnDataProcessor.cpp:186).
const SANITY_JUMP_MS: u64 = 10 * 1000;
/// Re-sync attempts after a failed sanity check (defensive bound; the C
/// recurses once per failed check).
const RESYNC_ATTEMPTS: usize = 3;

/// Wrap-aware device-timestamp converter, one per device session.
///
/// Feeding the raw frame timestamps of [`crate::Xtion::read_depth_frame`]
/// (or any other stream's) through [`TimestampConverter::update`] yields
/// monotonic microseconds since the first frame — the exact value the
/// reference driver stamps into its frames.
pub struct TimestampConverter {
    /// Device clock rate in ticks per microsecond (the firmware reports
    /// ~60.0 for the PS1080's 60 MHz tick).
    frequency: f64,
    anchor_device: u32,
    anchor_host: Option<Instant>,
    /// Per-stream state of the C's `XnTimeStampData`:
    initialized: bool,
    reference_ts: u32,
    total_ticks: u64,
    last_device_ts: u32,
    last_result_ms: u64,
}

impl TimestampConverter {
    /// New converter for a device reporting `frequency` ticks per
    /// microsecond (the `fDeviceFrequency` of ALGORITHM_PARAMS 0x80;
    /// ~60.0 for the PS1080's 60 MHz tick).
    pub fn new(ticks_per_us: f64) -> TimestampConverter {
        TimestampConverter {
            frequency: ticks_per_us,
            anchor_device: 0,
            anchor_host: None,
            initialized: false,
            reference_ts: 0,
            total_ticks: 0,
            last_device_ts: 0,
            last_result_ms: 0,
        }
    }

    /// The clock rate the converter was built with.
    pub fn frequency(&self) -> f64 {
        self.frequency
    }

    /// Feed one raw device timestamp; returns microseconds since the
    /// session anchor (the first frame ever fed).
    ///
    /// Gaps longer than one wrap period (71.6 s at 60 MHz) between fed
    /// timestamps are resolved modulo one wrap, and a resulting forward
    /// jump of more than 10 s triggers the same re-estimation the
    /// reference driver performs (`bFirst = TRUE` + retry) — the
    /// re-synced frame restarts from zero, exactly like the C.
    pub fn update(&mut self, device_ts: u32) -> u64 {
        if self.anchor_host.is_none() {
            self.anchor_device = device_ts;
            self.anchor_host = Some(Instant::now());
        }
        for _ in 0..RESYNC_ATTEMPTS {
            let (result_us, sane) = self.update_once(device_ts);
            if sane {
                return result_us;
            }
            // sanity failed: restart the first-frame estimation
            // (the C sets bFirst = TRUE and recurses)
            self.initialized = false;
            self.last_result_ms = 0;
        }
        // bounded fall-through: return the last computed value anyway
        self.update_once(device_ts).0
    }

    /// One conversion pass. Returns `(microseconds, sane)`.
    ///
    /// Mirrors the C's control flow exactly: the first-frame block only
    /// sets up the reference state, and the normal/wrap branch right
    /// after it computes the result for the SAME frame (the C's
    /// `nResultInTicks = 0` inside the init block is always overwritten).
    fn update_once(&mut self, device_ts: u32) -> (u64, bool) {
        let wrap_point: u64 = 1 << 32;
        let mut check_sanity = true;
        let mut ts = device_ts;

        if !self.initialized {
            // First frame of this stream: estimate how many wraps passed
            // since the session anchor using host time, then clip the
            // estimate to +/- half a wrap (XnDataProcessor.cpp:110-163).
            let host_us = self
                .anchor_host
                .map(|a| a.elapsed().as_micros() as f64)
                .unwrap_or(0.0);
            let wrap_us = wrap_point as f64 / self.frequency;
            let mut wraps = (host_us / wrap_us) as i64;
            let estimated_ticks = wraps * wrap_point as i64 + ts as i64 - self.anchor_device as i64;
            let estimated_us = estimated_ticks as f64 / self.frequency;
            if estimated_us < host_us - 0.5 * wrap_us {
                wraps += 1;
            } else if estimated_us > host_us + 0.5 * wrap_us {
                wraps -= 1;
            }
            // cases 3/4 (timestamp before the anchor): clamp
            if wraps < 0 || (wraps == 0 && ts < self.anchor_device) {
                ts = self.anchor_device;
                wraps = 0;
            }
            self.reference_ts = self.anchor_device;
            self.total_ticks = (wraps.max(0) as u64) * wrap_point;
            self.last_device_ts = 0;
            self.initialized = true;
            check_sanity = false;
        }

        let result_ticks: u64 = if ts > self.last_device_ts {
            // normal case
            self.total_ticks + (ts as i64 - self.reference_ts as i64) as u64
        } else {
            // wrap-around
            self.total_ticks += wrap_point + ts as u64 - self.reference_ts as u64;
            self.reference_ts = ts;
            self.total_ticks
        };
        // the C stores the (possibly clamped) timestamp as the new
        // reference point for monotonicity
        self.last_device_ts = ts;

        // ticks -> microseconds (the C computes in double on purpose)
        let result_us = result_ticks as f64 / self.frequency;
        let result_ms = (result_us / 1000.0) as u64;
        if check_sanity && result_ms > self.last_result_ms + SANITY_JUMP_MS {
            return (result_us as u64, false);
        }
        if check_sanity {
            self.last_result_ms = result_ms;
        }
        (result_us as u64, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 60 ticks per microsecond — the PS1080 clock.
    const FREQ: f64 = 60.0;
    const US_PER_FRAME: u64 = 33_333; // 30 fps

    #[test]
    fn first_frame_is_the_zero_point() {
        let mut c = TimestampConverter::new(FREQ);
        assert_eq!(c.update(1_000_000), 0);
    }

    #[test]
    fn steady_stream_advances_by_frame_period() {
        let mut c = TimestampConverter::new(FREQ);
        let mut ts = 1_000_000u32;
        let first = c.update(ts);
        for i in 1..=10u64 {
            ts += (US_PER_FRAME * FREQ as u64) as u32;
            let us = c.update(ts);
            assert_eq!(us - first, i * US_PER_FRAME);
        }
    }

    #[test]
    fn wrap_around_is_continuous() {
        // stream across the 2^32 wrap point: the delta must stay at
        // the frame period (this is the 71.6 s wrap that would jump
        // ~-71.6 s without wrap handling)
        let mut c = TimestampConverter::new(FREQ);
        let step = (US_PER_FRAME * FREQ as u64) as u32;
        let mut ts = u32::MAX - 5 * step; // last 5 frames before the wrap
        let first = c.update(ts);
        let mut last = first;
        for i in 1..=10u64 {
            ts = ts.wrapping_add(step);
            let us = c.update(ts);
            assert_eq!(
                us - last,
                US_PER_FRAME,
                "frame {i}: jump of {} us across the 2^32 wrap",
                us - last
            );
            last = us;
        }
        assert!(last - first == 10 * US_PER_FRAME);
    }

    #[test]
    fn many_wraps_stay_monotonic() {
        // >3 hours of frames at 30 fps = ~150 wraps
        let mut c = TimestampConverter::new(FREQ);
        let step = (US_PER_FRAME * FREQ as u64) as u32;
        let mut ts = 100u32;
        let first = c.update(ts);
        let mut last = first;
        for _ in 0..90u32 * 150 {
            ts = ts.wrapping_add(step);
            last = c.update(ts);
        }
        assert_eq!(last - first, 90u64 * 150 * US_PER_FRAME);
    }

    #[test]
    fn duplicate_timestamp_triggers_resync_not_a_jump() {
        // two frames carrying the same EOF timestamp: the C's wrap branch
        // adds a full wrap point, the sanity check catches the ~71.6 s
        // jump and re-syncs — the result must not run away.
        let mut c = TimestampConverter::new(FREQ);
        let ts = 500_000_000u32;
        let first = c.update(ts);
        let second = c.update(ts);
        // after resync the second frame maps to ~0 again
        assert!(
            second <= first + 1_000_000,
            "duplicate ts produced {second} after {first}"
        );
        // and the stream continues normally
        let third = c.update(ts + (US_PER_FRAME * FREQ as u64) as u32);
        assert!(third >= second);
        assert!(third - second <= 2 * US_PER_FRAME);
    }

    #[test]
    fn backwards_timestamp_clamps_to_the_anchor() {
        // a first frame whose timestamp is "before" the session anchor
        // (case 3 in the C's comment) clamps to the anchor, not to a
        // negative time
        let mut c = TimestampConverter::new(FREQ);
        let first = c.update(10);
        assert_eq!(first, 0);
        // a later frame of the same session
        let next = c.update(10 + (US_PER_FRAME * FREQ as u64) as u32);
        assert_eq!(next, US_PER_FRAME);
    }
}
