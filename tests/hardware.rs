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

#[test]
fn color_stream_roundtrip() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    // ground truth: which color modes does THIS camera support? The
    // firmware reports the list (opcode 36) and start_color validates
    // against it — a mode outside it is NACKed BadParams.
    let presets = cam.image_presets().expect("image presets");
    println!("image presets ({}):", presets.len());
    for preset in &presets {
        println!("  {preset}");
    }
    // endianness guard: real fps/res/format codes are tiny (fps <= 60,
    // resolution ids <= 15, format ids <= 15). If the parser ever swaps
    // bytes again, every field shifts by 256 and this fails instead of
    // producing a confusing "no matching mode" below.
    for preset in &presets {
        assert!(
            preset.fps <= 60 && preset.resolution <= 15 && preset.format <= 15,
            "preset {preset} looks byte-swapped (fields must be little-endian)"
        );
    }

    // Wire input format: the reference driver's non-Windows default is
    // the PS-COMPRESSED YUV422 (format 1) — "non win32 works with BULK,
    // so it's better to compress the image" (XnSensorImageStream.h:36-41).
    // The uncompressed variant (5) is listed in the firmware's preset
    // table but its stream starts were rejected on this bulk device
    // (0.2.4-0.2.7 hardware runs). Prefer VGA@30, fall back to QVGA@30.
    let compressed = xtion::ImageFormat::Yuv422.as_u16();
    let mode = presets
        .iter()
        .find(|p| p.format == compressed && p.resolution == 1 && p.fps == 30)
        .or_else(|| {
            presets
                .iter()
                .find(|p| p.format == compressed && p.resolution == 0 && p.fps == 30)
        });
    let (resolution, fps) = match mode {
        Some(preset) => {
            let resolution = if preset.resolution == 1 {
                xtion::Resolution::Vga
            } else {
                xtion::Resolution::Qvga
            };
            (resolution, preset.fps)
        }
        None => panic!("this camera offers no compressed-YUV422 color mode (see presets above)"),
    };
    let (width, height) = resolution.dimensions();
    println!("testing {resolution:?}@{fps} PS-compressed YUV422 (the C's Linux/bulk default)");

    cam.start_color(resolution, fps, xtion::ImageFormat::Yuv422)
        .expect("start color");

    // color and IR share firmware stream0 (the image pipe): while color
    // runs, starting IR must be refused by the driver without touching
    // the device (VALIDATION.md manual check 9, driver-side half)
    match cam.start_ir(xtion::IrConfig::default()) {
        Err(xtion::XtionError::BadConfig(_)) => {}
        other => {
            let _ = cam.stop_ir();
            panic!("starting IR while color runs must be BadConfig, got {other:?}");
        }
    }

    let frame = match cam.read_color_frame(Duration::from_secs(5)) {
        Ok(frame) => frame,
        Err(err) => {
            eprintln!(
                "color read failed: {err}; color ring stats: {:?}",
                cam.color_ring_stats()
            );
            panic!("one color frame: {err}");
        }
    };
    assert_eq!(frame.width, width);
    assert_eq!(frame.height, height);
    // RGB888 decoded from the PS-compressed YUV422 wire frame
    assert_eq!(frame.data.len(), (width * height * 3) as usize);
    // a live scene must produce non-constant color
    let min = *frame.data.iter().min().unwrap();
    let max = *frame.data.iter().max().unwrap();
    println!("color {min}..{max} (byte range)");
    assert!(max > min, "color frame is constant ({min}..{max})");

    let events = cam.drain_events();
    println!("events: {events:?}");

    cam.stop_color().expect("stop color");
}

