# Chat Session Backup — ASUS Xtion Linux research

**Version:** 27 (2026-11-23, turn 27): COLOR NACK re-audited against the
C source line-by-line; 0.2.9 (stream hygiene + full image-open C parity)
implemented, tested (106 lib + 2 golden + clippy 0 + fmt clean + hardware
target compiles) and PUSHED (711dd92). Previous: 26 (turn 26): 0.2.8
validated on CI, FEATURE_PLAN refreshed, firmware PDF Rev 2. **State:
camera attached, depth + IR pass, color still NACKed BadParams on the
stream0=COLOR write in 0.2.8; 0.2.9 ports every stream-mode guard the C
has (go-through-OFF, kill-at-open/shutdown/Drop) plus the four missing
image-open writes; NEXT ACTION: on-camera run of
`cargo test --test hardware -- color_stream_roundtrip --nocapture` — if
it still fails, run `color_start_probe` (now dumps image CMOS type =
the C's bImageSupported gate, the firmware boot log CLR/PRJ/TEC lines,
and walks the whole write sequence with readbacks) and paste BOTH.**

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
   committed). Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat /home/z/my-project/.gh-token)"
   > /tmp/gh-creds` then
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
- **Turn 23:** 0.2.6 — preset endianness fix (fields are LITTLE-endian).
- **Turn 24:** 0.2.7 — GET_MODE/SET_MODE (HostMode), soft_reset,
  color_start_probe; C open path verified write-for-write identical.
- **Turn 25:** 0.2.8 — init soft reset ported (1 ms Jango delay), color
  switched to the C's Linux/bulk PS-compressed format 1,
  `uncompress_yuv_image_ps` decoder, read-only command tier (12 opcodes).
  PUSHED 76ff606 + docs 674d7b0.
- **Turn 26:** user confirms 0.2.8 passes on their machine (camera NOT
  attached in that run); FEATURE_PLAN refreshed; firmware PDF Rev 2.
- **Turn 27 (this turn):** user runs the suite WITH the camera attached:
  identity/depth/phase1/IR all pass (depth 265,769 valid px, IR values
  10..1022); `color_stream_roundtrip` still NACKs BadParams on the
  stream0=COLOR write ("the test still fails"). THIS TURN = full C-source
  re-audit of the image path + 0.2.9 (below), pushed 711dd92.

## 3. Turn-27 findings (the 0.2.9 root-cause audit)

Facts re-verified against the OpenNI 2.2.0 source (extracted from
`firmware_dl/openni_ref/v2.2.0-debian.tar.gz` to /tmp/xtion-verify):

- **STREAM0_MODE (param 5) is a stream SELECTOR, not a format**:
  XnVideoStreamMode = OFF 0 / COLOR 1 / DEPTH 2 / IR 3
  (XnStreamParams.h:226-229). Image format/res/fps are params 12/13/14
  (XnParams.h). Our color writes were value-correct.
- **The reference HOST guard**: `XnSensorFirmwareParams::SetStreamMode`
  (cpp:585-600): "we require that every change to mode will go through
  OFF" — any mode→mode change without OFF is refused. The firmware
  enforces the same rule server-side with NACK(BadParams).
- **Every C session ends with all streams OFF**: the pre-5.2 soft-reset
  path writes stream0/1/2 OFF (XnHostProtocol.cpp:1804-1813) and device
  shutdown does the same (XnSensor.cpp:315-317). Our tests that panic
  never wrote OFF; the init soft reset does NOT clear the stream modes.
- **The image-open sequence has four writes xtion-rs skipped**:
  flicker (17) + firmware mirror (71) at configure
  (XnSensorImageStream::ConfigureStreamImpl cpp:298-321), and crop mode
  (50, disabled) + auto exposure (80, default TRUE) + auto white balance
  (77, default TRUE) at OpenStreamImpl for fw >= 5.8.9
  (bImageAdjustmentsSupported, XnHostProtocol.cpp:655-658).
- **XnSensorStreamHelper::Open fetches ALGORITHM_PARAMS FREQUENCY
  (0x80, one f32, resolution 0 / fps 0) on EVERY stream open** between
  configure and stream-on (cpp:123-143).
- **SetCmosConfig = blanking FETCH only** (host-side cache;
  XnCmosInfo.cpp:30-64) — our `read_cmos_blanking` is already 1:1.
- **Property layer does NOT skip unchanged values** — firmware params are
  `SetAlwaysSet(TRUE)` (XnSensorFirmwareParams.cpp:269), so the C re-writes
  stream0=COLOR even when the cache matches; skip-if-unchanged is NOT the
  difference.
- **`XnHostProtocolSetParam` retries 5x on timeout but treats BadParams as
  final**; per-param recv timeouts are 1 ms on Jango cores
  (nUSBDelaySetParam*Stream0Mode/Flicker etc., cpp:259-263, 546-563).
- **bImageSupported** = fixed params `nImageCmosType != 0`
  (XnSensorFirmware.cpp:128-140; XnImageCMOSType: 0 none / 1 MT9M112 /
  2 MT9D131 / 3 MT9M114). The C refuses to create the image stream when
  0. Our FixedParams already parses image_cmos_type (wire byte 76; our
  whole fixed-params parse is +1 word vs the C in-memory struct because
  the WIRE V3.0 layout has one extra head word — verified by the
  hardware-correct serial AND geometry floats).
- Firmware param table/preset tables are NOT in the main image (presets
  come from the flash param images, e.g. ImageCmos.xml.bin.fla).

## 4. 0.2.9 changes (commit 711dd92, pushed)

- `firmware_init_reset` step 7: best-effort SET_PARAM stream0/1/2 ← OFF
  after the init reset (C reset-path + shutdown parity).
- `impl Drop for Xtion`: cancel rings + best-effort stream kill (so a
  panicked test can never leave stale stream state).
- `start_depth`/`start_ir`/`start_color`: stream-mode ← OFF first.
- `configure_color_via` (transport-generic, unit-tested): OFF → flicker 0
  → mirror 0 → blanking fetch → FREQUENCY fetch (res 0/fps 0) → stream-on
  → crop-mode 0 → AE 1 → AWB 1. `start_color` writes format/res/fps
  before it (C property pipeline order) and the stream-on error now
  self-reports stream0 + firmware mode.
- `Param::ImageCropMode = 50` added; `read_device_frequency` added
  (algorithm 0x80).
- tests/hardware.rs: color test prints image_cmos_type + mode + stream0
  before starting; `color_start_probe` walks the FULL sequence with
  per-step readbacks, dumps the boot self-test log (CLR/PRJ/TEC lines)
  and the image CMOS type.
- Docs: README changelog 0.2.9, docs/FEATURE_PLAN.md section 0b,
  docs/PROTOCOL.md "Stream-mode hygiene (0.2.9)". Cargo.toml 0.2.9.
- Checks: 106 lib + 2 golden green, clippy 0, fmt clean, hardware target
  compiles.

## 5. Hardware test inventory (SEVEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green (camera-validated; emitter
   toggle NACKs BadParams on this fw build — expected, reported skip).
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — 0.2.9 rewrite pending on-camera validation.
6. `color_start_probe` — diagnostic; now dumps mode, image CMOS type,
   boot log (CLR/PRJ/TEC), presets, full write walk with readbacks.
7. `system_commands_readonly` — 0.2.8 read-only tier dump.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- color_stream_roundtrip --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v27.md` | This file (v27, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v27.zip` | Turn-27 zip (repo with .git at 0.2.9 + docs + this backup). |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (12 pages, 2026-11-23). |
| `xtion_rs_firmware_analysis_cover.html` | Cover source (HTML/Playwright pipeline deliverable). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (13 pages; historical, 0.1.5). |
| `xtion_rs_next_steps_and_validation.pdf` | Turn-17 validation plan (9 pages). |
| `asus_xtion_rust_stack_options.pdf` | Turn-6 options report. |

The driver lives in the git repo `/home/z/my-project/xtion-rs` (remote
origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 711dd92 (0.2.9)**; history: 76ff606 (0.2.8) -> 674d7b0 (docs) ->
f76da97 (recovery v26) -> 711dd92 (0.2.9)). The repo SURVIVED on disk this
turn (no re-clone needed); OpenNI reference re-extract:
`mkdir -p /tmp/xtion-verify && tar -xzf firmware_dl/openni_ref/
v2.2.0-debian.tar.gz -C /tmp/xtion-verify --strip-components=1`.

## 7. Workspace state (2026-11-23, after turn 27)

- `/home/z/my-project/xtion-rs/` — full clone WITH .git at 0.2.9. 0.2.9
  changed: src/device.rs (firmware_init_reset step 7, Drop impl, OFF-first
  in all three starts, configure_color_via, diagnostic stream-on error),
  src/protocol/mod.rs (ImageCropMode = 50), src/protocol/algorithm.rs
  (read_device_frequency), tests/hardware.rs (color diagnostics + probe
  upgrade), README.md, docs/PROTOCOL.md, docs/FEATURE_PLAN.md, Cargo.toml
  0.2.9.
- Firmware RE artifacts: /home/z/my-project/xtion-rs/firmware/5.8.22/
  (the .fla blobs). RE scripts: /home/z/my-project/scripts/ (fw_disasm.py,
  fw_scan_cmp.py, fw_at.py, fw_param_table.py, fw_presets.py — the latter
  two written this turn; presets/param tables are NOT in the main image).
  The Ghidra decompilation dump was recycled again this turn (only
  firmware_dl/openni_ref survived); rebuilding it requires the 446 MB
  Ghidra download + JDK 21 again.
- PDF pipeline (for future PDF updates): skills/pdf (report route) —
  cover via scripts/fw_cover.html -> html2poster.js, body via
  scripts/fw_report_body.py (TocDocTemplate + multiBuild, reportlab
  4.4.9: TableOfContents from reportlab.platypus.tableofcontents,
  TableStyle from reportlab.platypus.tables), merged via pypdf.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo PATH=/home/z/my-project/.cargo/
  bin:$PATH`).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **Stream-mode params are a state machine**: mode changes must go
  through OFF (host guard XnSensorFirmwareParams::SetStreamMode; the
  firmware NACKs BadParams on violations). Every C session ends with
  streams OFF (reset path + shutdown). The init soft reset does NOT
  clear stream modes.
