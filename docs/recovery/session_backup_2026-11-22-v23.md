# Chat Session Backup — ASUS Xtion Linux research

**Version:** 23 (2026-11-22, turn 22): 0.2.5 CMOS presets (opcode 36) + color
BadParams root cause pushed (3d9dc93). Previous: 22 (turn 21): 0.2.4 color
test (2ccd1d1). **Status: COLOR NACK ROOT CAUSE FOUND. The 0.2.4 hardware run
failed `start color: Nack(BadParams)` at tests/hardware.rs:192 (the four
known tests green: handshake, phase1, depth, IR). Root cause (C reference):
(1) the image stream mode must be one of the presets the FIRMWARE reports
via GET_CMOS_PRESETS (opcode 36, fw >= 5.3.29,
XnHostProtocolGetCmosPresets -> XnSensorImageStream::Init validates and
adapts its defaults); a mode outside the list is NACKed BadParams. (2)
ImageFormat::Yuv422 (1) is the PS-COMPRESSED variant
(XnPSCompressedImageProcessor -> XnStreamUncompressYUVImagePS, 4-bit
state-machine decompressor) that this driver does NOT decode; the plain-UYVY
decoder (color.rs) matches UncompressedYuv422 (5) only — the image stream's
DEFAULT resolution is QVGA (XnSensorImageStream.h:34). 0.2.5 (3d9dc93)
implements opcode 36 (BE u16 CMOS id request; BE {format,res,fps} triples
reply, each field individually endian-swapped, XnHostProtocol.cpp:3253-3296;
fw appends an extra fps-0 preset — dropped), CMOS_TYPE_IMAGE=0/DEPTH=1
(Include/PS1080.h:245-246), image_presets() accessor, start_color validates
the requested mode against the presets (fail-fast with the supported list)
and names every step in errors, Yuv422 rejected up front; the hardware test
now PRINTS the device's preset list and auto-picks VGA@30 or QVGA@30
uncompressed YUV422. 88 tests green (85 lib, 2 golden, 1 doc), clippy 0,
fmt clean, hardware target compiles. PUSHED 3d9dc93. NEXT RUN: user pulls
3d9dc93 and runs `cargo test --test hardware -- --nocapture` — the preset
list printout is the ground truth for the color CMOS; if no uncompressed
mode exists, the test panics with instructions to implement the PS
decompressor (Uncomp.cpp).**

> Purpose: upload this file in a new chat to restore full context. It records
> the standing rules, the task, everything that was found and decided, every
> file that was produced, and the state of the current workspace.

## 1. Standing user rules (apply to EVERY future turn)

1. **Communicate in English only** (all replies, all document content).
2. **Zip backup of generated files to `download/`** every turn.
3. **Do NOT run full cargo builds of the complete project — the sandbox does
   not have enough RAM.** Use targeted invocations: `cargo test --lib -j 2`,
   `cargo test --test golden -j 2`, `cargo clippy --lib -j 2`; never
   `cargo build`/`cargo test` over all targets, no release builds.
4. **Produce a downloadable session backup** (this file, refreshed with a new
   version/date each turn) so the chat can be continued/resumed later.

## 2. Task history (condensed)

- **Sessions 1-2, turns 1-5 (2026-11-19..21):** research report on the Xtion
  (1d27:0601) on CachyOS; NOT UVC — only the OpenNI2 PS1080 stack works;
  test + setup scripts; AUR openni2 PKGBUILD audited **clean**; delivered
  `openni2-nojava` package.
- **Turn 6:** Rust-stack options report; Option A verified end-to-end.
- **Turn 7:** Option D chosen — pure-Rust driver, 44 tests, pushed (deeb688).
- **Turn 8:** first on-hardware run: handshake fails — replies SPLIT across
  control-IN transfers; ported Execute/ReceiveReply semantics. 0.1.1.
- **Turn 9:** control path works (fw 5.8.22, PS1080-A6, serial 1408090603,
  ZPD 120 mm, ZPPS 0.1052 mm); whole-frame-buffer EOVERFLOW → 40xmaxpacket
  chunks; parallel-test Busy → mutex. 0.1.2.
- **Turn 10:** fragments (736..45424 px) → ported the C's async 16-URB ring.
  0.1.3.
- **Turn 11:** frames exactly 24560/307200 px → buf_size BIG-ENDIAN quirk +
  zero-payload dispatch bug. 0.1.4.
- **Turn 12:** full frames, 0 valid pixels → shift LUT max_shift 2047 +
  corrupt-frame drop. 0.1.5.
- **Turn 13:** SIXTH RUN GREEN — depth validated (87.4% valid, 12.17 MiB/s,
  29.97 fps). Driver overview PDF delivered.
