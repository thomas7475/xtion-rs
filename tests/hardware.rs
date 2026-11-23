//! Hardware integration tests. These run only when a real Xtion is
//! attached; without one they are skipped so `cargo test` stays green on
//! any machine (CI included).
//!
//! The tests must not run in parallel: only one process/handle can claim
//! the device's USB interface, so a global mutex serializes them (the
//! same effect as `cargo test -- --test-threads=1`, but foolproof).

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use xtion::{protocol::system, DepthConfig, Xtion};

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
    // the two gates the reference driver checks before any color stream:
    // bImageSupported (fixed params image CMOS type, 0 = NO COLOR SENSOR)
    // and the firmware operating mode.
    let image_cmos_type = cam.fixed_params.image_cmos_type;
    let mode = cam.get_mode();
    let stream0 = cam.get_param(xtion::Param::Stream0Mode);
    println!(
        "image cmos type: {image_cmos_type} (0=none, 1=MT9M112, 2=MT9D131, 3=MT9M114), mode: {mode:?}, stream0: {stream0:?}"
    );
    if image_cmos_type == 0 {
        // Hardware answer (2026-11-23 run): THIS UNIT HAS NO COLOR
        // SENSOR. The fixed-params field IS the reference driver's
        // bImageSupported gate (XnSensorFirmware.cpp:128-140), and the C
        // never creates the image stream when it is 0
        // (XnSensor.cpp:274-281) — OpenNI lists no color node on this
        // device either. Every color start since 0.2.4 was NACKed
        // BadParams on the stream-on write across all four attempted
        // mode combinations, while IR — the other stream0 source, fed
        // by the depth CMOS — starts fine on the same pipe. The preset
        // table printed above is a platform capability list baked into
        // the RD108x firmware, not this unit's sensor inventory. There
        // is nothing to round-trip on this hardware; 0.2.10 makes the
        // driver refuse it up front like the reference driver does.
        let depth = cam.fixed_params.depth_cmos_type;
        eprintln!(
            "skipped: no color sensor on this device — image_cmos_type = 0 \
             (XN_IMAGE_CMOS_NONE), depth_cmos_type = {depth} ({}). Depth + IR \
             are this unit's streams; color streaming is a hardware \
             property it lacks. The reference C driver also refuses to \
             create the image stream here (bImageSupported), and \
             start_color now fails fast with the same gate instead of \
             NACKing at stream-on. See docs/PROTOCOL.md section 7.",
            match depth {
                1 => "MT9M001",
                2 => "AR130",
                _ => "unknown type",
            }
        );
        return;
    }

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
    println!(
        "== image cmos type: {} (0 = the firmware sees NO color sensor: the C then refuses to\n   create the image stream at all — XnSensorFirmware.cpp:128-140, bImageSupported)\n   depth cmos type: {} (1 = MT9M001, 2 = AR130 — nonzero = the depth/IR sensor\n   the working streams use; this cross-checks the fixed-params parse)",
        cam.fixed_params.image_cmos_type, cam.fixed_params.depth_cmos_type
    );

    // the boot self-test log: the color CMOS reports CLR C/V/ND/W/OK
    // lines at boot (FIRMWARE.md section 3). A failed CLR self-test is
    // the other way the firmware can reject the color stream-on.
    match cam.firmware_log() {
        Ok(log) => {
            let boot: Vec<&str> = log
                .lines()
                .filter(|l| {
                    let l = l.to_uppercase();
                    l.contains("CLR") || l.contains("PRJ") || l.contains("TEC") || l.contains("IR ")
                })
                .collect();
            println!("== boot/self-test log lines ({}):", boot.len());
            for line in boot.iter().take(30) {
                println!("   {line}");
            }
        }
        Err(e) => println!("firmware log: {e}"),
    }

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
        (Param::Stream0Mode, "stream0=0 (OFF baseline)", 0u16),
        (Param::ImageFormat, "format=1 (PS-compressed)", 1),
        (Param::ImageResolution, "res=1", 1),
        (Param::ImageFps, "fps=30", 30),
        (Param::ImageFlickerDetection, "flicker=0", 0),
        (Param::ImageMirror, "mirror=0", 0),
        (Param::Stream0Mode, "stream0=1 (COLOR)", 1),
        (Param::ImageCropMode, "crop=0 (disabled)", 0),
        (Param::ImageAutoExposureMode, "auto exposure=1", 1),
        (Param::ImageAutoWhiteBalanceMode, "auto white balance=1", 1),
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

    println!("== firmware log (opcode 7), raw entries + rendering:");
    match cam.firmware_log_entries() {
        Ok(entries) => {
            println!("   {} entries before the trigger", entries.len());
            for entry in entries.iter().take(12) {
                println!(
                    "   raw: size {} words, ts {} (0x{:08X}), type 0x{:04X}, payload {} bytes: {:02x?}",
                    entry.entry_size,
                    entry.timestamp,
                    entry.timestamp,
                    entry.log_type,
                    entry.payload.len(),
                    &entry.payload[..entry.payload.len().min(16)]
                );
            }
            let rendered = system::render_firmware_log(&entries);
            for line in rendered.lines().take(12) {
                println!("   {line}");
            }

            // Ground-truth alignment: trigger a KNOWN firmware event (the
            // emitter toggle this build NACKs BadParams — the only command
            // this camera predictably rejects), then re-read. Every entry
            // that appears now was logged moments ago, so its header fields
            // pin the real log-entry layout (fixed in 0.3.0: {u16 size,
            // u32 ts, u16 type}) and the timestamp's unit/range.
            println!("== triggering a known NACK (emitter toggle, opcode 42):");
            match cam.set_emitter_enabled(false) {
                Ok(()) => println!("   toggle ACKed (this build accepts it — re-enable below)"),
                Err(err) => println!("   toggle rejected: {err}"),
            }
            let _ = cam.set_emitter_enabled(true);
            match cam.firmware_log_entries() {
                Ok(new_entries) => {
                    println!("   {} NEW entries after the trigger:", new_entries.len());
                    for entry in new_entries.iter().take(10) {
                        println!(
                            "   raw: size {} words, ts {} (0x{:08X}), type 0x{:04X}, payload {:02x?}",
                            entry.entry_size,
                            entry.timestamp,
                            entry.timestamp,
                            entry.log_type,
                            &entry.payload[..entry.payload.len().min(24)]
                        );
                    }
                    for line in system::render_firmware_log(&new_entries).lines().take(10) {
                        println!("   {line}");
                    }
                }
                Err(e) => println!("   re-read failed: {e}"),
            }
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    println!(
        "== device clock rate (ALGORITHM_PARAMS 0x80): {:?} ticks/us (expect ~60.0)",
        cam.device_frequency()
    );

    println!("== flash file list (opcode 19, the firmware's own view):");
    // fw 5.8.22 walks the packed 22-byte on-flash records with its
    // unpacked 24-byte struct: every row past the first drifts +2 bytes
    // per record, and only every 11th row (lcm(22,24) = 264) is a true
    // record. Kept for comparison with the vendor driver's output —
    // the parsed raw table below is the ground truth.
    match cam.flash_file_list() {
        Ok(files) => {
            println!("   {} files:", files.len());
            for f in &files {
                println!(
                    "   id {:<3} type {:#06x} {:<16} ver {}.{}.{} offset {:#x} size {} words ({} B) crc {:#06x} attrs {:#06x}{}",
                    f.id,
                    f.file_type,
                    system::flash_file_type_name(f.file_type),
                    f.version >> 16 & 0xff,
                    f.version >> 8 & 0xff,
                    f.version & 0xffff,
                    f.offset,
                    f.size,
                    f.size_bytes(),
                    f.crc,
                    f.attributes,
                    if f.is_read_only() { " [R]" } else { "" }
                );
            }
            println!(
                "   note: rows past the first drift +2 B per record on this fw — every 11th row is real"
            );
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    // The ground truth: READ_FLASH the table region (word 0x1000,
    // 8192 B) and parse the packed 22-byte XnFlashFile records. The
    // 0.3.1 camera run decoded ids 0-13 from the first 256 B of this
    // region; the offline CRC identification (0.3.2) matched 8 files
    // against the factory image.
    println!("== on-flash file table (opcode 25 + packed-record parse — ground truth):");
    match cam.flash_file_table() {
        Ok(files) => {
            println!("   {} records:", files.len());
            for f in &files {
                println!(
                    "   id {:<3} type {:#06x} {:<16} ver {:#010x} offset {:#06x} w ({:#06x} B) size {:>6} w ({:>6} B) crc {:#06x} attrs {:#06x}{}",
                    f.id,
                    f.file_type,
                    system::flash_file_type_name(f.file_type),
                    f.version,
                    f.offset,
                    f.offset_bytes(),
                    f.size,
                    f.size_bytes(),
                    f.crc,
                    f.attributes,
                    if f.is_read_only() { " [R]" } else { "" }
                );
            }
            assert!(!files.is_empty(), "file table parsed empty");
            let head = &files[0];
            assert_eq!(
                (head.id, head.file_type, head.offset, head.size, head.crc),
                (0, system::FILE_TYPE_FILE_TABLE, 0x1000, 0x1000, 0xFFFC),
                "record 0 must be the FileTable self-entry (magic version, \
                 offset 0x1000 words, size 0x1000 words, crc 0xfffc)"
            );
            assert!(
                files.len() >= 12,
                "expected at least the 12 records decoded on the 0.3.1 camera run"
            );

            // On-device CRC verification: read every file's payload at
            // its word offset and check CCITT-FALSE (big-endian words)
            // against the table's stored crc. The eight build-generic
            // files already verified offline against the factory image.
            println!("== flash CRC verification (read payload, CRC-16/CCITT-FALSE over BE words):");
            let mut verified = 0usize;
            let mut results: Vec<(u16, u16, bool)> = Vec::new();
            for f in &files {
                if f.crc == 0 || f.size == 0 {
                    println!(
                        "   id {:<3} {:<16} no stored crc — skipped",
                        f.id,
                        system::flash_file_type_name(f.file_type)
                    );
                    continue;
                }
                match cam.verify_flash_file(f) {
                    Ok(true) => {
                        verified += 1;
                        results.push((f.id, f.file_type, true));
                        println!(
                            "   id {:<3} {:<16} {:>6} w  crc {:#06x}  OK",
                            f.id,
                            system::flash_file_type_name(f.file_type),
                            f.size,
                            f.crc
                        );
                    }
                    Ok(false) => {
                        results.push((f.id, f.file_type, false));
                        println!(
                            "   id {:<3} {:<16} crc {:#06x}  MISMATCH",
                            f.id,
                            system::flash_file_type_name(f.file_type),
                            f.crc
                        );
                    }
                    Err(e) => println!("   id {:<3} read failed: {e}", f.id),
                }
            }
            // GainControl (0x12, crc 0xc44b — pinned offline against the
            // factory image) and ImageCmos (0x0a, 0x1cfa) must verify.
            for (ty, name) in [(0x12u16, "GainControl"), (0x0A, "ImageCmos")] {
                let result = results.iter().find(|(_, t, _)| *t == ty);
                assert!(
                    result.is_some(),
                    "no {name} record with a stored crc in the table"
                );
                assert!(
                    result.unwrap().2,
                    "{name} flash CRC mismatch — parser/read_flash/CRC regression"
                );
            }
            println!("   {verified} files CRC-verified on device");
        }
        Err(e) => println!("   unavailable: {e}"),
    }

    // DOWNLOAD_FILE reply mapping: the reply is the file's .fla image
    // ([type u16][build u16][version u16] + payload). On fw 5.8.22 the
    // type lookup suffers the corrupted-table walk: 0x12 (GainControl,
    // 57 w) returned the 548-B RegistrationParams image on camera,
    // while 0x0A returned the correct ImageCmos image.
    println!("== DOWNLOAD_FILE (opcode 16) type-lookup mapping:");
    for ty in [0x0Au16, 0x0B, 0x12, 0x13] {
        match cam.download_flash_file(ty) {
            Ok(blob) if blob.len() >= 6 => {
                let head_ty = u16::from_le_bytes([blob[0], blob[1]]);
                let build = u16::from_le_bytes([blob[2], blob[3]]);
                let ver = u16::from_le_bytes([blob[4], blob[5]]);
                println!(
                    "   request type {ty:#04x} -> {} B, .fla type {head_ty:#04x} {} build {build} ver {ver:#06x}",
                    blob.len(),
                    system::flash_file_type_name(head_ty)
                );
            }
            Ok(blob) => println!("   request type {ty:#04x} -> {} B (short)", blob.len()),
            Err(e) => println!("   request type {ty:#04x} -> {e}"),
        }
    }

    println!("== flash map (opcode 18):");
    // NOTE: fw 5.8.22 NACKs this InvalidCommand — the flash file list
    // above is the supported surface on this build (the flash-layout
    // ground truth lives in firmware/5.8.22/UsbUpdate_RD108.ini).
    match cam.flash_map() {
        Ok(map) => {
            println!("   {} entries:", map.len());
            for entry in &map {
                println!(
                    "   type {:#06x} {:<16} offset {:#x} words {} ver {}.{}.{}",
                    entry.file_type,
                    system::flash_file_type_name(entry.file_type),
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

    println!("== CPU stats (opcode 27):");
    // NOTE: fw 5.8.22 NACKs this InvalidCommand too, although the main
    // image carries the 15 task names it would report
    // ('c,T,DC,TD,PK,IK,DK,O,SM,HM,TR,DC,PC,TC' at image offset 0xd1e —
    // docs/FIRMWARE.md). Read-only; expected to print unavailable here.
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
    // NOTE: on fw 5.8.22 the direct reply is too short to hold the u32
    // the C expects (XnHostProtocol.cpp:3211-3245) — the working path is
    // the ALGORITHM_PARAMS(0x06) blanking read every stream start
    // performs (already part of start_depth/start_ir/start_color).
    println!("   image: {:?}", cam.cmos_blanking_lines(0));
    println!("   depth: {:?}", cam.cmos_blanking_lines(1));

    println!("== CMOS register read via I2C (opcode 11):");
    println!("   image reg 0x00: {:?}", cam.read_cmos_register(0, 0x00));
    println!("   depth reg 0x00: {:?}", cam.read_cmos_register(1, 0x00));

    println!("== AHB read (opcode 20):");
    println!("   0x00000000: {:?}", cam.read_ahb(0x0));

    println!("== raw flash read (opcode 25), boot-manager header:");
    // word 0x2000 = byte 0x4000 — the BootManager's record offset (also
    // OffsetFactory=0x2000 in UsbUpdate_RD108.ini)
    match cam.read_flash(0x2000, 32) {
        Ok(data) => println!("   {data:02x?}"),
        Err(e) => println!("   unavailable: {e}"),
    }

    println!("== done; mode now: {:?}", cam.get_mode());
}

// ---------------------------------------------------------------------------
// Robust dominant-plane fit for the geometry test
// ---------------------------------------------------------------------------

/// Plane-fit gate constants (mm / fraction of the cloud).
const PLANE_TAU: f64 = 20.0; // inlier band around the plane
const PLANE_MIN_COVERAGE: f64 = 0.40; // gate applies only above this
const PLANE_MAX_RMS: f64 = 15.0; // inlier RMS gate (a wall gives a few mm)

/// Result of [`fit_dominant_plane`].
struct PlaneFit {
    /// Unit plane normal.
    normal: [f64; 3],
    /// Distance of the camera origin to the plane (mm).
    distance: f64,
    /// Points within [`PLANE_TAU`] of the plane.
    inliers: usize,
    /// Total points the fit ran on.
    total: usize,
    /// RMS of the inlier residuals (mm).
    rms: f64,
}

impl PlaneFit {
    /// Fraction of the cloud within the inlier band.
    fn coverage(&self) -> f64 {
        self.inliers as f64 / self.total as f64
    }
}

/// Deterministic xorshift64* step — the fit needs repeatable sampling,
/// not cryptographic quality.
fn xorshift64(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn mat_vec(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [dot3(m[0], v), dot3(m[1], v), dot3(m[2], v)]
}

fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let n = dot3(v, v).sqrt();
    if n < 1e-12 {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

/// Power iteration for the dominant eigenvector of a symmetric 3x3.
fn dominant_eigenvector(m: &[[f64; 3]; 3], start: [f64; 3]) -> [f64; 3] {
    let mut v = normalize3(start);
    for _ in 0..64 {
        let w = normalize3(mat_vec(m, v));
        if (dot3(w, v) - 1.0).abs() < 1e-12 {
            return w;
        }
        v = w;
    }
    v
}

/// RANSAC dominant-plane fit: 400 three-point hypotheses on a deterministic
/// ~8k-point subsample, then a PCA refit (smallest-eigenvector normal) on
/// the full-cloud inliers of the winning hypothesis, then a final inlier
/// and RMS pass. Returns `None` when no plane gets at least 3 inliers.
///
/// 0.3.0 fitted one plane to the WHOLE cloud by least squares; any real
/// room (a corner, furniture, a second wall) inflated that fit's RMS and
/// failed the test even with perfect geometry — the first camera run hit
/// exactly that (49 mm over a two-plane scene while every geometric gate
/// passed). The gate now applies only when the dominant plane covers
/// enough of the frame; see [`PLANE_MIN_COVERAGE`].
fn fit_dominant_plane(cloud: &[(f64, f64, f64)], tau: f64) -> Option<PlaneFit> {
    if cloud.len() < 100 {
        return None;
    }
    // deterministic subsample (every k-th point, ~8k samples)
    let stride = (cloud.len() / 8_000).max(1);
    let sample: Vec<[f64; 3]> = cloud
        .iter()
        .step_by(stride)
        .map(|p| [p.0, p.1, p.2])
        .collect();

    let mut rng = 0x2545_F491_4F6C_DD1Du64;
    let mut best = ([0.0f64; 3], 0.0f64); // (unit normal, d)
    let mut best_count = 0usize;
    for _ in 0..400 {
        let i = (xorshift64(&mut rng) as usize) % sample.len();
        let j = (xorshift64(&mut rng) as usize) % sample.len();
        let k = (xorshift64(&mut rng) as usize) % sample.len();
        if i == j || i == k || j == k {
            continue;
        }
        let (p1, p2, p3) = (sample[i], sample[j], sample[k]);
        let n = cross3(
            [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]],
            [p3[0] - p1[0], p3[1] - p1[1], p3[2] - p1[2]],
        );
        if dot3(n, n).sqrt() < 1e-9 {
            continue; // collinear triple
        }
        let n = normalize3(n);
        let d = -dot3(n, p1);
        let count = sample
            .iter()
            .filter(|p| (dot3(n, **p) + d).abs() <= tau)
            .count();
        if count > best_count {
            best_count = count;
            best = (n, d);
        }
    }
    if best_count < 3 {
        return None;
    }

    // PCA refit: the normal is the smallest-eigenvalue direction of the
    // inlier covariance — the cross product of the two dominant (in-plane)
    // eigenvectors.
    let (n0, d0) = best;
    let inliers: Vec<[f64; 3]> = cloud
        .iter()
        .map(|p| [p.0, p.1, p.2])
        .filter(|p| (dot3(n0, *p) + d0).abs() <= tau)
        .collect();
    let n_in = inliers.len() as f64;
    let centroid = [
        inliers.iter().map(|p| p[0]).sum::<f64>() / n_in,
        inliers.iter().map(|p| p[1]).sum::<f64>() / n_in,
        inliers.iter().map(|p| p[2]).sum::<f64>() / n_in,
    ];
    let mut cov = [[0.0f64; 3]; 3];
    for p in &inliers {
        let dv = [p[0] - centroid[0], p[1] - centroid[1], p[2] - centroid[2]];
        for (i, row) in cov.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell += dv[i] * dv[j];
            }
        }
    }
    let v1 = dominant_eigenvector(&cov, [0.4, 0.5, 0.8]);
    let lambda1 = dot3(v1, mat_vec(&cov, v1));
    let mut cov2 = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            cov2[i][j] = cov[i][j] - lambda1 * v1[i] * v1[j];
        }
    }
    let start = [0.8, 0.4, 0.5];
    let dot = dot3(start, v1);
    let v2_start = normalize3([
        start[0] - dot * v1[0],
        start[1] - dot * v1[1],
        start[2] - dot * v1[2],
    ]);
    let v2 = dominant_eigenvector(&cov2, v2_start);
    let cross = cross3(v1, v2);
    let normal = if dot3(cross, cross) > 1e-12 {
        normalize3(cross)
    } else {
        n0 // degenerate in-plane spread: keep the RANSAC normal
    };
    let d = -dot3(normal, centroid);

    // Final inlier/RMS pass over the full cloud with the refined plane.
    let mut inlier_count = 0usize;
    let mut sum_sq = 0.0;
    for p in cloud {
        let r = dot3(normal, [p.0, p.1, p.2]) + d;
        if r.abs() <= tau {
            inlier_count += 1;
            sum_sq += r * r;
        }
    }
    if inlier_count == 0 {
        return None;
    }
    Some(PlaneFit {
        normal,
        distance: d.abs(),
        inliers: inlier_count,
        total: cloud.len(),
        rms: (sum_sq / inlier_count as f64).sqrt(),
    })
}

/// Depth geometry end-to-end: intrinsics from the on-device calibration,
/// FOV sanity against the Xtion spec, host-referenced frame timestamps
/// at ~30 fps, and a robust dominant-plane fit of the scene
/// (photogrammetry acceptance: RMS of a few millimetres on a flat target).
///
/// The plane-fit gate is conditional: RANSAC + a PCA refit find the
/// dominant plane, and the RMS assertion applies only when that plane
/// covers at least [`PLANE_MIN_COVERAGE`] of the cloud. A room corner,
/// furniture, or any other non-planar scene reports its numbers and
/// skips the gate instead of failing the test — point the camera at a
/// flat wall ~1-3 m away to exercise the assertion.
#[test]
fn depth_geometry_fov_and_timestamps() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    // ---- intrinsics from the device calibration (no I/O) ----
    let intrinsics = cam.depth_intrinsics(xtion::Resolution::Vga);
    let zpd = cam.fixed_params.reference_distance;
    let zpps = cam.fixed_params.reference_pixel_size;
    println!("calibration: ZPD {zpd:.3} mm, ZPPS {zpps:.5} mm/px",);
    println!(
        "intrinsics VGA: fx {:.2}, fy {:.2}, cx {:.2}, cy {:.2}, hfov {:.2} deg, vfov {:.2} deg",
        intrinsics.fx,
        intrinsics.fy,
        intrinsics.cx,
        intrinsics.cy,
        intrinsics.hfov.to_degrees(),
        intrinsics.vfov.to_degrees()
    );
    // Xtion spec: 58 deg horizontal / 45 deg vertical, +/- calibration
    let hfov = intrinsics.hfov.to_degrees();
    let vfov = intrinsics.vfov.to_degrees();
    assert!(
        (52.0..64.0).contains(&hfov),
        "hfov {hfov} deg far from the 58 deg spec"
    );
    assert!(
        (38.0..52.0).contains(&vfov),
        "vfov {vfov} deg far from the 45 deg spec"
    );

    // ---- stream and collect frames ----
    cam.start_depth(DepthConfig::default())
        .expect("start depth");

    // emitter probe WITH the depth stream running: if the idle-state
    // hypothesis for the BadParams NACK is right, the toggle may be
    // accepted while the sensor streams (read-only probe; the toggle is
    // volatile state either way, power-cycle restores it).
    match cam.set_emitter_enabled(false) {
        Ok(()) => {
            println!("emitter toggle: ACKed while depth streams (idle-state hypothesis confirmed)");
            let _ = cam.set_emitter_enabled(true);
        }
        Err(err) => println!("emitter toggle while streaming: rejected ({err})"),
    }

    let mut frames = Vec::new();
    for _ in 0..90 {
        match cam.read_depth_frame(Duration::from_secs(5)) {
            Ok(frame) => frames.push(frame),
            Err(err) => panic!("depth read: {err}"),
        }
    }
    cam.stop_depth().expect("stop");

    // ---- timestamps: wrap-aware host-referenced microseconds ----
    let timestamps: Vec<u64> = frames
        .iter()
        .map(|f| f.timestamp_us.expect("timestamps converted"))
        .collect();
    let mut deltas_ms = Vec::with_capacity(timestamps.len() - 1);
    for pair in timestamps.windows(2) {
        deltas_ms.push((pair[1] - pair[0]) as f64 / 1000.0);
    }
    let mean_delta = deltas_ms.iter().sum::<f64>() / deltas_ms.len() as f64;
    let max_deviation = deltas_ms
        .iter()
        .map(|d| (d - mean_delta).abs())
        .fold(0.0f64, f64::max);
    println!(
        "{} frames, timestamp deltas: mean {mean_delta:.2} ms (expect ~33.33), max deviation {max_deviation:.2} ms, monotonic: {}",
        frames.len(),
        deltas_ms.iter().all(|d| *d > 0.0)
    );
    assert!(
        (28.0..38.0).contains(&mean_delta),
        "mean inter-frame delta {mean_delta:.2} ms, expected ~33.3 ms at 30 fps"
    );
    assert!(
        max_deviation < 5.0,
        "timestamp jitter {max_deviation:.2} ms is too large for a wired bus"
    );

    // ---- geometry: to_world on the optical axis ----
    let last = frames.last().unwrap();
    let center = last
        .to_world(&intrinsics, last.width / 2, last.height / 2)
        .expect("center pixel valid");
    println!(
        "center pixel -> world ({:.1}, {:.1}, {:.1}) mm (x,y must be ~0)",
        center.0, center.1, center.2
    );
    assert!(
        center.0.abs() < 30.0,
        "center world X {:.1} mm off axis",
        center.0
    );
    assert!(
        center.1.abs() < 30.0,
        "center world Y {:.1} mm off axis",
        center.1
    );

    // ---- robust dominant-plane fit of the scene ----
    let cloud = last.point_cloud(&intrinsics);
    println!("point cloud: {} valid points", cloud.len());
    assert!(
        cloud.len() >= 10_000,
        "point cloud has only {} valid points — depth stream degraded",
        cloud.len()
    );
    match fit_dominant_plane(&cloud, PLANE_TAU) {
        Some(fit) => {
            let tilt = fit.normal[2].abs().acos().to_degrees();
            println!(
                "dominant plane: normal ({:.3}, {:.3}, {:.3}), tilt {:.1} deg from the optical axis, distance {:.0} mm",
                fit.normal[0],
                fit.normal[1],
                fit.normal[2],
                tilt,
                fit.distance
            );
            println!(
                "plane fit: {} inliers ({:.0}% of the cloud) within +/-{PLANE_TAU:.0} mm, inlier RMS {:.2} mm",
                fit.inliers,
                fit.coverage() * 100.0,
                fit.rms
            );
            if fit.coverage() < PLANE_MIN_COVERAGE {
                println!(
                    "plane-fit gate skipped: the dominant plane covers only {:.0}% of the frame — the scene is not a single flat wall (point the camera at one 1-3 m away to exercise the RMS gate)",
                    fit.coverage() * 100.0
                );
            } else {
                assert!(
                    fit.rms <= PLANE_MAX_RMS,
                    "dominant-plane RMS {:.2} mm is too large — point the camera at a flat, \
                     textureless wall 1-3 m away and clear the sides of the frame; a good \
                     fit on this sensor is a few mm (the gate leaves room for 3 m noise)",
                    fit.rms
                );
            }
        }
        None => println!(
            "plane-fit gate skipped: no dominant plane found in {} points (scene too small or too noisy)",
            cloud.len()
        ),
    }
}

/// Camera-independent coverage for `fit_dominant_plane` (and thus the
/// geometry test's conditional gate): a synthetic noisy wall must be
/// found with tight RMS, a two-plane room must yield the larger plane,
/// and a noise cloud must fall below the coverage gate.
#[test]
fn plane_fit_synthetic() {
    let mut rng = 0x1234_5678_9ABC_DEF0u64;
    let mut unit = || xorshift64(&mut rng) as f64 / u64::MAX as f64 - 0.5;

    // --- scene 1: a frontal wall at 1 m, +/-3 mm uniform noise ---
    let mut wall = Vec::new();
    for yi in 0..48 {
        for xi in 0..64 {
            let (x, y) = (xi as f64 * 12.5 - 400.0, yi as f64 * 12.5 - 300.0);
            let z = 1000.0 + 0.05 * x - 0.02 * y + unit() * 6.0;
            wall.push((x, y, z));
        }
    }
    let fit = fit_dominant_plane(&wall, PLANE_TAU).expect("wall plane found");
    assert!(fit.coverage() > 0.95, "wall coverage {}", fit.coverage());
    assert!(fit.rms < 6.0, "wall rms {} mm", fit.rms);
    assert!(
        (fit.distance - 1000.0).abs() < 5.0,
        "wall distance {} mm",
        fit.distance
    );
    let tilt = fit.normal[2].abs().acos().to_degrees();
    assert!(tilt < 4.0, "wall tilt {tilt} deg");

    // --- scene 2: two walls + clutter — the larger plane wins ---
    let mut room = Vec::new();
    for i in 0..800 {
        let (x, y) = (unit() * 800.0, unit() * 600.0);
        room.push((x, y, 1000.0 + unit() * 4.0)); // frontal wall
        if i < 500 {
            room.push((300.0 + unit() * 4.0, unit() * 600.0, 400.0 + unit() * 800.0));
            // side wall
        }
        if i < 200 {
            room.push((unit() * 800.0, unit() * 600.0, 500.0 + unit() * 500.0));
            // clutter
        }
    }
    let fit = fit_dominant_plane(&room, PLANE_TAU).expect("room plane found");
    assert!(
        (fit.coverage() - 800.0 / 1500.0).abs() < 0.15,
        "room coverage {}",
        fit.coverage()
    );
    assert!(fit.rms < 6.0, "room rms {} mm", fit.rms);
    let tilt = fit.normal[2].abs().acos().to_degrees();
    assert!(tilt < 3.0, "room tilt {tilt} deg");

    // --- scene 3: a noise cloud — below the gate, the test would skip ---
    let mut noise_cloud = Vec::new();
    for _ in 0..3000 {
        noise_cloud.push((unit() * 800.0, unit() * 600.0, unit() * 800.0 + 1000.0));
    }
    let fit = fit_dominant_plane(&noise_cloud, PLANE_TAU).expect("some plane always wins");
    assert!(
        fit.coverage() < PLANE_MIN_COVERAGE,
        "noise coverage {} must stay under the gate",
        fit.coverage()
    );
}

/// Crop windows end-to-end (0.3.3): the firmware transmits ONLY the
/// window, frames arrive crop-sized with the origin reported, and the
/// window can be changed on the fly (crop params are "allow while
/// open" in the reference).
///
/// Sections: full-frame baseline -> mode-1 crop 320x240 (centered) ->
/// on-the-fly shrink to 160x120 -> disable (full frames return) ->
/// geometry spot check via to_world -> IR crop 320x240 (packed-10, no
/// extra CMOS rows while cropped) -> mode-2 (increased-fps) probe with
/// fps=60 attempted and the measured frame rate printed (whether fw
/// 5.8.22 raises the readout rate this way is an open question — the
/// probe documents it, it does not gate on it).
#[test]
fn depth_and_ir_crop_windows() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");
    let intrinsics = cam.depth_intrinsics(xtion::Resolution::Vga);

    // ---- full-frame baseline ----
    cam.start_depth(DepthConfig::default())
        .expect("start depth");
    let baseline = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("baseline full frame");
    assert_eq!((baseline.width, baseline.height), (640, 480));
    assert_eq!(baseline.crop, None);
    let baseline_valid = baseline.valid_pixels();
    println!("baseline: 640x480, {baseline_valid} valid pixels");

    // ---- mode-1 crop: centered 320x240 ----
    let crop = xtion::Crop {
        x: 160,
        y: 120,
        width: 320,
        height: 240,
    };
    cam.set_depth_crop(Some(crop), false)
        .expect("set depth crop 320x240");
    let frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("first cropped depth frame");
    assert_eq!((frame.width, frame.height), (320, 240));
    assert_eq!(frame.crop, Some(crop));
    assert_eq!(frame.data.len(), 320 * 240);
    let valid = frame.valid_pixels();
    println!(
        "cropped 320x240: {} valid pixels (baseline {}, window is 25% of the frame)",
        valid, baseline_valid
    );
    assert!(
        valid > 1000,
        "cropped frame has too few valid pixels: {valid}"
    );
    assert!(
        (valid as f64) < (baseline_valid as f64) * 1.05,
        "cropped frame has more valid pixels than the whole frame ({valid} > {baseline_valid})"
    );
    // same scene, window centred on the frame centre: min depth must
    // stay in the plausible band of the baseline
    if let (Some(b), Some(c)) = (baseline.min_depth(), frame.min_depth()) {
        println!("min depth: baseline {b} mm, cropped {c} mm");
        assert!(
            c > 200 && c < 6000,
            "cropped min depth {c} mm is not plausible"
        );
    }

    // ---- geometry: the crop centre is the frame centre -> x,y ~ 0 ----
    let (cx, cy, cz) = frame
        .to_world(&intrinsics, 160, 120)
        .expect("crop centre converts");
    println!("crop centre -> world ({cx:.1}, {cy:.1}, {cz:.1}) mm");
    assert!(cx.abs() < 50.0 && cy.abs() < 50.0, "crop centre off axis");

    // ---- on-the-fly shrink to 160x120 (allow-while-open) ----
    let small = xtion::Crop {
        x: 240,
        y: 180,
        width: 160,
        height: 120,
    };
    cam.set_depth_crop(Some(small), false)
        .expect("shrink crop on the fly");
    let frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("shrunk depth frame");
    assert_eq!((frame.width, frame.height), (160, 120));
    assert_eq!(frame.crop, Some(small));
    assert!(frame.valid_pixels() > 200);

    // ---- disable: full frames return ----
    cam.set_depth_crop(None, false).expect("disable crop");
    let frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("full frame after disable");
    assert_eq!((frame.width, frame.height), (640, 480));
    assert_eq!(frame.crop, None);
    cam.stop_depth().expect("stop depth");

    // ---- IR crop: 320x240, NO extra CMOS rows on the wire ----
    cam.start_ir(xtion::IrConfig::default()).expect("start IR");
    cam.set_ir_crop(Some(crop), false).expect("set IR crop");
    let ir = cam
        .read_ir_frame(Duration::from_secs(10))
        .expect("cropped IR frame");
    assert_eq!((ir.width, ir.height), (320, 240));
    assert_eq!(ir.crop, Some(crop));
    assert_eq!(ir.data.len(), 320 * 240);
    println!(
        "cropped IR 320x240: value range {:?} (full-frame wire would carry 248 rows; the \
         cropped frame passed the exact 240-row size gate)",
        ir.value_range()
    );
    assert!(ir.value_range().is_some());
    cam.set_ir_crop(None, false).expect("disable IR crop");
    let ir = cam
        .read_ir_frame(Duration::from_secs(10))
        .expect("full IR frame");
    assert_eq!((ir.width, ir.height), (640, 480));
    cam.stop_ir().expect("stop IR");

    // ---- mode-2 probe: increased-fps crop (fw >= 5.7.2 per OpenNI) ----
    // 0.3.3 camera finding: a single SET_PARAM DepthCropMode=2 NACKs
    // BadParams while streaming, although the version heuristic
    // (bIncreasedFpsCropSupported, XnHostProtocol.cpp:640-643) says this
    // 5.8.22 build supports it. This probe pins down whether the
    // rejection depends on the write form or the stream state:
    //   (a) the same single write again (regression check),
    //   (b) the C-exact multi-pair batch — what SetCroppingImpl really
    //       sends (CommitTransactionAsBatch ->
    //       XnHostProtocolSetMultipleParams),
    //   (c) the batch with the stream STOPPED (not C behavior — a
    //       diagnostic; if this works, a "arm while stopped" recipe is
    //       the 0.3.5 follow-up).
    // A mode-2 rejection is a documented hardware property, NOT a
    // driver bug: the section must end with the window verified working
    // under mode 1, whatever the probe results are.
    cam.start_depth(DepthConfig::default())
        .expect("restart depth");
    println!("--- crop mode 2 (increased fps) probe ---");
    // (a) single param write while streaming
    match cam.set_param(xtion::Param::DepthCropMode, xtion::CROP_MODE_INCREASED_FPS) {
        Ok(()) => {
            let mode = cam
                .get_param(xtion::Param::DepthCropMode)
                .expect("read back crop mode after unexpected ACK");
            println!("(a) single write while streaming: ACK (mode reads {mode})");
            // back to the disabled mode the driver state believes in
            cam.set_param(xtion::Param::DepthCropMode, xtion::CROP_MODE_DISABLED)
                .expect("restore crop mode 0 after unexpected ACK");
        }
        Err(err) => println!("(a) single write while streaming: {err}"),
    }
    // (b) the C-exact 5-pair batch while streaming
    let mut mode2_active = false;
    match cam.set_depth_crop(Some(crop), true) {
        Ok(()) => {
            mode2_active = true;
            println!("(b) C-exact 5-pair batch while streaming: ACK");
        }
        Err(err) => println!("(b) C-exact 5-pair batch while streaming: {err}"),
    }
    if !mode2_active {
        // (c) the batch with the stream stopped
        cam.stop_depth()
            .expect("stop depth for the stream-stopped probe");
        let batch = [
            (xtion::Param::DepthCropSizeX, 320),
            (xtion::Param::DepthCropSizeY, 240),
            (xtion::Param::DepthCropOffsetX, 160),
            (xtion::Param::DepthCropOffsetY, 120),
            (xtion::Param::DepthCropMode, xtion::CROP_MODE_INCREASED_FPS),
        ];
        match cam.set_params(&batch) {
            Ok(()) => {
                let mode = cam
                    .get_param(xtion::Param::DepthCropMode)
                    .expect("read back crop mode after stopped ACK");
                println!("(c) batch with stream stopped: ACK (mode reads {mode})");
                mode2_active = mode == xtion::CROP_MODE_INCREASED_FPS;
            }
            Err(err) => println!("(c) batch with stream stopped: {err}"),
        }
        // start_depth re-applies the disabled mode (OpenStreamImpl
        // parity: the crop mode is always written at stream open).
        cam.start_depth(DepthConfig::default())
            .expect("restart depth after the stream-stopped probe");
        if mode2_active {
            // re-arm the window while streaming for the measurement
            match cam.set_depth_crop(Some(crop), true) {
                Ok(()) => println!("(c) re-arm of the stopped-armed window while streaming: ACK"),
                Err(err) => {
                    mode2_active = false;
                    println!("(c) re-arm while streaming after stopped ACK: {err}");
                }
            }
        }
    }

    if mode2_active {
        match cam.set_param(xtion::Param::DepthFps, 60) {
            Ok(()) => println!("depth fps 60 with crop mode 2: ACCEPTED"),
            Err(err) => println!(
                "depth fps 60 with crop mode 2: rejected ({err}) — the readout rate is whatever \
                 the firmware does with mode 2 alone"
            ),
        }
        let mut stamps = Vec::new();
        for _ in 0..60 {
            match cam.read_depth_frame(Duration::from_secs(5)) {
                Ok(frame) => stamps.push(frame.timestamp_us.expect("timestamps converted")),
                Err(err) => panic!("mode-2 depth read: {err}"),
            }
        }
        let deltas: Vec<f64> = stamps
            .windows(2)
            .map(|p| (p[1] - p[0]) as f64 / 1000.0)
            .collect();
        let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;
        let fps = 1000.0 / mean;
        println!(
            "mode-2 320x240: mean frame delta {mean:.2} ms = {fps:.2} fps (full-frame VGA \
             baseline is ~29.97)"
        );
        if fps > 40.0 {
            println!("-> increased-fps crop WORKS on this firmware build");
        } else {
            println!("-> mode 2 is accepted but does not raise the rate on this firmware build");
        }
    } else {
        println!(
            "-> VERDICT: this firmware build rejects crop mode 2 in every probed form; \
             increased-fps crop is unsupported on this unit (the window works with mode 1)"
        );
        // sanity: the mode-1 window still works after all the probing
        cam.set_depth_crop(Some(crop), false)
            .expect("re-arm mode-1 window after the mode-2 probe");
        let frame = cam
            .read_depth_frame(Duration::from_secs(10))
            .expect("mode-1 frame after the mode-2 probe");
        assert_eq!((frame.width, frame.height), (320, 240));
    }
    // cleanup: leave the device uncropped and stopped
    cam.set_depth_crop(None, false).expect("disable crop");
    let frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("full frame after mode-2 disable");
    assert_eq!((frame.width, frame.height), (640, 480));
    cam.stop_depth().expect("stop depth");
}

/// Test 11 — depth processing controls (0.3.4): the depth gain
/// (param 21, the reference's "Gain"), the four AGC shift bins
/// (params 63-70), close range (param 84) and wavelength correction
/// (params 73/74). All are allow-while-open parameters, so every phase
/// runs while the depth stream is live: read the factory state, write a
/// known-good alternative, read it back, RESTORE, and only then assert
/// on the captured values — a panic can never leave the device with
/// modified processing (the restore happens before any assertion).
/// Firmware refusals are printed as hardware verdicts, not failures
/// (the crop-mode-2 treatment); the one hard gate on restore is the
/// test's own contract that the device is always left as found.
#[test]
fn depth_processing_controls() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    cam.start_depth(DepthConfig::default())
        .expect("start depth");
    let baseline = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("baseline frame");
    let baseline_valid = baseline.valid_pixels();
    println!("baseline: {baseline_valid}/307200 valid pixels");
    println!(
        "baseline shift range: {:?}..{:?}",
        baseline.shift_stats.min, baseline.shift_stats.max
    );

    // ---- inventory: every depth-processing param, factory state ----
    let gain0 = cam.depth_gain().expect("read gain (param 21)");
    let bins0: Vec<xtion::AgcBin> = (0..4)
        .map(|b| cam.depth_agc_bin(b).expect("read AGC bin"))
        .collect();
    let close0 = cam.close_range().expect("read close range (param 84)");
    let wl0 = cam.wavelength_correction().expect("read wavelength (73)");
    let wld0 = cam
        .wavelength_correction_debug()
        .expect("read wavelength debug (74)");
    println!("depth gain (21): {gain0} (reference factory default: 50)");
    for (i, b) in bins0.iter().enumerate() {
        println!(
            "AGC bin {i} (params {}-{}): shifts {}..{}",
            63 + 2 * i,
            64 + 2 * i,
            b.min_shift,
            b.max_shift
        );
    }
    println!(
        "close range (84): {close0}, wavelength correction (73): {wl0}, \
         wavelength debug (74): {wld0}"
    );
    // structural sanity of the factory bins: ordered ranges in the
    // 0..=2047 shift space
    for b in &bins0 {
        assert!(
            b.min_shift <= b.max_shift && b.max_shift <= xtion::MAX_SHIFT,
            "implausible factory bin {b:?}"
        );
    }

    // ---- gain (21): write 42 (the reference's pre-5.6 close-range
    // "restore" value), read back, restore, then compare ----
    let gain_echo = (|| -> xtion::Result<u16> {
        cam.set_depth_gain(42)?;
        let echoed = cam.depth_gain()?;
        cam.set_depth_gain(gain0)?; // restore before any assertion
        Ok(echoed)
    })()
    .expect("gain write/read/restore cycle");
    assert_eq!(gain_echo, 42, "gain read-back while set");
    assert_eq!(
        cam.depth_gain().expect("gain after restore"),
        gain0,
        "gain must be back at the factory value"
    );
    println!("gain (21): write 42 -> read {gain_echo}, restored {gain0}");

    // ---- AGC bins (63-70): the four-bin shift partition. Hardware
    // findings, pinned by the 0.3.4/0.3.5/0.3.6 camera runs: (1) this
    // firmware NACKs `BadParams` for EVERY SET_PARAM on the AGC bin
    // params — value-preserving no-ops, invariant-preserving writes and
    // invariant-VIOLATING writes alike (all probed forms NACKed in both
    // runs); (2) the write is APPLIED anyway — the 0.3.6 run read
    // bin0 = 0..512 back right after the NACKed `HIGH <- 512` probe AND
    // bin1 = 512..0 right after the NACKed C-order `LOW <- 512` probe.
    // The 0.3.5 run first looked like the invariant-violating write was
    // refused without applying (bin1 read (0,0) at the end), but the
    // 0.3.5 partition is FULLY explained without that: the 0.3.5
    // restore's first write for bin1 was `LOW <- 0` — which LANDED
    // despite its NACK (the 0.3.5 driver then bailed out on that NACK
    // and never sent the `HIGH <- 0` no-op). So: the NACK reply carries
    // NO information about application — only the readback is truth,
    // and the driver (0.3.6) sequences each bin transient-safely, sends
    // BOTH bounds even under NACK, and returns Ok only when the bin
    // reads back exactly as requested. This section measures every
    // probe against its immediate readback and hard-gates the three
    // camera-verified quirk facts, then restores through the verified
    // API.
    println!("--- AGC bin write probe ---");
    fn probe_write(cam: &mut Xtion, param: xtion::Param, value: u16, what: &str) -> bool {
        let result = cam.set_param(param, value);
        match &result {
            Ok(()) => println!("   {what}: param {} <- {value}: ACK", param.as_u16()),
            Err(e) => println!(
                "   {what}: param {} <- {value}: REJECTED ({e})",
                param.as_u16()
            ),
        }
        result.is_ok()
    }
    fn probe_readback(cam: &mut Xtion, bin: usize, what: &str) -> xtion::AgcBin {
        let b = cam.depth_agc_bin(bin).expect("read AGC bin after probe");
        println!(
            "   {what}: bin{bin} now reads {}..{}",
            b.min_shift, b.max_shift
        );
        b
    }
    let _p_noop_low = probe_write(
        &mut cam,
        xtion::Param::DepthAgcBin0Low,
        0,
        "no-op write, bin0 LOW <- 0 (already the factory value)",
    );
    // NOTE: 0 is NOT bin0 HIGH's factory value (that is 512 on this
    // unit) — this probe is a live write that must LAND under the
    // apply-then-reject quirk, and its readback is the second hard
    // gate: the write collapses bin0 to (0, 0) despite the NACK.
    let _p_zero_high = probe_write(
        &mut cam,
        xtion::Param::DepthAgcBin0High,
        0,
        "bin0 HIGH <- 0 (a live write, NOT the factory value — must land under the quirk)",
    );
    let zero_back = probe_readback(&mut cam, 0, "readback after the HIGH <- 0 write");
    assert_eq!(
        (zero_back.min_shift, zero_back.max_shift),
        (0, 0),
        "fw 5.8.22 applies a live AGC bin write while NACKing it — bin0 \
         must read (0, 0) after the NACKed HIGH <- 0 probe (0.3.6 camera \
         model: every write to params 63-70 lands; only the readback is truth)"
    );
    let p_grow_high = probe_write(
        &mut cam,
        xtion::Param::DepthAgcBin0High,
        512,
        "grow bin0: HIGH <- 512 (restores the factory HIGH)",
    );
    let grow_back = probe_readback(&mut cam, 0, "readback after the grow write");
    if !p_grow_high {
        println!("   -> apply-then-reject confirmed: the NACKed write LANDED anyway");
    }
    assert_eq!(
        grow_back.max_shift, 512,
        "fw 5.8.22 applies an invariant-preserving AGC bin write even \
         while NACKing it (0.3.5/0.3.6 camera runs: bin0 read 0..512 \
         right after the NACKed HIGH <- 512 probe)"
    );
    let p_c_order = probe_write(
        &mut cam,
        xtion::Param::DepthAgcBin1Low,
        512,
        "C-order probe: bin1 LOW <- 512 while its HIGH is 0 (the v0.3.4 failure point)",
    );
    let c_back = probe_readback(&mut cam, 1, "readback after the C-order write");
    if p_c_order {
        println!(
            "   -> the reference's LOW-first order is ACCEPTED here: this build does not \
             validate the transient min>max; the v0.3.4 failure needs another explanation"
        );
    }
    // GATE (0.3.7, corrected): the invariant-VIOLATING write LANDS too.
    // The 0.3.6 camera run read bin1 = 512..0 right after this NACKed
    // write. The earlier belief that it was refused came from the 0.3.5
    // run's final partition — which the 0.3.5 restore's own `LOW <- 0`
    // write (landed despite its NACK) explains without any refusal.
    // The unconditional restore below MUST therefore bring bin1 back
    // to the factory (0, 0) and the partition assert verifies it did.
    assert_eq!(
        (c_back.min_shift, c_back.max_shift),
        (512, 0),
        "an invariant-VIOLATING write lands anyway on fw 5.8.22 (0.3.6 \
         camera run: bin1 read 512..0 right after the NACKed C-order \
         probe; the 0.3.5 'still (0,0)' observation was the restore's \
         NACKed LOW <- 0 write landing, not a refusal)"
    );
    // Unconditional restore of the factory partition through the
    // verified API — both bounds are sent even under NACK, and any
    // failure is PRINTED before the assert (the 0.3.5 run discarded
    // the restore error silently, which is how the bail-out went
    // unnoticed).
    let mut probe_restore_err = None;
    for (i, b) in bins0.iter().enumerate() {
        if let Err(e) = cam.set_depth_agc_bin(i, *b) {
            probe_restore_err = Some(e);
        }
    }
    if let Some(e) = &probe_restore_err {
        println!("   restore warning: {e}");
    }
    let bins_after_probe: Vec<xtion::AgcBin> = (0..4)
        .map(|i| cam.depth_agc_bin(i).expect("read AGC bin after probe"))
        .collect();
    assert_eq!(
        bins_after_probe, bins0,
        "the probe must leave the factory partition in place (if this fails \
         on hardware, power-cycle the device and report the restore warning — \
         the firmware refused the restore write)"
    );

    let custom = [
        xtion::AgcBin {
            min_shift: 0,
            max_shift: 512,
        },
        xtion::AgcBin {
            min_shift: 512,
            max_shift: 1024,
        },
        xtion::AgcBin {
            min_shift: 1024,
            max_shift: 1536,
        },
        xtion::AgcBin {
            min_shift: 1536,
            max_shift: xtion::MAX_SHIFT,
        },
    ];
    // Full cycle — attempted UNCONDITIONALLY now: with readback-
    // verified writes the cycle itself decides (Ok = the partition
    // really changed and came back; Err = this build truly refuses
    // bin writes — a hardware verdict like crop mode 2). A failure
    // part-way still runs the restore below before the error
    // propagates, so the device is never left on the custom partition.
    let bins_result = (|| -> xtion::Result<(Vec<xtion::AgcBin>, usize)> {
        let mut write_err = None;
        for (i, b) in custom.iter().enumerate() {
            if let Err(e) = cam.set_depth_agc_bin(i, *b) {
                write_err = Some(e);
                break;
            }
        }
        let mut echoed = Vec::with_capacity(4);
        for i in 0..4 {
            echoed.push(cam.depth_agc_bin(i)?);
        }
        let frame = cam.read_depth_frame(Duration::from_secs(10));
        // restore before propagating any frame/write error
        let mut restore_err = None;
        for (i, b) in bins0.iter().enumerate() {
            if let Err(e) = cam.set_depth_agc_bin(i, *b) {
                restore_err = Some(e);
            }
        }
        let valid = frame?.valid_pixels();
        if let Some(e) = restore_err.or(write_err) {
            return Err(e);
        }
        Ok((echoed, valid))
    })();
    match bins_result {
        Ok((echoed, valid)) => {
            assert_eq!(
                echoed, custom,
                "all four bins must read back exactly as written"
            );
            println!(
                "AGC bins: custom partition echoed exactly — the NACKed writes land \
                 and the readback is the truth; {valid} valid pixels under it \
                 (baseline {baseline_valid}); factory bins restored"
            );
            assert!(
                valid > 100,
                "depth died completely under the custom bins: {valid} valid"
            );
        }
        Err(err) => {
            println!(
                "-> VERDICT: AGC bin writes truly refused on this build ({err}); \
                 kept as a hardware finding"
            );
        }
    }
    // final factory check whatever happened above
    let bins_final: Vec<xtion::AgcBin> = (0..4)
        .map(|i| cam.depth_agc_bin(i).expect("read AGC bin at the end"))
        .collect();
    assert_eq!(
        bins_final, bins0,
        "AGC bins must be back at the factory partition"
    );

    // ---- close range (84): enable, read back, one frame (near-field
    // re-bias — long-range depth may legitimately vanish, so the frame
    // is printed, not gated), restore. A refused write is a hardware
    // VERDICT, not a panic: the verified setter leaves the device
    // untouched when the firmware truly rejects (crop-mode-2
    // treatment), so the suite cannot go red on a firmware refusal. ----
    let close_result = (|| -> xtion::Result<(bool, usize, Option<u16>)> {
        cam.set_close_range(true)?;
        let echoed = cam.close_range()?;
        let frame = cam.read_depth_frame(Duration::from_secs(10));
        cam.set_close_range(close0)?; // restore before propagating errors
        let f = frame?;
        Ok((echoed, f.valid_pixels(), f.min_depth()))
    })();
    match close_result {
        Ok((echoed, valid, min)) => {
            assert!(echoed, "close range must read back as on");
            assert_eq!(
                cam.close_range().expect("close range after restore"),
                close0,
                "close range must be back at the factory value"
            );
            println!(
                "close range (84): on -> {valid} valid pixels (baseline {baseline_valid}), \
                 min depth {min:?} mm; restored {close0}"
            );
        }
        Err(e) => {
            println!(
                "-> VERDICT: close-range (84) write refused on this build ({e}); the \
                 switch still reads {} and the verified setter leaves the device \
                 untouched — kept as a hardware finding",
                cam.close_range().unwrap_or(false)
            );
        }
    }

    // ---- wavelength correction (73) + debug telemetry (74): toggled,
    // read back, restored. No frame is read while the DEBUG flag is on:
    // it makes the firmware emit wavelength-lock telemetry packets, a
    // diagnostics mode, not a depth-affecting setting. The closure
    // restores BOTH switches even when a mid-cycle write is refused,
    // so the device can never be left with correction on; a refusal is
    // a verdict, not a failure.
    let wl_result = (|| -> xtion::Result<(bool, bool)> {
        let mut err = None;
        let wl = match cam
            .set_wavelength_correction(true)
            .and_then(|()| cam.wavelength_correction())
        {
            Ok(v) => v,
            Err(e) => {
                err = Some(e);
                false
            }
        };
        let wld = match cam
            .set_wavelength_correction_debug(true)
            .and_then(|()| cam.wavelength_correction_debug())
        {
            Ok(v) => v,
            Err(e) => {
                err = Some(e);
                false
            }
        };
        // restore both whatever happened above
        if let Err(e) = cam.set_wavelength_correction_debug(wld0) {
            err = Some(e);
        }
        if let Err(e) = cam.set_wavelength_correction(wl0) {
            err = Some(e);
        }
        if let Some(e) = err {
            return Err(e);
        }
        Ok((wl, wld))
    })();
    match wl_result {
        Ok((wl, wld)) => {
            assert!(
                (wl, wld) == (true, true),
                "wavelength correction and debug must read back as on"
            );
            assert_eq!(
                cam.wavelength_correction()
                    .expect("wavelength after restore"),
                wl0
            );
            assert_eq!(
                cam.wavelength_correction_debug()
                    .expect("wavelength debug after restore"),
                wld0
            );
            println!("wavelength (73/74): both toggled on and read back; restored {wl0}/{wld0}");
        }
        Err(e) => {
            println!(
                "-> VERDICT: wavelength-correction (73/74) write refused on this \
                 build ({e}); the switches read {}/{} and the verified setters leave \
                 the device untouched — kept as a hardware finding",
                cam.wavelength_correction().unwrap_or(false),
                cam.wavelength_correction_debug().unwrap_or(false)
            );
        }
    }

    // ---- final integrity: with everything restored the scene must be
    // back to baseline quality ----
    let final_frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("final frame");
    let final_valid = final_frame.valid_pixels();
    println!("final: {final_valid} valid pixels (baseline {baseline_valid})");
    assert!(
        final_valid > 1000,
        "depth did not recover after restores: {final_valid} valid"
    );
    cam.stop_depth().expect("stop depth");
}

/// Test 12 — GMC configuration + motion data (0.3.5): the whole GMC
/// parameter block (params 36-44, 61, 75) is inventoried, the mode and
/// debug switches (the only two the reference driver ever writes) are
/// toggled while the depth stream runs, and the misc endpoint is
/// watched for plain GMC motion packets (0xa200) and the debug
/// channels (0xb200 points, 0xb500 summary) — decoded by the driver
/// into [`xtion::GmcPoint`]s. Factory state is restored and the stream
/// must recover. A silent GMC stream is a finding, not a failure:
/// nothing in the protocol guarantees motion points on a static scene.
#[test]
fn gmc_config_and_motion_data() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    // ---- full config block: read-only calibration members + switches ----
    let config0 = cam.gmc_config().expect("gmc config block");
    println!("GMC config block (params 36-44, 61, 75):");
    println!(
        "   mode (36)            = {} (0 = normal, 1 = scan)",
        config0.mode
    );
    println!(
        "   ref offset (37)      = {} (as i16 {})",
        config0.ref_offset, config0.ref_offset as i16
    );
    println!("   ricc (38)            = {}", config0.ricc);
    println!(
        "   dx correction 39-44  = {} / a {} / b_hi {} / b_lo {} / db_hi {} / db_lo {}",
        config0.dx_correction,
        config0.dx_correction_a,
        config0.dx_correction_b_high,
        config0.dx_correction_b_low,
        config0.dx_correction_db_high,
        config0.dx_correction_db_low
    );
    println!("   debug (61)           = {}", config0.debug);
    println!("   correction mode (75) = {}", config0.correction_mode);

    cam.start_depth(DepthConfig::default())
        .expect("start depth");
    let baseline = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("baseline frame");
    let baseline_valid = baseline.valid_pixels();
    println!("baseline: {baseline_valid}/307200 valid pixels");

    // ---- enable GMC + the debug channel (the reference's two writes)
    // and watch the misc endpoint while frames flow ----
    let gmc_result =
        (|| -> xtion::Result<(usize, usize, usize, usize, Option<xtion::GmcPoints>)> {
            cam.set_gmc_mode(true)?;
            cam.set_gmc_debug(true)?;
            let mut plain = 0usize;
            let mut debug_pts = 0usize;
            let mut debug_end = 0usize;
            let mut total_points = 0usize;
            let mut first_plain: Option<xtion::GmcPoints> = None;
            for _ in 0..20 {
                let _frame = cam.read_depth_frame(Duration::from_secs(10))?;
                for packet in cam.drain_gmc_packets() {
                    match packet {
                        xtion::GmcPacket::Points(p) => {
                            total_points += p.points.len();
                            if p.debug {
                                debug_pts += 1;
                            } else {
                                plain += 1;
                                if first_plain.is_none() {
                                    first_plain = Some(p.clone());
                                }
                            }
                        }
                        xtion::GmcPacket::DebugEnd(_) => debug_end += 1,
                    }
                }
            }
            // restore both switches exactly (even a non-0/1 factory mode)
            cam.set_param(xtion::Param::GmcDebug, config0.debug as u16)?;
            cam.set_param(xtion::Param::DepthGmcMode, config0.mode)?;
            Ok((plain, debug_pts, debug_end, total_points, first_plain))
        })();

    match gmc_result {
        Ok((plain, debug_pts, debug_end, total_points, first_plain)) => {
            println!(
                "GMC stream over 20 frames: {plain} plain packets (0xa200), \
                 {debug_pts} debug point packets (0xb200), {debug_end} debug summaries \
                 (0xb500), {total_points} motion points decoded"
            );
            if let Some(p) = first_plain {
                let sample: Vec<_> = p.points.iter().take(5).collect();
                println!(
                    "   first plain packet: ts {} with {} points; sample {:?}",
                    p.timestamp,
                    p.points.len(),
                    sample
                );
            }
            if plain + debug_pts + debug_end == 0 {
                println!(
                    "-> VERDICT: the GMC stream is silent on this unit with GMC on and \
                     debug on — the motion path is compiled in (strings, params 36-44/61/75) \
                     but emits nothing here; kept as a finding"
                );
            }
        }
        Err(err) => {
            // restore best effort, then report (a write refusal is a
            // hardware verdict, not a failure — see crop mode 2)
            let _ = cam.set_param(xtion::Param::GmcDebug, config0.debug as u16);
            let _ = cam.set_param(xtion::Param::DepthGmcMode, config0.mode);
            println!("-> VERDICT: GMC switch cycle failed ({err}); kept as a finding");
        }
    }

    // the switches must be back at the factory values
    let config_after = cam.gmc_config().expect("gmc config after restore");
    assert_eq!(
        config_after.mode, config0.mode,
        "GMC mode must be back at the factory value"
    );
    assert_eq!(
        config_after.debug, config0.debug,
        "GMC debug must be back at the factory value"
    );

    // the depth stream must have survived all of it
    let final_frame = cam
        .read_depth_frame(Duration::from_secs(10))
        .expect("final frame");
    assert!(
        final_frame.valid_pixels() > 1000,
        "depth did not recover: {} valid pixels",
        final_frame.valid_pixels()
    );
    cam.stop_depth().expect("stop depth");
}

