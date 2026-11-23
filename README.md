# xtion-rs

A from-scratch, pure-Rust userspace driver for the **ASUS Xtion /
PrimeSense PS1080** coded-light depth camera (USB `1d27:0601` /
`1d27:0400`). No OpenNI, no PS1080 C++ code, no system packages - the
crate talks the device's USB protocol directly.

This is the pure-Rust driver (Option D) from the ASUS Xtion Linux research
series; the alternative Rust bindings over the C OpenNI2 stack live in the
`rust-xtion-check` workspace of that series.

## Status

| Area | State |
|---|---|
| USB transport (rusb), endpoint probing | done, unit-tested |
| Command protocol (v2.5/v2.6, retry quirks, split-reply accumulation) | done, unit-tested incl. first-run hardware regression |
| Version handshake + identity (serial, versions, platform string, USB core) | done |
| Calibration (fixed params, depth info) | done |
| Shift-to-depth tables | done, **byte-identical to the C reference** (golden vectors) |
| Packed-11 depth decode | done, **byte-identical to the C reference** (golden vectors) |
| Bulk-stream frame assembly | done, unit-tested incl. chunk splits |
| Depth stream (VGA@30, packed-11 or raw 16-bit) | **hardware-validated** (87% valid pixels @ 29.97 fps) |
| Color stream (YUV422 -> RGB888) | implemented — **PS-compressed YUV422 (format 1, the C's Linux/bulk default) and uncompressed (format 5)**; mode validated against the firmware's CMOS presets (opcode 36); compressed frames decoded by the ported `XnStreamUncompressYUVImagePS`; **0.2.10, hardware answer: this unit reports image_cmos_type = 0 (XN_IMAGE_CMOS_NONE) — no color sensor on this hardware; `start_color` fails fast with that explanation and the hardware test skips with the reason; the full round-trip awaits a unit with an RGB sensor** |
| IR stream (16-bit grayscale, image pipe) | implemented in 0.2.0; wire-size fix in 0.2.3 (MT9M001 sends `height + 8` rows — hardware-measured, see docs/PROTOCOL.md section 8); **hardware-validated 2026-11-22, all four tests green** |
| Hardware registration, emitter on/off, OVERHEAT/PROJECTOR_FAULT events | implemented in 0.2.0 |
| Runtime options: mirror, hole filter, AGC, close range, GMC mode, exposure, white balance, gain, quality, flicker | implemented in 0.2.0 |
| Cropping, depth AGC bins, GMC data, audio | not yet (Phase 3-4 of docs/FEATURE_PLAN.md) |
| System commands (0.2.8): firmware log, CPU stats, BIST, TEC + emitter telemetry, CMOS blanking/register reads (I2C), AHB read, flash map / file list / file download / raw flash read | implemented — strictly READ-ONLY; the write-side commands stay excluded |
| Pre-3.0 firmware (bulk control endpoints) | not supported (explicit error) |

Everything that can be verified without the camera **is** verified: 85
tests pass on a bare CI machine (`cargo test`), and the numerical cores
(packed-11 depth, 10-bit IR, shift-to-depth) are proven equal to the
original C++ implementation by golden vectors generated from a verbatim C
transcription (`reference/c_golden.c` -> `reference/golden.txt` ->
`tests/golden.rs`). The 0.2.0 feature additions (identity, IR,
registration, options, events) carry FakeTransport wire tests derived
from the OpenNI 2.2.0 source.

**Safety by design**: the driver never issues flash, file-system, I2C,
AHB or calibration writes - anything that could delete calibration data
or overwrite configs is excluded on purpose (docs/FEATURE_PLAN.md,
"Excluded features").

## Usage

```rust
use std::time::Duration;
use xtion::{DepthConfig, Xtion};

let mut cam = Xtion::open()?;                    // handshake + calibration
println!("{} serial {}", cam.version, cam.serial_number()?);
println!("platform {} ({:?})", cam.platform_string()?, cam.usb_core_type()?);

cam.start_depth(DepthConfig::default())?;        // VGA 640x480 @ 30 fps
cam.set_registration(true)?;                     // depth onto color, in firmware
cam.set_hole_filter(true)?;                      // fill single-pixel holes
let frame = cam.read_depth_frame(Duration::from_secs(2))?;
println!("min depth {} mm, {} valid pixels",
         frame.min_depth().unwrap_or(0), frame.valid_pixels());

for event in cam.drain_events() {                // OVERHEAT / PROJECTOR_FAULT
    eprintln!("device event: {event:?}");
}
```

The infrared stream (`cam.start_ir(IrConfig::default())` ->
`cam.read_ir_frame(..)` -> 16-bit grayscale) shares the image pipe with
the color camera, so IR and color are mutually exclusive; IR pairs with
depth, which is the combination hardware registration is designed for.
The IR projector can be toggled at runtime with
`cam.set_emitter_enabled(false)`.

Examples:

```bash
cargo run --example list          # enumerate + dump identity/calibration
cargo run --example depth_dump -- 30
```

Linux permissions: the camera is accessed via libusb; install the OpenNI2
udev rule (or run as root) to allow user access to `1d27:0601`.

## Documentation

- `docs/PLAN.md` - the rewrite plan: milestones, file structure, full test
  matrix, known gaps.
- `docs/PROTOCOL.md` - the extracted PS1080 USB protocol reference, every
  constant cited to its OpenNI 2.2.0 source file.
- `docs/FEATURE_PLAN.md` - what the OpenNI 2 driver implements that this
  crate does not (yet): gap analysis, phased plan ordered by effort and
  risk, and the excluded (flash/calibration) features.
- `docs/FIRMWARE.md` - reverse-engineered overview of the camera's firmware
  5.8.22: boot self-test, runtime architecture, the complete command
  surface, and the functionality this driver does not use yet.
- `docs/recovery/` - snapshot of the research workspace for session
  recovery.
- `firmware/` - the final ASUS firmware release for the camera (FWUpdate
  5.8.22, the version the validation camera runs), mirrored from ASUS's
  official download with the flash images extracted and documented.

## Attribution

The USB protocol implemented here was derived from the OpenNI 2 PS1080
driver (Apache-2.0, PrimeSense Ltd.); see `NOTICE.md`. The decode cores
are verified byte-identical to that implementation via golden vectors.

## Changelog

- **0.2.10 — color NACK final root cause: no color sensor on this unit** -
  the 0.2.9 on-camera run printed the answer the new diagnostics were
  built for: `image cmos type: 0` — the fixed-params block reports
  XN_IMAGE_CMOS_NONE for this unit. That field IS the reference
  driver's capability gate `bImageSupported`
  (XnSensorFirmware.cpp:128-140, fw >= 5.6.9), and the C never creates
  the image stream when it is 0 (XnSensor.cpp:274-281) — OpenNI lists
  no color node on this device either. Every stream-on NACK since
  0.2.4 (compressed/uncompressed x VGA/QVGA) was the firmware
  refusing to route COLOR from an absent sensor, while IR — the other
  stream0 source, fed by the depth CMOS — starts fine on the same
  pipe. The opcode-36 preset table is a platform (RD108x) capability
  list, not a per-unit sensor inventory, and the USB PID cannot settle
  it (1d27:0601 was used for late batches of both the depth-only
  Xtion Pro and the Pro Live). Since 0.2.10: `Xtion::image_supported()`
  exposes the gate, `start_color` fails fast with an explicit
  no-color-sensor error BEFORE writing any image params (device state
  stays clean), and `color_stream_roundtrip` skips with the full
  explanation, printing depth_cmos_type as the parse cross-check.
  Color on this hardware is impossible, not broken.
- **0.2.9 — stream hygiene + full image-open C parity** - the 0.2.8
  color start still NACKed BadParams on the stream-on write on real
  hardware. Source-level re-audit of the whole reference image path
  found what the C does that xtion-rs still did not: its stream-mode
  writes are guarded by "we require that every change to mode will go
  through OFF" (XnSensorFirmwareParams::SetStreamMode), every C
  session ends with all three stream modes written OFF (reset path
  XnHostProtocol.cpp:1804-1813 + shutdown XnSensor.cpp:315-317), and
  the image-open sequence carries steps xtion-rs skipped. 0.2.9 ports
  all of it: (a) `open()`/`soft_reset` now kill all three streams
  after the init reset (best effort); (b) `Drop` for `Xtion` repeats
  the shutdown stream kill, so a panicked test cannot leave stale
  stream state behind; (c) `start_depth`/`start_ir`/`start_color` go
  through stream-mode OFF first; (d) `start_color` writes the full
  reference sequence — flicker (17) and firmware mirror (71) defaults,
  the CMOS blanking read, the device-frequency read (algorithm 0x80,
  `XnSensorStreamHelper::Open` parity, res 0/fps 0), stream-on, crop
  mode disabled (50), auto exposure (80) and auto white balance (77)
  on with their reference defaults (fw >= 5.8.9,
  bImageAdjustmentsSupported); (e) a NACK on the color stream-on now
  self-reports the device state (stream0 selector + operating mode)
  in the error; (f) `color_start_probe` walks the full sequence with
  per-step readbacks, dumps the image CMOS type (the C's
  bImageSupported gate: 0 = no color sensor) and the boot self-test
  log lines (CLR/PRJ/TEC). New `Param::ImageCropMode = 50`, new
  `read_device_frequency` (algorithm 0x80). 106 lib tests.

- **0.2.8 — color uses the C's Linux/bulk mode + read-only system
  commands** - the turn-24/25 decompilation + C cross-read settled the
  color NACK: (a) the reference driver soft-resets the firmware at
  EVERY init and xtion-rs never did — after a failed stream start the
  device rejects even plain mode-parameter writes (NACK BadParams), so
  `open()` now ports `XnSensorFirmware::Init(bReset=TRUE)` exactly
  (GET_MODE + safe-mode refusal, KeepAlive x5, fire-and-forget
  SET_MODE(SOFT_RESET), `nUSBDelaySoftReset` = **1 ms on Jango cores of
  fw >= 5.3.16** / 800 ms otherwise, KeepAlive x10, safe-mode check
  again); (b) the C's image default on non-Windows is the
  **PS-COMPRESSED** YUV422 (format 1, `XnSensorImageStream.h:36-41`:
  "non win32 works with BULK, so it's better to compress the image") —
  the uncompressed mode (5) the presets advertise is the Windows/ISO
  path and its stream start is what this bulk device rejected. New:
  `uncompress_yuv_image_ps` (port of `XnStreamUncompressYUVImagePS`,
  4-bit diff/full stream, U/V channel carry, per-line reset; golden
  tests incl. line-boundary reset), `read_color_frame` decompresses
  and gates on the DECODED size for compressed frames, `start_color`
  accepts formats 1 + 5, `IMAGE_DEFAULT_INPUT_FORMAT` = compressed.
  Also 0.2.8 implements the excluded commands that do NOT write flash
  (all read-only): GET_LOG 7 (text/overflow/structured entry render),
  I2C_READ 11 (+ CMOS register convenience via the fixed-params bus/
  slave, `XnHostProtocolGetCMOSRegisterI2C` parity), DOWNLOAD_FILE 16,
  GET_FLASH_MAP 18, GET_FILE_LIST 19, READ_AHB 20, READ_FLASH 25,
  GET_CPU_STATS 27, BIST 28 (300 s-class timeout), GET_TEC_DATA 30 /
  GET_FAST_CONVERGENCE_TEC 38 (post-5.4 layout), GET_EMITTER_DATA 32,
  GET_CMOS_BLANKING 35 (direct, distinct from ALGORITHM_PARAMS 0x06);
  `FixedParams` now exposes the CMOS I2C buses/addresses; HostMode
  gained WAIT_FOR_ENUM(9)/SAFE_MODE(10); new `system_commands_readonly`
  hardware test prints log, file list, flash map, ImageCmos download,
  CPU stats, TEC/emitter telemetry, blanking, CMOS registers, AHB and
  raw flash. Flash writes, calibrations, I2C/AHB writes, TAKE_SNAPSHOT
  and file upload/delete/execute remain excluded. 105 lib + 2 golden
  green, clippy 0, fmt clean, hardware target compiles.
- **0.2.7 — color-start NACK diagnostics (GET_MODE/SET_MODE + probe)** -
  the 0.2.6 run validated the preset fix (the camera reports
  `format 5 @ res 1 @ 30 fps` = uncompressed-YUV422 VGA@30 and the test
  picks it), but the NACK moved inside start_color: with the image
  params written and ACKed, the final `STREAM0_MODE=COLOR` write is
  rejected BadParams (occasionally, right after a failed attempt, even
  the first image-format write). The C-reference open path was verified
  write-for-write identical (plain values, PARAM ids 12/13/14/5, no
  version gates, SetAlwaysSet(TRUE), no conversion callbacks,
  CheckClaimStream is host-side only) — so the gap must be DEVICE STATE.
  The biggest divergence found: the reference soft-resets the firmware
  at EVERY init (XnSensorFirmware::Init with bResetOnStartup=TRUE:
  GetMode, KeepAlive, SetMode(SOFT_RESET), 800 ms, KeepAlive x10,
  GetMode + safe-mode check) and xtion-rs never did. New in 0.2.7:
  `HostMode` enum (XnHostProtocol.h modes 0-8, WEBCAM=0), `get_mode()`,
  `soft_reset()` (fire-and-forget SET_MODE + 800 ms + keep-alive liveness
  loop — the C discards the SET_MODE reply: "there is nobody to ACK"),
  `cmos_presets(cmos)` accessor (depth cross-check), and the
  `color_start_probe` hardware test that REPORTS instead of gating: mode
  + image-pipe params before/after the soft reset, the full C-parity
  write sequence with readbacks, and one driver start_color attempt.
  85 lib tests + 2 golden + 1 doc green, clippy 0, fmt clean.
- **0.2.6 — preset-table endianness fix (the color mode now matches)** -
  the 0.2.5 hardware run printed the camera's preset list as
  `format 256 @ res 0 @ 7680 fps` — every field shifted by 256 — so the
  requested (format 5, res 1, fps 30) mode never matched and the test
  aborted before `start_color`. Root cause: the opcode-36 parser read
  the `{format, resolution, fps}` triples big-endian, but
  `XN_PREPARE_VAR16_IN_BUFFER` in the C reference is the IDENTITY on
  x86 and Windows (ThirdParty/PSCommon/XnLib/Include/Linux-x86/
  XnOSLinux-x86.h) — the Nios II firmware is little-endian, like the
  rest of the packet protocol. After the fix the same bytes decode to
  `format 5 @ res 1 @ 30 fps` = uncompressed-YUV422 VGA@30, exactly the
  mode the driver requests; the decoded table is QVGA(0)/VGA(1) in
  formats 1/5/7 at 15/30/60 fps, SXGA(2)/720p(14)/1280x960(15)
  Bayer-only (0/6), plus one (9, 1, 30) entry with a format id OpenNI
  never exposes. The request payload is little-endian now too
  (`cmos.to_le_bytes()` — the old big-endian write was invisible only
  because CMOS id 0 is byte-symmetric). The hardware test gained an
  endianness guard (every field <= 60/15/15 or the table is swapped)
  and the enum meanings are documented from PS1080.h:253-261 and
  XnStreamParams.h:183-202. 88 tests green (85 lib, 2 golden, 1 doc),
  clippy 0.
- **0.2.5 — color BadParams root cause + CMOS presets (opcode 36)** - the
  first color hardware run failed at `start color: Nack(BadParams)`
  (tests/hardware.rs:192). Root cause (C reference): the image stream
  mode must be one of the presets the FIRMWARE reports via
  GET_CMOS_PRESETS (opcode 36, fw >= 5.3.29;
  `XnHostProtocolGetCmosPresets` -> `XnSensorImageStream::Init`
  validates and adapts its defaults to that list), and
  `ImageFormat::Yuv422` (1) is the PS-COMPRESSED variant
  (`XnPSCompressedImageProcessor` -> `XnStreamUncompressYUVImagePS`)
  that this driver does not decode — the plain-UYVY decoder matches
  `UncompressedYuv422` (5). Changes: opcode 36 implemented (BE u16 CMOS
  id request, BE {format,res,fps} triples reply, fw fps-0-entry
  workaround — XnHostProtocol.cpp:3253-3296); `image_presets()`
  accessor; `start_color` validates the requested mode against the
  presets and fails fast with the supported list in the message, and
  every step names itself in the error; `ImageFormat::Yuv422` now
  rejected up front with an explanation. The hardware test prints the
  device's preset list and picks VGA@30 or QVGA@30 uncompressed
  automatically. Unit tests: BE request payload, multi-entry parse +
  fps-0 skip, malformed-size error. 88 tests green (85 lib, 2 golden,
  1 doc), clippy 0, fmt clean.
- **0.2.4 — color stream hardware test + color fail-fast** - the 0.2.3
  run validated IR on camera (all four hardware tests green; the
  "emitter toggle skipped" line is benign). Next image-pipe feature:
  color. `read_color_frame` now validates the wire size (YUV422 =
  width x height x 2 — color has NO extra CMOS rows,
  XnImageProcessor::CalculateExpectedSize) and fails fast after 5
  unusable frames with assembler + ring counters in the message (the
  same pattern that made the IR failure diagnosable in 0.2.2); a
  zero-data timeout now prints the ring counters too. New hardware test
  `color_stream_roundtrip`: VGA@30 YUV422 -> RGB888 roundtrip, the
  shared-image-pipe mutual-exclusion guard (`start_ir` while color
  runs must be BadConfig), and a non-constant-frame check;
  `color_ring_stats()` accessor added for symmetry.
- **0.2.3 — IR wire-size fix (the fourth on-hardware failure)** - the
  0.2.2 diagnostics run finally produced the IR failure signature the
  fail-fast logic was built for: `frame #5 390400 bytes (expected
  384000 for 640x480 packed-10)` with the assembler closing 5 full
  frames and the ring perfectly healthy (100 completions, 0 failures).
  390,400 = 488 rows x 800 B: **the MT9M001 IR CMOS transmits 8 more
  rows than the configured resolution**, exactly as the reference
  driver handles it ("there are additional 8 rows (this is how the
  CMOS is configured)", XnIRProcessor.cpp:244-282; "in IR, in all
  resolutions except SXGA, we get additional 8 lines",
  XnSensorIRStream.cpp:411-422). The fix reads the depth CMOS type from
  the fixed params (already parsed since 0.1.2 — none/MT9M001/AR130,
  Include/PS1080.h:418), expects `(height + 8) x width x 10/8` on the
  wire for MT9M001 non-SXGA modes, and crops the trailing rows off the
  decoded frame exactly like the reference ("don't report additional
  rows out", XnIRProcessor.cpp:281-282 — the FIRST `height` rows are
  kept). Depth (packed-11) and color (YUV422) have no extra rows and
  are untouched. Regression tests encode the hardware signature
  (390,400-byte wire accepted, rows 480-487 dropped); new `ir_dump`
  example streams IR and saves a PGM for visual checks. 85 tests green
  (82 lib incl. the CMOS-type matrix, 2 golden, 1 doc).

