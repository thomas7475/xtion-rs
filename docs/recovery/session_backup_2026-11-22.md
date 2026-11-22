# Chat Session Backup — ASUS Xtion Linux research

**Version:** 13 (2026-11-22, turn 12) · **Status: Fifth on-hardware run: buf_size fix CONFIRMED — full 307200-px frames at 30 fps, 12.13 MB/s, 0 failures — but every pixel decoded 0/invalid. Full decode-chain audit vs the C found ONE confirmed bug: shift LUT was built with device_max_shift=1024 instead of XN_SHIFTS_MAX_SHIFT=2047 (packed-11 spans 0..2047; valid window for this calibration = shifts 1..1052 = 315..9870 mm; old table zeroed all >= 1024 i.e. 5.5-9.9 m). BUT the exact Python simulation shows a normal 0.5-4 m scene sits at shifts 403..1001 — INSIDE the old table — so max_shift alone cannot explain 0 valid pixels; remaining suspects: all-invalid stream (scene > 5.5 m / too close < 31.5 cm / occluded / emitter) or something not yet found. Added per-frame raw-shift histogram (DepthFrame::shift_stats + depth_dump) — the NEXT RUN's histogram line decides. Also ported the C's corrupt-frame drop (partial first frames like 150024 px). 0.1.5 (3948f07); user pulls, points camera at a wall ~1-2 m, re-runs depth_dump**

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
- **Turn 12 (2026-11-22, this turn):** fifth run: FULL frames @30fps,
  0 valid pixels → shift LUT max_shift 1024→2047 + corrupt-frame drop
  + shift histogram diagnostics. 0.1.5 (3948f07). Validity pending.

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
| `asus_xtion_workspace_backup_2026-11-21_v12.zip` | Turn-12 zip (CURRENT). Older zips v1-v11 kept. |
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

- `/home/z/my-project/xtion-rs/` — 0.1.5, 61 tests green, clippy 0, fmt
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