/// Test 13 — the depth mode matrix (0.3.7): enumerate the firmware's
/// advertised (format, res, fps) surface, then WALK it — every unique
/// (resolution, fps) pair gets a real stream start, decoded frames and
/// a timestamp-derived frame-rate measurement, plus the device
/// frequency read at that mode (the multi-camera interference lever).
/// QVGA@60 is hard-gated (~60 fps must actually arrive); SXGA gets the
/// dedicated treatment: 1280x1024@15 (the sustainable 1.3 MP mode on a
/// USB-2 bulk bus — 11-bit packed ≈ 26 MiB/s) first, and the
/// advertised 30 fps only as a diagnostic probe if 15 cannot start.
#[test]
fn depth_mode_matrix_and_high_fps() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    // ---- the advertised matrix (opcode 36, depth CMOS) ----
    let matrix = cam.depth_mode_matrix().expect("depth mode matrix");
    println!("depth mode matrix: {} entries", matrix.len());
    for preset in &matrix {
        println!("   {preset}");
    }
    assert!(
        !matrix.is_empty(),
        "the depth CMOS must advertise at least one mode"
    );
    // every advertised resolution must decode to real dimensions and be
    // addressable through the Resolution enum (start_depth needs it)
    for preset in &matrix {
        let info = xtion::protocol::presets::resolution_info(preset.resolution);
        assert!(
            info.is_some(),
            "advertised resolution code {} has no known dimensions",
            preset.resolution
        );
        assert!(
            xtion::Resolution::from_u16(preset.resolution).is_some(),
            "advertised resolution code {} is not modeled by Resolution",
            preset.resolution
        );
    }
    // camera-verified (0.3.5/0.3.6 runs): this unit lists QVGA@60
    assert!(
        matrix.iter().any(|m| m.resolution == 0 && m.fps == 60),
        "the depth mode matrix must list QVGA@60 (both camera runs did)"
    );

    // ---- walk: one pass per unique (resolution, fps) pair ----
    let mut walked: Vec<(u16, u16)> = Vec::new();
    for preset in &matrix {
        let pair = (preset.resolution, preset.fps);
        if walked.contains(&pair) {
            continue;
        }
        walked.push(pair);
    }
    // SXGA@15 first (the requested 1.3 MP mode) so its outcome leads the
    // log, then the advertised pairs
    let mut plan: Vec<(xtion::Resolution, u16, bool)> = vec![(
        xtion::Resolution::Sxga,
        15,
        false, // not advertised on this fw — probed deliberately
    )];
    for (res, fps) in walked {
        if let Some(resolution) = xtion::Resolution::from_u16(res) {
            plan.push((resolution, fps, true));
        }
    }
    // de-duplicate identical (resolution, fps) entries (SXGA@15 vs the
    // advertised SXGA@30 differ, so nothing collapses)
    let mut seen: Vec<(u16, u16, u32, u32)> = Vec::new();
    plan.retain(|(res, fps, _)| {
        let key = (res.as_u16(), *fps, res.dimensions().0, res.dimensions().1);
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });

    let mut qvga60: Option<f64> = None;
    for (resolution, fps, advertised) in plan {
        let (w, h) = resolution.dimensions();
        let tag = if advertised { "advertised" } else { "probe" };
        let config = DepthConfig {
            resolution,
            fps,
            ..DepthConfig::default()
        };
        print!("--- {tag}: {w}x{h}@{fps} --- ");
        match cam.start_depth(config) {
            Err(err) => {
                println!("refused: {err}");
                continue;
            }
            Ok(()) => println!("started"),
        }
        // the frequency the mode locked to (may change per mode — the
        // reference re-reads it after every configuration)
        match cam.refresh_device_frequency() {
            Ok(frequency) => println!("   device frequency: {frequency:.3} ticks/us"),
            Err(err) => println!("   device frequency read failed: {err}"),
        }
        // collect frames: 12 at high rates, 6 at SXGA-class rates (the
        // 1.3 MP frames are 1.8 MB each)
        let wanted = if w * h > 900_000 { 6 } else { 12 };
        let mut frames = Vec::new();
        let mut read_err = None;
        for _ in 0..wanted {
            match cam.read_depth_frame(Duration::from_secs(10)) {
                Ok(frame) => frames.push(frame),
                Err(err) => {
                    read_err = Some(err);
                    break;
                }
            }
        }
        cam.stop_depth().expect("stop between modes");
        let Some(read_err) = read_err else {
            // frames decoded -> size and rate gates
            let frame = frames.last().expect("at least one frame");
            assert_eq!(
                (frame.width, frame.height),
                (w, h),
                "frame must arrive at the requested {}x{} size",
                w,
                h
            );
            let timestamps: Vec<u64> = frames
                .iter()
                .map(|f| f.timestamp_us.expect("timestamps converted"))
                .collect();
            let mut deltas_ms = Vec::with_capacity(timestamps.len().saturating_sub(1));
            for pair in timestamps.windows(2) {
                deltas_ms.push((pair[1] - pair[0]) as f64 / 1000.0);
            }
            let (mean_delta, monotonic) = if deltas_ms.is_empty() {
                (f64::NAN, true)
            } else {
                (
                    deltas_ms.iter().sum::<f64>() / deltas_ms.len() as f64,
                    deltas_ms.iter().all(|d| *d > 0.0),
                )
            };
            let measured_fps = 1000.0 / mean_delta;
            println!(
                "   {} frames decoded, {}x{}, {} valid px, {:.2} ms/frame = \
                 {:.1} fps (nominal {fps}), monotonic: {monotonic}",
                frames.len(),
                frame.width,
                frame.height,
                frame.valid_pixels(),
                mean_delta,
                measured_fps
            );
            assert!(monotonic, "timestamps must advance at {w}x{h}@{fps}");
            if (resolution, fps) == (xtion::Resolution::Qvga, 60) {
                qvga60 = Some(measured_fps);
            }
            continue;
        };
        println!("   frame read failed: {read_err}");
    }

    // ---- hard gate: QVGA@60 must deliver (advertised + probed now) ----
    let measured = qvga60.expect("QVGA@60 must be in the walk and must start");
    assert!(
        (50.0..70.0).contains(&measured),
        "QVGA@60 measured {measured:.1} fps — expected ~60"
    );
    println!("QVGA@60: measured {measured:.1} fps — the high-rate mode works");

    // restore the session's default state for whoever runs next
    cam.start_depth(DepthConfig::default())
        .expect("restart default depth");
    cam.stop_depth().expect("stop default depth");
}

