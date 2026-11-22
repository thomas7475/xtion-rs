# Chat Session Backup — ASUS Xtion Linux research

**Version:** 8 (2026-11-22, turn 7) · **Status: Option D pure-Rust driver (xtion-rs) implemented as far as possible without hardware — 44 tests green incl. C-reference golden vectors; code + recovery state pushed to github.com/thomas7475/xtion-rs; on-camera validation (M5) is the next action**

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
- **Turn 6 (2026-11-22):** Rust-stack options report. Verified Option A
  (openni2 crate 0.4.0, 2025-10-12) end-to-end in a container on Rust
  1.100.0: cargo build + full init/driver-load/enumeration chain against
  the nojava-built libOpenNI2.so (via an LD_PRELOAD glob64/chdir/dlopen
  redirect). Delivered `asus_xtion_rust_stack_options.pdf` (12 pages) and
  `rust-xtion-check/` smoke-test crate. Options A (bindings), B (vendored
  build), C (Rust plugin), D (pure-Rust driver), E (ROS 2); no pure-Rust
  PS1080 driver existed anywhere.
- **Turn 7 (2026-11-22, this turn):** user chose Option D: rewrite the
  complete driver in Rust, test it, and push to
  **github.com/thomas7475/xtion-rs** together with the project state for
  recovery (user provided a GitHub PAT; stored only in /tmp/gh-creds,
  never in any repo file or log output).

## 3. Turn-7 work: the xtion-rs driver

**Protocol extraction (M0, done):** re-downloaded the OpenNI 2.2.0 tarball
(sha256 verified: 08f6842f20d1098ab2ebafadaac0130ffae5abd34cdf464bb6100cbe
01ed95a8) and extracted the complete PS1080 protocol with per-file
citations → `xtion-rs/docs/PROTOCOL.md`. Key facts: command framing
(v2.6 "MG"/"BR" 8-byte header, size in WORDS, CRC always 0), control
transfers = vendor EP0 (0x40/0xC0, bRequest 0, wValue/wIndex 0), param ids
(STREAM0=color/1=depth, formats, resolutions), 168-byte fixed-params V3
layout (ZPD@100, ZPPS@104, LDDIS@92), ALGORITHM_PARAMS 10-byte request
(DEPTH_INFO → u16 ConstShift), bulk packets = 12-byte header with the
FIRMWARE magic ("BR") + type 0x71/72/75 (depth) 81/82/85 (image) 92
(audio), frame = SOF+data+EOF, frame timestamp = EOF packet's, fw>=5.1
SOF timestamp = padding pixels (start<<16|end), fw<5.0 first-chunk garbage
skip, packed-11 bit layout (8,3,5,6,2,8,1,7,4,4,7,1,8,2,6,5,3,8), S2D math
with u32 const-shift adjustment, YUV422 UYVY BT.601 integer conversion,
serial via opcode 37 ASCII reply, XnVersions 33-byte layout + fw≥5
hex-build quirk.

**Implementation (M1-M4 code, done):** crate at `/home/z/my-project/xtion-rs`
(lib name `xtion`): `src/usb.rs` (rusb: discovery, endpoint probe 0x81→old
layout, EP0 vendor control, bulk reads), `src/protocol/` (command.rs
exchange+validation+retries, fixed_params.rs, algorithm.rs, mod.rs
constants), `src/depth/` (packed11.rs + inverse, shift_to_depth.rs LUT),
`src/color.rs` (YUV422→RGB), `src/stream.rs` (state machine incl.
partial-magic stitching — the 12-byte header INCLUDES the magic bytes!),
`src/device.rs` (Xtion::open handshake w/ 2s-retry quirk + v25 fallback,
serial, get/set param, start/stop depth+color, blocking frame reads with
corrupted-frame drop), examples (list, depth_dump), tests (golden.rs,
hardware.rs auto-skip).

**Verification (done):** `cargo test` = **44 pass, 0 fail** without
hardware (39 unit + 2 golden + 2 hardware-skip + 1 doc); clippy 0 warnings
after fixes; rustfmt clean. **Golden vectors:** `reference/c_golden.c` is
a verbatim C transcription of XnShiftToDepthUpdate + Unpack11to16;
`reference/golden.txt` (2 calibration configs, full 1025-entry tables +
512 random packed11 groups) is committed and `tests/golden.rs` asserts the
Rust output is **byte-identical** — the ported math provably matches the
original C++. Bugs found and fixed by the chunk-split tests: EOF close
must fire only when the packet's last byte arrives (not per bulk chunk),
and the partial-magic stitch must pre-seed BOTH magic bytes into the
header.

