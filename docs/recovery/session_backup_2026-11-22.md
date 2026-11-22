# Chat Session Backup — ASUS Xtion Linux research

**Version:** 12 (2026-11-22, turn 11) · **Status: Fourth on-hardware failure diagnosed and fixed — 0.1.3's async ring worked perfectly (12.05 MB/s = full stream, 0 failures, High speed, mps 512) but frames assembled at EXACTLY 24560/307200 px @30fps: buf_size is the ONE BIG-ENDIAN field in the otherwise little-endian 12-byte stream packet header (the C's parse applies an extra UNCONDITIONAL xnOSEndianSwapUINT16 to nBufSize alone); LE-read made 512-B packets parse as buf_size 2 → payload 0 → frames filled with false-magic junk; ALSO fixed a second real bug (zero-payload EOF packets not dispatched at chunk end); added raw_dump example + fw_magic/tap_depth_stream diagnostics; pushed as 0.1.4 (commit 6720e97); user should pull and re-run**

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
- **Turn 11 (2026-11-22, this turn):** fourth run: ring perfect
  (12.05 MB/s, 0 failures) but frames EXACTLY 24560/307200 px @ 30 fps
  → buf_size big-endian quirk + zero-payload dispatch bug. 0.1.4
  (6720e97). Full-frame validation pending.

## 3. Turn-11 evidence, root cause and fix (the important part)

**Run 4 output (user's machine):** `list` shows usb speed High, mps 512
depth+image. `depth_dump 30`: every frame EXACTLY 24560/307200 px (first
frame 25312), ~55-67 valid, min 315 mm, timestamps advancing by EXACTLY
2,002,155 ticks/frame; ring: 634 completions, 12,804,988 B in 1.01 s =
12.05 MB/s, **0 failures** — the FULL stream arrives. 2,002,155 ticks per
33.33 ms frame ⇒ ~60 MHz device clock (frame timestamps come from the EOF
packet header, matching the C's CreateTimestampFromDevice in
OnEndOfFrame).

**Diagnosis path:** transport ruled out → parser must be dropping 92% of
payload. Re-checked the C's response header struct (XnDeviceSensorProtocol.h:
magic, type, packetID, bufSize, timestamp) — my field order was already
correct. Then the C's parse in XnDeviceSensorProtocol.cpp: all fields pass
XN_PREPARE_VAR16_IN_BUFFER (verified: no-op on Linux-x86 LE), but nBufSize
alone gets an additional UNCONDITIONAL xnOSEndianSwapUINT16 (verified in
XnLinuxMemory.cpp: plain byteswap, no host check) ⇒ **buf_size is
BIG-ENDIAN on the wire**; everything else little-endian. My LE read of a
512-B packet's field bytes [02, 00] = 2 → payload 2−12 → 0 → every real
data packet contributed NOTHING; a bare 12-B EOF (BE 12 = [00, 0C]) parsed
as 3072 → injected ~3 KB junk per frame; false magics inside payload
added the rest. Steady scene → identical packed bytes each frame →
IDENTICAL junk count (24560) every frame — the regularity was the tell.

**Fix (0.1.4):** `PacketHeader::parse` reads buf_size via
`u16::from_be_bytes`; stream.rs test builder writes it BE; new regression
test encodes a real 512-B wire packet and asserts 500-B payload survives
assembly. That test exposed a SECOND real bug: a zero-payload packet whose
header completed at the END of a read chunk was never dispatched until the
next chunk (feed loop exits on empty chunk) — delayed every frame close by
one chunk and would lose the final frame on stop. Fixed: zero-payload
packets dispatch immediately after header completion; EOF close check
extracted to close_if_eof(). New ground-truth tooling: examples/raw_dump.rs
(captures raw stream via new Xtion::tap_depth_stream, decodes headers
standalone showing buf_size BOTH ways, per-type counts, px estimate) +
Xtion::fw_magic(); new XtionError::Io variant. Verified against C while at
it: packet types 0x7100/0x7200/0x7500/0x8100/0x8200/0x8500 match
XnDeviceSensorProtocol.h exactly; SOF-padding and EOF-timestamp semantics
match XnDepthProcessor/XnFrameStreamProcessor.

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `asus_xtion_workspace_backup_2026-11-21_v11.zip` | Turn-11 zip (CURRENT). Older zips v1-v10 kept. |
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

- `/home/z/my-project/xtion-rs/` — 0.1.4, 60 tests green, clippy 0, fmt
  clean. Turn 11 changed: protocol/mod.rs (BE buf_size), stream.rs (zero-payload dispatch, close_if_eof, BE tests), error.rs (Io variant), device.rs (fw_magic, tap_depth_stream), examples/raw_dump.rs (new), Cargo.toml 0.1.4. Turn 10 added `src/ring.rs` and touched: device.rs (rings,
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

- **Re-run on hardware (the immediate next action):** user pulls 6720e97
  and runs:
  `cargo test --test hardware -- --nocapture` and
  `cargo run --example depth_dump -- 30`.
  Expected now: full 307,200-pixel frames, high valid-pixel ratio (point
  the camera at a normal scene), ring ~12 MB/s, 0 failures.
  If anything is still off, run `cargo run --example raw_dump` — it
  captures the raw stream to /tmp/xtion_raw.bin and decodes packet
  headers standalone (type/id/size BE + LE-misread/ts + payload hex,
  per-type counts) — paste that output; it is definitive ground truth.
- After depth works: color stream (YUV422→RGB888 implemented, untested on
  hardware), then M6-M8 (registration, MJPEG, COMPRESSED_PS, keep-alive,
  timestamps, LED/emitter) per docs/PLAN.md.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds with the user's token if /tmp was recycled;
  never print or commit the token).
- Golden vectors: `cd reference && gcc -O2 -o /tmp/c_golden c_golden.c &&
  /tmp/c_golden > golden.txt && cargo test --test golden`.
