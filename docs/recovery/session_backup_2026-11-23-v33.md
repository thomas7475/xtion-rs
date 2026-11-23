# Chat Session Backup — ASUS Xtion Linux research

**Version:** 33 (2026-11-23, turn 33): **0.3.4 — depth processing controls
(gain 21, AGC bins 63-70, wavelength 73/74, close range 84) + the C-exact
crop batch write + the mode-2 probe rework; pushed c1a0e3b; Driver
Architecture PDF regenerated as Rev 2 (10 pages).** The turn started from
the 0.3.3 camera run (10 tests, 9 green; test 10 panicked at
`set_depth_crop(Some, true)` → "crop mode 2 (increased fps):
Nack(BadParams)" while mode 1 worked everywhere). Root-cause research in
the OpenNI 2.2.0 source found the real crop wire form: SetCroppingImpl
wraps its five writes in a firmware transaction and commits them with
`CommitTransactionAsBatch` → `XnHostProtocolSetMultipleParams`
(XnHostProtocol.cpp:2830-2860) — ONE opcode-3 request carrying N
{u16 param, u16 value} pairs (`XnInnerParamData`, PS1080.h:460-466). The
0.3.3 driver's five single writes were a deviation that the fw happened
to accept for mode 1. 0.3.4 sends the batch (disable = one-pair batch =
wire-identical to the old single write) and ports the C's NACK rollback
(`UpdateFromFirmware` re-read of the five crop params). AGC findings:
param 21 is the "Gain" property (`m_DepthGain` ← PARAM_DEPTH_AGC — the
name lies; plain integration value, default 50; the pre-5.6 close-range
recipe steps it 1/42 with AHB pokes), bins = params 63-70 as
(LOW=MinShift, HIGH=MaxShift) per bin, written as TWO individual
SET_PARAMs (`XnHostProtocolSetDepthAGCBin`); OpenNI's public bin API is
millimetre-based (translated via the depth↔shift tables) — our API keeps
the wire's raw shift units. Wavelength 73 = correction enable (fw ≥ 5.2),
74 = debug telemetry switch (streams `XnWavelengthCorrectionDebugPacket`:
TEC set-point steps, hop counts). Close range 84 (fw ≥ 5.6) is a single
param on this fw; the pre-5.6 AHB recipe is not ported. All four groups
are allow-while-open (`MapFirmwareProperty(..., TRUE)`,
XnSensorDepthStream.cpp:280-314). The misleading `set_depth_agc(bool)`
0.2.0 one-liner was REMOVED (it wrote 0/1 into the gain). Hardware test
10's mode-2 section is now a three-form probe (single write while
streaming / C-exact batch / stream-stopped batch) that never gates on a
rejection and re-verifies the mode-1 window afterwards; new hardware
test 11 (`depth_processing_controls`) runs gain/bins/close-range/
wavelength with read-factory → write → read-back → RESTORE → assert
discipline. FEATURE_PLAN gained section 1b: the difficulty-rated
remaining-surface table (also reproduced in the PDF). 135 lib + 4 golden
+ 1 doc tests, clippy 0, fmt clean.

> Purpose: upload this file in a new chat to restore full context. It records
> the standing rules, the task, everything that was found and decided, every
> file that was produced, and the state of the current workspace.

## 1. Standing user rules (apply to EVERY future turn)

1. **Communicate in English only** (all replies, all document content).
2. **Zip backup of generated files to `download/`** every turn.
3. **Do NOT run full cargo builds of the complete project — the sandbox does
   not have enough RAM.** Use targeted invocations: `cargo test --lib -j 2`,
   `cargo test --test golden -j 2`, `cargo clippy --lib --tests -j 2`,
   `cargo test --test hardware -j 2 --no-run` (compile check only); never
   `cargo build`/all-target builds, no release builds. Also avoid `rg`
   without a path filter — use targeted patterns or the Grep tool.
4. **Produce a downloadable session backup** (this file, refreshed with a new
   version/date each turn) so the chat can be continued/resumed later.
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER printed or
   committed). Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat /home/z/my-project/.gh-token)"
   > /tmp/gh-creds` then
   `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.
   NOTE: the user has now pasted FOUR PATs in plaintext across turns (latest
   wins); remind them to rotate.
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
  **image_cmos_type = 0 — NO color sensor on this unit**. Read-only
  diagnostics tier (0.2.8), stream hygiene + image-open C parity (0.2.9).
