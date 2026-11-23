# Chat Session Backup — ASUS Xtion Linux research

**Version:** 36 (2026-11-23, turn 36): **0.3.7 — debug telemetry
channels + the full depth mode matrix + the corrected AGC-NACK model —
pushed bd557aa.** The turn's input was the 0.3.6 camera run (11/12,
pasted in full): every test green except `depth_processing_controls`
panicking at the C-order gate — and the panic line itself was the
decisive evidence: **the invariant-VIOLATING write (bin1 LOW <- 512
while its HIGH is 0) LANDED this time (bin1 read 512..0 right after
the NACK)**, refuting turn 35's two-tier model. Reconciliation: the
0.3.5 run's final `bin1 = (0,0)` was never proof of refusal — the
0.3.5 restore's own first write for bin1 was `LOW <- 0`, which
landed despite ITS NACK (the 0.3.5 bail-out bug then skipped only the
`HIGH <- 0` no-op). Corrected model, fitting both runs with zero
contradictions: **fw 5.8.22 replies NACK BadParams for EVERY SET_PARAM
on params 63-70 — no-op, invariant-preserving, invariant-violating
alike — while applying the value anyway. The NACK reply carries NO
information about application; only the readback is truth.** The
0.3.6 readback-verified write API was already the right design and
stays unchanged; test 11's gates were rewritten to the corrected model
(three hard gates: live `HIGH <- 0` collapses bin0 to (0,0) despite
the NACK; grow `HIGH <- 512` reads 0..512 despite the NACK; C-order
`LOW <- 512` leaves bin1 at (512,0) despite the NACK — then the
restore + partition assert). The requested features then landed, all
layouts pinned to the OpenNI 2.2.0 reference source (re-cloned this
turn — see section 7): the wavelength debug 0xc200 decoder (packed
44-byte `XnWavelengthCorrectionDebugPacket`), the TEC 0xd200 / NESA
0xd201 UTF-16 text channels, their BIST trigger (the full
`XnBistType`/`XnBistError` bit sets, TEC bit 6 / NESA bit 7 — the
official updater's `RunBISTNESAUnlimited` is the NESA_UNLIMITED bit-8
variant, non-terminating, documented as non-selectable), the full
depth mode matrix (`depth_mode_matrix()` + wide Resolution ids
480P/720P/1280x960), SXGA 1280x1024 probed at 15 fps (the USB-2 bulk
budget: 11-bit packed @ SXGA@15 ≈ 26 MiB/s fits; the advertised @30 ≈
52 MiB/s cannot), QVGA@60 hard-gated in the new hardware test 13, and
the modulation-frequency honest surface (`refresh_device_frequency()`
— the value FOLLOWS the stream mode; the reference re-reads it after
every configuration and has NO write anywhere; the factory value
lives in the AlgorithmParams flash image). 155 lib + 4 golden + 1 doc
tests green, clippy clean. **State: waiting for the 0.3.7 on-camera
rerun — expected 14/14; the new outputs to paste back are listed in
section 9.** NOTE: bin1 was left at (512,0) by the 0.3.6 panic (the
panic fired before the restore) — **power-cycle the camera before
the rerun** so the suite starts from the factory partition.

> Purpose: upload this file in a new chat to restore full context. It
> records the standing rules, the task, everything that was found and
> decided, every file that was produced, and the state of the current
> workspace.

## 1. Standing user rules (apply to EVERY future turn)

1. **Communicate in English only** (all replies, all document content).
2. **Zip backup of generated files to `download/`** every turn.
3. **Do NOT run full cargo builds of the complete project — the sandbox
   does not have enough RAM.** Use targeted invocations: `cargo test
   --lib -j 2`, `cargo test --test golden -j 2`, `cargo clippy --lib
   -j 2` (+`--tests`), `cargo test --test hardware -j 2` (or
   `--no-run`); never `cargo build`/all-target builds, no release
   builds. Also avoid `rg` without a path filter or `grep`-over-tree
   commands — use targeted patterns or the Grep tool (a bare `rg`
   over `firmware/` timed out this turn: the .fla blobs are large).
   Also: `cargo fmt -- <files>` does NOT scope rustfmt — it formatted
   the whole crate; revert untouched files (`git checkout -- <file>`)
   when it churns them (presets.rs this turn).
