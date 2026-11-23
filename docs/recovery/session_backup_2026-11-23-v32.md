# Chat Session Backup — ASUS Xtion Linux research

**Version:** 32 (2026-11-23, turn 32): **0.3.3 — firmware crop windows
implemented + pushed 827c3db; 0.3.2 camera run recorded (22-record
flash table, 20/20 on-device CRCs).** The turn started from the 0.3.2
on-camera run (9/9 green, full output pasted): every flash finding was
confirmed on hardware — the packed 22-byte table parse (22 records,
ids 0-21), the word-addressed READ_FLASH chunking fix (the 384 KiB
ReferenceVGA payload verified through it), the CRC-16/CCITT-FALSE
per-file CRC (20/20 files OK on device; only the FileTable self-entry
mismatches its own crc 0xfffc — placeholder), and the DOWNLOAD_FILE
type-lookup corruption (0x12 → RegistrationParams 548 B, 0x13 →
ProductionFile 4096 B, 0x0A/0x0B correct). The region past id 13 had
never been seen: ids 14-21 are the per-device sensor block (IDParams,
SensorTECParams, SensorNESAParams, SensorProjectorFaultParams,
RegistrationParams, **ProductionFile** (0x18, 2045 w), ReferenceQVGA,
ReferenceVGA). This turn then implemented the next step — **cropping
(params 46-60)** — pinned from the OpenNI source: write sequence
SizeX→SizeY→OffsetX→OffsetY→Mode-last (disable = mode-only, plain
allow-while-open params, NO "bigger crop first" — that was an OpenNI
1.x-ism), and the wire fact that while crop mode != 0 the firmware
transmits ONLY the window (depth `crop_w*crop_h` pixels; IR `crop_w*
crop_h` packed-10 with NO extra CMOS rows — the MT9M001 height+8 quirk
is full-frame only). New API `Crop` + `set_depth_crop/set_color_crop/
set_ir_crop` (increased_fps = mode 2, fw >= 5.7.2); frames carry
`crop: Option<Crop>` + cropped dims; `to_world` shifts by the origin so
world coordinates stay full-frame correct. Flash type names now cover
the complete `XnFlashFileType` enum. Hardware test 10
(`depth_and_ir_crop_windows`) added: mode-1 crop, on-the-fly shrink,
disable, geometry, IR crop, mode-2 fps probe. 131 lib + 4 golden + 1
doc tests green, clippy 0, fmt clean. **State: waiting for the
on-camera rerun — expected 10/10; paste back test 10's output (crop
dims, valid-pixel counts, the mode-2 measured fps and whether
DepthFps=60 was accepted).**

> Purpose: upload this file in a new chat to restore full context. It records
> the standing rules, the task, everything that was found and decided, every
> file that was produced, and the state of the current workspace.

## 1. Standing user rules (apply to EVERY future turn)

1. **Communicate in English only** (all replies, all document content).
2. **Zip backup of generated files to `download/`** every turn.
3. **Do NOT run full cargo builds of the complete project — the sandbox does
   not have enough RAM.** Use targeted invocations: `cargo test --lib -j 2`,
   `cargo test --test golden -j 2`, `cargo clippy --lib -j 2`,
   `cargo test --test hardware -j 2` (or `--no-run`); never `cargo build`/
   all-target builds, no release builds. Also avoid `rg` without a path
   filter or `grep`-over-tree commands — the sandbox OOM-kills them; use
   targeted patterns or the Grep tool.
4. **Produce a downloadable session backup** (this file, refreshed with a new
   version/date each turn) so the chat can be continued/resumed later.
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER printed or
   committed). Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat /home/z/my-project/.gh-token)"
   > /tmp/gh-creds` then
   `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.
   NOTE: the user has now pasted three PATs in plaintext across turns (all
   stored, latest wins); remind them to rotate.
   **Fetch/rebase before pushing** — the user sometimes pushes their own
   commits.
6. **Ignore the `research/idong100*` branches** on the remote:
   `research/idong100`, `-01-chinese-web`, `-02-products-software`,
   `-09-xeyez-analysis`, `-10-rust-driver-draft` — a multi-agent research
   fork. All xtion-rs work goes to `main` only.
7. **Cargo.lock is gitignored** (library crate) — do not `git add` it.

## 2. Task history (condensed)

- **Turns 1-13 (2026-11-19..22):** research → test scripts → pure-Rust
  driver (Option D, turn 7) → five on-hardware bugs fixed (0.1.1-0.1.5) →
  depth hardware-VALIDATED (87.4% valid, 12.17 MiB/s, 29.97 fps).
