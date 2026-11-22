# Chat Session Backup — ASUS Xtion Linux research

**Version:** 21 (2026-11-22, turn 20): 0.2.3 IR wire-size fix pushed (a8c7942) - see turn 20. Previous: 20 (2026-11-22, turn 19): 0.2.2 diagnostics pushed (89f0671). Previous: 15 (2026-11-22, turn 14): firmware mirrored. Previous: 14 (2026-11-22, turn 13) · **Status: IR ROOT CAUSE FOUND AND FIXED — the MT9M001 IR CMOS transmits `height + 8` rows per frame. The 0.2.2 diagnostics run delivered the exact signature the fail-fast logic was built for: IR frames close at 390,400 bytes = 488 rows x 800 B (480 visible + 8 CMOS rows) vs the old 384,000-byte expectation, with the ring fully healthy (100 completions, 1,959,684 B, 0 failures, in_flight 16) and 5 full frames closed — a pure wire-size mismatch, NOT a transport/framing problem. phase1_identity_and_options PASSED this run (usb-core Jango fallback + emitter fw gate work on camera). C-parity fix shipped as 0.2.3 (a8c7942): `ir::wire_extra_rows` gates on fixed-params `depth_cmos_type` (MT9M001=1 -> +8 rows non-SXGA, AR130=2 -> -64 rows at SXGA, none/unknown -> 0; Include/PS1080.h:418); `ir_wire_size` expects `(height+8) x width x 10/8` = 390,400 at VGA; `decode_ir_frame` validates the wire size and unpacks ONLY the first `height` rows — the reference crops the trailing rows off the delivered frame ("don't report additional rows out", XnIRProcessor.cpp:281-282, keeping the FIRST height rows). Depth (packed-11) and color (YUV422) have NO extra rows (XnImageProcessor::CalculateExpectedSize = XRes*YRes*bpp) — untouched. Regression tests encode the hardware signature (390,400-byte wire accepted; synthetic 10-row wire decodes to rows 0..1 with rows 2..9 dropped). New `examples/ir_dump.rs` streams IR + saves ir_frame.pgm (16-bit PGM, 10-bit values << 6). 85 tests green (82 lib, 2 golden, 1 doc), clippy 0, fmt clean, PUSHED a8c7942 — ls-remote verified. NEXT RUN: user pulls a8c7942 and re-runs `cargo test --test hardware -- --nocapture` — ALL FOUR tests expected green; the "emitter toggle skipped" line is benign; then `cargo run --example ir_dump -- 10` for a visual check (values ~non-constant, saves ir_frame.pgm).**

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
- **Turn 20 (2026-11-22, this turn):** 0.2.3 — IR ROOT CAUSE + FIX. The
  user's next on-camera run returned the new signature:
  `ir read failed: ... frame #5 390400 bytes (expected 384000 for
  640x480 packed-10); assembler AssemblerStats { depth_start: 0,
  image_start: 5, buffer: 695, end: 5, frames_closed: 5 }, ring
  RingStats { completions: 100, bytes: 1959684, failures: 0,
  in_flight: 16, queued: 0 }`. Diagnosis: wire math — 390,400 = 488 rows
  x 800 B (640 px packed-10), i.e. **480 visible + 8 extra CMOS rows**;
  ring healthy, framing clean (5 full frames closed, first-frame partial
  tolerated). OpenNI 2.2.0 source re-verified (fresh download, sha256
  08f6842f...ed95a8, extracted /tmp/xtion-verify):
  - `XnIRProcessor::OnEndOfFrame` (XnIRProcessor.cpp:244-282):
    non-SXGA + `m_DepthCMOSType == XN_DEPTH_CMOS_MT9M001` →
    `actualHeight += 8` ("there are additional 8 rows (this is how the
    CMOS is configured)"); validates the unpacked frame against
    `width * actualHeight * bpp`, then
    `UnsafeSetSize(width * height * bpp)` — "don't report additional
    rows out": the FIRST height rows are kept, trailing 8 dropped.
    SXGA + AR130 → `actualHeight -= 64`.
  - `XnSensorIRStream::CalcRequiredSize` (XnSensorIRStream.cpp:411-422):
    "in IR, in all resolutions except SXGA, we get additional 8 lines"
    — host-side frame buffer accounts for them.
  - `XnImageProcessor::CalculateExpectedSize` = XRes*YRes*bpp: color has
    NO extra rows; depth likewise (hardware: packed-11 frames close at
    exactly 480 rows).
  - CMOS type enum (Include/PS1080.h:418-423): NONE=0, MT9M001=1,
    AR130=2; arrives in GET_FIXED_PARAMS (`nDepthCmosType`) — our
    FixedParams has parsed it at offset 56 since 0.1.2 (empirically
    validated then: ZPD/ZPPS/LDDIS matched the device).
  FIX (a8c7942, version 0.2.3): ir.rs DEPTH_CMOS_* consts +
  `wire_extra_rows(cmos, resolution)` + `ir_wire_size(width, height,
  cmos, resolution)` (390,400 for VGA MT9M001); `decode_ir_frame(data,
  width, height, resolution, cmos, ts)` validates wire size, refuses
  wire < visible (AR130-SXGA-style), unpacks only first `height` rows;
  device.rs read_ir_frame passes fixed_params.depth_cmos_type, failure
  message now prints `{width}x{wire_rows} packed-10: {height} visible +
  {extra} CMOS rows, depth cmos {cmos}`; new examples/ir_dump.rs (stats +
  ir_frame.pgm, 10-bit << 6 into a 16-bit P5 PGM); tests: CMOS-type
  matrix, 488-row wire size, synthetic 10-row wire crop, hardware
  signature acceptance (390,400 bytes OK), old tests updated; docs:
  PROTOCOL.md new section 8 "IR decoding" (Identity→9, file map→10),
  VALIDATION.md triage "IR SOLVED (0.2.3)", FEATURE_PLAN.md section 0
  item 3, README status+changelog. 85 tests green (82 lib, 2 golden,
  1 doc), clippy 0, fmt clean. NOTE: sandbox was recycled this turn —
  toolchain reinstalled (.cargo/.rustup), repo re-cloned from GitHub,
  token re-stored. rustup minimal profile needed
  `rustup component add clippy rustfmt`. cargo fmt reflowed a few
  pre-existing lines (toolchain-version formatting drift) — applied
  repo-wide.