4. **Produce a downloadable session backup** (this file, refreshed with
   a new version/date each turn) so the chat can be continued/resumed
   later.
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER
   printed or committed). Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat
   /home/z/my-project/.gh-token)" > /tmp/gh-creds` then `git -c
   credential.helper='store --file=/tmp/gh-creds' push origin main`.
   NOTE: the user has now pasted FOUR PATs in plaintext across turns
   (all stored, latest wins); remind them to rotate ALL of them.
   **Fetch/rebase before pushing** — the user sometimes pushes their
   own commits.
6. **Ignore the `research/idong100*` branches** on the remote — a
   multi-agent research fork. All xtion-rs work goes to `main` only.
7. **Cargo.lock is gitignored** (library crate) — do not `git add` it.

## 2. Task history (condensed)

- **Turns 1-13 (2026-11-19..22):** research → test scripts → pure-Rust
  driver (Option D, turn 7) → five on-hardware bugs fixed (0.1.1-0.1.5)
  → depth hardware-VALIDATED (87.4% valid, 12.17 MiB/s, 29.97 fps).
- **Turn 15:** firmware decompiled (Ghidra); ASUS fw 5.8.22 mirrored.
- **Turns 16-20 (0.2.0-0.2.3):** identity + IR + events; Jango
  fallback; CMOS blanking; IR wire size (MT9M001 height+8).
- **Turns 21-28 (0.2.4-0.2.10):** color chase to its hardware root
  cause: **image_cmos_type = 0 — NO color sensor on this unit** (0.2.10
  gate + skip). Read-only diagnostics tier (0.2.8), stream hygiene +
  full image-open C parity (0.2.9).
- **Turn 29 (0.3.0):** photogrammetry milestone — geometry
  (`depth_intrinsics`, `to_world`, `point_cloud`, PLY example),
  wrap-aware host timestamps (`timestamp_us`), diagnostics-parse fixes.
  Pushed 0ecdc56.
- **Turn 30 (0.3.1):** scene-robust plane-fit gate (RANSAC dominant
  plane + PCA refit + conditional 15 mm RMS gate) + synthetic
  regression test. Pushed a717d85; camera run 9/9 green.
- **Turn 31 (0.3.2):** flash story closed offline: `XnFlashFile`
  pinned (packed 22 B), word addressing, the fw packed/unpacked
  table-walk bug, CRC-16/CCITT-FALSE per-file CRC, read_flash
  chunking fix. Pushed cf56f7a.
- **Turn 32 (0.3.3):** 0.3.2 run confirmed everything on camera
  (22-record table, 20/20 CRCs, per-device file block ids 14-21);
  cropping implemented (params 46-60, three streams, mode 2);
  hardware test 10. Pushed 827c3db.
- **Turn 33 (0.3.4):** depth processing controls (gain 21, AGC bins
  63-70, close range 84, wavelength 73/74) + crop batch write
  (C-exact multi-pair SET_PARAM). Pushed c1a0e3b. Camera run: 10/11 —
  `depth_processing_controls` died on the AGC write cycle (NACK
  BadParams at the LOW-first grow order).
- **Turn 34 (0.3.5):** transient-safe AGC bin write order + probe
  ladder (never-panic design); GMC complete surface (config block,
  mode/debug setters, 0xa200/0xb200/0xb500 decode, `drain_gmc_packets`);
  preset resolution names. Pushed 3cb58a1. Camera run: 11/12 — the
  probe ladder ran (all four probes NACKed) but the partition assert
  fired: bin0 stuck at 0..512.
- **Turn 35 (0.3.6):** root-caused the 0.3.5 failure as
  apply-then-reject + the bail-out bug; reworked all
  depth-processing writes to readback-verified semantics; camera run
  11/12 — the C-order gate fired (THIS turn's input, see the header).
- **Turn 36 (this turn, 0.3.7):** quirk model corrected from the new
  camera evidence (see the header); implemented the requested
  features — SXGA depth 1280x1024@15 probe + QVGA@60 + the full mode
  matrix (`depth_mode_matrix()`, wide Resolution ids), the
  modulation-frequency surface (`refresh_device_frequency`, honest
  no-write documentation), the wavelength-debug telemetry decode
  (0xc200), and the NESA (0xd201) / TEC (0xd200) debug channels (BIST
  trigger + UTF-16 text decode + `enable_misc_capture`). Pushed
  bd557aa.

## 3. Turn-36 findings (all evidence- or source-verified this turn)

- **The corrected AGC-NACK model (the load-bearing correction).** The
  0.3.6 camera run's failing assert was itself the proof: `bin1 LOW <-
  512` (invariant-VIOLATING) was NACKed and read back `512..0` — it
  LANDED. The 0.3.5 run's `bin1 = (0,0)` was fully reproducible
  WITHOUT any refusal: the 0.3.5 restore's first bin1 write (`LOW <-
  0`) landed despite its NACK (the bail-out bug then skipped only the
  HIGH no-op write). Therefore: on fw 5.8.22, SET_PARAM on params
  63-70 replies NACK BadParams unconditionally (observed for no-op /
  invariant-preserving / invariant-violating writes) while applying
  the value. Operational rule (unchanged since 0.3.6, now fully
  confirmed): NEVER infer "not applied" from a NACK on this build —
  the readback is the only truth, and the public setters already
  return Ok only when verified.
- **The three debug-channel layouts (from the OpenNI 2.2.0 source,
  master @ 115cf06c, cloned to /home/z/my-project/openni2-ref):**
  - **0xc200 wavelength debug** — `XnWavelengthCorrectionDebugPacket`
    (XnHostProtocol.h:289-303, pack(1), 44 bytes): fBLast/fBCurrent
    (f32), nIsHop, nCurrentSlidingWindow (u32), nCurrentHopsCount,
    nIsTecCalibrated, nWaitPeriod (u32), nIsWavelengthUnstable,
    `XnBestTecConf {u16 nBestHopsCount; u32 nBestSetPoint; i32
    nBestStep}` (6 B), nIsTotallyUnstable, nConfiguredTecSetPoint
    (u32), nCurrentStep (i32). The reference's
    XnWavelengthCorrectionDebugProcessor only dumps it to
    WavelengthCorrection.csv (16 columns = host ts + packet id + the
    14 fields).
  - **0xd200 TEC_DEBUG / 0xd201 NESA_DEBUG** — a UTF-16LE text line
    (≤ 256 B = `XN_SENSOR_TEC_DEBUG_MAX_PACKET_SIZE`), printed with
    the wide `%S` and dumped verbatim to TecDebug.csv / NesaDebug.txt
    (XnTecDebugProcessor.cpp / XnNesaDebugProcessor.cpp — they exist
    in OpenNI2; the earlier "fw-only" provenance in FEATURE_PLAN was
    wrong and is corrected). The fw's nearby strings: `OT %d` /
    `FRQ %d` (temperature/frequency telemetry) and the NESA status
    lines (OK / N-A / INV / ERR / TIME / DISABLED).
  - **BIST trigger** — `XN_BIST_TEC_TEST_MASK` (bit 6) narrates the
    TEC test on 0xd200, `XN_BIST_NESA_TEST_MASK` (bit 7) the NESA
    test on 0xd201; NESA_UNLIMITED (bit 8) is the updater's
    `RunBISTNESAUnlimited` finish function and never terminates on
    its own — deliberately not selectable. Reply = u32 failure word
    (`XnBistError` bits: RAM, CMOS buses, flash, potentiometer,
    audio, projector, TEC heater/disconnect/crossed/fault).
  - Firmware RE this turn (capstone re-installed) confirmed the
    telemetry strings but the packet magics were NOT findable as
    literals/movw in source.out.bin.fla (the emitter builds the type
    some other way); the OpenNI source is the layout authority.
- **The mode matrix semantics.** The depth cmos preset format field is
  NOT the stream depth format (param 18): the presets list only
  formats 0 and 6 while format 3 (packed-11, the Linux default)
  streams fine at VGA@30 — the preset format is a CMOS readout id (6,
  like the image list's 9, is outside the C's enums; semantics
  unresolved). The advertised fps is the CMOS maximum, not the USB
  budget: SXGA@30 needs ~52 MiB/s (bus ceiling ~40), SXGA@15 ≈ 26
  MiB/s fits. `start_depth` stays permissive (C parity at the property
  level); the firmware NACKs refused modes itself.
- **The modulation frequency story (settled honestly).** The reference
  driver only READS algorithm 0x80 (XnSensor.cpp:627,
  XnSensorStreamHelper.cpp:133 — re-read after every stream config
  because it "might change on specific stream configuration"); no
  SET_PARAM id exists, no write call anywhere. The factory value
  lives in the AlgorithmParams flash image (its 8-byte entries carry
  {res, fmt, ConstShift=200, Freq=60} + blanking floats ~58.2). The
  multi-camera interference recipe is different matrix entries per
  camera (different mode → different frequency). Delivered:
  `refresh_device_frequency()` + the wavelength channel's hop
  telemetry as the observable surface; documented in PROTOCOL.md,
  device.rs and FEATURE_PLAN.
- **Resolution enum extended** with 480P (9, 864x480), 720P (14),
  1280x960 (15) — the ids this firmware advertises; needed so the
  full matrix is addressable (`Resolution::from_u16` added too).

## 4. 0.3.7 changes (commit bd557aa, pushed to main)

- `src/protocol/mod.rs`: packet_type consts WAVELENGTH_CORRECTION_DEBUG
  (0xc200), TEC_DEBUG (0xd200), NESA_DEBUG (0xd201) with provenance
  and trigger docs; Resolution extended (+from_u16).
- `src/stream.rs`: `WavelengthDebugPacket` + `BestTecConf` (field-
  faithful ports), `DebugTextChannel`/`DebugTextPacket` (UTF-16LE
  lossy decode, NUL-terminated), `DebugTelemetryPacket` enum;
  assembler accumulators for the three families (chunk-span safe,
  channel re-stated per chunk, decode gated on the completing
  packet's kind); `pop_debug_packet`/`drain_debug_packets`;
  5 new unit tests (field offsets, split-channel survival, interleave
  independence, truncation drop, text decode both channels).
- `src/protocol/system.rs`: bist::TEC/NESA/NESA_UNLIMITED/ALL masks +
  `BistFailure`/`failure_bits` (the full `XnBistError` table); 2 unit
  tests.
- `src/device.rs`: `drain_debug_telemetry()` (pumps misc, drains all
  four assemblers), `enable_misc_capture`/`disable_misc_capture`
  (misc ring without a data stream — the BIST window),
  `depth_mode_matrix()` (documented: preset-format ≠ stream format,
  USB budget, interference lever), `refresh_device_frequency()`,
  corrected quirk-model docs on `write_agc_bin_via` /
  `set_param_verified_via` + the unit-test comments,
  `start_depth` mode-matrix docs.
- `src/lib.rs`: exports for all new public types.
- `tests/hardware.rs`: test 11's gates reversed/corrected (three hard
  quirk gates, honest probe labels, probe-2 readback added); NEW test
  13 `depth_mode_matrix_and_high_fps` (matrix enumeration + full walk
  + SXGA@15 probe + QVGA@60 hard gate + per-mode frequency read); NEW
  test 14 `debug_telemetry_channels` (wavelength window with restores
  + BIST TEC|NESA with misc capture + failure-bit decode + censuses;
  everything verdict-first — first camera exercise of all three).
- Docs: PROTOCOL.md (corrected AGC findings; new "depth mode matrix"
  section with the frequency story; new "Debug telemetry channels"
  section), README (0.3.7 changelog + status rows + 155 tests),
  FEATURE_PLAN (rows: SXGA/QVGA@60/wavelength/NESA-TEC done,
  frequency resolved-as-read-only + provenance fix; phase-4 items),
  VALIDATION (row 11 corrected; rows 13/14; 160 checks), FIRMWARE.md
  (debug-telemetry note + the BIST trigger). Cargo 0.3.7.
- 155 lib + 4 golden + 1 doc tests green, clippy clean (incl. --tests).

## 5. Hardware test inventory (FOURTEEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green.
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic (start_color gate).
7. `system_commands_readonly` — camera-validated 0.3.2.
8. `depth_geometry_fov_and_timestamps` — camera-validated.
9. `plane_fit_synthetic` — camera-independent.
10. `depth_and_ir_crop_windows` — camera-validated (mode 2 = verdict).
11. `depth_processing_controls` — **0.3.7 corrected gates; the 0.3.6
    run failed ONLY on the now-reversed C-order gate**; awaiting the
    0.3.7 rerun (power-cycle first — bin1 was left at (512,0) by the
    panic).
12. `gmc_config_and_motion_data` — camera-validated 0.3.5.
13. `depth_mode_matrix_and_high_fps` — 0.3.7, first run next: walks
    the matrix, hard-gates QVGA@60, probes SXGA@15.
14. `debug_telemetry_channels` — 0.3.7, first run next: wavelength
    debug window + BIST(T|N) text channels.

Run all: `RUST_BACKTRACE=full cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware depth_mode_matrix -- --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v36.md` | This file (v36, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v36.zip` | Turn-36 zip (repo with .git at 0.3.7 incl. this backup). |
| `xtion_rs_driver_overview.pdf` | Rev 3 driver overview (0.3.5, historical). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch
main, **HEAD = bd557aa (0.3.7)**; also committed at
docs/recovery/session_backup_2026-11-23-v36.md). If a future turn
finds the tree missing, re-clone with the stored token or restore
this zip; the remote also carries the `research/idong100*` branches
to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 36)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.7, clean, pushed.
- Rust toolchain: intact from turn 35 (1.100.0; RUSTUP_HOME=
  /home/z/my-project/.rustup CARGO_HOME=/home/z/my-project/.cargo;
  libusb dev at /tmp/libusb-dev with PKG_CONFIG_PATH + RUSTFLAGS —
  see the v35 backup section 7 for the full reinstall recipe).