- **Turn 29 (0.3.0):** photogrammetry milestone — intrinsics, to_world,
  point_cloud, PLY, wrap-aware timestamps. Pushed 0ecdc56.
- **Turn 30 (0.3.1):** RANSAC dominant-plane gate. Pushed a717d85.
  Camera run 9/9 (92% coverage, 3.04 mm RMS).
- **Turn 31 (0.3.2):** flash story closed (packed 22 B records, word
  addressing, CRC-16/CCITT-FALSE). Pushed cf56f7a.
- **Turn 32 (0.3.3):** crop windows (46-60) + full flash type names;
  camera run 9/10 — mode-2 crop NACK. Pushed 827c3db.
- **Turn 33 (this turn, 0.3.4):** the mode-2 diagnosis found the batch
  write form; implemented the batch + rollback, the four depth-processing
  param groups, the probe rework, test 11, the difficulty table, and the
  Driver Architecture PDF Rev 2. Pushed c1a0e3b.

## 3. Turn-33 findings (all source-verified this turn)

- **The crop write is a BATCH, not five singles:** `SetCroppingImpl` →
  `StartFirmwareTransaction` → five `SimpleSetFirmwareParam` →
  `CommitTransactionAsBatch` → `XnHostProtocolSetMultipleParams`: ONE
  opcode-3 request whose payload is N `{u16 param, u16 value}` pairs
  (XnHostProtocol.cpp:2830-2860; pair struct `XnInnerParamData`,
  PS1080.h:460-466). A one-pair batch is wire-identical to a single
  SET_PARAM. 0.3.3's five singles were accepted for mode 1 on this fw —
  the batch is the reference form and 0.3.4 uses it for ALL crop writes
  (enable = 5 pairs, mode LAST; disable = 1 pair).
- **On crop NACK the C re-reads the five params** (`UpdateFromFirmware`,
  XnSensorDepthStream.cpp:807-829) to re-sync host state; 0.3.4 ports
  this: `write_crop_via` takes `&mut Option<CropState>` and rewrites it
  from GET_PARAMs on failure, so the driver never keeps a window the fw
  does not have.
- **Param 21 = "Gain"** (`m_DepthGain` ← `PARAM_DEPTH_AGC`,
  XnSensorFirmwareParams.cpp:152): a plain integration-strength value
  (factory default 50, `XN_DEPTH_STREAM_DEFAULT_GAIN_OLD`), NOT a
  boolean. The pre-5.6 `CloseRangeControl` recipe steps gain 1 (on) /
  42 (off) plus AHB pokes 0x2a0038d4/0x2a003820 (XnSensorDepthStream.cpp:
  842-873) — on fw >= 5.6 close range is just param 84 (fw-internal).
