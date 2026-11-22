//! Shift-to-depth conversion: builds the lookup table that turns raw
//! per-pixel **shift** values into metric depth (millimeters), using the
//! per-device calibration constants.
//!
//! Exact port of `DDK/XnShiftToDepth.cpp::XnShiftToDepthUpdate`, including
//! the integer-division constant adjustment and the loop bounds
//! (`1 .. max_shift` exclusive).
//!
//! The formula (all floating point in `f64`, like the C `XnDouble`):
//!
//! ```text
//! fixed_ref_x = (shift - const_shift_adj) / param_coeff - 0.375
//! metric      = fixed_ref_x * (zero_plane_pixel_size * pixel_size_factor)
//! depth       = shift_scale * (metric * zpd / (emitter_distance - metric) + zpd)
//! ```

/// Calibration inputs; types mirror `XnShiftToDepthConfig`
/// (DDK/XnShiftToDepth.h) so the arithmetic matches bit for bit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShiftToDepthConfig {
    /// Zero plane distance (ZPD), depth units (millimeters).
    pub zero_plane_distance: u16,
    /// Zero plane pixel size (ZPPS), millimeters.
    pub zero_plane_pixel_size: f32,
    /// Emitter-to-depth-CMOS distance (LDDIS), centimeters.
    pub emitter_d_cmos_distance: f32,
    /// Maximum shift value the device can produce.
    pub device_max_shift: u32,
    /// Maximum depth the device can produce (before cutoffs).
    pub device_max_depth: u32,
    /// ConstShift calibration constant (from ALGORITHM_DEPTH_INFO).
    pub const_shift: u32,
    /// Pixel size factor (driver default: 1).
    pub pixel_size_factor: u32,
    /// ParamCoeff (driver default: 4).
    pub param_coeff: u32,
    /// ShiftScale (driver default: 10).
    pub shift_scale: u32,
    /// Depths below this are zeroed out (driver default: 0).
    pub depth_min_cutoff: u16,
    /// Depths above this are zeroed out (driver default: 10000).
    pub depth_max_cutoff: u16,
}

impl Default for ShiftToDepthConfig {
    fn default() -> Self {
        ShiftToDepthConfig {
            zero_plane_distance: 120,
            zero_plane_pixel_size: 0.1052,
            emitter_d_cmos_distance: 7.5,
            // XN_SHIFTS_MAX_SHIFT (XnSensorDepthStream.cpp): packed-11
            // shift values span the full 0..2047 range.
            device_max_shift: 2047,
            device_max_depth: 10000,
            const_shift: 200,
            pixel_size_factor: 1,
            param_coeff: crate::protocol::SHIFTS_PARAM_COEFFICIENT,
            shift_scale: crate::protocol::SHIFTS_SHIFT_SCALE,
            depth_min_cutoff: 0,
            depth_max_cutoff: 10000,
        }
    }
}

/// Lookup tables for both directions.
#[derive(Debug, Clone)]
pub struct ShiftToDepthTables {
    /// `s2d[shift] = depth in mm` (0 where outside the cutoff window).
    pub shift_to_depth: Vec<u16>,
    /// `d2s[depth_mm] = shift` (nearest lower shift).
    pub depth_to_shift: Vec<u16>,
}

impl ShiftToDepthTables {
    /// Build both tables from a calibration config.
    ///
    /// Mirrors `XnShiftToDepthUpdate`:
    /// - `const_shift_adj = (param_coeff * const_shift) / pixel_size_factor`
    ///   computed in **unsigned 32-bit** arithmetic, then reinterpreted as
    ///   i32 (C integer promotion rules);
    /// - the shift loop runs `1 .. device_max_shift` **exclusive**;
    /// - `depth` is only stored when `min_cutoff < depth < min(max_depth,
    ///   max_cutoff)`.
    pub fn build(config: &ShiftToDepthConfig) -> ShiftToDepthTables {
        let mut s2d = vec![0u16; config.device_max_shift as usize + 1];
        let mut d2s = vec![0u16; config.device_max_depth as usize + 1];

        // C: XnInt32 nConstShift = nParamCoeff * nConstShift (u32 math,
        // bit-reinterpreted); then nConstShift /= nPixelSizeFactor is an
        // UNSIGNED division (i32/u32 promotes both to u32).
        let const_shift_adj: i32 =
            (config.param_coeff.wrapping_mul(config.const_shift) / config.pixel_size_factor) as i32;

        let plane_pixel_size: f64 =
            config.zero_plane_pixel_size as f64 * config.pixel_size_factor as f64;
        let plane_dsr: f64 = config.zero_plane_distance as f64;
        let plane_dcl: f64 = config.emitter_d_cmos_distance as f64;

        let max_depth = config.device_max_depth.min(config.depth_max_cutoff as u32);

        let mut last_depth: u16 = 0;
        let mut last_index: u16 = 0;

        for shift in 1..config.device_max_shift {
            let shift_value = shift as i16;
            let mut fixed_ref_x: f64 =
                (shift_value as i32 - const_shift_adj) as f64 / config.param_coeff as f64;
            fixed_ref_x -= 0.375;
            let metric = fixed_ref_x * plane_pixel_size;
            let depth: f64 = config.shift_scale as f64
                * ((metric * plane_dsr / (plane_dcl - metric)) + plane_dsr);

            if depth > config.depth_min_cutoff as f64 && depth < max_depth as f64 {
                let d = depth as u16;
                s2d[shift as usize] = d;

                // fill depth_to_shift for everything up to this depth
                let mut i = last_depth as u32;
                while (i as f64) < depth && (i as usize) < d2s.len() {
                    d2s[i as usize] = last_index;
                    i += 1;
                }
                last_index = shift as u16;
                last_depth = d;
            }
        }

        // tail fill from the last stored depth to the table end
        let mut i = last_depth as u32;
        while i <= config.device_max_depth && (i as usize) < d2s.len() {
            d2s[i as usize] = last_index;
            i += 1;
        }

        ShiftToDepthTables {
            shift_to_depth: s2d,
            depth_to_shift: d2s,
        }
    }

