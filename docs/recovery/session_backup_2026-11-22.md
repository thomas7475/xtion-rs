# Chat Session Backup — ASUS Xtion Linux research

**Version:** 11 (2026-11-22, turn 10) · **Status: Third on-hardware failure diagnosed and fixed — after the EOVERFLOW fix the depth stream ran, but every assembled frame was a FRAGMENT (736..45424 of 307200 pixels, ~4% of stream bytes surviving): sync bulk reads (one URB at a time) lose data in the gaps between reads because the Xtion's FPGA FIFO drops data when no URB is queued; ported the C's async 16×40×maxpacket pre-submitted URB ring (src/ring.rs via libusb1-sys, since rusb 0.9 has no async API) with in-callback resubmit + event thread + leak-safe Drop; added bus-speed/mps/ring-stats diagnostics; pushed as 0.1.3 (commit 00909ed); user should pull and re-run**

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
- **Turn 10 (2026-11-22, this turn):** third on-hardware run: stream runs,
  `Busy`/`Overflow` gone, but frames are FRAGMENTS (see status). Ported the
  C's async URB ring. 0.1.3 (00909ed). Full-frame validation pending.

## 3. Turn-10 evidence, root cause and fix (the important part)

**Run 3 output (user's machine):** both hardware tests serialize fine now;
`open_handshake_and_identity` passes; `depth_dump` starts the stream
("640x480 @ 30 fps (Uncompressed11Bit)") and completes SOF→EOF groups, but
each assembled "frame" carries only a slice: `61/23072`, `102/40208`,
`3/1480`, `112/45424` valid/total pixels, min ≈ 315 mm (real close-range
data), timestamps advancing by EXACTLY 2,002,155 units (~60 frame periods
at 30 fps) per assembled frame; the hardware test got 14,888 px (≈ one
20 KB read-chunk of payload) instead of 307,200. Total surviving bytes
across 30 "frames" ≈ 2.3 real frames (~4%).

**Diagnosis:** not a parser bug — the framing port was re-verified against
`XnDeviceSensorProtocol.cpp::XnDeviceSensorProtocolUsbEpCb` (ignore-garbage
→ magic scan → 12-byte header → data; header includes the magic; buf_size
includes the header) and `XnFrameStreamProcessor.cpp::ProcessPacketChunk`
(SOF starts a frame at packet-chunk offset 0; frame closes when the LAST
chunk of an EOF packet arrives; SOF+data+EOF payloads concatenate into the
frame buffer; double-SOF is JPEG-image-only, correctly not ported). The
fragments are genuine DEVICE-SIDE DATA LOSS: with a sync read loop there is
a gap between reaping one 20 KiB URB and submitting the next; the Xtion's
small FPGA FIFO drops stream data whenever no URB is ready. The C driver's
read design exists precisely for this: `xnUSBInitReadThread` /
`XnUSBReadThreadMain` (XnLinuxUSB.cpp) pre-submit 16 async transfers
(XN_SENSOR_USB_DEPTH_BUFFERS, Linux) of 40 × maxpacket each, re-submit
each immediately after its completion is processed, run the thread at
CRITICAL priority, and warn "might cause loss of data" otherwise.

**Fix (0.1.3):** new `src/ring.rs` — `BulkRing`:
- 16 pre-submitted libusb bulk-IN transfers, 40×maxpacket each, infinite
  timeout, submitted BEFORE the stream mode is switched on.
- Completion callback (on the event thread) copies `actual_length` bytes
  to a queue, notifies a condvar, and RE-SUBMITS INSIDE the callback, so
  queued URBs never drop to zero.
- Dedicated event thread: `libusb_handle_events_timeout_completed` with a
  10 ms tick; exits on kill && in-flight == 0; marks events dead on fatal
  errors.
- Drop: kill flag → cancel loop until in-flight drains (handles a callback
  racing the flag) → join → free; if the event loop died with transfers
  in flight, LEAKS them like the C does on disconnect.
- Consumer: `pop(deadline)` (condvar), errors surface through the queue
  wake; read_depth_frame/read_color_frame feed popped chunks into the
  existing StreamAssembler.
- Integration: rings are Xtion fields declared BEFORE `usb` (drop order);
  start_depth/start_color start the ring after SET_MODE; stop_* drops the
  ring before SET_MODE off; UsbDevice now owns the rusb Context (raw
  pointer lifetime) and records bus speed.
- Diagnostics: Xtion::usb_speed / usb_max_packet_sizes /
  depth_ring_stats(RingStats); list example prints speed + mps;
  depth_dump prints ring config, expected-vs-got pixels per frame, and
  ring completions/bytes/MB/s + "needs ~12.7 MB/s" note; hardware test
  prints speed + mps. A full-speed bus (~1.2 MB/s ceiling) would now be
  visible immediately.

libusb1-sys 0.7 (rusb's own backend) + libc are direct deps now.
Rust notes hit this turn: rusb::Error variants are `Io`/`Other` (not
IoError); `libusb_transfer_cb_fn` is NOT Option-wrapped; raw pointers
can't cross `thread::spawn` — pass as `usize` (edition-2021 disjoint
capture defeats Send-wrapper field access); clippy vec_box allow is
load-bearing (Box = stable user_data addresses); XtionError now Clone.

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `asus_xtion_workspace_backup_2026-11-21_v10.zip` | Turn-10 zip (CURRENT). Older zips v1-v9 kept. |
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

- `/home/z/my-project/xtion-rs/` — 0.1.3, 59 tests green, clippy 0, fmt
  clean. This turn added `src/ring.rs` and touched: device.rs (rings,
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

- **Re-run on hardware (the immediate next action):** user pulls 00909ed
  and runs, Xtion on a USB 2.0 (high-speed) port:
  `cargo run --example list` (now prints usb speed + max packet sizes —
  MUST say High; if Full, the port/cable is the problem),
  `cargo test --test hardware -- --nocapture`,
  `cargo run --example depth_dump -- 30`.
  Expected now: full 307,200-pixel frames with mostly-valid pixels, and
  the ring summary showing ~12+ MB/s. If frames are STILL short but the
  ring shows ≥12 MB/s and 0 failures → the loss is elsewhere (then dump
  raw packet headers: add an env-gated hexdump in ring/assembler);
  if ring throughput caps at ~1.2 MB/s → bus negotiated full speed
  (port/cable/hub issue, not a driver bug).
- After depth works: color stream (YUV422→RGB888 implemented, untested on
  hardware), then M6-M8 (registration, MJPEG, COMPRESSED_PS, keep-alive,
  timestamps, LED/emitter) per docs/PLAN.md.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds with the user's token if /tmp was recycled;
  never print or commit the token).
- Golden vectors: `cd reference && gcc -O2 -o /tmp/c_golden c_golden.c &&
  /tmp/c_golden > golden.txt && cargo test --test golden`.