**Not yet possible without hardware (M5+):** the on-camera run
(`cargo test --test hardware -- --nocapture`, examples list/depth_dump on
a USB 2.0 port). Remaining features for M6-M8: registration, MJPEG,
COMPRESSED_PS depth (ISO low-band), keep-alive, device-timestamp
conversion, LED/emitter opcodes.

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `asus_xtion_workspace_backup_2026-11-21_v7.zip` | Turn-7 zip (CURRENT; includes this backup, both PDFs, scripts, packages, rust-xtion-check, xtion-rs source snapshot). Older zips v1-v6 kept. |
| `session_backup_2026-11-21.md` | This file (v8). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |
| `rust-xtion-check/` | Turn-6 Option-A smoke-test crate. |
| `openni2-nojava/` | Turn-5 package (C stack alternative). |
| `xtion_setup_cachyos.sh` / `xtion_test_cachyos.sh` | v1.3 / v1.2 helpers. |
| `asus_xtion_linux_research_report.pdf`, `asus_xtion_report_cover.html`, `diagram_stack.png` | Session-1 artifacts. |

The driver itself lives in the git repo `/home/z/my-project/xtion-rs`
(pushed to github.com/thomas7475/xtion-rs, branch main). Its
`docs/recovery/` contains a copy of this backup + the v6 workspace zip.

## 5. Workspace state (2026-11-22, after turn 7)

- `/home/z/my-project/xtion-rs/` — the driver repo (git clone of the
  target; remote `origin` = https://github.com/thomas7475/xtion-rs.git;
  push uses `-c credential.helper='store --file=/tmp/gh-creds'`, the token
  is ONLY in /tmp/gh-creds — if /tmp was recycled, ask the user for a new
  token, never store it in files). Structure: src/{lib,error,usb,stream,
  color,device}.rs, src/protocol/{mod,command,fixed_params,algorithm}.rs,
  src/depth/{mod,packed11,shift_to_depth}.rs, examples/{list,depth_dump}.
  rs, tests/{golden,hardware}.rs, reference/{c_golden.c,golden.txt},
  docs/{PLAN,PROTOCOL}.md, docs/recovery/, README.md, LICENSE (Apache-2.0),
  NOTICE.md, Cargo.toml (deps: rusb 0.9, thiserror 2, log 0.4).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup CARGO_HOME=/home/z/my-
  project/.cargo PATH=/home/z/my-project/.cargo/bin:$PATH`). clippy +
  rustfmt components installed.
- `/tmp/xtion-verify/` (recycled between turns): OpenNI2 tarball + source
  tree (re-download from https://github.com/occipital/OpenNI2/archive/
  v2.2.0-debian.tar.gz, verify sha256), gh-creds token file, c_golden
  binary. The reference source is only needed for further protocol work
  (registration tables, GMC, etc.).
- `scripts/` (in my-project): all generation/verification scripts from
  earlier turns (PDF report gen, preload shim, link-test prep, etc.).

## 6. Gotchas learned (cumulative, new first)

- The bulk packet header's `magic` field IS the firmware magic; the
  12-byte header INCLUDES those 2 bytes (partial-magic stitch must
  pre-seed both into the header buffer).
- EOF/END packets can span multiple bulk reads — close the frame only
  when the packet's last payload byte arrives (C: nDataOffset+nDataSize ==
  nBufSize), not per dispatch.
- A SOF packet's payload is part of the frame; new-frame bookkeeping only
  on the FIRST data chunk of an SOF packet (C: nDataOffset == 0).
- "UNCOMPRESSED_11_BIT" depth is PACKED 11-bit on the wire (processor
  selection in XnSensorDepthStream.cpp:921); raw u16 is only
  UNCOMPRESSED_16_BIT.
- Reply `size` counts the error word + data in 16-bit words, padded up
  (odd data lengths get one pad byte).
- rusb 0.9: `context.devices()` returns DeviceList — iterate with
  `.iter()`; descriptor structs borrow the config descriptor (don't
  return them).
- thiserror `#[from]` needs the source type to impl std::error::Error.
- assert_eq! with a message: max 3 args total (left, right, "literal").
- Earlier gotchas still valid: jdk/jre conflict; GL header split; blocked
  hosts (freedesktop GitLab, packages.debian.org) vs working (deb.debian
  org pool, crates.io API, github, apache.org); OPENNI2_REDIST build-time
  env; model cannot read images → programmatic PDF checks; pdf_qa 0.3pt
  page-size tolerance; glibc versioned-symbol interposition needs
  .symver + version script.

## 7. Open threads (if the user continues)

- **M5 on-camera validation** (the next action): with the Xtion on a USB
  2.0 port, in xtion-rs: `cargo test --test hardware -- --nocapture`,
  `cargo run --example list`, `cargo run --example depth_dump -- 30`.
  If the handshake or stream misbehaves, compare against the C stack
  (rust-xtion-check / NiViewer) on the same scene and check
  docs/PROTOCOL.md quirk list first.
- M6-M8 features (registration via ALGORITHM_REGISTRATION tables, MJPEG
  color, keep-alive thread, timestamp conversion, LED/emitter) — all
  mapped in docs/PLAN.md.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds with the user's token if /tmp was recycled;
  never print or commit the token).
- Regenerate golden vectors: `cd reference && gcc -O2 -o /tmp/c_golden
  c_golden.c && /tmp/c_golden > golden.txt && cargo test --test golden`.
