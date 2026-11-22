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
| Color stream (YUV422 -> RGB888) | implemented — uncompressed YUV422 (format 5) only; mode validated against the firmware's CMOS presets (opcode 36, little-endian since 0.2.6); on-camera validation pending |
| IR stream (16-bit grayscale, image pipe) | implemented in 0.2.0; wire-size fix in 0.2.3 (MT9M001 sends `height + 8` rows — hardware-measured, see docs/PROTOCOL.md section 8); **hardware-validated 2026-11-22, all four tests green** |
| Hardware registration, emitter on/off, OVERHEAT/PROJECTOR_FAULT events | implemented in 0.2.0 |
| Runtime options: mirror, hole filter, AGC, close range, GMC mode, exposure, white balance, gain, quality, flicker | implemented in 0.2.0 |
| Cropping, depth AGC bins, GMC data, audio, firmware log | not yet (Phase 3-4 of docs/FEATURE_PLAN.md) |
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
