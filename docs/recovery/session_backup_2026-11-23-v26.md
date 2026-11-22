# Chat Session Backup — ASUS Xtion Linux research

**Version:** 26 (2026-11-23, turn 26): 0.2.8 validated on CI by the user
(all hardware tests compile-and-pass; camera not attached in the pasted
run, so the on-camera run is still pending), FEATURE_PLAN refreshed with
the 0.2.8 status + roadmap, firmware analysis PDF rebuilt as **Rev 2**
(12 pages, QA pass). Previous: 25 (turn 24/25): 0.2.7 diagnostics +
0.2.8 implementation. **State: 0.2.8 pushed (76ff606 + docs 674d7b0);
color uses the C's Linux/bulk mode (PS-compressed format 1) with the
init soft reset ported; the read-only command tier (12 opcodes) is in.
NEXT ACTION: on-camera run of the full suite + `system_commands_readonly`;
NEXT FEATURE: firmware cropping (PARAMs 46-60), then registration
end-to-end.**

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
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER printed or
   committed; refreshed 2026-11-23 from the user's message). Push via
   `umask 077 && printf 'https://thomas7475:%s@github.com\n'
   "$(cat /home/z/my-project/.gh-token)" > /tmp/gh-creds` then
   `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.

## 2. Task history (condensed)

- **Sessions 1-2, turns 1-5 (2026-11-19..21):** research report on the Xtion
  (1d27:0601) on CachyOS; NOT UVC — only the OpenNI2 PS1080 stack works;
  test + setup scripts; AUR openni2 PKGBUILD audited **clean**; delivered
  `openni2-nojava` package.
- **Turn 6:** Rust-stack options report; Option A verified end-to-end.
- **Turn 7:** Option D chosen — pure-Rust driver, 44 tests, pushed (deeb688).
- **Turns 8-12:** the five on-hardware bugs: split replies (0.1.1),
  EOVERFLOW chunking (0.1.2), async 16-URB ring (0.1.3), big-endian
  buf_size (0.1.4), 2048-entry shift LUT (0.1.5).
- **Turn 13:** depth hardware-VALIDATED (87.4% valid, 12.17 MiB/s,
  29.97 fps); driver overview PDF.
- **Turn 15:** firmware decompiled (Ghidra, 165 functions); ASUS fw 5.8.22
  mirrored; firmware analysis PDF Rev 1.
- **Turn 16:** 0.2.0 — identity, IR stream, events, +20 methods.
- **Turn 18-19:** 0.2.1/0.2.2 — Jango fallback + CMOS blanking fetch
  (required before any stream produces packets).
- **Turn 20:** 0.2.3 — IR ROOT CAUSE: MT9M001 transmits height + 8 rows.
- **Turn 21:** 0.2.4 — color hardware test; `Nack(BadParams)` on start.
- **Turn 22:** 0.2.5 — CMOS presets (opcode 36) + validation.
- **Turn 23:** 0.2.6 — preset endianness fix (fields are LITTLE-endian;
  the 0.2.5 BE parse shifted everything x256).
- **Turn 24:** 0.2.7 — GET_MODE/SET_MODE (HostMode), soft_reset,
  color_start_probe; C open path verified write-for-write identical.
- **Turn 25:** 0.2.8 — COLOR NACK ROOT CAUSE + fix: (a) the C soft-resets
  the firmware at EVERY init (XnSensorFirmware::Init, bResetOnStartup=
  TRUE) — ported into open() with the true 1 ms Jango delay (fw >= 5.3.16,
  XnHostProtocol.cpp:559-564), not 800 ms; (b) the C's non-Windows image
  default is the PS-COMPRESSED format 1 ("non win32 works with BULK, so
  it's better to compress the image", XnSensorImageStream.h:36-41) — the
  uncompressed mode (5) the presets list is the Windows/ISO path and its
  stream start is what the bulk device rejected. Ported
  XnStreamUncompressYUVImagePS (4-bit diff/full stream, U/V channel carry,
  per-line reset) + read_color_frame gates compressed frames on the
  DECODED size. Read-only command tier shipped: GET_LOG 7, I2C_READ 11
  (+ CMOS registers via fixed-params bus/slave), DOWNLOAD_FILE 16,
  GET_FLASH_MAP 18, GET_FILE_LIST 19, READ_AHB 20, READ_FLASH 25,
  GET_CPU_STATS 27, BIST 28, GET_TEC_DATA 30/GET_FAST_CONVERGENCE_TEC 38,
  GET_EMITTER_DATA 32, GET_CMOS_BLANKING 35 (src/protocol/system.rs +
  12 Xtion methods + system_commands_readonly hardware test). FixedParams
  gained CMOS I2C bus/address; HostMode gained WaitForEnum(9)/SafeMode(10).
  105 lib + 2 golden + 1 doc green, clippy 0, fmt clean, hardware target
  compiles. PUSHED 76ff606.
- **Turn 26 (this turn):** user confirms 0.2.8 tests pass on their machine
  (pasted run shows all five hardware tests green; camera was NOT attached,
  so on-camera validation is still pending), supplies a refreshed GitHub
  PAT (stored, never printed). This turn: FEATURE_PLAN.md updated (Phase 3a
  done, exclusions narrowed to WRITE-side, roadmap renumbered; pushed
  674d7b0); firmware analysis PDF rebuilt as REV 2 (12 pages, QA PASS):
  driver-usage tables updated to 25/24/8/2, adoption ledger (section 7),
  wire-level addendum (section 9: bulk/ISO preset semantics, init reset +
  1 ms delay, preset LE truth + retraction of the Rev 1 byte-swap claim,
  PS-compression format, timing semantics, read-only reply layouts, CMOS
  blob TLV observation), method/sources updated. Workspace backup v26 zipped.

## 3. Hardware test inventory (SEVEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green (camera-validated).
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — now picks the COMPRESSED (1, VGA@30) mode;
   **on-camera validation pending** (0.2.4-0.2.7 runs NACKed the
   uncompressed path; 0.2.8 is the compressed rewrite).
6. `color_start_probe` — diagnostic, now walks the compressed C-parity
   sequence; informational only.
7. `system_commands_readonly` — NEW 0.2.8: reports log, file list, flash
   map, ImageCmos download, CPU stats, TEC + emitter telemetry, direct
   CMOS blanking, CMOS register reads (I2C), AHB read, raw flash read;
   each section may print `unavailable: ...` without failing.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- color_stream_roundtrip --nocapture`