- **Turn 15:** firmware decompiled (Ghidra, 165 functions); ASUS fw 5.8.22
  mirrored to xtion-rs/firmware/. Firmware analysis PDF.
- **Turn 16:** 0.2.0 — identity opcodes, IR stream (STREAM0 mode 3,
  packed-10), DeviceEvent capture, +20 methods; hardware.rs gained
  phase1 + ir tests. PERMANENT EXCLUSIONS: flash FS 13-25, I2C/AHB writes,
  calibrations 29/31/33, TAKE_SNAPSHOT 12, SET_MODE 6.
- **Turn 17:** beyond-OpenNI analysis + VALIDATION.md test plan (4 automated
  hw tests, 10 manual checks); next spec = cropping (params 46-60).
- **Turn 18:** 0.2.1 — usb-core Nack → Jango fallback; IR ControlTimeout →
  MISSING CMOS blanking fetch (ALGORITHM_PARAMS type 0x06) →
  read_cmos_blanking added to all stream starts.
- **Turn 19:** 0.2.2 — blanking fix CONFIRMED (image endpoint 11.6 MB/s);
  IR frames silently dropped → AssemblerStats + read_ir_frame fail-fast
  after 5 unusable frames; emitter gated to fw >= 5.8.15 (NACK tolerated).
- **Turn 20:** 0.2.3 — IR ROOT CAUSE: the MT9M001 IR CMOS transmits
  height + 8 rows (390,400 = 488 x 800 at VGA; XnIRProcessor.cpp:244-282);
  wire_extra_rows + ir_wire_size + first-height-rows crop; ir_dump example.
  Depth/color have NO extra rows (XnImageProcessor::CalculateExpectedSize).
- **Turn 21:** 0.2.4 — user CONFIRMS all four hardware tests pass (IR
  hardware-validated). Color step: read_color_frame fail-fast (expected =
  w*h*2 YUV422, 5-unusable-frames limit, timeout ring counters),
  color_ring_stats(), color_stream_roundtrip test. 85 tests green. 2ccd1d1.
- **Turn 22 (this turn):** 0.2.5 — COLOR NACK ROOT CAUSE. The user's 0.2.4
  run: 4 tests green, color failed `start color: Nack(BadParams)` at the
  .expect on start_color. Investigation (OpenNI 2.2.0 re-verified):
  - Wire ids CORRECT: PARAM_IMAGE_FORMAT=12/RESOLUTION=13/FPS=14
    (XnParams.h:41-43), STREAM0_MODE=5, STREAM1_MODE=6;
    XN_VIDEO_STREAM_COLOR=1 (XnStreamParams.h:227) matches our
    STREAM_MODE_COLOR; XN_RESOLUTION_VGA=1 matches our Resolution::Vga.
  - XnHostProtocolAlgorithmParams BLANKING=0x06 confirmed
    (XnHostProtocol.h:162) — blanking call identical to depth/IR.
  - **GET_CMOS_PRESETS (opcode 36, fw >= 5.3.29):** the firmware itself
    reports the supported (format,res,fps) presets per CMOS;
    XnSensorImageStream::Init (lines 125-175) validates the default mode
    against this list and substitutes aSupportedModes[0]/nValidInputFormat
    when needed; the C image DEFAULT is QVGA@30 (XnSensorImageStream.h:34)
    and non-Windows default format = YUV422 (1).
  - **Format 1 = PS-compressed** (XnPSCompressedImageProcessor →
    XnStreamUncompressYUVImagePS, Uncomp.cpp); format 5 =
    UNCOMPRESSED_YUV422 → plain UYVY→RGB (XnUncompressedYUV422toRGB...
    Processor) — matches our color.rs. Both 2 bpp on the wire (post-decomp).
  - bImageAdjustmentsSupported = fw >= 5.8.9 (XnHostProtocol.cpp:655-658):
    the C ALSO writes auto-exposure + auto-white-balance firmware params at
    every image open (OpenStreamImpl), after the stream0-mode write, plus
    crop-mode=0 — NOT yet ported (post-mode, cannot explain the NACK).
  - Image presets request: 2-byte BE CMOS id; reply: BE triples
    {format,res,fps}, drop fps==0 entries (fw bug) — XnHostProtocol.cpp
    3253-3296. OPCODE_GET_CMOS_PRESETS = 36 (XnHostProtocol.h:85).
    CMOS types: IMAGE=0, DEPTH=1 (PS1080.h:245-246).
  FIX (3d9dc93, 0.2.5): new src/protocol/presets.rs (CmosPreset,
  CMOS_TYPE_IMAGE/DEPTH, read_cmos_presets + 3 unit tests incl. full-packet
  fake-transport wire check); Opcode::CmosPresets = 36; lib.rs re-exports
  CmosPreset; device.rs: start_color rejects Yuv422 up front, queries the
  image presets, validates (res,fps) and (res,fps,format) with the
  supported list in the error, per-step error context on all four writes +
  blanking; image_presets() accessor. tests/hardware.rs color test: prints
  the preset list, auto-picks (VGA@30 → QVGA@30) uncompressed YUV422,
  sizes/dims from the chosen mode. Docs: PROTOCOL.md opcode list + new
  "CMOS presets" subsection + section 7 rewrite; VALIDATION.md triage;
  README status + changelog; Cargo.toml 0.2.5. 88 tests green (85 lib,
  2 golden, 1 doc), clippy 0 (-D warnings), fmt clean, hardware target
  compiles. PUSHED 3d9dc93 (+ docs commit with this backup).

