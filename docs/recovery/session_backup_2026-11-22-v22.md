# Chat Session Backup — ASUS Xtion Linux research

**Version:** 22 (2026-11-22, turn 21): 0.2.4 color hardware test + color fail-fast
pushed (2ccd1d1). Previous: 21 (turn 20): 0.2.3 IR wire-size fix (a8c7942).
**Status: IR CONFIRMED ON HARDWARE — the user reports the tests now PASS (0.2.3,
all four hardware tests green). Next implementation step shipped as 0.2.4
(2ccd1d1): `read_color_frame` now validates the YUV422 wire size
(width*height*2, NO extra CMOS rows — XnImageProcessor::CalculateExpectedSize)
and fails fast after 5 unusable frames with assembler + ring counters (the
0.2.2 IR lesson); zero-data timeouts print ring counters; `color_ring_stats()`
accessor added; NEW hardware test `color_stream_roundtrip` (VGA@30 YUV422 ->
RGB888, `start_ir` while color runs must be BadConfig, non-constant-frame
check). 85 tests green (82 lib + 2 golden + 1 doc), clippy 0 (-D warnings),
fmt clean, hardware target compiles (--no-run). PUSHED 2ccd1d1. NEXT RUN:
user pulls 2ccd1d1 and runs `cargo test --test hardware -- --nocapture` —
FIVE tests now; the four known ones should stay green, `color_stream_roundtrip`
runs on hardware for the FIRST time — if it fails, the fail-fast message now
prints the exact wire size + assembler + ring stats (color expected =
614,400 = 640x480x2). Then `cargo run --example ir_dump -- 10` for the IR
visual check if not done yet.**

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
  `openni2-nojava` package (no Java deps, fully sandbox-verified).
- **Turn 6 (2026-11-22):** Rust-stack options report (Options A-E); verified
  Option A (openni2 crate over the C stack) end-to-end in sandbox; delivered
  `asus_xtion_rust_stack_options.pdf` + `rust-xtion-check/` crate.
- **Turn 7 (2026-11-22):** Option D chosen — pure-Rust driver written,
  44 tests green incl. C-reference golden vectors, pushed to
  github.com/thomas7475/xtion-rs (commit deeb688).
- **Turn 8 (2026-11-22):** first on-hardware run failed at the version
  handshake — replies arrive SPLIT across multiple vendor control-IN
  transfers; ported the C's full Execute/ReceiveReply semantics; 0.1.1
  (823dd53) + recovery update (4afa6e3).
- **Turn 9 (2026-11-22):** second on-hardware run: control path fully works
  (fw 5.8.22, PS1080-A6 0x00212020, FPGA 5, serial 1408090603, ConstShift
  200, ZPD 120 mm, ZPPS 0.1052 mm, LDDIS 7.5 cm, dCMOS-rCMOS 2.63 cm,
  hFOV 58.59°). Fixed: whole-frame buffer (422,411 % 512 = 11) → EOVERFLOW
  → permanent 40×maxpacket chunks; parallel-test Usb(Busy) → mutex +
  DeviceBusy error. 0.1.2 (ca85e8e) + recovery (7b84d21).
- **Turn 10 (2026-11-22):** third on-hardware run: stream runs, no
  Overflow/Busy, but frames were fragments (736..45424 px, ~4% bytes
  surviving) → ported the C's async 16-URB ring. 0.1.3 (00909ed).
- **Turn 11 (2026-11-22):** fourth run: ring perfect (12.05 MB/s) but
  frames EXACTLY 24560/307200 px → buf_size big-endian quirk +
  zero-payload dispatch bug. 0.1.4 (6720e97).
- **Turn 12 (2026-11-22):** fifth run: FULL frames @30fps, 0 valid pixels
  → shift LUT max_shift 1024→2047 + corrupt-frame drop + histogram.
  0.1.5 (3948f07).
- **Turn 13 (2026-11-22):** SIXTH RUN GREEN — driver validated end-to-end
  on depth (87.4% valid pixels, 431 mm min, 12.17 MiB/s, 0 failures,
  29.97 fps). Delivered xtion_rs_driver_overview.pdf (13 pages, 8 chapters,
  4 verified flow diagrams).
- **Turn 15 (2026-11-22):** FIRMWARE DECOMPILED + ANALYZED (Ghidra 11.3.2
  headless, ARM Thumb, 165 functions; 41 opcodes + 85 params + 15 packet
  types mapped value-for-value vs OpenNI). Official ASUS fw 5.8.22 SFX
  downloaded from dlcdnet.asus.com, CAB extracted via libarchive, mirrored
  to xtion-rs/firmware/. Beyond-OpenNI surface catalogued.