## 4. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v26.md` | This file (v26, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v26.zip` | Turn-26 zip (repo with .git at 0.2.8 + docs + this backup). |
| `xtion_rs_firmware_analysis.pdf` | **Rev 2** firmware report (12 pages, 2026-11-23). |
| `xtion_rs_firmware_analysis_cover.html` | Cover source (HTML/Playwright pipeline deliverable). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (13 pages; historical, 0.1.5). |
| `xtion_rs_next_steps_and_validation.pdf` | Turn-17 validation plan (9 pages). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 674d7b0 (0.2.8 + FEATURE_PLAN docs)**; history: 76ff606 (0.2.8)
-> 674d7b0 (docs)). The repo SURVIVED on disk this turn (no re-clone
needed); if a future turn finds it missing, clone with the stored token.

## 5. Workspace state (2026-11-23, after turn 26)

- `/home/z/my-project/xtion-rs/` — full clone WITH .git at 0.2.8, 108
  checks green (105 lib, 2 golden, 1 doc), clippy 0, fmt clean, hardware
  target compiles. 0.2.8 changed: src/protocol/mod.rs (opcodes 7/11/16/
  18/19/20/25/27/28/30/32/35/38, HostMode 9/10, IMAGE_DEFAULT_INPUT_FORMAT
  = Yuv422 compressed), src/protocol/system.rs (NEW, ~620 lines + 17
  tests), src/protocol/fixed_params.rs (I2C bus/address fields),
  src/color.rs (uncompress_yuv_image_ps + 6 tests), src/device.rs
  (firmware_init_reset in open(), ColorConfig{resolution,format},
  start_color compressed+uncompressed, read_color_frame decoded-size gate,
  12 system methods), src/lib.rs (re-exports), tests/hardware.rs
  (compressed color test + system_commands_readonly), README.md,
  docs/VALIDATION.md, docs/PROTOCOL.md (opcode list + mode lifecycle +
  preset LE correction + color section rewrite), docs/FEATURE_PLAN.md
  (0.2.8 status + roadmap), Cargo.toml 0.2.8.
- Firmware RE artifacts: /home/z/my-project/xtion-rs/firmware/5.8.22/
  (the .fla blobs; source.out.bin.fla maps 1:1 at base 0);
  scripts/fw_disasm.py + fw_scan_cmp.py (capstone Thumb analysis, paths
  fixed to the repo copy). The v17 upload zip at /home/z/my-project/upload/
  holds openni2-nojava + rust-xtion-check snapshots (no Ghidra dump).