## 3. Turn-20 evidence (the important part)

**Run 7 output (user's machine, 0.2.2):** phase1_identity_and_options ok,
open_handshake_and_identity ok, depth_stream_roundtrip ok, ir FAILED:
`frame #5 390400 bytes (expected 384000 for 640x480 packed-10)`,
assembler { depth_start: 0, image_start: 5, buffer: 695, end: 5,
frames_closed: 5 }, ring { completions: 100, bytes: 1959684,
failures: 0, in_flight: 16, queued: 0 } (both misc and ir ring views).

Interpretation (all verified):
- 390,400 = 488 × 800 → 488 wire rows of 640 packed-10 pixels; the old
  expectation 384,000 = 480 rows. Delta = exactly 8 rows = 6,400 B.
- Ring healthy: 100 completions ≈ 100 × 20,480 B URB slots ≈ 2 MB in
  ~0.17 s (5 frames @ 30 fps) → full packed-10 VGA rate; in_flight 16 =
  all URBs queued; 0 failures.
- Assembler closed 5 full frames (image_start 5 = end 5 = frames_closed
  5) — framing works; buffer 695 = partial 6th frame at snapshot time.
- Conclusion: device streams IR fine; the DRIVER expected the wrong
  frame size. Fail-fast worked exactly as designed.

**C reference (OpenNI 2.2.0, all file:line verified this turn):**
- XnIRProcessor.cpp:256-263: `else if (GetResolution() != SXGA) if
  (m_DepthCMOSType == XN_DEPTH_CMOS_MT9M001) actualHeight += 8;`
- XnIRProcessor.cpp:264-271: SXGA + AR130 → `actualHeight -= 64;`
- XnIRProcessor.cpp:273-282: validate `width * actualHeight * bpp`,
  then `UnsafeSetSize(width * height * bpp)` — crop keeps FIRST rows.
- XnSensorIRStream.cpp:411-422 CalcRequiredSize: "in IR, in all
  resolutions except SXGA, we get additional 8 lines" (`nYRes += 8`).
- XnSensorIRStream.cpp:148-152: IR CMOS config uses
  `XN_CMOS_TYPE_DEPTH` (the IR camera IS the depth CMOS) for non-SXGA.
- Include/PS1080.h:418-423: XnDepthCMOSType = NONE 0 / MT9M001 1 /
  AR130 2. Fixed params field `nDepthCmosType` (XnDeviceSensor.h:281;
  our parse: offset 56 in the V3 168-byte blob).
- XnImageProcessor.cpp:75-88: color expected size = XRes*YRes*bpp — no
  extra rows anywhere in the color pipe.

**Fix shipped (0.2.3, a8c7942):** see turn-20 history above. Files
changed: src/ir.rs, src/device.rs, examples/ir_dump.rs (new),
Cargo.toml (0.2.3), README.md, docs/PROTOCOL.md, docs/VALIDATION.md,
docs/FEATURE_PLAN.md (+ fmt reflow in src/protocol/algorithm.rs).

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-22-v21.md` | This file (v21, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-22_v21.zip` | Turn-20 zip (repo with .git at a8c7942 + this backup). Older zips kept in earlier sessions. |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (13 pages). |
| `xtion_rs_next_steps_and_validation.pdf` | Turn-17 validation plan (9 pages). |
| `xtion_rs_firmware_analysis.pdf` | Turn-15 firmware analysis (11 pages). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |

Earlier single-session artifacts (research report, cover HTML,
openni2-nojava package, setup/test scripts, patch files) were produced in
previous sessions; the authoritative state now lives in the git repo.

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = a8c7942 "0.2.3: fix IR wire size..."**; history:
89f0671 (0.2.2) → 766dd12 (docs triage v20) → a8c7942 (0.2.3); a docs
commit with this backup in docs/recovery/ follows it). Its
docs/recovery/ holds a copy of this backup.