- **AGC bins = params 63-70**, per bin (LOW=MinShift, HIGH=MaxShift);
  `XnHostProtocolSetDepthAGCBin` writes TWO individual SET_PARAMs (low
  first — NO batch on this path). OpenNI's public `XnDepthAGCBin
  {nBin, nMin, nMax}` is mm-based and translates through the per-device
  depth↔shift tables (XnSensorDepthStream.cpp:722-766); our API carries
  raw shifts (0..=2047, `XN_SHIFTS_MAX_SHIFT`).
- **Wavelength correction:** param 73 enable (fw ≥ 5.2), param 74 =
  DEBUG switch that makes the fw emit wavelength-lock telemetry packets
  (`XnWavelengthCorrectionDebugPacket`: B coefficients, hop counts,
  sliding window, TEC set-point steps — the C dumps them to CSV). Not a
  depth-affecting setting.
- **Allow-while-open:** all depth-processing params map with TRUE
  (`MapFirmwareProperty`, XnSensorDepthStream.cpp:280-314) — gain 21,
  hole filter 22, crop 46-60, GMC 36, close range 84, wavelength 73/74.
- **`bIncreasedFpsCropSupported` is version-only** (fw >= 5.7.2,
  XnHostProtocol.cpp:640-643) — OpenNI would write mode 2 on this
  device with no further preconditions and would presumably get the same
  BadParams NACK. `GetFirmwareCroppingMode` maps INCREASED_FPS→2 only
  when supported (XnSensorStreamHelper.cpp:403-416). The 5.8.22 ASUS
  build rejects mode 2 (single write, streaming) — batch and
  stream-stopped probes pending the camera rerun.
- **Firmware disassembly status:** the dispatcher region
  0x21400-0x22000 resists clean linear decode (data interleaved with
  the compare chain); a triplet scan for an ascending param-validation
  table found only ASCII hex-digit tables. The turn-15 Ghidra decompiled
  dump (881 KB) did NOT survive the sandbox recycle — only the analysis
  PDF (xtion_rs_firmware_analysis.pdf), the .fla mirror, and the
  scripts/fw_*.py tooling remain. New tool this turn:
  `scripts/fw_crop_mode_scan.py` (crop-param reference scan).
- **cargo 1.100 quirk (sandbox):** after a recycle, `cargo test` may fail
  with "failed to find <crate> in path source" even though the registry
  cache looks populated — fix: `rm -rf ~/.cargo/registry && cargo fetch`
  (network to crates.io works).

## 4. 0.3.4 changes (commit c1a0e3b, pushed to main)

- `src/protocol/mod.rs`: Param gains `DepthAgcBin0Low..DepthAgcBin3High`
  (63-70, doc note: wire units are raw shifts), `WavelengthCorrectionEnabled`
  (73), `WavelengthCorrectionDebugEnabled` (74); NEW `MAX_SHIFT = 2047`;
  CROP_MODE_INCREASED_FPS doc carries the hardware verdict.
- `src/device.rs`:
  - NEW `set_params_via` (multi-pair opcode-3 batch, transport-generic)
    + `get_param_via`; `set_param_via` now delegates to the one-pair batch.
  - `write_crop_via` rewritten: ONE batch request (5 pairs enable / 1 pair
    disable), on NACK the C-parity rollback re-read (`read_crop_via`);
    signature takes `&mut Option<CropState>`; all callers updated
    (`set_*_crop`, `start_depth`, `configure_color_via`,
    `configure_ir_via` — the latter two now take `&mut Option<CropState>`).
  - NEW public depth-processing API: `depth_gain`/`set_depth_gain` (21),
    `AgcBin {min_shift, max_shift}` + `depth_agc_bin`/`set_depth_agc_bin`
    (63-70, low-then-high two singles, validation: ordered, ≤ 2047),
    `close_range`/`set_close_range` (84), `wavelength_correction`(+`_debug`)
    and setters (73/74).
  - NEW `set_params` public batch method. `Xtion::get_param` refactored
    onto `get_param_via`.
  - REMOVED `set_depth_agc(bool)` (wrote 0/1 into the gain — a bug);
    the old `set_close_range` one-liner replaced by the documented pair.
  - Tests: crop write is ONE 5-pair batch (mode last); disable = 1 pair;
    NEW `crop_mode2_nack_resyncs_state` (the 0.3.3 regression: batch
    NACK → error surfaces → state re-synced from GETs);
    `agc_bin_params_and_validation`; `agc_bin_write_is_two_separate_
    param_writes`; `depth_processing_switch_wire_ids`; param-id guard
    extended to 63-70/73/74. FakeTransport helpers: `getparam_reply`,
    `nack_reply`.
- `src/lib.rs`: re-exports `AgcBin` + `CROP_MODE_*` + `MAX_SHIFT`.
- `tests/hardware.rs`: test 10 mode-2 section = three-form probe
  ((a) single write while streaming — the 0.3.3 regression check,
  (b) C-exact batch while streaming, (c) batch with stream stopped +
  re-arm attempt) that NEVER gates on a rejection and re-verifies the
  mode-1 window afterwards; DepthFps=60 + 60-frame timing only when
  mode 2 actually sticks. NEW test 11 `depth_processing_controls`:
  factory inventory (gain, 4 bins as shift ranges, close/wl/wldebug) +
  structural bin sanity; gain 42 echo/restore; custom contiguous bin
  partition [0-512][512-1024][1024-1536][1536-2047] echo + frame under
  it + factory restore; close-range on/frame(printed, not gated)/
  restore; wavelength+debug echo/restore; final frame > 1000 valid.
  Every phase restores BEFORE asserting (panic-safe).
- Docs: PROTOCOL.md — param table rows 21/22/63-70/73/74/84, crop
  section rewritten (batch form + rollback + mode-2 verdict), NEW
  "Depth processing controls" section. FEATURE_PLAN.md — gap rows
  closed, item 9 crop note (batch + mode-2 verdict), item 11 IMPLEMENTED,
  NEW section 1b difficulty table. VALIDATION.md — tests 10/11 rows,
  135-test count. README — status rows, usage example, 0.3.4 changelog.
  Cargo 0.3.4.
- PDF: `download/xtion_rs_driver_overview.pdf` REGENERATED (Rev 2, 10
  pages): overview + release history, layer architecture (diagram) +
  module map, command protocol (incl. the batch), streaming (formats +
  crop + mode-2 verdict), calibration/geometry (shift-to-depth formula),
  flash, depth-processing controls, validation status, the difficulty
  table, sources. Sources: `scripts/driver_cover.html`,
  `scripts/driver_arch_diagram.html/.png`, `scripts/driver_report_body.py`,
  `scripts/driver_report_merge.py`.

## 5. Hardware test inventory (ELEVEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green.
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic.
7. `system_commands_readonly` — camera-validated 0.3.2.
8. `depth_geometry_fov_and_timestamps` — camera-validated 0.3.1.
9. `plane_fit_synthetic` — camera-independent RANSAC regression.
10. `depth_and_ir_crop_windows` — 9/10 at 0.3.3 (mode-2 NACK panic);
    probe reworked 0.3.4, awaiting camera.
11. `depth_processing_controls` — NEW 0.3.4, awaiting camera.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware crop -- --nocapture` (or
`depth_processing`).

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v33.md` | This file (v33, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v33.zip` | Turn-33 zip (repo with .git at 0.3.4 + this backup). |
| `xtion_rs_driver_overview.pdf` | Driver Architecture + Protocol Reference Rev 2 (0.3.4, 10 pages) — REPLACES the stale turn-13 overview. |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (historical). |
| `xtion_rs_next_steps_body.pdf` / `xtion_rs_next_steps_and_validation.pdf` | Turn-20 gap analysis (historical). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = c1a0e3b (0.3.4)**; also committed at
docs/recovery/session_backup_2026-11-23-v33.md). If a future turn finds
the tree missing, re-clone with the stored token or restore this zip; the
remote also carries the `research/idong100*` branches to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 33)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.4, clean, pushed.
- OpenNI 2.2.0 reference: `/home/z/my-project/firmware_dl/openni_ref/
  OpenNI2-2.2.0-debian/` (Sensor/ = XnParams.h, XnSensorDepthStream.cpp,
  XnSensorFirmwareParams.cpp, XnSensorStreamHelper.cpp, XnHostProtocol.cpp;
  Include/PS1080.h; some files have token-replacement-corrupted
  identifiers — trust structure, verify names).
