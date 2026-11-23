# Chat Session Backup — ASUS Xtion Linux research

**Version:** 28 (2026-11-23, turn 28): COLOR NACK FINAL ROOT CAUSE FOUND
AND SHIPPED — **this unit has NO color sensor.** The 0.2.9 on-camera run
of `color_stream_roundtrip` printed `image cmos type: 0` and still
NACKed BadParams at the stream-on write; that fixed-params field IS the
reference driver's `bImageSupported` gate (XnSensorFirmware.cpp:128-140,
fw >= 5.6.9: `image_cmos_type != 0`), and the C never creates the image
stream when it is 0 (XnSensor.cpp:274-281) — OpenNI lists no color node
on this device either. Every stream-on NACK since 0.2.4
(compressed/uncompressed x VGA/QVGA, all with full C parity by 0.2.9)
was the firmware refusing to route COLOR from an absent sensor, while
IR — the other stream0 source, fed by the depth CMOS — starts fine on
the same pipe. 0.2.10 (PUSHED 4099997): `Xtion::image_supported()`
exposes the gate, `start_color` fails fast with an explicit
no-color-sensor error BEFORE writing any image params, and the color
hardware test SKIPS with the full explanation (prints depth_cmos_type
as the parse cross-check). 106 lib + 2 golden tests green, clippy 0,
fmt clean, hardware target compiles. **State: color is closed as a
hardware capability question; the complete compressed-color
implementation waits for a unit with an RGB sensor. NEXT ACTION: user
runs the full suite on 0.2.10 — expected: all seven tests green with
`color_stream_roundtrip` skipping with the reason. NEXT FEATURE
(Phase 3): firmware cropping (params 46-60), then registration.**

> Purpose: upload this file in a new chat to restore full context. It records
> the standing rules, the task, everything that was found and decided, every
> file that was produced, and the state of the current workspace.

## 1. Standing user rules (apply to EVERY future turn)

1. **Communicate in English only** (all replies, all document content).
2. **Zip backup of generated files to `download/`** every turn.
3. **Do NOT run full cargo builds of the complete project — the sandbox does
   not have enough RAM.** Use targeted invocations: `cargo test --lib -j 2`,
   `cargo test --test golden -j 2`, `cargo clippy --lib -j 2`,
   `cargo test --test hardware --no-run -j 2`; never `cargo build`/
   all-target builds, no release builds.
4. **Produce a downloadable session backup** (this file, refreshed with a new
   version/date each turn) so the chat can be continued/resumed later.
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER printed or
   committed; re-verified 2026-11-23 against the user's latest message).
   Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat /home/z/my-project/.gh-token)"
   > /tmp/gh-creds` then
   `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.
6. **Ignore the `research/idong100*` branches** on the remote (user
   instruction, turn 28): `research/idong100`,
   `research/idong100-01-chinese-web`,
   `research/idong100-02-products-software`,
   `research/idong100-09-xeyez-analysis`,
   `research/idong100-10-rust-driver-draft` — a multi-agent research
   fork (Chinese-web findings, i-dong products, xeyez stack, a Rust
   driver draft). All xtion-rs work goes to `main` only.

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
- **Turns 16-20 (0.2.0-0.2.3):** identity + IR stream + events; Jango
  fallback; CMOS blanking fetch; IR wire size (MT9M001 height+8).
- **Turns 21-25 (0.2.4-0.2.8):** color test NACKed BadParams at
  stream-on through every iteration — presets (0.2.5), LE fix (0.2.6),
  probe + soft reset (0.2.7), PS-compressed + init soft reset +
  read-only tier (0.2.8, PDF Rev 2, FEATURE_PLAN refresh).
- **Turn 27 (lost-context tail, recovered from this repo):** 0.2.9 —
  stream hygiene (OFF-first, kill-at-open/shutdown/Drop) + full
  image-open C parity (flicker 17, mirror 71, blanking, FREQUENCY 0x80,
  stream-on, crop 50, AE 80, AWB 77) + the self-reporting stream-on
  error + cmos-type/boot-log diagnostics. PUSHED 711dd92.