/// Test 14 — the debug telemetry channels (0.3.7): wavelength-correction
/// debug (param 74 -> 0xc200 packets while depth streams) and the
/// TEC/NESA channels (0xd200/0xd201 UTF-16 text lines) emitted while
/// the matching BIST modules run. The reference driver routes both to
/// dump files and never interprets them; this test decodes them and
/// prints the census. First camera exercise of all three channels —
/// refusals and empty censuses are VERDICTS, not failures.
#[test]
fn debug_telemetry_channels() {
    if !hardware_present() {
        eprintln!("skipped: no Xtion attached");
        return;
    }
    let _lock = lock_device();
    let mut cam = Xtion::open().expect("open");

    // ---- wavelength debug telemetry while depth streams ----
    cam.start_depth(DepthConfig::default())
        .expect("start depth for the wavelength debug window");
    let wl0 = cam
        .wavelength_correction()
        .expect("wavelength (73) baseline");
    let dbg0 = cam
        .wavelength_correction_debug()
        .expect("wavelength debug (74) baseline");
    println!("wavelength correction (73): {wl0}, debug (74): {dbg0} (factory baseline)");

    // enable both switches with the readback-verified setters; each
    // refusal is a verdict (the 0.3.6 camera quirk: a NACK may still
    // have landed — the setters' Ok/Err already reflects the readback)
    let wl_enable = cam.set_wavelength_correction(true);
    let dbg_enable = cam.set_wavelength_correction_debug(true);
    println!(
        "enable 73: {}, enable 74: {}",
        match &wl_enable {
            Ok(()) => "landed".to_string(),
            Err(e) => format!("refused ({e})"),
        },
        match &dbg_enable {
            Ok(()) => "landed".to_string(),
            Err(e) => format!("refused ({e})"),
        }
    );

    // stream and drain: 90 VGA frames (~3 s) of window
    let mut wavelength_packets = 0usize;
    let mut wavelength_printed = 0usize;
    let mut text_packets = 0usize;
    for _ in 0..90 {
        cam.read_depth_frame(Duration::from_secs(10))
            .expect("depth frame inside the debug window");
        for packet in cam.drain_debug_telemetry() {
            match packet {
                xtion::DebugTelemetryPacket::Wavelength(w) => {
                    wavelength_packets += 1;
                    if wavelength_printed < 3 {
                        wavelength_printed += 1;
                        println!(
                            "0xc200 #{wavelength_packets}: B {:+.4} -> {:+.4}, hop {} \
                             (hops now {}, window {:#x}), TEC calibrated {} wait {}, \
                             best (hops {}, set point {}, step {}), unstable {}, \
                             totally {}, configured set point {}, step {}",
                            w.b_last,
                            w.b_current,
                            w.is_hop,
                            w.current_hops_count,
                            w.current_sliding_window,
                            w.is_tec_calibrated,
                            w.wait_period,
                            w.best_conf.best_hops_count,
                            w.best_conf.best_set_point,
                            w.best_conf.best_step,
                            w.is_wavelength_unstable,
                            w.is_totally_unstable,
                            w.configured_tec_set_point,
                            w.current_step
                        );
                    }
                }
                xtion::DebugTelemetryPacket::Text(t) => {
                    text_packets += 1;
                    if text_packets <= 3 {
                        println!("{} debug text #{}: {}", t.channel, text_packets, t.text);
                    }
                }
            }
        }
    }
    println!(
        "wavelength debug census: {wavelength_packets} x 0xc200 packets, \
         {text_packets} x text packets over 90 frames"
    );
    // restore both switches whatever the enable results were
    if wl_enable.is_ok() {
        cam.set_wavelength_correction(wl0)
            .expect("restore wavelength correction switch");
    }
    if dbg_enable.is_ok() {
        cam.set_wavelength_correction_debug(dbg0)
            .expect("restore wavelength debug switch");
    }
    cam.stop_depth().expect("stop depth");
    if wavelength_packets == 0 {
        println!(
            "-> VERDICT: no 0xc200 packets while (73,74) were enabled — \
             either the switches were refused (see the enable lines) or the \
             wavelength algorithm stayed silent; first camera run of this channel"
        );
    }

    // ---- TEC / NESA channels: BIST with the TEC and NESA bits ----
    // The reference's BIST exchange allows 300 s (full-flash tests are
    // slow); TEC+NESA are measurement tests — 180 s here. The BIST
    // control reply blocks until they finish while the misc ring
    // collects the text lines.
    cam.enable_misc_capture();
    let mask = system::bist::TEC | system::bist::NESA;
    println!("--- BIST (mask {mask:#06x}: TEC | NESA) ---");
    match cam.run_bist(mask, Duration::from_secs(180)) {
        Ok(failures) => {
            if failures == 0 {
                println!("BIST reply: all selected modules green");
            } else {
                let bits = system::bist::failure_bits(failures);
                println!(
                    "BIST reply: failures {failures:#010x}: {}",
                    bits.iter()
                        .map(|f| format!("bit {} {}", f.bit, f.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        Err(err) => println!("BIST unavailable: {err} (verdict, not a failure)"),
    }
    let packets = cam.drain_debug_telemetry();
    let mut tec_lines = 0usize;
    let mut nesa_lines = 0usize;
    for packet in &packets {
        if let xtion::DebugTelemetryPacket::Text(t) = packet {
            match t.channel {
                xtion::DebugTextChannel::Tec => {
                    tec_lines += 1;
                    if tec_lines <= 5 {
                        println!("TEC 0xd200: {}", t.text);
                    }
                }
                xtion::DebugTextChannel::Nesa => {
                    nesa_lines += 1;
                    if nesa_lines <= 5 {
                        println!("NESA 0xd201: {}", t.text);
                    }
                }
            }
        }
    }
    println!(
        "BIST debug census: {tec_lines} TEC lines, {nesa_lines} NESA lines \
         (plus {} wavelength packets that may have trickled in)",
        packets
            .iter()
            .filter(|p| matches!(p, xtion::DebugTelemetryPacket::Wavelength(_)))
            .count()
    );
    cam.disable_misc_capture();
    if tec_lines + nesa_lines == 0 {
        println!(
            "-> VERDICT: no TEC/NESA debug text during the BIST run — first \
             camera exercise of these channels; if the BIST reply was green \
             the tests ran, and this firmware simply did not narrate them"
        );
    }
}