- **Turn 16 (2026-11-22):** 0.2.0 — OpenNI gap analysis + implementation:
  identity opcodes (platform 39, usb-core 40, emitter 42), IR stream
  (STREAM0 mode 3 on the image pipe, packed-10 unpack), DeviceEvent capture
  (0xf31f OVERHEAT / 0xdead PROJECTOR_FAULT), +20 device methods, 13 Param
  ids; hardware.rs gained phase1_identity_and_options + ir_stream_roundtrip.
  PERMANENT EXCLUSIONS: flash FS 13-25, I2C/AHB writes, calibrations
  29/31/33, TAKE_SNAPSHOT 12, SET_MODE 6. e62efbe.
- **Turn 17 (2026-11-22):** beyond-OpenNI analysis + docs/VALIDATION.md test
  plan (4 automated hw tests, 10 manual checks incl. emitter-off IR-mean
  collapse); next spec = firmware cropping (params 46-55 image/depth,
  56-60 IR; SizeX→SizeY→OffsetX→OffsetY→Mode last). c3a5fc9.
- **Turn 18 (2026-11-22):** 0.2.1 — user's first `cargo test --test
  hardware` run: depth+handshake passed, phase1 failed at
  usb_core_type Nack(InvalidCommand) → OpenNI ignores it and "assumes
  JANGO" (XnDeviceSensorInit.cpp:233) → driver now falls back to
  UsbCore::Jango on any probe failure. IR failed at first frame
  ControlTimeout → start_ir was MISSING the CMOS blanking fetch
  (ALGORITHM_PARAMS type 0x06, 16 bytes, per (res,fps)) without which the
  fw ACKs everything but never arms sensor timing → read_cmos_blanking
  added to start_ir/start_depth/start_color. 5f7eaa6.
- **Turn 19 (2026-11-22):** 0.2.2 — on-camera run: BLANKING FIX CONFIRMED
  (image endpoint carried 1782 chunks / 34.9 MB in 3 s ≈ 11.6 MB/s;
  previously ZERO bytes). IR read loop still failed: frames silently
  dropped until timeout (the "control transfer timed out" text was
  ControlTimeout's Display, NOT a real control error). FIX: StreamAssembler
  counters (AssemblerStats: depth/image start, buffer, end, frames_closed)
  + read_ir_frame fail-fast after 5 unusable frames with actual size +
  assembler + ring stats in the message. phase1 progressed past usb-core,
  then set_emitter_enabled NACK BadParams → fw gate (opcode 42 only on
  fw >= 5.8.15) → hardware test skips emitter when gated; prints cam.version.
  89f0671 + docs triage 766dd12.
- **Turn 20 (2026-11-22):** 0.2.3 — IR ROOT CAUSE + FIX. The 0.2.2 run's
  signature: `frame #5 390400 bytes (expected 384000 for 640x480
  packed-10)` with the ring healthy (100 completions, 0 failures) and 5
  full frames closed. 390,400 = 488 × 800 → **the MT9M001 IR CMOS
  transmits height + 8 rows** in every resolution except SXGA (AR130:
  -64 at SXGA), verified against OpenNI 2.2.0 (XnIRProcessor.cpp:244-282
  "there are additional 8 rows (this is how the CMOS is configured)";
  XnSensorIRStream.cpp:411-422 CalcRequiredSize `nYRes += 8`;
  Include/PS1080.h:418-423 CMOS type enum; fixed-params nDepthCmosType at
  offset 56 of the V3 168-byte blob, parsed since 0.1.2). Depth (packed-11)
  and color (YUV422) have NO extra rows (XnImageProcessor::
  CalculateExpectedSize = XRes*YRes*bpp). FIX (a8c7942): ir.rs
  DEPTH_CMOS_* consts + wire_extra_rows(cmos, resolution) +
  ir_wire_size (390,400 for VGA MT9M001); decode_ir_frame validates wire
  size, unpacks only the FIRST height rows (crop, "don't report additional
  rows out" XnIRProcessor.cpp:281-282); read_ir_frame passes
  fixed_params.depth_cmos_type, failure prints the row math; new
  examples/ir_dump.rs (stats + ir_frame.pgm, 10-bit << 6 into 16-bit P5);
  tests: CMOS matrix, 488-row wire size, synthetic 10-row crop, hardware
  signature acceptance; PROTOCOL.md section 8, VALIDATION/FEATURE_PLAN/
  README updates.
- **Turn 21 (2026-11-22, this turn):** 0.2.4 — user CONFIRMS the hardware
  tests pass (0.2.3). Continued implementation: color on-hardware step.
  `read_color_frame` (device.rs) upgraded: expected = width*height*2
  (YUV422, no extra rows, XnImageProcessor.cpp:75-88), fail-fast after 5
  unusable frames with assembler + ring stats in the message, timeout
  branch logs ring counters; `color_ring_stats()` accessor (device.rs
  ~line 737). tests/hardware.rs gained `color_stream_roundtrip`:
  start_color(Vga, 30, Yuv422) → start_ir must be BadConfig (shared
  image pipe) → read_color_frame(5 s) → 640x480, data.len() == 640*480*3,
  byte range non-constant → drain_events → stop_color. Cargo.toml 0.2.4;
  README status (IR hardware-validated, color test note) + changelog 0.2.4;
  VALIDATION.md triage: IR CONFIRMED on hardware + color fail-fast
  contract. 85 tests green (82 lib, 2 golden, 1 doc), clippy 0
  (-D warnings), fmt clean, hardware target compiles (--no-run).
  PUSHED 2ccd1d1 (+ this docs commit).

## 3. Hardware test inventory (the user's current ask)

`tests/hardware.rs` — FIVE tests (serialized by a global device mutex;
each self-skips when no camera is attached):

1. `open_handshake_and_identity` — open, fw version (>= 3), serial,
   fixed-params calibration sanity.
2. `depth_stream_roundtrip` — VGA@30 packed-11 depth frame: 640x480,
   > 1000 valid pixels, stop.
3. `phase1_identity_and_options` — platform string, usb-core probe
   (Jango fallback), registration/hole-filter/mirror/close-range
   roundtrips, emitter toggle (skipped if NACKed).
4. `ir_stream_roundtrip` — IR VGA frame (390,400-byte wire → 480 visible
   rows), non-constant, event drain, stop.
5. `color_stream_roundtrip` — NEW 0.2.4: VGA@30 YUV422 → RGB888
   (614,400-byte wire → 921,600 RGB bytes), start_ir-during-color must
   be BadConfig, non-constant, event drain, stop.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- color_stream_roundtrip --nocapture`

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-22-v22.md` | This file (v22, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-22_v22.zip` | Turn-21 zip (repo with .git at 2ccd1d1 + docs commit + this backup). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (13 pages). |
| `xtion_rs_next_steps_and_validation.pdf` | Turn-17 validation plan (9 pages). |
| `xtion_rs_firmware_analysis.pdf` | Turn-15 firmware analysis (11 pages). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 2ccd1d1 "0.2.4: color stream hardware test + read_color_frame
fail-fast"**; history: a8c7942 (0.2.3) → b4fee1e (docs v21) → 2ccd1d1
(0.2.4); a docs commit with this backup in docs/recovery/ follows it).