- Firmware blobs: `xtion-rs/firmware/5.8.22/*.fla` (16 files) +
  `UsbUpdate_RD108.ini`. The turn-15 Ghidra dump is GONE (recycled);
  the analysis PDF + scripts/fw_*.py survive.
- Sandbox scripts: `fw_*.py` (disasm/table tooling),
  `fw_crop_mode_scan.py` (NEW), `flash_*.py`, `driver_cover.html`,
  `driver_arch_diagram.html/.png`, `driver_report_body.py`,
  `driver_report_merge.py`, `shot_diagram.js` (in
  skills/pdf/scripts/ — 2x PNG diagram screenshots).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (`export
  RUSTUP_HOME=/home/z/my-project/.rustup CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`). If cargo fails with
  "path source" errors: `rm -rf /home/z/my-project/.cargo/registry &&
  cargo fetch`.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **Crop writes are batched:** one opcode-3 request with N (param,value)
  pairs; a NACK triggers the C's re-read rollback. Mode 2 NACKs
  BadParams on this ASUS build (single write, streaming); treat BadParams
  from mode 2 as "unsupported on this unit", never retry.
- **Param 21 is a gain (default 50), not a switch**; bins 63-70 carry raw
  shift bounds; wavelength 74 is a telemetry switch, not a depth effect.