- **Turn 28 (this turn):** the user's 0.2.9 on-camera run: 4 tests green
  (identity/depth/phase1/IR), color still NACKed at stream-on — but the
  new diagnostic line printed `image cmos type: 0`. Root cause closed
  (section 3). 0.2.10 implemented + PUSHED 4099997. Local tree had been
  wiped by a sandbox recycle (re-cloned from GitHub; /home/z/my-project
  itself now carries a snapshot `.git` from the recycle — the real repo
  is /home/z/my-project/xtion-rs).

## 3. Turn-28 findings (the color root cause, CLOSED)

- **`image_cmos_type = 0` = `XN_IMAGE_CMOS_NONE`** (PS1080.h:425-431:
  0 none / 1 MT9M112 / 2 MT9D131 / 3 MT9M114) for this unit (fw 5.8.22,
  serial 1408090603), parsed from fixed-params byte 76.
- **That field IS the C's `bImageSupported` gate**:
  XnSensorFirmware.cpp:128-140 — for fw >= 5.6.9
  (`bGetImageCmosTypeSupported`, XnHostProtocol.cpp:630-633),
  `bImageSupported = (image_cmos_type != 0)`; pre-5.6.9 firmwares use an
  image-blanking heuristic instead.
- **The C never creates the image stream when the gate is false**
  (XnSensor.cpp:274-281: `AddSupportedStream(XN_STREAM_TYPE_IMAGE)` is
  inside `if (bImageSupported)`) — so OpenNI lists no color node on this
  device, and the C never writes STREAM0_MODE=COLOR, never meeting the
  NACK. The driver's 0.2.4-0.2.9 attempts could not have succeeded:
  the firmware rejects routing COLOR from an absent sensor.
- **Parse cross-validation**: `depth_cmos_type` (byte 56: 1 = MT9M001,
  2 = AR130, PS1080.h:418-423) sits in the same struct and DRIVES the
  IR wire-size path (`wire_extra_rows`) — IR passes on hardware, so the
  fixed-params layout is hardware-verified and the image_cmos_type = 0
  reading is trustworthy. The color test now prints depth_cmos_type too.
- **The opcode-36 preset table (25 color modes) is a PLATFORM (RD108x)
  capability list**, not a per-unit sensor inventory — the firmware
  answers with the compiled-in table regardless of the fitted sensor;
  the C validates modes against it only AFTER the bImageSupported gate.
- **The USB PID cannot settle Pro vs Pro Live**: the C's enumeration
  table (XnDeviceEnumeration.cpp:36-39) covers 1D27:0500/0600/0601/0609
  with no product mapping, and web sources show 0601 on late batches of
  BOTH the depth-only Xtion Pro and the Pro Live. The CMOS-type report
  is the only reliable answer — which is exactly why the C gates on it.
- Independent confirmations available to the user:
  `color_start_probe` (prints both CMOS types + the boot self-test log —
  color lines start with "CLR"), `lsusb -v` product string, and the
  physical check (depth-only units have two optical windows, Pro Live
  has three).

## 4. 0.2.10 changes (commit 4099997, pushed to main)

- `Xtion::image_supported()` — the bImageSupported gate as a public
  accessor; `image_presets()` doc now notes the platform-list caveat.
- `start_color`: fw >= 5.6.9 && image_cmos_type == 0 → BadConfig with
  the full no-color-sensor explanation, BEFORE any image param write
  (device state stays clean). Doc comment carries the finding.
- `color_stream_roundtrip`: prints the gates, then SKIPS with the full
  explanation when image_cmos_type == 0 (mirrors the no-camera skip);
  prints depth_cmos_type (1=MT9M001, 2=AR130) as the parse cross-check.
- `color_start_probe`: prints both CMOS types.
- Docs: PROTOCOL.md §7 "No color sensor (bImageSupported)",
  FEATURE_PLAN.md §0c + status, VALIDATION.md table row 5 + FINAL triage
  entry, README status row + 0.2.10 changelog. Cargo.toml 0.2.10.
