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

    // ---- AGC bins (63-70): a contiguous custom partition of the shift
    // space, read back, one frame under it, restore, then compare ----
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
    let bins_result = (|| -> xtion::Result<(Vec<xtion::AgcBin>, usize)> {
        for (i, b) in custom.iter().enumerate() {
            cam.set_depth_agc_bin(i, *b)?;
        }
        let mut echoed = Vec::with_capacity(4);
        for i in 0..4 {
            echoed.push(cam.depth_agc_bin(i)?);
        }
        let frame = cam.read_depth_frame(Duration::from_secs(10));
        // restore before propagating any frame error
        for (i, b) in bins0.iter().enumerate() {
            cam.set_depth_agc_bin(i, *b)?;
        }
        let valid = frame?.valid_pixels();
        Ok((echoed, valid))
    })()
    .expect("AGC bin write/read/restore cycle");
    assert_eq!(
        bins_result.0,
        custom.to_vec(),
        "all four bins must read back exactly as written"
    );
    for (i, b) in bins0.iter().enumerate() {
        assert_eq!(
            cam.depth_agc_bin(i).expect("bin after restore"),
            *b,
            "bin {i} must be back at the factory value"
        );
    }
    println!(
        "AGC bins: custom partition echoed exactly; {} valid pixels under it \
         (baseline {baseline_valid}); factory bins restored",
        bins_result.1
    );
    assert!(
        bins_result.1 > 100,
        "depth died completely under the custom bins: {} valid",
        bins_result.1
    );

    // ---- close range (84): enable, read back, one frame (near-field
    // re-bias — long-range depth may legitimately vanish, so the frame
    // is printed, not gated), restore ----
    let close_result = (|| -> xtion::Result<(bool, usize, Option<u16>)> {
        cam.set_close_range(true)?;
        let echoed = cam.close_range()?;
        let frame = cam.read_depth_frame(Duration::from_secs(10));
        cam.set_close_range(close0)?; // restore before propagating errors
        let f = frame?;
        Ok((echoed, f.valid_pixels(), f.min_depth()))
    })()
    .expect("close-range write/read/restore cycle");
    assert!(close_result.0, "close range must read back as on");
    assert_eq!(
        cam.close_range().expect("close range after restore"),
        close0,
        "close range must be back at the factory value"
    );
    println!(
        "close range (84): on -> {} valid pixels (baseline {baseline_valid}), min depth {:?} mm; \
         restored {close0}",
        close_result.1, close_result.2
    );

    // ---- wavelength correction (73) + debug telemetry (74): toggled,
    // read back, restored. No frame is read while the DEBUG flag is on:
    // it makes the firmware emit wavelength-lock telemetry packets, a
    // diagnostics mode, not a depth-affecting setting.
    let wl_result = (|| -> xtion::Result<(bool, bool)> {
        cam.set_wavelength_correction(true)?;
        let wl = cam.wavelength_correction()?;
        cam.set_wavelength_correction_debug(true)?;
        let wld = cam.wavelength_correction_debug()?;
        // restore both, then return what was read
        cam.set_wavelength_correction_debug(wld0)?;
        cam.set_wavelength_correction(wl0)?;
        Ok((wl, wld))
    })()
    .expect("wavelength write/read/restore cycle");
    assert!(
        wl_result == (true, true),
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