- **Turn 15:** firmware decompiled (Ghidra); ASUS fw 5.8.22 mirrored.
- **Turns 16-20 (0.2.0-0.2.3):** identity + IR + events; Jango fallback;
  CMOS blanking; IR wire size (MT9M001 height+8).
- **Turns 21-28 (0.2.4-0.2.10):** color chase to its hardware root cause:
  **image_cmos_type = 0 — NO color sensor on this unit** (0.2.10 gate +
  skip). Read-only diagnostics tier (0.2.8), stream hygiene + full
  image-open C parity (0.2.9).
- **Turn 29 (0.3.0):** photogrammetry milestone — geometry
  (`depth_intrinsics`, `to_world`, `point_cloud`, PLY example),
  wrap-aware host timestamps (`timestamp_us`), diagnostics-parse fixes.
  Pushed 0ecdc56.
- **Turn 30 (0.3.1):** scene-robust plane-fit gate (RANSAC dominant
  plane + PCA refit + conditional 15 mm RMS gate) + `plane_fit_synthetic`
  regression test. Pushed a717d85. The 0.3.1 camera run: 9/9 green
  (dominant plane 92% coverage, 3.04 mm inlier RMS).
- **Turn 31 (0.3.2):** flash story closed offline from the 0.3.1 dump:
  `XnFlashFile` pinned (packed 22 B), word addressing, the fw
  packed/unpacked table-walk bug, CRC-16/CCITT-FALSE per-file CRC (8/8
  vs factory image), read_flash chunking fix. Pushed cf56f7a.
- **Turn 32 (this turn, 0.3.3):** the 0.3.2 camera run CONFIRMED
  everything (22-record table, 20/20 CRCs on device, DOWNLOAD_FILE
  mapping, 9/9 green) and revealed the per-device file block (ids
  14-21, incl. the ProductionFile). Cropping implemented (params
  46-60, three streams, mode 2), flash type names completed, 0.3.2
  ground truth recorded in docs, hardware test 10 added. Pushed 827c3db.

## 3. Turn-32 findings (all source- or data-verified this turn)

- **The 0.3.2 camera run (pasted in full, 9/9 green) confirmed:**
  on-flash table = 22 records (ids 0-21); record 0 = the FileTable
  self-entry (magic 0x12345678, word 0x1000, 0x1000 w, crc 0xfffc —
  which does NOT verify against its own 8 KiB: placeholder, the one
  expected MISMATCH); every other file with a stored crc verified ON
  DEVICE through the fixed chunked read (incl. ReferenceVGA 393,216 B —
  proving the word-offset chunk loop at 384 KiB scale); ScratchFile has
  no crc. The emitter toggle ACKed while depth streams (idle-NACK
  hypothesis stays confirmed). I2C depth reg 0 = 33841 (0x8431, the
  MT9M001 part id — nice cross-check).
- **New flash ground truth (ids 14-21, never seen before):** id 14
  IDParams (0x14, 148 w), 15 SensorTECParams (0x15, 4 w), 16
  SensorNESAParams (0x1C, 36 w), 17 SensorProjectorFaultParams (0x17,
  26 w), 18 RegistrationParams (0x13, 271 w = the 548-B "GainControl"
  download), 19 **ProductionFile** (0x18, 2045 w = the 4096-B type-0x13
  download), 20 ReferenceQVGA (49,152 w @ word 0xA8000 = byte 0x150000),
  21 ReferenceVGA (196,608 w @ word 0x78000 = byte 0xF0000, ending
  exactly where ReferenceQVGA begins — contiguous). Full flash map now
  complete; every type name comes from `XnFlashFileType`
  (PS1080.h:335-368, incl. 0x16 SensorAPCParams, 0x19 UpgradeInProgress,
  0x1A WavelengthCorrection, 0x1B GMCReferenceOffset, 0x1D SensorFault,
  0x1E VendorData).