- Checks: 106 lib + 2 golden green, clippy 0 (-D warnings), fmt clean,
  hardware target compiles.

## 5. Hardware test inventory (SEVEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated; 265,769 valid px).
3. `phase1_identity_and_options` — green (camera-validated; emitter
   toggle NACKs on this fw — expected skip).
4. `ir_stream_roundtrip` — green (camera-validated; values 10..1022).
5. `color_stream_roundtrip` — **on this unit: SKIPS with the no-sensor
   explanation (0.2.10)**; on units with image_cmos_type != 0 runs the
   full PS-compressed VGA@30 round-trip.
6. `color_start_probe` — diagnostic; dumps mode, BOTH cmos types, boot
   log (CLR/PRJ/TEC), presets, full write walk with readbacks.
7. `system_commands_readonly` — 0.2.8 read-only tier dump.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- color_stream_roundtrip --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v28.md` | This file (v28, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v28.zip` | Turn-28 zip (repo with .git at 0.2.10 + this backup). |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (12 pages, 2026-11-23; restored from upload/ after the recycle wiped download/). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (13 pages; historical, 0.1.5; restored from upload/). |
| `xtion_rs_next_steps_and_validation.pdf` | Turn-17 validation plan — LOST in the recycle; recover from `upload/asus_xtion_workspace_backup_2026-11-21_v17.zip` or older zips if needed. |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report — LOST in the recycle; same recovery path. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 0.2.10 lineage: 711dd92 (0.2.9) -> 1ed1aa4 (v27 docs) ->
4099997 (0.2.10) -> docs commit with this backup**). If a future turn
finds the tree missing, re-clone with the stored token; the remote also
carries the `research/idong100*` branches to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 28)

- `/home/z/my-project/xtion-rs/` — fresh clone WITH .git at 0.2.10 (+docs
  commit). 0.2.10 changed: src/device.rs (image_supported, start_color
  gate + doc, image_presets doc), tests/hardware.rs (skip + cmos prints),
  docs/PROTOCOL.md, docs/FEATURE_PLAN.md, docs/VALIDATION.md, README.md,
  Cargo.toml.
- **Sandbox-recycle artifact:** /home/z/my-project itself now contains a
  snapshot `.git` ("Initial commit" by "Z User <z@container>" with the
  upload/download contents + `.env`). Harmless; the real project repo is
  the xtion-rs subdirectory. Do not confuse the two (check `git rev-parse
  --show-toplevel`).
- OpenNI 2.2.0 reference source: extract from
  `/home/z/my-project/firmware_dl/openni_ref/v2.2.0-debian.tar.gz`
  (SURVIVED the recycle) to /tmp/xtion-verify:
  `mkdir -p /tmp/xtion-verify && tar -xzf /home/z/my-project/firmware_dl/
  openni_ref/v2.2.0-debian.tar.gz -C /tmp/xtion-verify
  --strip-components=1`.
- Firmware RE artifacts: /home/z/my-project/xtion-rs/firmware/5.8.22/
  (the .fla blobs) + scripts/fw_*.py in my-project/scripts/ (survived).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **bImageSupported = `image_cmos_type != 0`** for fw >= 5.6.9
  (XnSensorFirmware.cpp:128-140); the C only then creates the image
  stream (XnSensor.cpp:274-281). A unit reporting
  XN_IMAGE_CMOS_NONE (0) CANNOT stream color — the firmware NACKs
  STREAM0_MODE=COLOR BadParams; OpenNI shows no color node. Depth+IR
  come from the depth CMOS (byte 56: 1=MT9M001, 2=AR130) and are
  unaffected.
- **The opcode-36 preset table is a platform capability list** (RD108x
  firmware), NOT a per-unit sensor inventory — a sensorless unit still
  lists 25 color modes. Validate the gate BEFORE trusting presets.
- **1d27:0601 covers late Xtion Pro AND Pro Live batches** — the PID
  cannot tell depth-only from RGB units; only the CMOS-type report can.
- The `research/idong100*` remote branches are a separate multi-agent
  research fork — ignore them; all work goes to main.
