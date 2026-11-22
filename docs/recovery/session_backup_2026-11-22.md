# Chat Session Backup — ASUS Xtion Linux research

**Version:** 9 (2026-11-22, turn 8) · **Status: First on-hardware run diagnosed and fixed — root cause was replies arriving SPLIT across multiple control-IN transfers (C's 'Get rest of data' loop was not ported); fix + full Execute/ReceiveReply semantics + regression tests pushed as 0.1.1 (commit 823dd53); user should pull and re-run the hardware tests**

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
  github.com/thomas7475/xtion-rs (commit deeb688) with docs (PLAN, PROTOCOL,
  recovery). Sandbox had no camera → on-camera validation left to the user.
- **Turn 8 (2026-11-22, this turn):** user ran `cargo test --test hardware`
  on their machine (Xtion attached): both tests failed at
  `Xtion::open()` with `BadConfig("version reply too short: 12 bytes")`.
  User asked to research the problem, fix, and push (token re-supplied).

## 3. Turn-8 root cause and fix (the important part)

**Diagnosis method:** re-downloaded the OpenNI 2.2.0 tarball (sha256
08f6842f20d1098ab2ebafadaac0130ffae5abd34cdf464bb6100cbe01ed95a8) and read
`XnHostProtocol.cpp` COMPLETELY this time (USBReceive, ReceiveReply,
Execute, ValidateReplyV26/V25, GetVersion tail).

**Root cause:** `XnHostProtocolExecute` has a **"Get rest of data" loop**
(Execute lines ~1185-1205): a reply may arrive **split across several
vendor control-IN transfers**; the first read carries the reply header +
first data chunk; the C keeps issuing control reads (512 bytes each for
fw >= 5.0, XN_PROTOCOL_MAX_PACKET_SIZE_V5_0) and appending until
`nDataSize*2` bytes are collected. The Rust port returned after the FIRST
control read — the user's device delivered only 12 of the ~34 version
data bytes in the first transfer → parse error "version reply too short:
12 bytes". (Proof the rest of the port worked on real hardware: the magic
scan, id/opcode match and ACK all validated on the device's real reply.)

**Fix (commit 823dd53, version 0.1.1) — full Execute/ReceiveReply port:**
- `CommandLink::exchange` now accumulates follow-up control reads until the
  promised `size_words - 1` words of data are complete.
- Stale replies (wrong request id) are discarded and the receive repeats.
- No-magic reply buffers are re-received every 10 ms until budget expiry.
- `NACK_NOT_READY` re-sends the SAME request (same id), 3 attempts, 1 s
  apart (XN_HOST_PROTOCOL_NOT_READY_RETRIES).
- Per-command delays ported: 1 ms pre-send, 10 ms post-send; reads use
  512-byte buffers.
- `DeviceVersion::parse` rewritten per the C: only the first 12 bytes are
  authoritative (SDK/HW/FW/Sensor/ChipVer are recomputed host-side; the C
  memcpy's 33 bytes from a partially-filled buffer, so short replies are
  valid — zero-pad); **the first two wire bytes are minor, major** (the C
  byte-swaps the struct's first u16 after memcpy: major=b[1], minor=b[0]);
  hex-build quirk for fw >= 5 applied AFTER the swap; odd data padded to
  whole words by the device (33 → 34 bytes).
- Parse errors now embed the raw reply bytes (hex) for remote debugging.
- `Xtion::handshake` made generic over `ControlTransport` → unit-testable.

**Tests:** 53 pass, 0 fail without hardware (48 unit incl. new
regressions: split-reply accumulation = the exact user scenario, stale-id
discard, NOT_READY re-send, version wire quirks, split-version handshake,
v2.5 fallback [takes ~2 s by design]; + 2 golden + 2 hardware-skip +
1 doc). Clippy 0 warnings, rustfmt clean. Regression-test gotchas found:
each NEW exchange call allocates the next request id (the fake replies
must use incrementing ids across handshake attempts); the first control
read must contain the full header (real transfers do); v2.6 NACK replies
have NO crc field.

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `asus_xtion_workspace_backup_2026-11-21_v8.zip` | Turn-8 zip (CURRENT). Older zips v1-v7 kept. |
| `session_backup_2026-11-21.md` | This file (v9). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |
| `rust-xtion-check/` | Turn-6 Option-A smoke-test crate. |
| `openni2-nojava/` | Turn-5 package (C stack alternative). |
| `xtion_setup_cachyos.sh` / `xtion_test_cachyos.sh` | v1.3 / v1.2 helpers. |
| `asus_xtion_linux_research_report.pdf`, `asus_xtion_report_cover.html`, `diagram_stack.png` | Session-1 artifacts. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 823dd53 "Fix first on-hardware failure..."**, previous: deeb688
initial port). Its docs/recovery/ holds a copy of this backup + the v6
workspace zip.

## 5. Workspace state (2026-11-22, after turn 8)

- `/home/z/my-project/xtion-rs/` — fresh clone + fix applied + pushed.
  Same structure as turn 7 (src/{lib,error,usb,stream,color,device}.rs,
  src/protocol/{mod,command,fixed_params,algorithm}.rs,
  src/depth/{mod,packed11,shift_to_depth}.rs, examples, tests,
  reference/, docs/), now at **0.1.1** with the exchange rework in
  `src/protocol/command.rs` and the version-parse rework in
  `src/device.rs`.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup CARGO_HOME=/home/z/my-
  project/.cargo PATH=/home/z/my-project/.cargo/bin:$PATH`). clippy +
  rustfmt installed.
- `/tmp` (recycled between turns): gh-creds token file (recreate from the
  user's token via `printf 'https://thomas7475:<TOKEN>@github.com\n' >
  /tmp/gh-creds`, umask 077; never print/commit it), OpenNI2 source at
  /tmp/xtion-verify (re-download: github.com/occipital/OpenNI2/archive/
  v2.2.0-debian.tar.gz, verify sha256 above).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`
  on their machine (from the test output path).

## 6. Gotchas learned (cumulative, new first)

- **Device replies can be SPLIT across multiple vendor control-IN
  transfers** — always accumulate until the size-promised byte count
  (Execute's "Get rest of data" loop).
- XnVersions wire quirks: first two bytes are minor, major (C byte-swaps
  the first u16); only the 12-byte base struct is authoritative; SDK/HW/
  FW/Sensor/ChipVer in the reply are IGNORED and recomputed; odd data
  padded to whole words.
- v2.6 (MG/BR) replies have NO CRC field — the 10-byte V25-style layout
  misparses as a 0-data ACK.
- Each exchange call = new request id; NOT_READY retries within ONE
  exchange keep the same id (request built once, re-sent).
- First control read of a reply always contains at least the full header
  on real hardware.
- Read XnHostProtocol.cpp COMPLETELY before touching exchange logic —
  the first pass missed Execute's tail (the accumulation loop) and
  ReceiveReply's id/magic retry loops.
- Earlier gotchas still valid: the bulk packet header's magic IS the fw
  magic and the 12-byte header INCLUDES it (partial-magic stitch seeds
  both bytes); EOF close only when the packet's last byte arrives; SOF
  bookkeeping only on first data chunk; "UNCOMPRESSED_11_BIT" depth is
  packed-11 on the wire; rusb 0.9 devices() needs .iter(); thiserror
  #[from] needs std::error::Error impl; jdk/jre conflict; GL header split;
  blocked hosts (freedesktop GitLab, packages.debian.org) vs working
  (deb.debian.org, crates.io, github, apache.org); OPENNI2_REDIST
  build-time env; model cannot read images → programmatic PDF checks.

## 7. Open threads (if the user continues)

- **Re-run on hardware (the immediate next action):** the user pulls
  823dd53 and runs, with the Xtion on a USB 2.0 port:
  `cargo test --test hardware -- --nocapture`,
  `cargo run --example list`, `cargo run --example depth_dump -- 30`.
  If something still fails, the version-parse error now prints the RAW
  reply bytes — ask the user for that output; it pinpoints any remaining
  wire-format mismatch.
- If GetVersion now works but later steps fail: fixed-params chunk reads
  and ALGORITHM_PARAMS go through the SAME exchange path (now robust);
  stream start uses set_param sequences; the likely next candidates are
  timing/delays after stream-mode changes (see XnSensorFirmwareParams
  SetStreamMode + nUSBDelaySetParamStream*Mode = 1 ms for fw >= 5.3.16).
- M6-M8 features (registration, MJPEG, COMPRESSED_PS, keep-alive,
  timestamp conversion, LED/emitter) mapped in docs/PLAN.md.
- If pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds with the user's token if /tmp was recycled;
  never print or commit the token).
- Golden vectors: `cd reference && gcc -O2 -o /tmp/c_golden c_golden.c &&
  /tmp/c_golden > golden.txt && cargo test --test golden`.