- **The crop contract (pinned from OpenNI 2.2.0 source, not assumed):**
  params per stream — image 46-50, depth 51-55, IR 56-60 (SIZE_X,
  SIZE_Y, OFFSET_X, OFFSET_Y, MODE; XnParams.h). Firmware modes
  (`XnFirmwareCroppingMode`, PS1080.h:308-313): 0 disabled, 1 normal,
  2 increased-fps (fw >= 5.7.2 via `bIncreasedFpsCropSupported`,
  XnHostProtocol.cpp:640-643; 5.8.22 has it). Enable sequence
  (`SetCroppingImpl`, XnSensorDepthStream.cpp:768-840, image 541-612,
  IR 304-380 — identical): SizeX, SizeY, OffsetX, OffsetY, then Mode
  LAST; disable = Mode alone; on NACK part-way the old window stays
  active (mode-last ordering is the rollback safety). At every stream
  open `OpenStreamImpl` re-applies geometry (when active) + ALWAYS
  writes the mode (depth 412-425, image 359-372, IR 190-203 — AFTER the
  stream-on write). Validation = `XnPixelStream::ValidateCropping`
  (non-zero, x+w<=XRes, y+h<=YRes) — no evenness constraint anywhere.
  **NO "bigger crop first" protocol exists in OpenNI 2.2.0** (that note
  was wrong / an OpenNI 1.x-ism): crop params are plain
  allow-while-open SET_PARAMs (MapFirmwareProperty bAllowWhileOpen=TRUE).
- **The crop wire fact (the load-bearing discovery):** while crop mode
  != 0 the firmware transmits ONLY the window —
  `XnDepthProcessor::CalculateExpectedSize` (XnDepthProcessor.cpp:
  112-125) sizes the buffer to crop_w*crop_h; frames report width/
  height = crop size + cropOrigin (lines 157-163).
  `XnSensorImageStream::CalculateExpectedSize` (786-817) same.
  **IR special case (XnIRProcessor.cpp:244-282): when cropped,
  `actualHeight = height` — the MT9M001 `height + 8` extra rows apply
  to FULL frames only.** Cropped IR wire size = crop_w*crop_h*10/8.
- **IR mirror + crop interaction (ported as a documented note):** the
  C mirrors IR host-side AFTER cropping ("which is bad" — its comment),
  so it flips the crop offset X for mirrored IR:
  `nXOffset = XRes - originX - width` (XnSensorIRStream.cpp:324-330).
  Our driver forces the firmware IR mirror OFF, so crop coordinates
  pass through unflipped. Depth/color mirror in firmware — no
  interaction with crop coordinates.
- **Crop + packed wire formats:** the firmware packs the readout as a
  flat bitstream — packed-11 depth consumes pixels in groups of 8 per
  11 B (crop pixel count must be % 8 == 0), packed-10 IR in groups of 4
  per 5 B (% 4 == 0), 16-bit formats unconstrained, YUV422 byte-per-
  component unconstrained. These divisibility rules are OUR addition
  (the C never states them; its processors unpack flat), documented as
  such.

## 4. 0.3.3 changes (commit 827c3db, pushed to main)

- `src/protocol/mod.rs`: Param enum gains the 15 crop ids (46-60; 50
  already existed); NEW constants `CROP_MODE_DISABLED/NORMAL/
  INCREASED_FPS` (0/1/2) with the wire-effect documentation.
- `src/device.rs`:
  - NEW `pub struct Crop { x, y, width, height }` + private
    `validate(res_w, res_h, group)` (C parity + pixel-group rule);
    private `CropState { crop, mode }` + `CropParams` (the five Param
    ids per stream) with IMAGE/DEPTH/IR consts.
  - NEW `write_crop_via` free function (transport-generic, the
    SetCroppingImpl port); `configure_color_via`/`configure_ir_via`
    gain a crop-state parameter and write the crop after the
    stream-on write (OpenStreamImpl tail; color previously hardcoded
    mode 0 — now goes through the same path).
  - NEW public `set_depth_crop` / `set_color_crop` / `set_ir_crop`
    (`Option<Crop>`, `increased_fps: bool`) — validate against the
    ACTIVE stream config (stream must be running), write immediately
    (on-the-fly capable), keep state; + private `crop_state` helper.
  - `start_depth` now writes the crop params after Stream1Mode=DEPTH
    (always the mode; geometry when active) — new C parity.
  - `read_depth_frame`/`read_color_frame`/`read_ir_frame`: crop-aware
    dimensions + expected sizes; IR uses the new
    `decode_ir_frame_cropped` when cropped.
  - `DepthFrame`/`ColorFrame` gain `pub crop: Option<Crop>`;
    `DepthFrame::to_world` shifts by the crop origin (world
    coordinates stay full-frame); `point_cloud` inherits it.
  - Tests: crop write sequence (enable 5 writes exact order + disable
    1 write), crop validation (zero/oob/divisibility + full-frame ok),
    cropped to_world shift, param-id guard extended to all 15 crop
    ids; configure_ir/color sequence tests updated for the new
    always-written crop mode.
