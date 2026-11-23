//! Depth camera geometry: pinhole intrinsics from the PrimeSense
//! calibration and depth-to-world (projective-to-real) conversion.
//!
//! Ports two reference implementations:
//! - the field of view, from the PS1080 depth stream init
//!   (XnSensorDepthStream.cpp:184-191):
//!   `hfov = 2*atan(ZPPS * XN_SXGA_X_RES / 2 / ZPD)` and
//!   `vfov = 2*atan(ZPPS * 2*XN_VGA_Y_RES / 2 / ZPD)`;
//! - the depth-to-world conversion, from the OpenNI2 core
//!   (`Source/Core/OniStream.cpp:399-404`, `convertDepthToWorldCoordinates`).
//!
//! Both are pure functions of the fixed-params calibration (ZPD = zero
//! plane distance, ZPPS = zero-plane pixel size), so they need no device
//! I/O and are fully covered by unit tests with the published Xtion
//! values.
//!
//! Coordinate convention (OpenNI): millimetres, X to the right, **Y up**,
//! Z forward away from the sensor — the pixel row axis is flipped, which
//! is why [`CameraIntrinsics::to_world`] computes `world_y` from
//! `(0.5 - y/height)`.

use crate::protocol::Resolution;

/// The depth CMOS's native active width (XN_SXGA_X_RES) — the horizontal
/// FOV is defined by the full sensor line, not by the streamed window
/// (XnSensorDepthStream.cpp:187).
const CMOS_NATIVE_WIDTH: f64 = 1280.0;
/// The vertical size the FOV is defined by: 2 * XN_VGA_Y_RES = 960
/// (XnSensorDepthStream.cpp:190 uses `XN_VGA_Y_RES*2`).
const CMOS_NATIVE_HEIGHT: f64 = 960.0;

/// Pinhole intrinsics of the depth (and IR) sensor for one resolution.
///
/// Built from the device calibration via
/// [`CameraIntrinsics::from_calibration`]; see [`crate::Xtion::depth_intrinsics`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraIntrinsics {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Focal length in pixel units, horizontal.
    pub fx: f64,
    /// Focal length in pixel units, vertical.
    pub fy: f64,
    /// Principal point, x (image centre).
    pub cx: f64,
    /// Principal point, y (image centre).
    pub cy: f64,
    /// Horizontal field of view, radians.
    pub hfov: f64,
    /// Vertical field of view, radians.
    pub vfov: f64,
    /// Zero plane distance (ZPD), millimetres — the calibration the
    /// intrinsics were derived from.
    pub zpd: f64,
    /// Zero plane pixel size (ZPPS), millimetres.
    pub zpps: f64,
}

impl CameraIntrinsics {
    /// Derive the intrinsics of one resolution from the PrimeSense
    /// calibration constants.
    ///
    /// `zpd` = reference (zero-plane) distance in mm, `zpps` =
    /// zero-plane pixel size in mm/px — both from GET_FIXED_PARAMS
    /// (`FixedParams::reference_distance` / `reference_pixel_size`).
    ///
    /// The FOVs are **resolution independent** (they describe the optics:
    /// hfov spans the CMOS's native 1280-pixel line, vfov its 960-pixel
    /// height); `fx`/`fy`/`cx`/`cy` adapt to the requested frame size.
    pub fn from_calibration(zpd: f64, zpps: f64, resolution: Resolution) -> CameraIntrinsics {
        let (width, height) = resolution.dimensions();
        let w = width as f64;
        let h = height as f64;
        // XnSensorDepthStream.cpp:187-191
        let hfov = 2.0 * ((zpps * CMOS_NATIVE_WIDTH / 2.0) / zpd).atan();
        let vfov = 2.0 * ((zpps * CMOS_NATIVE_HEIGHT / 2.0) / zpd).atan();
        // OniStream.cpp:439-446: fx = (w/2)/tan(hfov/2), centre = w/2.
        // (For the VGA depth window this reduces to fx = zpd/(2*zpps),
        //  i.e. square pixels: the native 1280-wide line maps to 640 px.)
        let fx = w / 2.0 / (hfov / 2.0).tan();
        let fy = h / 2.0 / (vfov / 2.0).tan();
        CameraIntrinsics {
            width,
            height,
            fx,
            fy,
            cx: w / 2.0,
            cy: h / 2.0,
            hfov,
            vfov,
            zpd,
            zpps,
        }
    }

    /// Convert one depth pixel to sensor-frame world coordinates
    /// (millimetres, X right, Y up, Z forward).
    ///
    /// Port of `VideoStream::convertDepthToWorldCoordinates`
    /// (Source/Core/OniStream.cpp:399-404):
    /// `world_x = (x/w - 0.5) * z * 2*tan(hfov/2)`,
    /// `world_y = (0.5 - y/h) * z * 2*tan(vfov/2)`, `world_z = z`.
    ///
    /// Returns `None` for invalid pixels (`z == 0`).
    pub fn to_world(&self, x: u32, y: u32, depth_mm: u16) -> Option<(f64, f64, f64)> {
        if depth_mm == 0 {
            return None;
        }
        let z = depth_mm as f64;
        let normalized_x = x as f64 / self.width as f64 - 0.5;
        let normalized_y = 0.5 - y as f64 / self.height as f64;
        let world_x = normalized_x * z * (self.hfov / 2.0).tan() * 2.0;
        let world_y = normalized_y * z * (self.vfov / 2.0).tan() * 2.0;
        Some((world_x, world_y, z))
    }