## 5. Workspace state (2026-11-22, after turn 21)

- `/home/z/my-project/xtion-rs/` — full clone WITH .git at 2ccd1d1
  (+ docs commit), 85 tests green, clippy 0, fmt clean. This turn
  changed: src/device.rs (read_color_frame fail-fast + color_ring_stats),
  tests/hardware.rs (+color_stream_roundtrip), Cargo.toml (0.2.4),
  README.md, docs/VALIDATION.md.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (stable,
  minimal profile + clippy + rustfmt). Use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`.
- OpenNI 2.2.0 reference source: tarball verified at
  `firmware_dl/openni_ref/v2.2.0-debian.tar.gz` (sha256
  08f6842f20d1098ab2ebafadaac0130ffae5abd34cdf464bb6100cbe01ed95a8);
  extract after recycle:
  `tar -xzf firmware_dl/openni_ref/v2.2.0-debian.tar.gz -C /tmp/
  xtion-verify --strip-components=1` (key files:
  Source/Drivers/PS1080/Sensor/*, Include/PS1080.h).
- GitHub token: `/home/z/my-project/.gh-token` (chmod 600, gitignored,
  never printed/committed). Push via
  `umask 077 && printf 'https://thomas7475:%s@github.com\n'
  "$(cat /home/z/my-project/.gh-token)" > /tmp/gh-creds` then
  `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`
  (from their test output paths).

## 6. Gotchas learned (cumulative, new first)

- **The IR CMOS (MT9M001) transmits `height + 8` wire rows** in every
  resolution except SXGA (AR130: -64 at SXGA; color and depth pipes have
  NO extra rows). VGA IR wire frame = 390,400 B = 488 x 800. Expected
  size comes from fixed-params `depth_cmos_type`; the crop keeps the
  FIRST `height` rows and drops the trailing 8 (XnIRProcessor.cpp:281-282).
- **Color wire size is exactly width x height x 2 (YUV422)** — no extra
  rows anywhere in the color pipe (XnImageProcessor.cpp:75-88). Since
  0.2.4 read_color_frame fail-fasts on any deviation; expected VGA value
  = 614,400 B.
- **rustup minimal profile lacks clippy/rustfmt** — run
  `rustup component add clippy rustfmt` after a fresh toolchain install.
- **XN_SHIFTS_MAX_SHIFT = 2047** — the shift LUT must cover the full
  0..2047 packed-11 range; 2047 = no-depth marker, 0 = invalid; for the
  reference calibration valid = shifts 1..1052 (315..9870 mm),
  0.5-4 m → 403..1001.
- **A frame whose assembled size != expected must be dropped** (the
  partial first frame after stream start), per the C's OnEndOfFrame check.
- **buf_size in the 12-byte stream packet header is BIG-ENDIAN** (the
  only BE field; the C applies an unconditional xnOSEndianSwapUINT16 to
  it alone).
- Zero-payload packets must be dispatched immediately when their header
  completes at a chunk boundary, or frame closes lag/lose the last frame.
- The PS1080 stream works on HIGH speed USB at ~12.7 MB/s payload (depth
  packed-11) / ~11.7 MB/s (IR packed-10 incl. extra rows); the device
  clock ticks ~60 MHz (≈2,002,155 ticks per 30 fps frame period).
- **Sync bulk reads are not enough for PS1080 streams**: the device
  drops data when no URB is queued. Always keep a ring of pre-submitted
  async URBs (16 × 40 × maxpacket, re-submit in the callback).
- rusb 0.9 has no async API; use libusb1-sys (rusb's backend crate) with
  `DeviceHandle::as_raw()` / `Context::as_raw()`; keep the rusb Context
  alive in a struct field for pointer lifetime.
- libusb1-sys: `libusb_transfer_cb_fn` is a bare fn pointer (no Option);
  transfer status constants live in `libusb1_sys::constants`.
- Raw pointers across `thread::spawn`: pass as `usize` and cast in the
  closure (edition-2021 disjoint capture ignores Send-wrappers when you
  access `.0`).
- clippy `vec_box` on `Vec<Box<Slot>>` may be load-bearing: Box pins
  each slot's address for libusb `user_data`.
- Bulk-IN read lengths must be a multiple of wMaxPacketSize (EOVERFLOW).
- rusb 0.9.4 `read_bulk` returns Ok(n) on timeout with partial data.
- Only one handle (per process) can claim a USB interface → Busy.
- Device replies can be SPLIT across multiple control-IN transfers;
  XnVersions wire quirks (minor/major first, 12-byte base authoritative);
  v2.6 replies have NO CRC; new request id per exchange; NOT_READY
  retries keep the id; the 12-byte bulk packet header INCLUDES the fw
  magic; EOF closes the frame on its last byte; packed-11 on the wire;
  ignore one chunk of garbage per endpoint on fw < 5.0.
- Xtion Pro Live ships fw 5.8.22, chip 0x00212020 (PS1080-A6), FPGA
  0x0005 (RD1082); 640x480@30 needs full-speed-impossible bandwidth —
  high-speed USB only.
- GET_USB_CORE_TYPE (opcode 40) is NACKed by some 5.x builds — OpenNI
  ignores the failure and assumes Jango (XnDeviceSensorInit.cpp:233);
  the driver does the same.
- The CMOS blanking fetch (ALGORITHM_PARAMS type 0x06) is REQUIRED
  before any stream will produce packets (0.2.1 fix).
- Opcode 42 (emitter) is gated to fw >= 5.8.15 and still NACKs BadParams
  on some builds — driver reports BadConfig, hardware test skips.
- rusb 0.9 devices() needs .iter(); thiserror #[from] needs
  std::error::Error impl; blocked hosts (freedesktop GitLab,
  packages.debian.org) vs working (deb.debian.org, crates.io, github,
  apache.org); OPENNI2_REDIST build-time env; model cannot read images →
  programmatic PDF checks.

## 7. Open threads (if the user continues)

- **Re-run on hardware (the immediate next action):** user pulls 2ccd1d1
  and runs `cargo test --test hardware -- --nocapture`. Expected: the
  four known tests stay green; `color_stream_roundtrip` runs for the
  first time on camera. If color fails: the fail-fast message prints the
  actual wire size + assembler + ring stats — a size that is not 614,400
  would mean a color-pipe config deviation (compare vs the C's image
  command sequence; check ImageFormat/ImageResolution/ImageFps writes
  and the blanking fetch).
- After color passes: M8 cropping (params 46-55 image/depth + 56-60 IR,
  sequence SizeX → SizeY → OffsetX → OffsetY → Mode last) per the
  turn-17 spec in docs/FEATURE_PLAN.md, then the VALIDATION.md manual
  checks (emitter-off IR-mean collapse, registration edge alignment,
  clean-stop, mutual exclusion device-side half, etc.).
- Visual checks if not yet done: `cargo run --example ir_dump -- 10`
  (ir_frame.pgm) and `cargo run --example depth_dump -- 10`.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds from /home/z/my-project/.gh-token if /tmp was
  recycled; never print or commit the token).
- Golden vectors: `cargo test --test golden` (reference/golden.txt is
  committed; re-generate with gcc if the C reference changes).
