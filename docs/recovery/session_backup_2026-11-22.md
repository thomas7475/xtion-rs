# Chat Session Backup — ASUS Xtion Linux research

**Version:** 20 (2026-11-22, turn 19): 0.2.2 diagnostics pushed (89f0671) - see turn 19. Previous: 19 (turn 18): 0.2.1 blanking/Jango fixes (5f7eaa6). Previous: 15 (2026-11-22, turn 14): firmware mirrored. Previous: 14 (2026-11-22, turn 13) · **Status: SIXTH RUN GREEN - DRIVER HARDWARE-VALIDATED: both integration tests pass (268,435/307,200 = 87.4% valid pixels, min 431 mm, shift range 291-797 = 430-1,181 mm, 12.7% no-depth markers, 12.17 MiB/s, 0 failures, 29.97 fps). The 0.1.5 LUT fix confirmed; the earlier 0-valid scene was beyond the old table's range. Delivered xtion_rs_driver_overview.pdf (13 pages, 8 chapters, 4 verified flow diagrams). NOTE: sandbox recycled - /tmp/gh-creds gone, no token in .env, so the M5 docs update could NOT be pushed; delivered as xtion-rs-m5-validated.patch instead. Earlier: fifth on-hardware run: buf_size fix CONFIRMED — full 307200-px frames at 30 fps, 12.13 MB/s, 0 failures — but every pixel decoded 0/invalid. Full decode-chain audit vs the C found ONE confirmed bug: shift LUT was built with device_max_shift=1024 instead of XN_SHIFTS_MAX_SHIFT=2047 (packed-11 spans 0..2047; valid window for this calibration = shifts 1..1052 = 315..9870 mm; old table zeroed all >= 1024 i.e. 5.5-9.9 m). BUT the exact Python simulation shows a normal 0.5-4 m scene sits at shifts 403..1001 — INSIDE the old table — so max_shift alone cannot explain 0 valid pixels; remaining suspects: all-invalid stream (scene > 5.5 m / too close < 31.5 cm / occluded / emitter) or something not yet found. Added per-frame raw-shift histogram (DepthFrame::shift_stats + depth_dump) — the NEXT RUN's histogram line decides. Also ported the C's corrupt-frame drop (partial first frames like 150024 px). 0.1.5 (3948f07); user pulls, points camera at a wall ~1-2 m, re-runs depth_dump**

> Purpose: upload this file in a new chat to restore full context. It records
> the standing rules, the task, everything that was found and decided, every
> file that was produced, and the state of the current workspace.

## 1. Standing user rules (apply to EVERY future turn)

