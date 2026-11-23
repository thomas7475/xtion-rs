//! Point-cloud capture: depth frames -> world coordinates -> ASCII PLY.
//!
//! Photogrammetry-oriented example: prints the on-device calibration and
//! the derived pinhole intrinsics, captures N depth frames, converts the
//! last one to a world-coordinate point cloud and writes it as an ASCII
//! PLY (x/y/z in metres, OpenNI convention: X right, Y up, Z forward —
//! the same axes CloudCompare/MeshLab use for the sensor frame).
//!
//! ```sh
//! cargo run --release --example point_cloud -- out.ply 60
//! ```

use std::time::Duration;

use xtion::{DepthConfig, Resolution, Xtion};

fn main() {
    let mut args = std::env::args().skip(1);
    let out_path = args.next().unwrap_or_else(|| "cloud.ply".into());
    let frames: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(30);

    let mut cam = Xtion::open().expect("no Xtion found");
    let serial = cam.serial_number().unwrap_or_default();
    println!("firmware {} (serial {serial})", cam.version);

    // The calibration this driver reads from the device, and the
    // intrinsics derived from it (see src/geometry.rs for the formulas).
    let intrinsics = cam.depth_intrinsics(Resolution::Vga);
    println!(
        "calibration: ZPD {:.3} mm, ZPPS {:.5} mm/px",
        cam.fixed_params.reference_distance, cam.fixed_params.reference_pixel_size
    );
    println!(
        "VGA intrinsics: fx {:.2} fy {:.2} cx {:.2} cy {:.2} hfov {:.3} deg vfov {:.3} deg",
        intrinsics.fx,
        intrinsics.fy,
        intrinsics.cx,
        intrinsics.cy,
        intrinsics.hfov.to_degrees(),
        intrinsics.vfov.to_degrees()
    );

    cam.start_depth(DepthConfig::default())
        .expect("start depth");
    let mut last = None;
    for i in 0..frames {
        let frame = cam
            .read_depth_frame(Duration::from_secs(5))
            .expect("depth frame");
        println!(
            "frame {}/{}: {} valid pixels, min depth {} mm, ts_us {:?}",
            i + 1,
            frames,
            frame.valid_pixels(),
            frame.min_depth().unwrap_or(0),
            frame.timestamp_us
        );
        last = Some(frame);
    }
    cam.stop_depth().expect("stop depth");

    let frame = last.expect("at least one frame");
    let cloud = frame.point_cloud(&intrinsics);
    println!("cloud: {} points", cloud.len());

    // ASCII PLY, metres. (PLY has no unit field; metres is the mesh-tool
    // default. Multiply by 1000 for millimetres if your pipeline expects
    // the raw sensor unit.)
    let mut ply = String::with_capacity(cloud.len() * 40);
    ply.push_str("ply\nformat ascii 1.0\n");
    ply.push_str("comment xtion-rs point cloud (OpenNI frame: mm->m, X right, Y up, Z forward)\n");
    ply.push_str(&format!("element vertex {}\n", cloud.len()));
    ply.push_str("property float x\nproperty float y\nproperty float z\n");
    ply.push_str("end_header\n");
    for (x, y, z) in &cloud {
        ply.push_str(&format!(
            "{:.4} {:.4} {:.4}\n",
            x / 1000.0,
            y / 1000.0,
            z / 1000.0
        ));
    }
    std::fs::write(&out_path, ply).expect("write ply");
    println!("wrote {out_path}");
}