- **STREAM0_MODE (param 5) values are OFF/COLOR/DEPTH/IR = 0/1/2/3**,
  NOT the image format; image format/res/fps are params 12/13/14.
- **The image-open sequence in the C**: format->res->fps->flicker(17)->
  mirror(71)->blanking(0x06)->FREQUENCY(0x80, res0/fps0)->stream0=COLOR->
  crop(50)->AE(80,1)->AWB(77,1); AE/AWB defaults TRUE on fw >= 5.8.9.
- **bImageSupported** = fixed params nImageCmosType != 0 (0 = no color
  sensor; the C never creates the image stream then).
- **The wire fixed-params layout is +1 word vs the C's in-memory struct**
  (extra head word); our offsets (serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108) are hardware-verified.
- **Property SetAlwaysSet(TRUE)**: the C re-writes firmware params even
  when the cached value matches — skip-if-unchanged is NOT C behavior.
- **SET_PARAM retry policy**: 5 retries on timeout only; BadParams is
  final. Per-param recv timeouts are 1 ms on Jango cores.
- **The firmware preset/param tables are NOT in the main image** (they
  come from the flash param images).
- The C's soft-reset delay is 1 ms, not 800 ms (Jango cores of
  fw >= 5.3.16); the keep-alive retry loops are the real liveness gate.