/// Diagnostic for the color-start NACK (0.2.6 hardware run: with the
/// (format 5, res 1, fps 30) mode written and ACKed, the final
/// STREAM0_MODE=COLOR write is rejected BadParams — sometimes even the
/// first image-format write, right after a previous failed attempt).
///
/// This probe REPORTS what the device accepts instead of gating on it:
/// 1. GET_MODE + the image-pipe parameter state before any write
/// 2. a reference-init soft reset (SET_MODE 6 = SOFT_RESET) and the
///    state afterwards — the C does this at EVERY startup and we never
///    did
/// 3. the full C-parity write sequence with readback after each step
///    (format -> resolution -> fps -> flicker -> mirror -> stream0)
/// 4. one full start_color attempt through the driver
#[test]
fn color_start_probe() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    use xtion::Param;

    println!("== device mode: {:?}", cam.get_mode());

    // depth-CMOS presets cross-check the preset mechanism against a
    // second CMOS (id 1)
    match cam.cmos_presets(1) {
        Ok(presets) => {
            println!("depth cmos presets ({}):", presets.len());
            for preset in &presets {
                println!("  {preset}");
            }
        }
        Err(e) => println!("depth cmos presets: {e}"),
    }

    let watched = [
        (Param::Stream0Mode, "stream0"),
        (Param::ImageFormat, "format"),
        (Param::ImageResolution, "res"),
        (Param::ImageFps, "fps"),
        (Param::ImageAgc, "agc"),
        (Param::ImageFlickerDetection, "flicker"),
        (Param::ImageMirror, "mirror"),
    ];
    println!("== image-pipe params BEFORE any write:");
    for (p, name) in &watched {
        println!("   {name:<8} ({}) = {:?}", p.as_u16(), cam.get_param(*p));
    }

    println!("== soft reset (reference init does this every start) ==");
    match cam.soft_reset() {
        Ok(()) => println!("   device answered keep-alive again"),
        Err(e) => println!("   soft reset/liveness failed: {e:?}"),
    }
    println!("mode after reset: {:?}", cam.get_mode());
    println!("== image-pipe params AFTER reset:");
    for (p, name) in &watched {
        println!("   {name:<8} = {:?}", cam.get_param(*p));
    }

    println!(
        "== C-parity write sequence with readbacks (compressed mode, the C's Linux default) =="
    );
    let steps = [
        (Param::ImageFormat, "format=1 (PS-compressed)", 1u16),
        (Param::ImageResolution, "res=1", 1),
        (Param::ImageFps, "fps=30", 30),
        (Param::ImageFlickerDetection, "flicker=0", 0),
        (Param::ImageMirror, "mirror=0", 0),
        (Param::Stream0Mode, "stream0=1 (COLOR)", 1),
    ];
    for (p, name, value) in &steps {
        match cam.set_param(*p, *value) {
            Ok(()) => println!("   set {name}: ACK"),
            Err(e) => println!("   set {name}: REJECTED ({e})"),
        }
        println!("      readback {:?} = {:?}", p, cam.get_param(*p));
    }
    // leave stream0 off before the driver attempt
    match cam.set_param(Param::Stream0Mode, 0) {
        Ok(()) => println!("   set stream0=0 (OFF): ACK"),
        Err(e) => println!("   set stream0=0 (OFF): REJECTED ({e})"),
    }

    println!("== full driver start_color attempt (compressed) ==");
    match cam.start_color(xtion::Resolution::Vga, 30, xtion::ImageFormat::Yuv422) {
        Ok(()) => {
            println!("   start_color: OK");
            match cam.read_color_frame(Duration::from_secs(5)) {
                Ok(frame) => {
                    let min = *frame.data.iter().min().unwrap();
                    let max = *frame.data.iter().max().unwrap();
                    println!(
                        "   frame {}x{} len {} bytes {min}..{max}",
                        frame.width,
                        frame.height,
                        frame.data.len()
                    );
                }
                Err(e) => println!(
                    "   read_color_frame: {e} (ring {:?})",
                    cam.color_ring_stats()
                ),
            }
            let _ = cam.stop_color();
        }
        Err(e) => println!("   start_color: {e}"),
    }
    println!("== done; mode now: {:?}", cam.get_mode());
}