## 3. Hardware test inventory (FIVE tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green.
2. `depth_stream_roundtrip` — green (266,975 valid px, min 529 mm).
3. `phase1_identity_and_options` — green ("emitter toggle skipped" benign).
4. `ir_stream_roundtrip` — green (value range 13..1022).
5. `color_stream_roundtrip` — 0.2.4 run failed Nack(BadParams); 0.2.5
   rewritten: prints presets, auto-picks uncompressed VGA@30/QVGA@30.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- color_stream_roundtrip --nocapture`

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-22-v23.md` | This file (v23, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-22_v23.zip` | Turn-22 zip (repo with .git at 3d9dc93 + docs commit + this backup). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (13 pages). |
| `xtion_rs_next_steps_and_validation.pdf` | Turn-17 validation plan (9 pages). |
| `xtion_rs_firmware_analysis.pdf` | Turn-15 firmware analysis (11 pages). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 3d9dc93 "0.2.5: color BadParams root cause + CMOS presets
(opcode 36)"**; history: 2ccd1d1 (0.2.4) → f520989 (docs v22) → 3d9dc93
(0.2.5); a docs commit with this backup in docs/recovery/ follows it).

## 5. Workspace state (2026-11-22, after turn 22)

- `/home/z/my-project/xtion-rs/` — full clone WITH .git at 3d9dc93
  (+ docs commit), 88 tests green, clippy 0, fmt clean. This turn
  changed: src/protocol/presets.rs (NEW), src/protocol/mod.rs (opcode +
  module wiring), src/lib.rs (re-export), src/device.rs (start_color
  validation + image_presets), tests/hardware.rs (color test rewrite),
  Cargo.toml (0.2.5), README.md, docs/PROTOCOL.md, docs/VALIDATION.md.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (stable,
  minimal profile + clippy + rustfmt). Use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`.
- OpenNI 2.2.0 reference source: tarball verified at
  `firmware_dl/openni_ref/v2.2.0-debian.tar.gz`; extract after recycle:
  `mkdir -p /tmp/xtion-verify && tar -xzf firmware_dl/openni_ref/
  v2.2.0-debian.tar.gz -C /tmp/xtion-verify --strip-components=1`
  (key files: Source/Drivers/PS1080/Sensor/*, Include/PS1080.h,
  Source/Drivers/PS1080/Include/XnStreamParams.h).
- GitHub token: `/home/z/my-project/.gh-token` (chmod 600, gitignored,
  never printed/committed). Push via
  `umask 077 && printf 'https://thomas7475:%s@github.com\n'
  "$(cat /home/z/my-project/.gh-token)" > /tmp/gh-creds` then
  `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 6. Gotchas learned (cumulative, new first)

- **The firmware NACKs BadParams any stream mode outside its CMOS preset
  list** — query opcode 36 first and validate (the C does this at Init;
  0.2.5 does it in start_color). The list comes from the DEVICE, not the
  C's hardcoded fallback tables.
- **ImageFormat::Yuv422 (1) is PS-COMPRESSED** (XnPSCompressedImage-
  Processor); uncompressed is format 5. The image stream default
  resolution is QVGA, not VGA (XnSensorImageStream.h:34).
- **GET_CMOS_PRESETS wire format differs from other commands**: request =
  ONE BE u16 CMOS id (IMAGE=0, DEPTH=1); reply = BE {format,res,fps}
  triples, each field individually byte-swapped (XN_PREPARE_VAR16_IN_
  BUFFER), plus a bogus fps-0 entry to drop. ALGORITHM_PARAMS replies by
  contrast are plain LE blobs.
- **bImageAdjustmentsSupported (fw >= 5.8.9):** the C writes auto-exposure
  (PARAM 80) + auto-white-balance (PARAM 77) firmware params at EVERY
  image stream open (OpenStreamImpl, AFTER the stream0-mode write), plus
  crop-mode 50 = disabled — not yet ported; do it if color misbehaves.
- The IR CMOS (MT9M001) transmits `height + 8` wire rows in every
  resolution except SXGA (AR130: -64 at SXGA; color and depth pipes have
  NO extra rows). VGA IR wire frame = 390,400 B = 488 x 800; crop keeps
  the FIRST height rows (XnIRProcessor.cpp:281-282).
- Color wire size is exactly width x height x 2 (YUV422), no extra rows;
  since 0.2.4 read_color_frame fail-fasts on any deviation (VGA
  expected 614,400 B).
- rustup minimal profile lacks clippy/rustfmt — `rustup component add
  clippy rustfmt` after a fresh toolchain install.
- XN_SHIFTS_MAX_SHIFT = 2047 — the shift LUT covers the full packed-11
  range; 2047 = no-depth marker, 0 = invalid.
- A frame whose assembled size != expected must be dropped (partial
  first frame after stream start), per the C's OnEndOfFrame check.
- buf_size in the 12-byte stream packet header is BIG-ENDIAN (the only
  BE field there; the C swaps it alone).
- Zero-payload packets must be dispatched immediately when their header
  completes at a chunk boundary.
- PS1080 stream needs HIGH speed USB, ~12.7 MB/s payload (depth
  packed-11) / ~11.7 MB/s (IR packed-10); device clock ~60 MHz.
- Sync bulk reads are not enough: always keep the async 16-URB ring.
- rusb 0.9 has no async API; use libusb1-sys with raw pointers; keep the
  Context alive in a struct field.
- Bulk-IN read lengths must be a multiple of wMaxPacketSize (EOVERFLOW).
- Device replies can be SPLIT across multiple control-IN transfers;
  XnVersions wire quirks (minor/major first, 12-byte base authoritative);
  v2.6 replies have NO CRC; new request id per exchange (starting at 0);
  NOT_READY retries keep the id; the 12-byte bulk packet header INCLUDES
  the fw magic; EOF closes the frame on its last byte; packed-11 on the
  wire; ignore one chunk of garbage per endpoint on fw < 5.0.
- Xtion Pro Live: fw 5.8.22, chip 0x00212020 (PS1080-A6), FPGA 0x0005
  (RD1082), serial 1408090603; VGA@30 needs high-speed USB.
- GET_USB_CORE_TYPE (opcode 40) NACKed by some 5.x builds → assume Jango
  (XnDeviceSensorInit.cpp:233); the driver does the same.
- The CMOS blanking fetch (ALGORITHM_PARAMS 0x06) is REQUIRED before any
  stream produces packets (0.2.1 fix, verified).
- Opcode 42 (emitter) gated to fw >= 5.8.15, still NACKs BadParams on
  some builds — BadConfig, hardware test skips; power-cycle restores on.
- The CommandLink fake-transport unit tests need FULL reply packets
  (V26 header: magic LE, size words LE counting error word + data words,
  opcode LE, id LE, then error word, then data) — the first exchange's
  request id is always 0.

## 7. Open threads (if the user continues)

- **Re-run on hardware (immediate next action):** user pulls 3d9dc93 and
  runs `cargo test --test hardware -- --nocapture`. The color test now
  PRINTS the device's image-CMOS preset list first — paste that output.
  Expected paths: (a) an uncompressed (format 5) VGA@30 or QVGA@30 mode
  exists → the test streams it and validates the UYVY→RGB path;
  (b) only PS-compressed (format 1) modes exist → the test panics with
  "no uncompressed-YUV422 color mode" → next task: port
  XnStreamUncompressYUVImagePS (Uncomp.cpp:45+, 4-bit state machine with
  nLastFullValue[4] and channel/line tracking) as color::ps_decompress.
- After color passes: M8 cropping (params 46-55 image/depth + 56-60 IR,
  SizeX → SizeY → OffsetX → OffsetY → Mode last) per the turn-17 spec;
  optionally port the OpenStreamImpl extras (crop-mode 50=0 write,
  auto-exposure 80 / auto-white-balance 77 writes) for full C parity.
- Visual checks if not yet done: `cargo run --example ir_dump -- 10`
  (ir_frame.pgm) and `cargo run --example depth_dump -- 10`.
- Pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds from /home/z/my-project/.gh-token if /tmp was
  recycled; never print or commit the token).
- Golden vectors: `cargo test --test golden`.