- The preset table is a capability list across BOTH USB interfaces; on
  bulk endpoints the vendor stack uses PS-COMPRESSED (format 1).
- GET_LOG entries: {u16 entry_size = PAYLOAD words, u16 log_type, u32
  timestamp} + payload; text type 0x1200, overflow 0x1201. I2C bytes
  travel as u16 words; CMOS register value = (w0 << 8) + w1. READ_FLASH
  takes a BYTE offset; DOWNLOAD_FILE takes WORDS. XnFlashFile = 24 bytes
  packed; flash-map entry = 14 bytes.
- BIST masks (PS1080.h:373-379): IMAGE_CMOS 1, IR_CMOS 2, POTENTIOMETER
  4, FLASH 8, FULL_FLASH 16, PROJECTOR 32, TEC 64; reply = u32 failure
  bitmask; 300 s timeout.
- rustup minimal profile lacks clippy/rustfmt — add them after install.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker, 0 = invalid.
- A frame whose assembled size != expected must be dropped (partial
  first frame after stream start).
- buf_size in the 12-byte stream packet header is BIG-ENDIAN.
- Zero-payload packets must be dispatched immediately at chunk
  boundaries.
- PS1080 stream needs HIGH speed USB (~12.7 MB/s depth packed-11).
- Sync bulk reads are not enough: always keep the async 16-URB ring.
- rusb 0.9 has no async API; use libusb1-sys with raw pointers.
- Bulk-IN read lengths must be a multiple of wMaxPacketSize (EOVERFLOW).
- Device replies can be SPLIT across multiple control-IN transfers;
  XnVersions wire quirks; v2.6 replies have NO CRC; new request id per
  exchange; NOT_READY retries keep the id.
- Xtion Pro Live: fw 5.8.22, chip 0x00212020 (PS1080-A6), FPGA 0x0005
  (RD1082), serial 1408090603; VGA@30 needs high-speed USB.
- GET_USB_CORE_TYPE (40) NACKed by some 5.x builds → assume Jango.
- CMOS blanking fetch (ALGORITHM_PARAMS 0x06) is REQUIRED before any
  stream produces packets (0.2.1 fix, verified).
- Opcode 42 (emitter) gated to fw >= 5.8.15, still NACKs BadParams on
  some builds — power-cycle restores the default (on).
- The CommandLink fake-transport unit tests need FULL reply packets.

## 9. Open threads (if the user continues)

- **Immediate:** on-camera run of 0.2.9 —
  `cargo test --test hardware -- color_stream_roundtrip --nocapture`.
  If it passes: run the full suite + `system_commands_readonly` and the
  color visual check. If it STILL NACKs: run
  `cargo test --test hardware -- color_start_probe --nocapture` and
  paste BOTH outputs — the probe now shows image_cmos_type (0 = no
  color sensor: hardware-level answer, the C refuses too), the boot log
  (CLR self-test lines) and per-step ACK/NACK with readbacks.
- **Next feature (Phase 3):** firmware cropping (PARAMs 46-60, "bigger
  crop first, then shrink", SizeX -> SizeY -> OffsetX -> OffsetY -> Mode
  last) per the turn-17 spec; then registration end-to-end (algorithm
  0x02 tables) — the highest-value item for the photogrammetry use case.
- Phase 4 backlog: AGC bins (63-70) + wavelength (73/74, algorithm
  0x80), GMC (0xa200 + opcode 26), audio (large), remaining stream modes.
- A `color_dump` example does not exist yet (nice-to-have alongside the
  color validation).
- Golden vectors: `cargo test --test golden`.
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token).