- OpenNI 2.2.0 reference source: tarball verified at
  `firmware_dl/openni_ref/v2.2.0-debian.tar.gz`; extract after recycle:
  `mkdir -p /tmp/xtion-verify && tar -xzf firmware_dl/openni_ref/
  v2.2.0-debian.tar.gz -C /tmp/xtion-verify --strip-components=1`
  (key files: Source/Drivers/PS1080/Sensor/*, Include/PS1080.h,
  Source/Drivers/PS1080/Include/XnStreamParams.h).
- PDF pipeline (this turn): skills/pdf (report route) — cover via
  scripts/fw_cover.html -> html2poster.js (Template 01 HUD, palette.cascade),
  body via scripts/fw_report_body.py (TocDocTemplate + multiBuild,
  reportlab 4.4.9: TableOfContents lives in
  reportlab.platypus.tableofcontents, TableStyle in
  reportlab.platypus.tables), merged via pypdf with
  cover.scale_to(A4); QA: pdf_qa.py --skip-cover PASS.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (stable,
  minimal profile + clippy + rustfmt). Use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 6. Gotchas learned (cumulative, new first)

- **The C's soft-reset delay is 1 ms, not 800 ms** — the 800 ms default
  only applies to pre-5.3.16 firmware or gadgetfs cores; Jango cores of
  fw >= 5.3.16 get nUSBDelaySoftReset = 1 (XnHostProtocol.cpp:559-564).
  The keep-alive retry loops are the real liveness gate.
- **The preset table is a capability list across BOTH USB interfaces**;
  on bulk endpoints the vendor stack uses PS-COMPRESSED (format 1) —
  uncompressed modes (5/7) belong to the Windows/ISO path and their
  stream starts NACK on this device. The C's own words:
  "non win32 works with BULK, so it's better to compress the image".
- **After a failed stream start the firmware rejects even plain
  mode-parameter writes** (the "redundant-write rejection" suspicion of
  0.2.7 was wrong) — the C never sees this because it soft-resets at
  every init. Any driver that skips the reset must expect dirty state.
- **PS-compressed YUV (XnStreamUncompressYUVImagePS)**: 4-bit elements
  (0x0-0xC diff = value-6, 0xD dummy, 0xF full + 2 nibbles), channel
  rotation Y0/U/Y1/V with V:=U after U and U:=V after V (case 2/case 4),
  per-line state reset at width*2 output bytes; frames have VARIABLE raw
  size — gate on the DECODED size (width*height*2), not the wire size.
- **reportlab 4.4.9 imports**: TableOfContents from
  reportlab.platypus.tableofcontents; TableStyle from
  reportlab.platypus.tables (NOT reportlab.lib.styles).
- **GET_LOG entries**: {u16 entry_size = PAYLOAD words, u16 log_type,
  u32 timestamp} + payload; text type 0x1200, overflow 0x1201 (fw >= 3.0).
  I2C bytes travel as u16 words; CMOS register value = (w0 << 8) + w1.
  READ_FLASH takes a BYTE offset; DOWNLOAD_FILE takes WORDS.
  XnFlashFile = 24 bytes packed; flash-map entry = 14 bytes.
- **BIST masks** (PS1080.h:373-379): IMAGE_CMOS 1, IR_CMOS 2,
  POTENTIOMETER 4, FLASH 8, FULL_FLASH 16, PROJECTOR 32, TEC 64; only the
  low 16 bits go on the wire; reply = u32 failure bitmask; 300 s timeout.
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

- **Immediate:** on-camera run of 0.2.8 — plug the camera, run the full
  suite (`cargo test --test hardware -- --nocapture`) and paste the
  output; the color test now uses the compressed (1, VGA@30) mode and
  `system_commands_readonly` dumps the new read-only surface. If color
  still fails: run `color_start_probe` (it now walks the compressed
  C-parity sequence) and paste; the next diagnostic step would be a
  usbmon capture of a real OpenNI color session.
- **Next feature (Phase 3):** firmware cropping (PARAMs 46-60, "bigger
  crop first, then shrink", SizeX -> SizeY -> OffsetX -> OffsetY -> Mode
  last) per the turn-17 spec; then the image-open C-parity extras
  (crop-mode 50 = 0, params 80/77 writes at open, bImageAdjustments-
  Supported fw >= 5.8.9); then registration end-to-end (algorithm 0x02
  tables) — the highest-value item for the photogrammetry use case.
- Phase 4 backlog: AGC bins (63-70) + wavelength (73/74, algorithm 0x80),
  GMC (0xa200 + opcode 26), audio (large), remaining stream modes.
- Visual checks: `cargo run --example ir_dump -- 10` (ir_frame.pgm) and
  `cargo run --example depth_dump -- 10`; a `color_dump` example does
  not exist yet (nice-to-have alongside the color validation).
- Golden vectors: `cargo test --test golden`.
- Pushing again: `cd /home/z/my-project/xtion-rs && git -c
  credential.helper='store --file=/tmp/gh-creds' push origin main`
  (recreate /tmp/gh-creds from /home/z/my-project/.gh-token if /tmp was
  recycled; never print or commit the token).