## 5. Workspace state (2026-11-22, after turn 20)

- `/home/z/my-project/xtion-rs/` — full clone WITH .git at a8c7942
  (+ docs commit), 85 tests green, clippy 0, fmt clean. This turn
  changed: src/ir.rs (CMOS consts, wire_extra_rows, ir_wire_size,
  decode crop + 6 tests), src/device.rs (read_ir_frame CMOS-aware),
  examples/ir_dump.rs (new), Cargo.toml 0.2.3 + [[example]] ir_dump,
  README.md, docs/PROTOCOL.md (+section 8, renumber), docs/VALIDATION.md
  (triage), docs/FEATURE_PLAN.md (section 0 item 3),
  src/protocol/algorithm.rs (fmt only).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (stable,
  minimal profile + clippy + rustfmt components). Use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`.
- OpenNI 2.2.0 reference source: tarball verified at
  `firmware_dl/openni_ref/v2.2.0-debian.tar.gz` (sha256
  08f6842f20d1098ab2ebafadaac0130ffae5abd34cdf464bb6100cbe01ed95a8),
  extracted to `/tmp/xtion-verify` (RE-CREATE /tmp after recycle:
  `tar -xzf firmware_dl/openni_ref/v2.2.0-debian.tar.gz -C /tmp/
  xtion-verify --strip-components=1`; key files:
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

- **Re-run on hardware (the immediate next action):** user pulls a8c7942
  and runs `cargo test --test hardware -- --nocapture`. Expected: ALL
  FOUR tests green (open_handshake_and_identity,
  phase1_identity_and_options, depth_stream_roundtrip,
  ir_stream_roundtrip). The `emitter toggle skipped: ...` line is
  expected/benign. Then `cargo run --example ir_dump -- 10` — expect
  non-constant, non-black value ranges and `ir_frame.pgm` showing a
  recognizable IR scene (10-bit values scaled to 16-bit).
- If IR STILL fails with a different size: the error message now prints
  `{width}x{wire_rows} ... {height} visible + {extra} CMOS rows,
  depth cmos {cmos}` — compare against `ir_wire_size`; a size that is
  neither 384,000 nor 390,400 would mean a config deviation
  (check raw_dump, compare the IR command sequence vs the C).
- After IR passes: color stream validation (`start_color` +
  read_color_frame; YUV422→RGB888 implemented, untested on hardware),
  then M8 cropping (params 46-55 image/depth + 56-60 IR, sequence SizeX
  → SizeY → OffsetX → OffsetY → Mode last) per turn-17 spec, then the
  VALIDATION.md manual checks (emitter-off IR-mean collapse etc.).
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds from /home/z/my-project/.gh-token if /tmp was
  recycled; never print or commit the token).
- Golden vectors: `cargo test --test golden` (reference/golden.txt is
  committed; re-generate with gcc if the C reference changes).