- `src/ir.rs`: `IrFrame` gains `crop`; NEW
  `decode_ir_frame_cropped(data, crop, ts)` (exact crop_w*crop_h*10/8
  wire size, no extra rows) + test (incl. rejecting a full 248-row
  frame as the cropped size).
- `src/protocol/system.rs`: `flash_file_type_name` covers the complete
  `XnFlashFileType` enum 0x00-0x1E + test.
- `tests/hardware.rs`: NEW test 10 `depth_and_ir_crop_windows` —
  baseline → 320x240 centered mode-1 crop (dims + crop field + valid
  ratio + min-depth plausibility) → to_world centre check → on-the-fly
  160x120 → disable → IR crop 320x240 (exact size gate) + disable →
  mode-2 probe (crop mode 2, DepthFps=60 attempted with both outcomes
  printed, 60 frames timed, fps printed, NOT gated) → cleanup.
- Docs: PROTOCOL.md crop section (params/modes/sequence/wire effect/
  IR no-extra-rows/mirror note) + flash section camera-validated
  ground truth; FEATURE_PLAN.md item 9 CLOSED (with the corrected
  no-bigger-crop-first note); VALIDATION.md test 7 camera-validated +
  test 10 row; README status row + 0.3.3 changelog + 131 test count.
  Cargo 0.3.3.

## 5. Hardware test inventory (TEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green.
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic.
7. `system_commands_readonly` — **camera-validated 0.3.2** (22-record
   table, 20/20 CRCs on device, DOWNLOAD_FILE mapping).