- **AGC bin writes are two separate singles** (low, high) — no batch,
  unlike crop.
- **The turn-15 Ghidra decompiled dump does not survive recycles** — if
  firmware RE is needed again, budget for re-decompilation or work from
  the PDF + capstone scripts.
- **cargo 1.100 "path source" failure after recycle:** clear
  `~/.cargo/registry` and refetch.
- Crop wire semantics: while crop mode != 0 the fw transmits ONLY the
  window; depth = crop_w*crop_h pixels (packed-11: % 8 == 0); IR =
  crop_w*crop_h*10/8 bytes, NO +8 rows (full-frame only).
- No "bigger crop first" in OpenNI 2.2.0; crop params are
  allow-while-open; the in-flight frame is dropped as corrupt.
- FileTable record 0's crc 0xfffc does not self-verify (placeholder);
  20/20 other stored CRCs verified on device.
- DOWNLOAD_FILE on fw 5.8.22 serves wrong files for some types
  (0x12→RegistrationParams, 0x13→ProductionFile) — ground truth =
  READ_FLASH + parse.
- All flash addresses are 16-bit WORDS (byte = ×2) everywhere.
- `XnFlashFile` is 22 B packed / 24 unpacked; fw walks on-flash records
  with the unpacked stride (GET_FILE_LIST drifts +2 B/record).
- Flash CRC = CRC-16/CCITT-FALSE over payload words fed big-endian.
- Plane-fit gate: coverage is the discriminator; the 15 mm RMS gate
  applies once coverage ≥ 40%.
- Emitter opcode 42: BadParams while IDLE, ACK while depth streams.
- fw 5.8.22 does not log NACKs; GET_LOG returns boot history only.
- fDeviceFrequency is ticks per MICROSECOND (~60.0); wrap = 71.58 s.
- Wire fixed-params layout is +1 word vs the C struct; offsets
  hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- SET_PARAM retries 5x on timeout only; BadParams is final.
- Stream-mode params are a state machine: changes go through OFF; the
  init soft reset does NOT clear stream modes.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker.
- Split replies; v2.6 replies have no CRC; high-speed USB required;
  bulk reads multiple of wMaxPacketSize; buf_size big-endian.
- Git: fetch + rebase before push; Cargo.lock gitignored.

## 9. Open threads (if the user continues)

- **Immediate:** rerun the suite on 0.3.4:
  `cargo test --test hardware -- --nocapture` — expected **11/11**.
  PASTE BACK: (a) test 10's mode-2 probe — the (a)/(b)/(c) lines: did
  the C-exact batch or the stream-stopped write get mode 2 in? If (c)
  ACKs but the re-arm NACKs, a "arm crop while stopped" recipe is the
  0.3.5 follow-up; (b) test 11's full output — the factory inventory
  (gain expected ≈ 50, the four bins as SHIFT ranges — first time
  anyone has read them on this unit, close/wl/wldebug expected 0), the
  custom-bin echo (if the fw NACKs the shared-boundary partition, print
  the error — the partition may need gaps), the close-range frame valid
  count, and the final recovery.
- **If the batch crop write breaks mode 1** (wrong-size warnings
  repeating): the fw's opcode-3 multi-pair handling differs from the C
  model on this build — paste the drop messages; fallback = revert to
  five singles for mode 1 and keep the batch only for the probe.
- **Then (per the difficulty table, FEATURE_PLAN 1b):** robustness tier
  M6 (keep-alive thread, hotplug, reconnection — diff 3, highest
  production value), ONI recording (3), GMC (3); registration tables
  only on RGB units; COMPRESSED_PS only when SXGA@30 matters.
- **Crates.io 0.3.4** is a natural release point (crop + depth
  processing + flash ground truth + geometry); pre-release polish:
  README example refresh, docs.rs check.
- Golden vectors: `cargo test --test golden`.
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token); fetch + rebase first (rule 5).
- **PAT rotation reminder:** four tokens have been pasted in plaintext
  across the conversation.