/// The 0.2.8 read-only system commands (flash FS reads, telemetry,
/// logs, CMOS register reads). Everything here is safe to run at any
/// time: nothing writes flash, nothing changes device state.
#[test]
fn system_commands_readonly() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    println!("== firmware log (opcode 7):");
    match cam.firmware_log() {
        Ok(log) => {
            let lines: Vec<&str> = log.lines().collect();
            println!("   {} entries", lines.len());
            for line in lines.iter().take(10) {
                println!("   {line}");
            }
            if lines.len() > 10 {
                println!("   ... {} more", lines.len() - 10);
            }
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== flash file list (opcode 19):");
    match cam.flash_file_list() {
        Ok(files) => {
            println!("   {} files:", files.len());
            for f in &files {
                println!(
                    "   id {:<3} type {:#06x} {:<16} ver {}.{}.{} offset {:#x} size {} crc {:#06x}",
                    f.id,
                    f.file_type,
                    xtion::protocol::system::flash_file_type_name(f.file_type),
                    f.version >> 16 & 0xff,
                    f.version >> 8 & 0xff,
                    f.version & 0xffff,
                    f.offset,
                    f.size,
                    f.crc
                );
            }
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== flash map (opcode 18):");
    match cam.flash_map() {
        Ok(map) => {
            println!("   {} entries:", map.len());
            for entry in &map {
                println!(
                    "   type {:#06x} {:<16} offset {:#x} words {} ver {}.{}.{}",
                    entry.file_type,
                    xtion::protocol::system::flash_file_type_name(entry.file_type),
                    entry.offset_in_flash,
                    entry.size_in_words,
                    entry.version_major,
                    entry.version_minor,
                    entry.version_build
                );
            }
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== download ImageCmos calibration (opcode 16, type 0x0A):");
    match cam.download_flash_file(0x0A) {
        Ok(blob) => println!(
            "   {} bytes (head {:02x?})",
            blob.len(),
            &blob[..blob.len().min(16)]
        ),
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== CPU stats (opcode 27):");
    match cam.cpu_stats() {
        Ok(tasks) => {
            println!("   {} tasks:", tasks.len());
            for t in tasks.iter().take(8) {
                println!("   runs {} us {}", t.times_executed, t.time_microseconds);
            }
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== TEC data (opcode 38):");
    match cam.tec_data() {
        Ok(tec) => println!(
            "   set point {}.{:02} C, measured {}.{:02} C, duty {}%, mode {}",
            tec.set_point_temperature / 100,
            (tec.set_point_temperature % 100).abs(),
            tec.measured_temperature / 100,
            (tec.measured_temperature % 100).abs(),
            tec.tec_duty_cycle,
            tec.scan_mode
        ),
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== emitter data (opcode 32):");
    match cam.emitter_data() {
        Ok(em) => println!(
            "   state {} setpoint {} mV pd {} apc {}",
            em.state, em.set_point_voltage, em.pd_reading, em.is_apc_enabled
        ),
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== CMOS blanking, direct (opcode 35):");
    println!("   image: {:?}", cam.cmos_blanking_lines(0));
    println!("   depth: {:?}", cam.cmos_blanking_lines(1));

    println!("== CMOS register read via I2C (opcode 11):");
    println!("   image reg 0x00: {:?}", cam.read_cmos_register(0, 0x00));
    println!("   depth reg 0x00: {:?}", cam.read_cmos_register(1, 0x00));

    println!("== AHB read (opcode 20):");
    println!("   0x00000000: {:?}", cam.read_ahb(0x0));

    println!("== raw flash read (opcode 25), boot-manager header:");
    match cam.read_flash(0x2000, 32) {
        Ok(data) => println!("   {:02x?}", &data),
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== done; mode now: {:?}", cam.get_mode());
}
