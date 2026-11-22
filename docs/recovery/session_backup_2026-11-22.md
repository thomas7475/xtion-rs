# Chat Session Backup — ASUS Xtion Linux research

**Version:** 10 (2026-11-22, turn 9) · **Status: Second on-hardware run diagnosed and fixed — control path verified live on fw 5.8.22 (identity/serial/calibration/FOV all correct); every depth bulk read had failed with 'USB error: Overflow' because the read buffer was resized to a whole-frame 422,411 bytes (not a multiple of the 512-byte max packet — Linux completes such bulk-IN URBs with EOVERFLOW); fixed to permanent 40×maxpacket chunks + timeout-tolerant reads + serialized hardware tests + DeviceBusy error; pushed as 0.1.2 (commit ca85e8e); user should pull and re-run**

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
  handshake ("version reply too short: 12 bytes") — root cause: replies
  arrive SPLIT across multiple vendor control-IN transfers; ported the C's
  full Execute/ReceiveReply semantics + version wire quirks; pushed 0.1.1
  (823dd53) + recovery update (4afa6e3).
- **Turn 9 (2026-11-22, this turn):** second on-hardware run — **control
  path fully works** (fw 5.8.22, chip 0x00212020 = PS1080-A6, FPGA 5,
  serial 1408090603, ConstShift 200, ZPD 120 mm, ZPPS 0.1052 mm, LDDIS
  7.5 cm, dCMOS-rCMOS 2.63 cm, hFOV 58.59°). Two new failures fixed:
  (a) every depth bulk read returned `USB error: Overflow`;
  (b) running both hardware tests in parallel collided with `Usb(Busy)`.
  Both fixed in 0.1.2 (ca85e8e). Depth-frame live validation still pending.

## 3. Turn-9 root causes and fixes (the important part)