    /// Inverse of [`CameraIntrinsics::to_world`]: world point (mm) back to
    /// pixel coordinates + depth. Port of
    /// `VideoStream::convertWorldToDepthCoordinates` (OniStream.cpp:416-418).
    pub fn to_pixel(&self, world_x: f64, world_y: f64, world_z: f64) -> (u32, u32, f64) {
        let coeff_x = self.width as f64 / ((self.hfov / 2.0).tan() * 2.0);
        let coeff_y = self.height as f64 / ((self.vfov / 2.0).tan() * 2.0);
        let px = coeff_x * world_x / world_z + self.cx;
        let py = self.cy - coeff_y * world_y / world_z;
        (px.round() as u32, py.round() as u32, world_z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published Xtion calibration (typical values for the
    /// 0.2.x validation camera; the actual per-unit values come from
    /// GET_FIXED_PARAMS and may differ slightly).
    const ZPD: f64 = 120.0;
    const ZPPS: f64 = 0.1052;

    #[test]
    fn vga_fov_matches_xtion_spec() {
        // The Xtion spec sheet: 58 deg horizontal, 45 deg vertical, +/- a
        // little for per-unit calibration.
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        let hfov_deg = i.hfov.to_degrees();
        let vfov_deg = i.vfov.to_degrees();
        assert!((57.5..59.0).contains(&hfov_deg), "hfov {hfov_deg}");
        assert!((44.5..46.5).contains(&vfov_deg), "vfov {vfov_deg}");
    }

    #[test]
    fn vga_intrinsics_are_the_closed_form() {
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        // fx = 320/tan(hfov/2) = zpd/(2*zpps) for the VGA window
        let closed_fx = ZPD / (2.0 * ZPPS);
        assert!(
            (i.fx - closed_fx).abs() < 1e-9,
            "fx {} vs {closed_fx}",
            i.fx
        );
        // square pixels: fy == fx, centre in the image middle
        assert!((i.fy - i.fx).abs() < 1e-9);
        assert_eq!(i.cx, 320.0);
        assert_eq!(i.cy, 240.0);
        assert_eq!(i.width, 640);
        assert_eq!(i.height, 480);
    }

    #[test]
    fn fov_is_resolution_independent_but_focal_scales() {
        let vga = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        let sxga = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Sxga);
        let qvga = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Qvga);
        // same optics -> same FOV at every window size
        assert!((vga.hfov - sxga.hfov).abs() < 1e-12);
        assert!((vga.vfov - sxga.vfov).abs() < 1e-12);
        // double resolution -> double focal length, same centre ratio
        assert!((sxga.fx - 2.0 * vga.fx).abs() < 1e-9);
        assert!((qvga.fx - vga.fx / 2.0).abs() < 1e-9);
    }

    #[test]
    fn center_pixel_is_straight_ahead() {
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        let (x, y, z) = i.to_world(320, 240, 1000).unwrap();
        assert!(x.abs() < 1e-9, "x {x}");
        assert!(y.abs() < 1e-9, "y {y}");
        assert_eq!(z, 1000.0);
    }

    #[test]
    fn world_conversion_matches_oni_stream_form() {
        // Mirror of the exact C statement:
        //   normalizedX = depthX / resolutionX - .5f
        //   normalizedY = .5f - depthY / resolutionY
        //   worldX = normalizedX * depthZ * xzFactor  (xzFactor = tan(hfov/2)*2)
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        let (x, y, z) = i.to_world(0, 0, 2000).unwrap();
        // independent recomputation via the C's cache form:
        let xz_factor = (i.hfov / 2.0).tan() * 2.0;
        let yz_factor = (i.vfov / 2.0).tan() * 2.0;
        let want_x = (0.0f64 / 640.0 - 0.5) * 2000.0 * xz_factor;
        let want_y = (0.5 - 0.0 / 480.0) * 2000.0 * yz_factor;
        assert!((x - want_x).abs() < 1e-9, "x {x} vs {want_x}");
        assert!((y - want_y).abs() < 1e-9, "y {y} vs {want_y}");
        assert_eq!(z, 2000.0);
    }

    #[test]
    fn y_axis_points_up() {
        // upper image rows must map to positive (up) world Y
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        let up = i.to_world(320, 0, 1500).unwrap().1;
        let down = i.to_world(320, 479, 1500).unwrap().1;
        assert!(up > 0.0);
        assert!(down < 0.0);
        // near-symmetric: rows 0 and 479 are both ~half a frame from the
        // centre (240.0), so their world Y nearly cancels (the residual
        // is the half-pixel row offset: z * yzFactor / 960).
        assert!((up + down).abs() < 5.0, "asymmetry {} mm", up + down);
    }

    #[test]
    fn zero_depth_is_invalid() {
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        assert!(i.to_world(10, 10, 0).is_none());
    }

    #[test]
    fn pixel_roundtrip() {
        let i = CameraIntrinsics::from_calibration(ZPD, ZPPS, Resolution::Vga);
        let (x0, y0, z0) = (123u32, 321u32, 2345u16);
        let (wx, wy, wz) = i.to_world(x0, y0, z0).unwrap();
        let (px, py, pz) = i.to_pixel(wx, wy, wz);
        assert!((px as f64 - x0 as f64).abs() <= 1.0, "px {px} vs {x0}");
        assert!((py as f64 - y0 as f64).abs() <= 1.0, "py {py} vs {y0}");
        assert_eq!(pz, z0 as f64);
    }
}