8. `depth_geometry_fov_and_timestamps` — camera-validated 0.3.1
   (RANSAC plane gate; 92% coverage, 3.04 mm RMS on the user's room).
9. `plane_fit_synthetic` — camera-independent RANSAC regression.
10. `depth_and_ir_crop_windows` — NEW 0.3.3, awaiting camera run
    (mode-1 crop + on-the-fly + disable + geometry + IR crop + mode-2
    fps probe).

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware crop -- --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v32.md` | This file (v32, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v32.zip` | Turn-32 zip (repo with .git at 0.3.3 + this backup). |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (historical). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (historical, 0.1.5). |
| `xtion_rs_next_steps_body.pdf` / `xtion_rs_next_steps_and_validation.pdf` | Turn-20 gap analysis (crop was its recommendation — now implemented). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 827c3db (0.3.3)**; also committed at
docs/recovery/session_backup_2026-11-23-v32.md). If a future turn finds
the tree missing, re-clone with the stored token or restore this zip; the
remote also carries the `research/idong100*` branches to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 32)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.3, clean, pushed.
- OpenNI 2.2.0 reference: ALREADY EXTRACTED at
  `/home/z/my-project/firmware_dl/openni_ref/OpenNI2-2.2.0-debian/`
  (Include/PS1080.h = flash structs + crop modes + XnFlashFileType;
  Sensor/*.cpp = crop sequences/processors; XnPixelStream.cpp =
  ValidateCropping). NOTE: some files have mangled identifier names
  (token-replacement corruption) — verify names, trust structure.
- Firmware blobs: `xtion-rs/firmware/5.8.22/*.fla` (16 files) +
  `UsbUpdate_RD108.ini` + `FWUpdate_5_8_22.zip`.
- Sandbox scripts (survive under /home/z/my-project/scripts):
  `flash_crc_sweep.py`, `flash_fla_inventory.py`, `fw_*.py`.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`). Verified working.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **Crop wire semantics:** while crop mode != 0 the firmware transmits
  ONLY the window (any mode, not just increased-fps). Depth expected
  size = crop_w*crop_h pixels; IR = crop_w*crop_h*10/8 bytes with NO
  +8 rows (MT9M001 quirk is full-frame only); image = crop_w*crop_h*bpp.
- **Crop write order matters for safety:** geometry first, MODE LAST;
  disable writes only the mode. A NACK part-way leaves the old window
  active. At stream open, always write the mode (kills stale fw state).
- **No "bigger crop first" in OpenNI 2.2.0** — crop params are
  allow-while-open; the in-flight frame when the window changes is
  dropped as corrupt. (The FEATURE_PLAN note that claimed otherwise was
  wrong; corrected in 0.3.3.)
- **Crop divisibility (our rule, not the C's):** packed-11 depth →
  crop pixel count % 8 == 0; packed-10 IR → % 4 == 0; 16-bit/YUV
  unconstrained.
- **Mirrored IR flips crop offset X** in the C (host-side mirror after
  crop): `nXOffset = XRes - originX - width`. We keep fw IR mirror off,
  so unflipped. Depth/color mirror in firmware, no crop interaction.
- **FileTable record 0's crc 0xfffc does not self-verify** — expected
  MISMATCH; every other stored crc verified on device (0.3.2 run).
- **DOWNLOAD_FILE on fw 5.8.22 serves wrong files for some types**
  (0x12→RegistrationParams, 0x13→ProductionFile; 0x0A/0x0B correct) —
  same packed/unpacked table-walk bug as GET_FILE_LIST. Ground truth =
  READ_FLASH + parse.
- All flash addresses are 16-bit WORDS (byte = ×2): READ_FLASH offsets
  AND chunk advances, DOWNLOAD_FILE offsets, XnFlashFile fields, the
  updater ini.
- `XnFlashFile` is 22 bytes PACKED, 24 unpacked — fw walks the on-flash
  table with the unpacked stride (GET_FILE_LIST rows drift +2 B/record;
  every 11th row real, lcm(22,24)=264).
- Flash CRC = CRC-16/CCITT-FALSE over the payload words fed big-endian
  (per-word MSB first); the 6-B `.fla` header is NOT included.
- DOWNLOAD_FILE replies = the file's `.fla` image ([type u16][build
  u16][version u16] + payload words).
- The fw's file-table self-entry (record 0) carries magic 0x12345678
  in `version`; regular files carry 0x05080022.
- Plane-fit gate design (0.3.1): RANSAC inlier RMS is bounded by the
  inlier band BY CONSTRUCTION — coverage is the discriminator; the
  15 mm RMS gate applies once coverage >= 40%.
- Emitter opcode 42: BadParams while IDLE, ACK while depth streams.
- fw 5.8.22 does not log NACKs — GET_LOG returns boot history only.
- XnLogEntryHeader field order: {u16 size, u32 ts, u16 type}; the
  structured-entry param is a u32.
- fDeviceFrequency is ticks per MICROSECOND (~60.0); wrap = 71.58 s.
- Wire fixed-params layout is +1 word vs the C struct; offsets
  hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- SET_PARAM retries 5x on timeout only, BadParams is final.
- Stream-mode params are a state machine: changes go through OFF;
  every C session ends with all streams OFF; the init soft reset does
  NOT clear stream modes.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker; partial first
  frames dropped; buf_size big-endian; high-speed USB required; split
  replies; v2.6 replies have no CRC; one garbage chunk per endpoint on
  fw < 5.0; bulk reads multiple of wMaxPacketSize.
- Git: fetch + rebase before push; Cargo.lock gitignored.

## 9. Open threads (if the user continues)

- **Immediate:** rerun the suite on 0.3.3:
  `cargo test --test hardware -- --nocapture` — expected **10/10**.
  PASTE BACK test 10's full output: (a) the cropped-frame sections
  (dims 320x240 / 160x120, crop fields, valid-pixel counts vs
  baseline, min depth), (b) the IR crop section (whether the exact
  96,000-byte gate passed — this CONFIRMS the no-extra-rows-while-
  cropped rule on silicon), (c) the mode-2 probe (was DepthFps=60
  accepted with crop mode 2? what fps was measured?) — that decides
  whether 0.3.4 adds a documented high-fps recipe or a mode-2-only
  note.
- **If the crop gate drops frames forever** (wrong-size warnings
  repeating), the firmware's cropped wire framing differs from the C
  model — paste the drop messages (they print actual vs expected
  sizes) and the read path gets adapted with real numbers.
- **Then (Phase 3 remainder):** depth AGC bins (63-70) + wavelength
  correction (73/74), depth write-read sweep, QVGA@60 mode matrix
  (now trivially combinable with crop), registration end-to-end (RGB
  units only).
- **Crates.io 0.3.3** is the natural release point (flash ground truth
  + crop + geometry + timestamps); pre-release polish: README example
  for crop + point_cloud, docs.rs check.
- Phase 4 backlog: AGC bins 63-70, wavelength 73/74, GMC 26/0xa200,
  audio, COMPRESSED_PS depth (SXGA@30 needs it), M6 robustness
  (keep-alive thread, hotplug, soak).
- Golden vectors: `cargo test --test golden` (flash table + CRC
  vectors included).
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token); fetch + rebase first (rule 5).