- **OpenNI 2.2.0 reference source: re-acquired this turn** —
  `/home/z/my-project/openni2-ref` (github.com/OpenNI/OpenNI2 master
  @ 115cf06c, the 2.2-era final state). Key files for the debug
  channels: Sensor/XnWavelengthCorrectionDebugProcessor.cpp,
  XnTecDebugProcessor.cpp, XnNesaDebugProcessor.cpp,
  XnHostProtocol.h:282-303, Include/PS1080.h:370-423 (BIST),
  XnFirmwareStreams.cpp:330+ (routing). Note the "2.2.0.33" tag does
  not exist on that remote — master IS the final 2.2 state.
- Firmware blobs: intact at `xtion-rs/firmware/5.8.22/*.fla`.
- capstone re-installed (pip); RE scripts at
  /home/z/my-project/scripts/fw_*.py (paths fixed from the stale
  xtion-rs/xtion-rs prefix).
- The user clones to /home/t/Projekte/Photogrammetrie/cameras/xtion-rs.

## 8. Gotchas learned (cumulative, new first)

- **fw 5.8.22 NACKs-and-applies on params 63-70 (both runs, model
  final):** every SET_PARAM there replies BadParams while landing;
  readback is the only truth. `write_agc_bin_via` /
  `set_param_verified_via` encode it; the public setters return Ok
  only when verified. A panic between probe and restore leaves bins
  dirty (0.3.6 run: bin1 at (512,0)) — power-cycle before the next
  run; runtime params do not persist.