- **0.1.5 — HARDWARE VALIDATED** - after 0.1.4 assembled full 307,200-pixel
  frames at 30 fps
  but every pixel decoded as invalid, audited the whole decode chain
  against the reference driver. Confirmed fix: the shift-to-depth LUT
  must cover the full 0..2047 shift range (`XN_SHIFTS_MAX_SHIFT`) — the
  1024-entry table silently zeroed every shift >= 1024 (scenes from
  ~5.5 m out; for the reference calibration the valid window is shifts
  1..1052 = 315..9870 mm). Also ported the reference driver's
  corrupt-frame drop (assembled size != expected at EOF, which is how
  the partial first frame of a stream is discarded) and added per-frame
  raw-shift histograms to `depth_dump` (zeros / 2047 markers / valid
  range buckets) plus `DepthFrame::shift_stats`. **Validated on hardware
  (run 6, 2026-11-22): both integration tests pass — 87.4% valid pixels
  (268,435/307,200), min depth 431 mm, shifts 291-797, 12.17 MiB/s with
  0 USB failures at 29.97 fps.** Full driver overview with flow diagrams:
  `xtion_rs_driver_overview.pdf`.

- **0.1.4** - fixed the fourth on-hardware failure: with the async ring
  delivering the full 12 MB/s stream (0 failures), frames still assembled
  at exactly 24560 of 307200 pixels. Root cause: `buf_size` is the ONE
  big-endian field in the otherwise little-endian 12-byte stream packet
  header — the reference driver applies an extra unconditional
  `xnOSEndianSwapUINT16` to exactly that field. Read little-endian, a
  512-byte packet parses as buf_size 2 → zero payload, and frames filled
  with false-magic junk. Also fixed: zero-payload packets (bare EOF
  headers) were not dispatched until the next read chunk arrived. New
  `raw_dump` example captures the raw stream and decodes packet headers
  independently, plus `Xtion::fw_magic`/`tap_depth_stream` diagnostics.