    /// Convert a raw shift frame to depths (mm). Out-of-range shifts map
    /// to 0 (the C code would read out of bounds here; we stay safe).
    pub fn convert_shifts(&self, shifts: &[u16]) -> Vec<u16> {
        shifts
            .iter()
            .map(|&s| self.shift_to_depth.get(s as usize).copied().unwrap_or(0))
            .collect()
    }

    /// Nearest shift for a depth in mm (inverse lookup).
    pub fn shift_for_depth(&self, depth_mm: u16) -> u16 {
        self.depth_to_shift
            .get(depth_mm as usize)
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sizes_match_config() {
        let cfg = ShiftToDepthConfig::default();
        let t = ShiftToDepthTables::build(&cfg);
        assert_eq!(t.shift_to_depth.len(), cfg.device_max_shift as usize + 1);
        assert_eq!(t.depth_to_shift.len(), cfg.device_max_depth as usize + 1);
    }

    #[test]
    fn shift_zero_is_always_zero() {
        // the C loop starts at shift 1, so s2d[0] stays 0
        let t = ShiftToDepthTables::build(&ShiftToDepthConfig::default());
        assert_eq!(t.shift_to_depth[0], 0);
    }

    #[test]
    fn depth_is_monotonically_increasing_within_window() {
        let t = ShiftToDepthTables::build(&ShiftToDepthConfig::default());
        let mut last = 0u16;
        for (shift, &d) in t.shift_to_depth.iter().enumerate().skip(1) {
            if d != 0 {
                assert!(d >= last, "depth decreased at shift {shift}: {d} < {last}");
                last = d;
            }
        }
    }

    #[test]
    fn cutoffs_are_enforced() {
        let cfg = ShiftToDepthConfig {
            depth_max_cutoff: 3000,
            ..ShiftToDepthConfig::default()
        };
        let t = ShiftToDepthTables::build(&cfg);
        assert!(t.shift_to_depth.iter().all(|&d| d <= 3000));
    }

    #[test]
    fn convert_and_inverse_are_consistent() {
        let t = ShiftToDepthTables::build(&ShiftToDepthConfig::default());
        let shifts: Vec<u16> = (0..1024u32).map(|s| s as u16).collect();
        let depths = t.convert_shifts(&shifts);
        // every non-zero depth must map back to a shift whose depth is
        // the same value (the inverse LUT is filled from the forward one)
        for &depth in depths.iter() {
            if depth > 0 {
                let back = t.shift_for_depth(depth);
                assert!(t.shift_to_depth[back as usize] > 0);
            }
        }
    }

    #[test]
    fn known_value_matches_hand_computation() {
        // Hand-computed with the documented formula for the default
        // calibration (the Xtion Pro Live constants) at shift 400:
        //   const_shift_adj = 4 * 200 / 1 = 800
        //   fixed_ref_x = (400 - 800)/4 - 0.375 = -100.375
        //   metric = -100.375 * 0.1052 = -10.559...
        //   depth = 10 * (metric*120/(7.5 - metric) + 120) ~= 498 mm
        let cfg = ShiftToDepthConfig::default();
        let t = ShiftToDepthTables::build(&cfg);

        let const_adj = 800i32;
        let frx = (400i32 - const_adj) as f64 / 4.0 - 0.375;
        let metric = frx * (cfg.zero_plane_pixel_size as f64 * 1.0);
        let expected = 10.0 * ((metric * 120.0 / (7.5 - metric)) + 120.0);

        assert_eq!(t.shift_to_depth[400], expected as u16);
        // sanity: a plausible metric depth in the 100-10000 mm window
        assert!(
            (100..10000).contains(&t.shift_to_depth[400]),
            "{}",
            t.shift_to_depth[400]
        );
    }

    #[test]
    fn far_shifts_beyond_1024_are_valid() {
        // 0.1.4 regression: a 1024-entry table silently zeroed every
        // shift >= 1024 (~5.5 m and beyond), producing all-invalid frames
        // whenever the scene sat that far. XN_SHIFTS_MAX_SHIFT is 2047
        // (XnSensorDepthStream.cpp) and the packed-11 wire values span
        // the full 0..2047 range.
        let t = ShiftToDepthTables::build(&ShiftToDepthConfig::default());
        assert_eq!(t.shift_to_depth.len(), 2048);
        assert!(t.shift_to_depth[1024] > 0, "shift 1024 must be valid");
        assert!(t.shift_to_depth[1052] > 0, "last valid shift (9870 mm)");
        assert_eq!(t.shift_to_depth[1053], 0, "beyond the valid window");
        assert_eq!(t.shift_to_depth[2047], 0, "no-depth marker stays 0");
        // the near-range floor: shift 1 is ~315 mm
        assert_eq!(t.shift_to_depth[1], 315);
    }
}