1. **Communicate in English only** (all replies, all document content).
2. **Zip backup of generated files to `download/`** every turn.
3. **Produce a downloadable session backup** (this file, refreshed with a new
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
- **Turn 13 (2026-11-22):** SIXTH RUN GREEN
- **Turn 19 (2026-11-22, this turn):** SECOND ON-CAMERA RUN: BLANKING FIX
  CONFIRMED (1782 chunks / 34.9 MB in 3 s = 11.6 MB/s = exactly packed-10
  VGA@30 on the image endpoint; previously ZERO bytes). Two failures remain
  with new signatures: (1) IR - read loop silently dropped every frame
  until the 3 s timeout ("control transfer timed out after retries" is
  ControlTimeout's Display text, error.rs:89 - NOT a real control error;
  pump_misc is pure queue-draining, no control calls). Framing logic
  verified symmetric (close_if_eof handles IMAGE_END; pop_frame has no
  kind filter); cause is in actual wire content - either frames close at
  wrong size (silent drop) or no STARTs seen. FIX: StreamAssembler
  counters (AssemblerStats: depth/image start, buffer, end, frames_closed)
  + read_ir_frame fail-fast after 5 unusable frames with actual size,
  assembler stats and ring stats in the error message (first frame can be
  partial: ring starts after stream-mode write). (2) phase1 - progressed
  past usb-core fallback ("usb core: Jango"), then set_emitter_enabled
  NACK BadParams. Request verified byte-identical to OpenNI
  (XnVSetEmitterStateRequest = one u16; gate: opcode 42 assigned only on
  fw >= 5.8.15, XnHostProtocol.cpp:660-664; caller XnSensor.cpp:793).
  Interpretation: NACK=command EXISTS but params rejected on that build -
  cannot fix blind; FW gate added to driver (fw < 5.8.15 -> clean
  BadConfig), hardware test reports skip instead of failing, phase1 now
  prints cam.version (user's exact fw unknown so far - version print will
  reveal it). 0.2.2 pushed 89f0671; clippy clean; 86 tests green (79 lib
  incl. assembler counters test). NEXT RUN OUTCOMES: IR error message
  shows either "framing never completes" (start/end counters) or actual
  frame size vs 384000 - the latter is a one-line expected_wire_size fix.
  Emitter skip line is expected/benign. VALIDATION.md triage updated.

- **Turn 18 (2026-11-22):** ON-CAMERA TEST FAILURES FIXED, PUSHED
  (5f7eaa6). User ran cargo test --test hardware on their machine (fw 5.8.22,
  platform RD108x-112): depth + handshake passed, phase1 FAILED at
  usb_core_type Nack(InvalidCommand), ir FAILED at first frame
  ControlTimeout. DIAGNOSIS (call-site verified): (A) OpenNI probes
  GET_USB_CORE_TYPE (opcode 40) at open and deliberately IGNORES failure -
  'assume JANGO' (XnDeviceSensorInit.cpp:233); several fw 5.x builds NACK
  it; xtion-rs treated the NACK as fatal. FIX: usb_core_type() falls back
  to UsbCore::Jango on any probe failure. (B) start_ir was MISSING the
  CMOS blanking fetch all three reference streams issue at configure:
  SetCmosConfig -> ALGORITHM_PARAMS type 0x06 (BLANKING), 16 bytes
  (2x{f32,f32} XnCmosBlankingInformation), requested per (res,fps)
  (XnSensorIRStream.cpp:147-152; depth stream tail does it too - explains
  why depth worked: boot defaults coincide). Without it fw ACKs everything
  but never arms sensor timing -> zero IR packets. FIX:
  protocol::algorithm::read_cmos_blanking (generic read_algorithm_params
  machinery reused), issued in start_ir + start_depth + start_color
  (parity). Wire: request {u16 0x06, u16 0, u16 res, u16 fps, u16 offset}
  opcode 22. Also: read_ir_frame logs ring stats on timeout +
  pub ir_ring_stats() (completions==0 => device sent nothing);
  hardware.rs ir test prints stats on failure; VALIDATION.md triage +
  FEATURE_PLAN.md section 0 updated; version 0.2.1. 85 tests green
  (78 lib incl. IR-sequence 5-command assert + blanking parse, 4 golden,
  2 doc, 1 noto). Patch file: download/xtion-rs-0.2.1-fixes.patch.
  NOTE: fixes NOT yet re-validated on camera - user re-runs
  cargo test --test hardware; if IR still fails with completions==0,
  next suspect is the image endpoint claim/URB path.

- **Turn 17 (2026-11-22):** BEYOND-OPENNI ANALYSIS + VALIDATION
  PLAN, PUSHED (c3a5fc9). Sandbox survived. QUESTION: firmware capabilities
  OpenNI 2.2.0 never uses? METHOD: call-site grep of every
  XnHostProtocol* wrapper (not header defines) + AddFirmwareParam registry
  diff vs XnParams.h ids + XnFirmwareStreams packet switch. FINDINGS:
  audio capture disabled at factory (TODO: enable, XnSensor.cpp ~470);
  plain GMC packet 0xa200 UNROUTED (only 0xb200 debug); file-upload trio
  13/14/15 never called (download/read/delete/execute ARE); KILL 999 and
  TAKE_SNAPSHOT 12 never called (12 defined at XnHostProtocol.cpp:2440,
  zero callers); params 0/3/4/31/32/85 unmapped (85 only inside flash
  lock at :2036); V110/V300 enums = dead ancient dialects. VALIDATION:
  crop modes confirmed in Include/PS1080.h (DISABLED=0, NORMAL=1,
  INCREASED_FPS=2 - crop can RAISE fps!). DELIVERED: docs/VALIDATION.md
  (full camera test plan: 4 automated hardware tests + M5 run-6 pass
  criteria, 10 manual checks incl. emitter-off IR-mean collapse test,
  regression matrix, triage); FEATURE_PLAN.md section 5 (beyond-OpenNI
  table); PDF report download/xtion_rs_next_steps_and_validation.pdf
  (9 pages, Template 01 cover, TOC fixed to displayed numbering, pdf_qa
  clean) + cover HTML. NEXT STEP (spec'd in report section 4): firmware
  cropping - params 46-55 (image/depth) + 56-60 (IR), sequence SizeX ->
  SizeY -> OffsetX -> OffsetY -> Mode last, host-side processors crop +
  report cropped dims, mirror interplay (offset in mirrored space), API
  start_depth_cropped/set_*_crop, 5-part test plan. 84 tests green;
  camera still not attached (on-camera validation remains first task
  next hardware session).

- **Turn 16 (2026-11-22):** 0.2.0 - OPENNI GAP ANALYSIS +
  IMPLEMENTATION, PUSHED (e62efbe). SANDBOX WAS RECYCLED: xtion-rs/ and
  the Rust toolchain were gone; recovered the repo WITH git history from
  download/..._v15.zip's embedded tmp/xtion-src-v15.zip, re-stored
  .gh-token (600, gitignored), installed Rust 1.100 via rustup-init into
  my-project/.cargo+.rustup (piping-to-shell is blocked; direct binary),
  re-extracted OpenNI 2.2.0 from the surviving tarball in
  firmware_dl/openni_ref. ANALYSIS (from OpenNI source): IR stream =
  STREAM0_MODE=3 on the IMAGE pipe (not depth!) as IMAGE packets with
  4x10-bit packed pixels (XnIRProcessor::Unpack10to16); registration =
  PARAM 2 done IN FIRMWARE on PS1080 (DecideFirmwareRegistration);
  platform string (opcode 39) = one char per u16 word; usb core type
  (40) = single u16 (Jango=0/gadgetfs=1); emitter (42) = {u16 active};
  image controls: exposure 80/81, AWB 77, gain=AGC 15, quality 16,
  flicker 17, mirror 71; depth: mirror 23, hole filter 22, AGC 21, close
  range 84 (fw>=5.6), GMC 36; events 0xf31f/0xdead -> device error state
  (XnFirmwareStreams.cpp); audio is TODO-disabled even in OpenNI 2.2.0.
  IMPLEMENTED (docs/FEATURE_PLAN.md Phases 1-2): protocol/identity.rs
  (platform/usb-core/emitter), stream.rs DeviceEvent capture, ir.rs
  (10-bit unpack + IrConfig/IrFrame, golden vectors Python-verified),
  device.rs (+20 methods, IR stream, misc-endpoint 0x83 ring while
  streaming, event draining), usb.rs misc mps + best-effort interface
  claims 1-2, 13 new Param ids + wire-contract test, Cargo 0.2.0.
  EXCLUDED PERMANENTLY (user rule: no writes that could delete
  calibration or overwrite configs): flash file system 13-25, I2C/AHB
  writes, calibrations 29/31/33, TAKE_SNAPSHOT 12, SET_MODE 6. TESTS:
  84 green without hardware (77 lib incl. new wire/golden/event tests;
  clippy 0 warnings; fmt clean); hardware.rs gained
  phase1_identity_and_options + ir_stream_roundtrip (skip without
  camera - camera NOT attached this turn, on-camera validation still
  pending). Docs: FEATURE_PLAN.md (30-feature gap table, 4 phases,
  exclusions), README Status/Usage updated, PLAN.md M7 partial,
  FIRMWARE.md corrected (serial was already used; IR is STREAM0 not
  STREAM1).

- **Turn 15 (2026-11-22):** FIRMWARE DECOMPILED + ANALYZED,
  ALL PUSHED TO GITHUB. User supplied a PAT (stored /home/z/my-project/
  .gh-token, chmod 600, gitignored, never echoed/committed). Pushed
  firmware mirror (b6a4944), M5 docs (5614195), and the new analysis
  (8a129b5) to github.com/thomas7475/xtion-rs - ls-remote verified.
  ANALYSIS: .fla images are Packer-wrapped ARM Thumb (ARM926-class, TI
  toolchain, UTF-16LE strings). Ghidra 11.3.2 headless (downloaded 446MB +
  Temurin JDK 21 - sandbox had JRE only; language ARM:LE:32:v5t, TMode
  forced, 299 capstone-verified seeds): 165 functions, 44,940
  instructions, 149 decompiled (881KB C dump, kept OUT of the repo as a
  derived work; artifacts in firmware_dl/analysis/). KEY FINDINGS:
  opcode dispatcher located at image 0x21400-0x22000 (78 Thumb cmp
  covering 31 opcodes, value-for-value with OpenNI XnHostProtocol.h);
  v2.6 magic pair MG/BR at 0xc14/0xc16; boot self-test sequence from
  UTF-16 strings (TEC/PRJ/CLR/IR/FL/NESA + ZO per-frame quality monitor);
  USB surface: depth 0x81, image 0x82, misc 0x83, USB Audio Class
  "Microphone" + Feature Unit. Full vendor surface (OpenNI 2.2.0 source
  from occipital/OpenNI2 v2.2.0-debian + same-vintage
  XnDeviceSensorV2.dll strings): 41 opcodes + 85 params + 15 packet
  types + 6 algorithm tables. xtion-rs uses 6/8/6/1. UNUSED FUNCTIONALITY
  (the user's goal): audio mic (PCM 16-bit 8-48kHz stereo + volume/mute),
  IR stream (params 25-29/56-60/72), registration, ENABLE_EMITTER 42,
  OVERHEAT 0xf31f + PROJECTOR_FAULT 0xdead packets (currently skipped!),
  depth AGC+bins/hole-filter/mirror/decimation/close-range, image
  exposure/WB/sharpness/flicker/crop, accelerometer (ACC LR/ACCOR
  strings), TEC/emitter calibration + BIST + CPU stats + GET_LOG, flash
  file system (opcodes 13-25, NACK OVERLAY_NOT_LOADED), I2C/AHB raw
  access, GMC motion points, serial/platform-string/USB-core-type
  identity opcodes. DELIVERABLES: docs/FIRMWARE.md (repo, pushed),
  xtion_rs_firmware_analysis.pdf (11 pages, QA PASS, download/),
  crossref.md + scripts/firmware_*.py (triage/strings16/archtest/
  constants/dispatch/crossref + FirmwareGhidra.py + download_ghidra.sh).
  Recommended driver roadmap: 1) OVERHEAT/PROJECTOR_FAULT events, 2)
  serial/platform/USB-core, 3) IR stream, 4) registration, 5) emitter
  on/off, 6) mirror/close-range/hole-filter/AGC, 7) audio, 8) log/TEC/
  CPU stats. Found the last
  official ASUS firmware (FWUpdate 5.8.22, 2013) still live on
  dlcdnet.asus.com; downloaded (2.25 MB, sha256 6f9c0ec9...43caad), extracted
  the LZX-compressed CAB inside the SFX exe via libarchive (no cabextract
  in sandbox): 15 .fla flash images (source.out.bin.fla = main 5.8.22
  image, BootManager, 13 params), UsbUpdate_RD108.ini flash map,
  Windows updater + OpenNI1 DLLs + NESIDB.txt (not mirrored). Built
  xtion-rs/firmware/ (README with provenance/flash map/license caveat,
  checksums.txt, original zip, extracted 5.8.22/). Delivered
  xtion-rs-firmware.zip; push NOT possible (no token) - user must push. — driver validated
  end-to-end. Analyzed the run (87.4% valid, clean histogram), delivered
  the PDF driver overview with 4 geometry-verified flow diagrams
  (xtion_rs_driver_overview.pdf + cover HTML). Sandbox recycled the
  xtion-rs tree (restored from zip v12's xtion-src.zip) and the GitHub
  token; M5 docs update delivered as a patch file, NOT pushed.

## 3. Turn-12 evidence, audit and fix (the important part)

**Run 5 output (user's machine):** depth_dump frames 2-30 assemble at
EXACTLY 307,200 px @ 30 fps (ts +2,002,155/frame again), ring 676
completions / 13,655,112 B / 1.07 s = 12.13 MB/s, 0 failures — the
buf_size fix held; transport + assembly are DONE. Remaining: 0 valid
pixels, min 0 mm on every full frame. First frame was partial
(146,184 px; the hw test's first frame 150,024 px) and was DELIVERED
(the C would drop it).

**Decode-chain audit vs the C (all verified, file:line in PROTOCOL.md):**
- packed-11 unpack: bit-for-bit identical to
  XnPacked11DepthProcessor::Unpack11to16.
- Wire payload 425,367 B/frame ≈ 422,400 + headers = 640x480x11/8 → the
  device sends packed-11; XN_IO_DEPTH_FORMAT_UNCOMPRESSED_11_BIT = 3 IS
  the packed format ("uncompressed" = no PS-compression; enum has no
  separate PACKED_11). My DepthFormat enum matches the C exactly.
- LUT formula (DDK/XnShiftToDepth.cpp) port verified; config inputs:
  ZPD 120 / ZPPS 0.1052 / LDDIS 7.5 / ConstShift 200 from the device,
  ParamCoeff=4 + ShiftScale=10 are driver constants
  (XnSensorDepthStream.cpp:158-161), pixel_size_factor=1 (reference
  resolution SXGA; only depth@60fps or IR@QVGA → VGA/2 per
  RecalculateReferenceResolution + DecidePixelSizeFactor), cutoffs 0 and
  10000 (XN_DEVICE_SENSOR_MAX_DEPTH_1_MM), no laser/emitter command
  exists in the PS1080 driver (only projector FAULT calibration).
- **THE BUG: device_max_shift was 1024; XN_SHIFTS_MAX_SHIFT = 2047**
  (XnSensorDepthStream.cpp:42). Packed-11 spans 0..2047; a 1024-entry
  table silently zeroes every shift >= 1024 (plus loop-exclusive entry
  1024). Fixed to 2047 + regression tests.
- Exact Python simulation of the C LUT with this calibration: valid
  window = shifts 1..1052 → 315..9870 mm; shift 1024 → 5460 mm; a
  0.5-4 m scene = shifts 403..1001 → **inside even the old broken
  table** → max_shift alone does NOT explain 0 valid pixels. Either the
  stream is genuinely all-invalid (scene beyond 5.5 m, closer than
  31.5 cm, occluded/lens cap, or emitter issue) or something not yet
  found. The new per-frame shift histogram decides in one run:
  - all 2047/0 → device/scene issue (re-aim at a wall 1-2 m)
  - spread in 1024-2046 → far scene, NOW fixed by the 2047 table
  - spread in 1-1023 with still 0 valid → impossible per simulation;
    would point to a new bug (then dump raw bytes via raw_dump)

**Also ported:** the C's corrupt-frame drop (XnDepthProcessor::
OnEndOfFrame: assembled size != expected → FrameIsCorrupted → frame
not delivered; this is what happens to the partial first frame of a
stream). depth_dump now prints per-frame: `shifts: 0:N 2047:N 1-1023:N
1024-2046:N min-max a..b` via new DepthFrame::shift_stats (ShiftStats
computed during decode; tiny, no extra allocation kept).

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `asus_xtion_workspace_backup_2026-11-21_v13.zip` | Turn-13 zip (CURRENT; incl. the PDF + patch). Older zips v1-v12 kept. |
| `session_backup_2026-11-21.md` | This file (v11). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |
| `rust-xtion-check/` | Turn-6 Option-A smoke-test crate. |
| `openni2-nojava/` | Turn-5 package (C stack alternative). |
| `xtion_setup_cachyos.sh` / `xtion_test_cachyos.sh` | v1.3 / v1.2 helpers. |
| `asus_xtion_linux_research_report.pdf`, `asus_xtion_report_cover.html`, `diagram_stack.png` | Session-1 artifacts. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 00909ed "Fix third on-hardware failure..."**; history: deeb688 →
823dd53 (0.1.1) → 4afa6e3 → ca85e8e (0.1.2) → 7b84d21 → 00909ed (0.1.3)).
Its docs/recovery/ holds a copy of this backup + the v6 workspace zip.

## 5. Workspace state (2026-11-22, after turn 10)

- `/home/z/my-project/xtion-rs/` — 0.1.5 restored from zip (no .git),
  61 tests green, clippy 0, fmt; docs updated with M5-complete
  clean. Turn 12 changed: depth/shift_to_depth.rs (max_shift 2047 + tests), device.rs (ShiftStats + corrupt-frame drop + DepthFrame.shift_stats), examples/depth_dump.rs (histogram print), docs, 0.1.5. Turn 11 changed: protocol/mod.rs (BE buf_size), stream.rs, error.rs, device.rs, examples/raw_dump.rs, 0.1.4. Turn 10 added `src/ring.rs`, 0.1.3.
  ring-based read loops, diagnostics accessors, BULK_POLL_SLICE removed),
  usb.rs (context+speed owned, raw_context/raw_handle pub(crate)),
  error.rs (Clone), lib.rs (ring exports), both examples, tests/hardware.rs,
  docs (PROTOCOL "Async URB ring" section, PLAN M5 + matrix, README
  changelog), Cargo.toml (libusb1-sys + libc deps, 0.1.3).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup CARGO_HOME=/home/z/my-
  project/.cargo PATH=/home/z/my-project/.cargo/bin:$PATH`).
- `/tmp` (recycled between turns): gh-creds token file (recreate from the
  user's token via `printf 'https://thomas7475:<TOKEN>@github.com\n' >
  /tmp/gh-creds`, umask 077; never print/commit it); OpenNI2 source at
  /tmp/xtion-verify (re-download: github.com/occipital/OpenNI2/archive/
  v2.2.0-debian.tar.gz, sha256 08f6842f20d1098ab2ebafadaac0130ffae5abd3
  4cdf464bb6100cbe01ed95a8; key files: Source/Drivers/PS1080/Sensor/*,
  ThirdParty/PSCommon/XnLib/Source/Linux/XnLinuxUSB.cpp).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`
  (from their test output paths).