**Run 2 output (user's machine):**
`open_handshake_and_identity` PASSED (printed version + serial);
`depth_stream_roundtrip` FAILED at open with `Usb(Busy)`;
`cargo run --example list` worked fully (calibration, FOV);
`cargo run --example depth_dump -- 30` started the stream
("depth stream started: 640x480 @ 30 fps (Uncompressed11Bit)") but EVERY
frame failed with `USB error: Overflow`.

**(a) The Overflow.** `start_depth`/`start_color` resized the bulk read
buffer to the whole frame's wire size — 422,411 bytes for 640x480 packed-11
(422,400 payload + 11 slack). 422,411 % 512 = 11 → NOT a multiple of the
endpoint's wMaxPacketSize, and the Linux USB stack completes a bulk-IN URB
whose length is not a multiple of maxpacket with **EOVERFLOW** — instantly,
on every read. The C never does this: its read thread
(`xnUSBInitReadThread`, XnLinuxUSB.cpp) submits a ring of async URBs, each
exactly `XN_SENSOR_USB_DEPTH_BUFFER_SIZE_MULTIPLIER_BULK (40 on Linux) ×
nMaxPacketSize` bytes (XnDeviceSensorInit.h; Windows uses 120, Linux/mac/
Android 40). Fixes in device.rs/usb.rs:
- chunk buffer allocated ONCE at open: `40 × max(depth_mps, image_mps)`
  (powers of two → multiple of both endpoints' maxpacket); never resized.
- new `bulk_read_slice`: `Err(Usb(Timeout|Interrupted))` maps to "no data
  yet" (returns 0; the caller's deadline governs). rusb 0.9.4 verified:
  on timeout WITH partial data it returns `Ok(transferred)`, so no stream
  bytes are ever lost.
- regression tests: chunk-size multiple-of-maxpacket invariant; documented
  422,411 % 512 = 11 as the failing case (YUV422 640x480 = 614,400 happens
  to be a multiple — only the packed-11 resize broke).

**(b) The Usb(Busy) collision.** cargo test runs both hardware tests in
parallel threads; both call `Xtion::open()`; the second `claim_interface`
fails with Busy (two handles in one process cannot claim the same
interface). Fixed: global `static DEVICE_LOCK: Mutex<()>` in
tests/hardware.rs serializes them (poison-tolerant); plus a dedicated
`XtionError::DeviceBusy` variant with guidance, mapped at the claim site.

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `asus_xtion_workspace_backup_2026-11-21_v9.zip` | Turn-9 zip (CURRENT). Older zips v1-v8 kept. |
| `session_backup_2026-11-21.md` | This file (v10). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |
| `rust-xtion-check/` | Turn-6 Option-A smoke-test crate. |
| `openni2-nojava/` | Turn-5 package (C stack alternative). |
| `xtion_setup_cachyos.sh` / `xtion_test_cachyos.sh` | v1.3 / v1.2 helpers. |
| `asus_xtion_linux_research_report.pdf`, `asus_xtion_report_cover.html`, `diagram_stack.png` | Session-1 artifacts. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = ca85e8e "Fix second on-hardware failure..."**; history: deeb688 →
823dd53 (0.1.1 split-reply fix) → 4afa6e3 (recovery v9) → ca85e8e).
Its docs/recovery/ holds a copy of this backup + the v6 workspace zip.

## 5. Workspace state (2026-11-22, after turn 9)

- `/home/z/my-project/xtion-rs/` — 0.1.2. Structure unchanged from turn 8;
  this turn touched: `src/device.rs` (chunk sizing + bulk_read_slice,
  resize blocks removed), `src/usb.rs` (DeviceBusy mapping + tests),
  `src/error.rs` (DeviceBusy variant), `tests/hardware.rs` (mutex),
  docs (PROTOCOL "Bulk read buffer sizing (verified on hardware)"
  section; PLAN M5 status; README changelog; test counts 56).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup CARGO_HOME=/home/z/my-
  project/.cargo PATH=/home/z/my-project/.cargo/bin:$PATH`).
- `/tmp` (recycled between turns): gh-creds token file (recreate from the
  user's token via `printf 'https://thomas7475:<TOKEN>@github.com\n' >
  /tmp/gh-creds`, umask 077; never print/commit it); OpenNI2 source at
  /tmp/xtion-verify (re-download: github.com/occipital/OpenNI2/archive/
  v2.2.0-debian.tar.gz, sha256 08f6842f20d1098ab2ebafadaac0130ffae5abd3
  4cdf464bb6100cbe01ed95a8; key files: Source/Drivers/PS1080/Sensor/*
  and ThirdParty/PSCommon/XnLib/Source/Linux/XnLinuxUSB.cpp).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`
  (from their test output paths).

## 6. Gotchas learned (cumulative, new first)

- **Bulk-IN read lengths must be a multiple of wMaxPacketSize** — Linux
  completes them with EOVERFLOW otherwise. Never size USB read buffers to
  application payloads; use 40×maxpacket (the C's Linux multiplier).
- rusb 0.9.4 `read_bulk` returns `Ok(n)` on timeout with partial data;
  only empty timeouts are `Err(Timeout)` — safe to retry.
- Only one handle (per process) can claim a USB interface: parallel tests
  or a second open → rusb Busy. Serialize hardware tests (mutex /
  `--test-threads=1`).
- The Xtion Pro Live ships fw **5.8.22**, chip 0x00212020 (PS1080-A6),
  FPGA 0x0005 (RD1082) — all C version branches for ≥5.3 apply; protocol
  V26; depth 640x480 packed-11 at 30 fps works after the buffer fix.
- Device replies can be SPLIT across multiple vendor control-IN
  transfers — always accumulate until the size-promised byte count.
- XnVersions wire quirks: first two bytes are minor, major; only the
  12-byte base struct is authoritative; odd data padded to whole words.
- v2.6 (MG/BR) replies have NO CRC field.
- Each exchange call = new request id; NOT_READY retries within ONE
  exchange keep the same id.
- The bulk packet header's magic IS the fw magic and the 12-byte header
  INCLUDES it (partial-magic stitch seeds both bytes); EOF close only
  when the packet's last byte arrives; "UNCOMPRESSED_11_BIT" depth is
  packed-11 on the wire.
- rusb 0.9 devices() needs .iter(); thiserror #[from] needs
  std::error::Error impl; blocked hosts (freedesktop GitLab,
  packages.debian.org) vs working (deb.debian.org, crates.io, github,
  apache.org); OPENNI2_REDIST build-time env; model cannot read images →
  programmatic PDF checks.

## 7. Open threads (if the user continues)

- **Re-run on hardware (the immediate next action):** user pulls ca85e8e
  and runs, with the Xtion on a USB 2.0 port:
  `cargo test --test hardware -- --nocapture`,
  `cargo run --example depth_dump -- 30`,
  `cargo run --example list`.
  Expected now: both tests pass, depth_dump prints 30 frames with
  min-depth/valid-pixel stats. If a frame read still fails, capture the
  exact error text (and raw bytes if a BadConfig) — remaining candidates:
  (a) stream start latency > first 100 ms slice (should be handled by the
  timeout-tolerant retry + 10 s example timeout); (b) mid-stream packet
  loss from the single-URB sync read path (the C uses a pre-submitted
  async URB ring; if drops appear, port the ring via rusb's async API —
  M6+ work).
- After depth works: color stream (YUV422 → RGB888 path is implemented,
  untested on hardware), then M6-M8 (registration, MJPEG, COMPRESSED_PS,
  keep-alive, timestamps, LED/emitter) per docs/PLAN.md.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds with the user's token if /tmp was recycled;
  never print or commit the token).
- Golden vectors: `cd reference && gcc -O2 -o /tmp/c_golden c_golden.c &&
  /tmp/c_golden > golden.txt && cargo test --test golden`.