- **0.1.3** - fixed the third on-hardware failure: after the EOVERFLOW
  fix the depth stream ran, but every assembled frame was a fragment
  (736..45424 of 307200 pixels, ~4% of the stream bytes surviving).
  Root cause: a synchronous read loop (one bulk URB at a time) leaves
  gaps between reads in which the device's FPGA FIFO drops data. The
  C driver avoids this with a ring of 16 pre-submitted async URBs that
  are re-submitted the moment they complete; this release ports that
  design (`src/ring.rs`, via `libusb1-sys` since rusb has no async
  API). The examples now also print USB bus speed, endpoint max packet
  size, ring throughput stats and expected-vs-got pixel counts, so a
  mis-negotiated bus speed shows up immediately.

- **0.1.2** - fixed the second on-hardware failure: every depth bulk read
  returned `USB error: Overflow` because the read buffer was resized to
  the whole frame's wire size (422,411 bytes - not a multiple of the
  512-byte max packet size; the Linux USB stack completes such bulk-IN
  URBs with EOVERFLOW). Reads now always use 40 x maxpacket chunks like
  the C reference, read timeouts are retried until the caller's deadline
  instead of failing, parallel hardware tests are serialized (the
  `Usb(Busy)` collision), and a busy device now reports a dedicated
  `DeviceBusy` error with guidance.

- **0.1.1** - fixed the first on-hardware failure: replies can arrive split
  across multiple vendor control-IN transfers (the first run of
  `tests/hardware.rs` got "version reply too short: 12 bytes"); the
  exchange now accumulates follow-up reads like the C reference. Also
  ported stale-reply re-receive, NACK_NOT_READY re-send, the 1 ms/10 ms
  command delays, 512-byte control reads, and the XnVersions wire quirks
  (minor/major byte order, 12-byte base struct, word padding).
- **0.1.0** - initial port: USB transport, command protocol, calibration,
  packed-11 + shift-to-depth decode (C-verified), bulk-stream framing,
  depth/color streaming API.