- Stream-mode params are a state machine: mode changes must go through
  OFF; every C session ends with all streams OFF; the init soft reset
  does NOT clear stream modes (0.2.9).
- STREAM0_MODE values: OFF/COLOR/DEPTH/IR = 0/1/2/3 (image pipe =
  stream0: color XOR IR; depth = stream1).
- The image-open C sequence: format->res->fps->flicker(17)->mirror(71)->
  blanking(0x06)->FREQUENCY(0x80, res0/fps0)->stream0=COLOR->crop(50)->
  AE(80,1)->AWB(77,1) on fw >= 5.8.9.
- The wire fixed-params layout is +1 word vs the C's in-memory struct;
  offsets hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- The C's soft-reset delay is 1 ms on Jango cores (fw >= 5.3.16).
- PS-compressed YUV (format 1) is the C's non-Windows/bulk image default
  (XnSensorImageStream.h:36-41); uncompressed (5) is the Windows/ISO
  path. `uncompress_yuv_image_ps` decodes format 1 (4-bit diff/full
  stream, channel carry, per-line reset, gate on DECODED size).
- Property SetAlwaysSet(TRUE): the C re-writes firmware params even when
  cached; SET_PARAM retries 5x on timeout only, BadParams is final.
- The firmware preset/param tables are NOT in the main image (they come
  from the flash param images, e.g. ImageCmos.xml.bin.fla).
- GET_LOG entries: {u16 entry_size = PAYLOAD words, u16 log_type, u32
  timestamp} + payload; text 0x1200. I2C bytes as u16 words; CMOS reg =
  (w0<<8)+w1. READ_FLASH takes BYTE offsets; DOWNLOAD_FILE takes WORDS.
- BIST masks (PS1080.h:373-379): IMAGE_CMOS 1, IR_CMOS 2, POT 4, FLASH
  8, FULL_FLASH 16, PROJECTOR 32, TEC 64; 300 s timeout.
- rustup minimal profile lacks clippy/rustfmt — add after install.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker, 0 = invalid.
- Partial first frames must be dropped (assembled != expected).
- buf_size in the 12-byte stream packet header is BIG-ENDIAN.
- PS1080 stream needs HIGH speed USB (~12.7 MB/s depth packed-11);
  always keep the async 16-URB ring; bulk reads multiple of
  wMaxPacketSize; replies can be SPLIT across control-IN transfers;
  v2.6 replies have NO CRC; the 12-byte bulk packet header INCLUDES the
  fw magic; ignore one garbage chunk per endpoint on fw < 5.0.
- Opcode 42 (emitter) NACKs BadParams on this build — expected skip.
- The CommandLink fake-transport unit tests need FULL reply packets.

## 9. Open threads (if the user continues)

- **Immediate:** pull + run the full suite on 0.2.10:
  `cargo test --test hardware -- --nocapture` — expected ALL GREEN with
  `color_stream_roundtrip` printing the skip explanation. Optional
  extra evidence: `cargo test --test hardware -- color_start_probe
  --nocapture` (both CMOS types + boot log CLR lines) and a physical
  check of the housing (two optical windows = depth-only unit).
- **If the user needs COLOR for the photogrammetry work:** this unit
  cannot provide it — an Xtion Pro Live (three windows) or another
  RGB-capable unit is required; the driver's compressed-color path is
  complete and will run on such hardware unchanged.
- **Next feature (Phase 3):** firmware cropping (PARAMs 46-60, "bigger
  crop first, then shrink", SizeX -> SizeY -> OffsetX -> OffsetY ->
  Mode last) per the turn-17 spec; then registration end-to-end
  (algorithm 0x02 tables) — note registration aligns depth onto COLOR,
  so on this unit it is moot; IR-based work (cropping, GMC) is the
  useful track for this hardware.
- Phase 4 backlog: AGC bins (63-70) + wavelength (73/74), GMC
  (0xa200 + opcode 26), audio (large), remaining stream modes.
- Golden vectors: `cargo test --test golden`.
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token).
