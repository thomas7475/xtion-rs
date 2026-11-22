//! Hardware integration tests. These run only when a real Xtion is
//! attached; without one they are skipped so `cargo test` stays green on
//! any machine (CI included).
//!
//! The tests must not run in parallel: only one process/handle can claim
//! the device's USB interface, so a global mutex serializes them (the
//! same effect as `cargo test -- --test-threads=1`, but foolproof).

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use xtion::{DepthConfig, Xtion};

/// Serializes device access across the test threads.
static DEVICE_LOCK: Mutex<()> = Mutex::new(());

/// Take the device lock; poisoning (a previously panicked test) is
/// ignored so the remaining tests still run.
fn lock_device() -> MutexGuard<'static, ()> {
    DEVICE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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
    let _lock = lock_device();
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
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");
    println!(
        "usb speed {:?}, depth max packet {} B",
        cam.usb_speed(),
        cam.usb_max_packet_sizes().0
    );
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

#[test]
fn phase1_identity_and_options() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");
    println!("version: {}", cam.version);

    // platform string and USB core. The usb-core probe falls back to
    // Jango inside the driver when the firmware NACKs opcode 40
    // (reference behaviour, XnDeviceSensorInit.cpp:233), so this call
    // must always succeed.
    let platform = cam.platform_string().expect("platform string");
    println!("platform: {platform}");
    assert!(!platform.is_empty(), "platform string must not be empty");

    let core = cam.usb_core_type().expect("usb core type");
    println!("usb core: {core:?}");

    // runtime options round-trip through GET_PARAM
    cam.set_registration(true).expect("registration on");
    assert!(cam.registration_enabled().unwrap());
    cam.set_registration(false).expect("registration off");
    assert!(!cam.registration_enabled().unwrap());

    cam.set_hole_filter(true).expect("hole filter");
    cam.set_depth_mirror(true).expect("depth mirror");
    cam.set_close_range(false).expect("close range");

    // emitter toggle: off and back on (runtime state only). The request
    // is byte-identical to the reference driver's, but some 5.8.x builds
    // answer NACK(BadParams) anyway — treat that as a reported skip, not
    // a failure; power-cycling restores the default (on).
    match cam.set_emitter_enabled(false) {
        Ok(()) => {
            cam.set_emitter_enabled(true)
                .unwrap_or_else(|err| eprintln!("emitter re-enable failed: {err}"));
        }
        Err(err) => eprintln!("emitter toggle skipped: {err}"),
    }
}

#[test]
fn ir_stream_roundtrip() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    cam.start_ir(xtion::IrConfig::default()).expect("start ir");
    let frame = match cam.read_ir_frame(Duration::from_secs(3)) {
        Ok(frame) => frame,
        Err(err) => {
            // distinguish "device never sent anything" (ring completions
            // == 0: sensor timing not armed) from "data but no frame"
            eprintln!(
                "ir read failed: {err}; ir ring stats: {:?}",
                cam.ir_ring_stats()
            );
            panic!("one ir frame: {err}");
        }
    };
    println!(
        "ir {}x{}, value range {:?}",
        frame.width,
        frame.height,
        frame.value_range()
    );
    assert_eq!(frame.width, 640);
    assert_eq!(frame.height, 480);
    assert_eq!(frame.data.len(), 640 * 480);
    // with the projector on, a scene must produce non-constant IR values
    let (min, max) = frame.value_range().expect("value range");
    assert!(max > min, "IR frame is constant ({min}..{max})");
    assert!(max > 0, "IR frame is all black");

    // no device events expected on a healthy camera, and draining must
    // not panic
    let events = cam.drain_events();
    println!("events: {events:?}");

    cam.stop_ir().expect("stop ir");
}