- **cargo fmt -- <files> formats the WHOLE crate** (the file list does
  not scope it); revert untouched files when it churns them
  (presets.rs this turn — the known 1.100-vs-older skew).
- `rg` without a path over the repo (firmware blobs) times out at
  120 s — scope every search.
- OpenNI2's master has no 2.2.0.33 tag — master @ 115cf06c is the
  final 2.2 source; the PS1080 driver subdirs list the debug
  processors under Source/Drivers/PS1080/Sensor/.
- The TEC/NESA/wavelength debug packet types are NOT literals in the
  fw binary (no movw/literal hits) — the OpenNI source is the layout
  authority for all three.
- Multi-write sequences must not bail between steps on a NACK (the
  0.3.5 lesson — unchanged); test restores print their errors.
- Emitter opcode 42: BadParams while IDLE, ACK while depth streams.
- Crop: while mode != 0 the firmware transmits ONLY the window; IR
  has no +8 CMOS rows while cropped; geometry pairs first, MODE
  last, one multi-pair batch; mode 2 NACKed in every form on this
  build.
- FileTable record 0's crc 0xfffc does not self-verify (expected
  MISMATCH); every other stored crc verified on device (0.3.2 run).
- DOWNLOAD_FILE on fw 5.8.22 serves wrong files for some types —
  ground truth = READ_FLASH.
