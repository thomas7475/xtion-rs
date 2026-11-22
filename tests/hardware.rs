//! Hardware integration tests. These run only when a real Xtion is
//! attached; without one they are skipped so `cargo test` stays green on
//! any machine (CI included).

use std::time::Duration;

use xtion::{DepthConfig, Xtion};

fn hardware_present() -> bool {
    match xtion::list_devices() {
        Ok(list) => !list.is_empty(),
        Err(_) => false,
    }
}

#[test]
fn open_handshake_and_identity() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let mut cam = Xtion::open().expect("open");
    println!("version: {}", cam.version);
    assert!(
        cam.version.major >= 3,
        "expected a modern firmware, got {}",
        cam.version
    );

    let serial = cam.serial_number().expect("serial");
    println!("serial: {serial}");
    assert!(!serial.is_empty());

    // calibration must be sane for an Xtion
    assert!(cam.fixed_params.reference_distance > 50.0);
    assert!(cam.fixed_params.reference_distance < 500.0);
    assert!(cam.fixed_params.reference_pixel_size > 0.01);
    assert!(cam.fixed_params.reference_pixel_size < 1.0);
}

#[test]
fn depth_stream_roundtrip() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let mut cam = Xtion::open().expect("open");
    let config = DepthConfig::default();
    cam.start_depth(config).expect("start depth");

    let frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("first depth frame");
    assert_eq!(frame.width, 640);
    assert_eq!(frame.height, 480);
    assert_eq!(frame.data.len(), 640 * 480);
    // a live scene must have SOME valid pixels (cover the lens with
    // something within 10 m and this will be large)
    println!(
        "frame: min {:?} mm, {} valid pixels",
        frame.min_depth(),
        frame.valid_pixels()
    );
    assert!(
        frame.valid_pixels() > 1000,
        "too few valid pixels: {}",
        frame.valid_pixels()
    );

    cam.stop_depth().expect("stop");
}