## 6. Gotchas learned (cumulative, new first)

- **XN_SHIFTS_MAX_SHIFT = 2047** — the shift LUT must cover the full 0..2047
  packed-11 range; 2047 = no-depth marker, 0 = invalid; for the reference
  calibration valid = shifts 1..1052 (315..9870 mm), 0.5-4 m → 403..1001.
- **A frame whose assembled size != expected must be dropped** (the partial
  first frame after stream start), per the C's OnEndOfFrame check.
- **buf_size in the 12-byte stream packet header is BIG-ENDIAN** (the only BE
  field; the C applies an unconditional xnOSEndianSwapUINT16 to it alone).
- Zero-payload packets must be dispatched immediately when their header
  completes at a chunk boundary, or frame closes lag/lose the last frame.
- The PS1080 stream works on HIGH speed USB at ~12.7 MB/s payload; the device
  clock ticks ~60 MHz (≈2,002,155 ticks per 30 fps frame period).
- **Sync bulk reads are not enough for PS1080 streams**: the device drops
  data when no URB is queued. Always keep a ring of pre-submitted async
  URBs (16 × 40 × maxpacket, re-submit in the callback).
- rusb 0.9 has no async API; use libusb1-sys (rusb's backend crate) with
  `DeviceHandle::as_raw()` / `Context::as_raw()`; keep the rusb Context
  alive in a struct field for pointer lifetime.
- libusb1-sys: `libusb_transfer_cb_fn` is a bare fn pointer (no Option);
  transfer status constants live in `libusb1_sys::constants`.
- Raw pointers across `thread::spawn`: pass as `usize` and cast in the
  closure (edition-2021 disjoint capture ignores Send-wrappers when you
  access `.0`).
- clippy `vec_box` on `Vec<Box<Slot>>` may be load-bearing: Box pins each
  slot's address for libusb `user_data`.
- Bulk-IN read lengths must be a multiple of wMaxPacketSize (EOVERFLOW).
- rusb 0.9.4 `read_bulk` returns Ok(n) on timeout with partial data.
- Only one handle (per process) can claim a USB interface → Busy.
- Device replies can be SPLIT across multiple control-IN transfers;
  XnVersions wire quirks (minor/major first, 12-byte base authoritative);
  v2.6 replies have NO CRC; new request id per exchange; NOT_READY retries
  keep the id; the 12-byte bulk packet header INCLUDES the fw magic; EOF
  closes the frame on its last byte; packed-11 on the wire; ignore one
  chunk of garbage per endpoint on fw < 5.0.
- Xtion Pro Live ships fw 5.8.22, chip 0x00212020 (PS1080-A6), FPGA 0x0005;
  640x480@30 packed-11 needs ~12.7 MB/s — full-speed USB cannot carry it.
- rusb 0.9 devices() needs .iter(); thiserror #[from] needs
  std::error::Error impl; blocked hosts (freedesktop GitLab,
  packages.debian.org) vs working (deb.debian.org, crates.io, github,
  apache.org); OPENNI2_REDIST build-time env; model cannot read images →
  programmatic PDF checks.

## 7. Open threads (if the user continues)

- **Re-run on hardware (the immediate next action):** user pulls 3948f07,
  POINTS THE CAMERA AT A WALL ~1-2 m AWAY (not a far corner, not closer
  than ~35 cm, no lens cap), and runs:
  `cargo run --example depth_dump -- 10` and
  `cargo test --test hardware -- --nocapture`.
  Expected: full 307,200-px frames with a high valid count. If 0 valid
  persists, the new `shifts:` histogram line decides:
  - `0:N 2047:307200-ish` → the device streams no-depth markers:
    scene/emitter problem (re-aim, check for cover, try another USB
    port / replug).
  - `1024-2046:large` → far scene; the 0.1.5 table now decodes these.
  - `1-1023:large` but valid still 0 → contradicts the verified
    simulation; run `cargo run --example raw_dump` and paste output.
- After depth works: color stream (YUV422→RGB888 implemented, untested on
  hardware), then M6-M8 (registration, MJPEG, COMPRESSED_PS, keep-alive,
  timestamps, LED/emitter) per docs/PLAN.md.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds with the user's token if /tmp was recycled;
  never print or commit the token).
- Golden vectors: `cd reference && gcc -O2 -o /tmp/c_golden c_golden.c &&
  /tmp/c_golden > golden.txt && cargo test --test golden`.