- All flash addresses are 16-bit WORDS (byte = ×2); `XnFlashFile` is
  22 B packed; flash CRC = CRC-16/CCITT-FALSE over big-endian words.
- Plane-fit gate: coverage is the discriminator; 15 mm RMS gate once
  coverage >= 40%.
- fw 5.8.22 does not log NACKs — GET_LOG returns boot history only.
- fDeviceFrequency is ticks per MICROSECOND (~60.0); wrap 71.58 s.
- Wire fixed-params layout is +1 word vs the C struct; offsets
  hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- SET_PARAM retries 5x on timeout only, BadParams is final.
- Stream-mode params are a state machine: changes go through OFF;
  sessions end with all streams OFF; init soft reset does NOT clear
  stream modes.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker; partial first
  frames dropped; buf_size big-endian; high-speed USB required;
  split replies; v2.6 replies have no CRC; bulk reads multiple of
  wMaxPacketSize.
- Git: fetch + rebase before push; Cargo.lock gitignored.

## 9. Open threads (if the user continues)

- **Immediate: POWER-CYCLE the camera, then rerun the suite on 0.3.7**
  (expected 14/14). PASTE BACK:
  (a) test 13's full walk (every mode's started/refused line, the
  measured fps + valid pixels + per-mode device frequency, the SXGA@15
  probe outcome — if the firmware refuses (SXGA,15) at the blanking
  read, the SXGA@30 fallback outcome too);
  (b) test 14's census lines (the wavelength enable landed/refused
  lines, the 0xc200 packet printouts if any, the BIST reply +
  failure bits, the TEC/NESA text lines if any);
  (c) test 11's three quirk gates + the custom-partition cycle line +
  the close-range and wavelength sections (their first full run —
  0.3.6 never got past the C-order gate).
- If test 13 shows QVGA@60 delivering but ~30 fps (mode not actually
  applied): check whether the fps NACK needs the C's reference-
  resolution switch (depth@60 forces the VGA reference,
  XnSensorFirmwareParams::RecalculateReferenceResolution) — a
  param-2x write may be needed for C parity.
- If test 14's wavelength channel is silent: try enabling 73 BEFORE
  74 (the test writes both; a dependent ordering is plausible), and
  check the TEC data (opcode 38) before/after for set-point motion.
- If BIST(T|N) times out: raise the timeout toward the C's 300 s and
  re-run ONLY test 14 (`cargo test --test hardware debug_telemetry
  -- --nocapture`); a wedged device recovers via the next open()'s
  soft reset.
- **Crates.io 0.3.7** is a natural release point once the camera run
  is green; polish: README example for the mode matrix + telemetry,
  docs.rs check.
- Phase 4 backlog: COMPRESSED_PS depth (SXGA@30 needs it), audio,
  M6 robustness (keep-alive thread, hotplug, soak), registration
  end-to-end (RGB units only), accelerometer (fw RE).
- Golden vectors: `cargo test --test golden` (flash table + CRC
  vectors included).
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/
  .gh-token (never print or commit the token); fetch + rebase first
  (rule 5).
- SECURITY: rotate the four pasted PATs when this work concludes.
