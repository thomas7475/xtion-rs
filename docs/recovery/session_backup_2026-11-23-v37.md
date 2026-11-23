# Chat Session Backup — ASUS Xtion Linux research

**Version:** 37 (2026-11-23, turn 37): **0.3.8 — the silent-stream fix
(`start_depth_verified` + `FrameTimeout`) + the ASUS/PrimeSense
firmware-history research — pushed 4808517.** The turn's input was
the first full 0.3.7 camera run (pasted in full): **10/14 — tests 2,
8, 11 and 12 all died at their baseline depth frame with
"control transfer timed out after retries"**, which turned out to be a
MISNOMER: `read_depth_frame` returned `ControlTimeout` on its no-data
deadline path, and no control transfer had failed at all. The real
phenomenon (pinned from the run's own evidence): **fw 5.8.22 sometimes
ACKs a complete stream-on sequence (params + blanking + ON) and then
delivers ZERO bytes on the depth bulk endpoint.** Proof chain: (a) the
suite is serialized by the tests' own `DEVICE_LOCK`, so this was
single-client behavior; (b) the four failures were 5-10 s windows with
zero data; (c) test 13's own mode 1 (QVGA@30) hit the same silence
AFTER the refused SXGA@15 probe and then RECOVERED on the next mode —
stop + restart revives it; (d) test 8's "death after the emitter
toggle" was the same thing (its frame loop STARTS after the toggle; it
never streamed); (e) tests 14/10 streamed fine BEFORE test 13 began.
Correlation (unproven, next run decides): the dead sessions clustered
after the walk's REFUSED starts (SXGA@15 probe → mode 1 dead;
SXGA@30/480P refusals → all four baselines dead). Soft resets do NOT
clear it (each failing test opened fresh with the init soft reset);
power cycle does. The fix: `XtionError::FrameTimeout` (no-data gets
its own name; ring + assembler counters logged on every occurrence),
`Xtion::start_depth_verified(config, attempts)` (probe one frame, stop/
restart on silence — the camera-proven recovery, returns the attempt
that delivered), verified starts at ALL depth-start sites in the
suite, and ring-counter prints on every failure path — so the next
run separates "endpoint never sent data" (completions = 0) from "data
that never assembles" (bytes > 0, no frames). The requested research
also landed: the public ASUS/PrimeSense firmware timeline (5.7.x ship
era → 5.7.9 → 5.8.14 "OpenNI 2.2 Ready" → 5.8.22 FINAL 2013-07-31)
documented in FIRMWARE.md section 6 — 5.8.x IS the OpenNI 2.x surface
this driver speaks; downgrading is not viable. Camera answers from
the run recorded in the docs: QVGA@60 measured 60.0 fps (VALIDATED),
VGA@30 30.0, QQVGA@30 30.0; SXGA@15/SXGA@30/480P@30 all refused with
BadParams at the blanking read (no 15 fps SXGA preset entry on this
build); device frequency 60.000 ticks/us at every mode; test 14: both
wavelength enables landed, 0xc200 census 0/90 frames, BIST TEC|NESA
all green, no narration. **State: waiting for the 0.3.8 on-camera
rerun — POWER-CYCLE the camera first** (the 0.3.7 run ended with the
device in the silent-stream state, and bin1 may still hold the
0.3.6 leftover (512,0)); the outputs to paste back are in section 9.

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
   `--no-run`), `cargo test --doc -j 2`; never `cargo build`/all-target
   builds, no release builds. Also avoid `rg` without a path filter or
   `grep`-over-tree commands — use targeted patterns or the Grep tool.
   Also: `cargo fmt -- <files>` does NOT scope rustfmt — it formatted
   the whole crate once; do not use it, hand-format instead.
4. **Produce a downloadable session backup** (this file, refreshed with
   a new version/date each turn) so the chat can be continued/resumed
   later.
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER
   printed or committed). Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat
   /home/z/my-project/.gh-token)" > /tmp/gh-creds` then `git -c
   credential.helper='store --file=/tmp/gh-creds' push origin main`
   (rm /tmp/gh-creds after). NOTE: the user has pasted FOUR PATs in
   plaintext across turns (all stored, latest wins); remind them to
   rotate ALL of them. **Fetch/rebase before pushing** — the user
   sometimes pushes their own commits.
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
- **Turn 30 (0.3.1):** scene-robust plane-fit gate (RANSAC + PCA
  refit). Camera run 9/9 green.
- **Turn 31-32 (0.3.2-0.3.3):** flash story closed offline + verified
  on camera (22-record table, 20/20 CRCs); cropping implemented
  (params 46-60, three streams; mode 2 NACKed on this build).
- **Turn 33-35 (0.3.4-0.3.6):** depth-processing controls + the AGC
  NACK saga → readback-verified writes; GMC surface.
- **Turn 36 (0.3.7):** the quirk model corrected (fw NACKs-and-applies
  on params 63-70); the requested features — full depth mode matrix,
  SXGA@15 probe, QVGA@60 gate, modulation-frequency read surface,
  wavelength-debug 0xc200 decode, TEC/NESA 0xd200/0xd201 channels +
  BIST trigger. Pushed bd557aa.
- **Turn 37 (this turn, 0.3.8):** the 0.3.7 camera run analyzed (10/14
  — the four baseline deaths root-caused as the SILENT-STREAM quirk,
  not control timeouts); `FrameTimeout` + `start_depth_verified` +
  ring-counter diagnostics landed in the driver and the suite; the ASUS
  firmware-history research documented (FIRMWARE.md section 6). Pushed
  4808517.

## 3. Turn-37 findings (all evidence- or source-verified this turn)

- **The silent-stream quirk (the load-bearing finding).** A `start_depth`
  that ACKs everything can still deliver zero endpoint bytes: `Ok` from
  start proves the CONTROL plane only. 0.3.7 run evidence: tests 2/8/11/12
  dead at baseline (5-10 s, zero data) + test 13's mode 1 dead then
  recovered on mode 2 (stop+start = the cure). The old code returned
  `ControlTimeout` ("control transfer timed out after retries") from the
  frame-read deadline path — pure misdiagnosis; no control transfer had
  failed. Correlation to be confirmed next run via the new ring-counter
  logs: the dead sessions clustered after REFUSED starts (the walk's
  SXGA@15/SXGA@30/480P probes). Soft reset does NOT clear it (four fresh
  opens stayed dead); power cycle does. `start_depth_verified` encodes
  the proven recovery; `FrameTimeout` + logged counters make the next
  occurrence self-diagnosing (completions = 0 → endpoint never sent;
  bytes > 0 with no frames → unframeable data).
- **The ASUS/PrimeSense firmware timeline (the research deliverable,
  FIRMWARE.md section 6):** ship-era 5.7.x (OpenNI 1.x-only) → 5.7.9
  (2013-04, the earlier PrimeSense update for Xtion Pro/Pro Live) →
  **5.8.14 (2013-04-26, "OpenNI 2.2 Ready": USB 3.0 support, UAC
  audio, enhanced zoom/crop, QQVGA depth added, 1280x960 color, 8-bit
  grayscale; OpenNI 1.x sound REMOVED)** → **5.8.22 (2013-07-31, the
  FINAL release — "Carmine 1.08x [1.081 & 1.082] RD v5.8.22", release
  notes v1.1; ASUS shipped it as FWUpdate_5_8_22.zip, UsbUpdate.exe +
  !Update-RD108x!.bat, dlcdnet.asus.com; NOT for Carmine 1.09).**
  Sources: kheresy.wordpress.com 2013-04-26 (the 5.8.14 notes),
  Artec3D support article 201831841 (the 5.8.22 OpenNI-2.0 update
  procedure), the Scribd-mirrored 5.8.22 release notes, MAGICC wiki +
  jurn.link 2021 (the ASUS download URL; "Primesense Carmine 1.08"
  device identity). Implication: 5.8.x IS the OpenNI 2.x surface the
  driver speaks — downgrading to 5.7.x loses the protocol itself; the
  camera-confirmed refusals (crop mode 2, SXGA@15, SXGA@30, 480P@30)
  are the FINAL firmware's steady state. Interesting mismatch: 5.8.14
  advertises "enhanced zoom and crop support", yet 5.8.22 NACKs crop
  mode 2 in every probed form.
- **Camera answers from the 0.3.7 run (now in the docs):** QVGA@60
  measured **60.0 fps — the high-rate mode is VALIDATED** (16.66
  ms/frame, monotonic); VGA@30 30.0 fps; QQVGA@30 30.0 fps; QVGA@30
  fell to the silent-stream quirk that run. SXGA@15, SXGA@30 and
  480P@30: **all refused, NACK BadParams at the blanking read** (the
  preset table carries no 15 fps SXGA entry; @30 exceeds the USB-2
  bulk budget; 480P refused despite being advertised). Device
  frequency read 60.000 ticks/us at EVERY mode on this unit (the
  multi-camera lever exists in principle; this unit did not show a
  mode-dependent value). Test 14 verdicts: wavelength (73)/(74)
  enables LANDED, 0xc200 census **0/90 frames** (the algorithm stayed
  silent or this build does not emit it); BIST TEC|NESA **all green**,
  zero TEC/NESA narration lines; the BIST itself completes fine.
- **The test suite was ALREADY serialized** (tests/hardware.rs
  `DEVICE_LOCK`, one client at a time) — the 0.3.7 failures were NOT
  parallel-client contention; do not re-theorize that.
- **The 0.2.x precedent for silent streams:** start_ir once went silent
  for want of the CMOS-blanking fetch (v22 backup) — the blanking
  fetch matters; the new quirk happens WITH the fetch present, so it
  is a different (firmware-internal) failure mode.

## 4. 0.3.8 changes (commit 4808517, pushed to main)

- `src/error.rs`: new `XtionError::FrameTimeout` ("no frame data
  arrived within the read timeout — the stream mode is on but the
  endpoint is silent"). NOTE: enum addition is a breaking change for
  exhaustive matches — nothing in this crate matched exhaustively.
- `src/device.rs`: the four frame-read deadline paths (depth, IR,
  color, tap) return `FrameTimeout` instead of `ControlTimeout`; the
  depth path logs ring + assembler counters (`log::error!`) when it
  fires; NEW `start_depth_verified(config, attempts) -> Result<u32>` —
  start, probe one frame (3 s budget), on `FrameTimeout` stop+restart
  up to `attempts`, return the attempt number that delivered (1 =
  healthy); other errors returned as-is after stopping.
- `tests/hardware.rs`: verified starts at every depth-start site
  (tests 2, 8, 10 ×2, 11, 12, 13 walk + final restart, 14); revived
  starts print `start depth: attempt N (silent start revived)`; test
  13's walk prints `started (silent start, revived at attempt N)` and
  prints the ring counters on any mode-read failure; test 8's frame
  loop prints the ring counters before panicking.
- Docs: README (0.3.8 changelog + status rows: silent-stream recovery;
  mode-matrix row camera outcomes), PROTOCOL.md (NEW section "The
  silent-stream quirk and the verified start (0.3.8)" + mode-matrix
  camera answers), FIRMWARE.md (NEW section 6 "The public firmware
  timeline" + Sources renumbered to 7), VALIDATION.md (rows 2/8/13/14 +
  the 0.3.7-run outcome paragraph + power-cycle instruction),
  FEATURE_PLAN (SXGA row: camera-refused, steady state; QVGA@60 row:
  camera-validated). Cargo 0.3.8.
- 155 lib + 4 golden + 1 doc tests green, clippy clean (incl.
  `--tests`), hardware test binary compiles (`--no-run`).

## 5. Hardware test inventory (FOURTEEN tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — **0.3.7 run: DEAD at baseline (the
   silent-stream quirk); 0.3.8 verified start covers it — rerun**.
3. `phase1_identity_and_options` — green.
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic.
7. `system_commands_readonly` — camera-validated 0.3.2.
8. `depth_geometry_fov_and_timestamps` — **0.3.7 run: DEAD at first
   frame read; 0.3.8 verified start + loop diagnostics — rerun** (its
   emitter toggle is NOT the killer — the frame loop starts after the
   toggle; the stream was dead from the start).
9. `plane_fit_synthetic` — camera-independent, green.
10. `depth_and_ir_crop_windows` — green in the 0.3.7 run (mode 2 =
    refused verdict); 0.3.8: verified starts added.
11. `depth_processing_controls` — **0.3.7 run: DEAD at baseline — the
    0.3.7-corrected AGC gates have STILL never run on camera; the
    0.3.8 rerun is their first exercise**.
12. `gmc_config_and_motion_data` — camera-validated 0.3.5; 0.3.7 run:
    DEAD at baseline; rerun.
13. `depth_mode_matrix_and_high_fps` — **PASSED in the 0.3.7 run**
    (QVGA@60 60.0 fps validated; SXGA@15/30 + 480P refused verdicts;
    QVGA@30 silent that run — 0.3.8 verified starts cover the walk).
14. `debug_telemetry_channels` — **PASSED in the 0.3.7 run** (censuses
    recorded; verdicts: wavelength 0/90, BIST green, no narration).

Run all: `RUST_BACKTRACE=full cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware depth_mode_matrix -- --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v37.md` | This file (v37, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v37.zip` | Turn-37 zip (repo with .git at 0.3.8 incl. this backup). |
| `xtion_rs_driver_overview.pdf` | Rev 3 driver overview (0.3.5, historical). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch
main, **HEAD = 4808517 (0.3.8)**; also committed at
docs/recovery/session_backup_2026-11-23-v37.md). If a future turn
finds the tree missing, re-clone with the stored token or restore
this zip; the remote also carries the `research/idong100*` branches
to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 37)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.8, clean, pushed.
- Rust toolchain: intact (1.100.0; RUSTUP_HOME=/home/z/my-project/
  .rustup CARGO_HOME=/home/z/my-project/.cargo). libusb dev at
  /tmp/libusb-dev (layout: /tmp/libusb-dev/usr/{lib,include}); every
  cargo invocation needs `PKG_CONFIG_PATH=/tmp/libusb-dev/usr/lib/
  pkgconfig RUSTFLAGS="-L/tmp/libusb-dev/usr/lib"` and
  `PATH=/home/z/my-project/.cargo/bin:$PATH`.
- OpenNI 2.2.0 reference source: `/home/z/my-project/openni2-ref`
  (master @ 115cf06c). Firmware blobs: `xtion-rs/firmware/5.8.22/`.
- capstone installed (pip); RE scripts at /home/z/my-project/scripts/
  fw_*.py.
- The user clones to /home/t/Projekte/Photogrammetrie/cameras/xtion-rs.

## 8. Gotchas learned (cumulative, new first)

- **fw 5.8.22 silent-stream quirk (0.3.7 run, model current):** a fully
  ACKed stream-on can deliver zero bulk bytes; NOT a control failure
  (the old `ControlTimeout` from frame reads was a misnomer — 0.3.8
  returns `FrameTimeout` there). Cure: stop + restart on the same
  handle (`start_depth_verified`); power cycle clears it; soft reset
  does NOT. Suspected trigger (unconfirmed): clustered after REFUSED
  mode starts (blanking NACK at SXGA/480P-class modes). When it fires,
  read the logged ring counters: completions = 0 → endpoint never
  sent; bytes > 0 → unframeable.
- **The suite is serialized** (`DEVICE_LOCK` in tests/hardware.rs) —
  never re-diagnose hardware-test failures as parallel-client
  contention.
- fw 5.8.22 NACKs-and-applies on params 63-70 (unchanged model): every
  SET_PARAM there replies BadParams while landing; readback is the
  only truth; panics between probe and restore leave bins dirty —
  power-cycle before reruns.
- `cargo fmt -- <files>` formats the WHOLE crate; `rg` without a path
  over the repo (firmware blobs) times out at 120 s.
- OpenNI2's master has no 2.2.0.33 tag — master @ 115cf06c is the
  final 2.2 source. The TEC/NESA/wavelength packet types are NOT
  literals in the fw binary — the OpenNI source is the layout
  authority.
- Emitter opcode 42: BadParams while IDLE, ACK while depth streams
  (the toggle is volatile; power-cycle restores it).
- Crop: while mode != 0 the firmware transmits ONLY the window; IR has
  no +8 CMOS rows while cropped; geometry pairs first, MODE last, one
  multi-pair batch; mode 2 NACKed in every form on this build.
- FileTable record 0's crc 0xfffc does not self-verify (expected
  MISMATCH); DOWNLOAD_FILE on fw 5.8.22 serves wrong files for some
  types — ground truth = READ_FLASH.
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
  frames dropped; buf_size big-endian; high-speed USB required; split
  replies; v2.6 replies have no CRC; bulk reads multiple of
  wMaxPacketSize.
- Git: fetch + rebase before push; Cargo.lock gitignored.

## 9. Open threads (if the user continues)

- **Immediate: POWER-CYCLE the camera, then rerun the suite on 0.3.8**
  (expected 14/14). PASTE BACK:
  (a) every `start depth: attempt N (silent start revived)` /
  `started (silent start, revived at attempt N)` line — the quirk's
  incidence and which starts it hits;
  (b) any `ring at failure: ...` line (completions/bytes/failures) —
  this decides "endpoint never sent data" vs "unframeable data" and
  confirms/kills the refused-start correlation;
  (c) test 11's full run (baseline, AGC three quirk gates, the custom
  partition cycle, close-range and wavelength sections) — its gates
  have still never run on camera;
  (d) test 13's full walk with verified starts + the SXGA@15 probe
  verdict line.
- If `start_depth_verified` exhausts its 3 attempts (the
  "stayed silent through 3 start attempts" BadConfig): that is the
  hard-wedge variant — power-cycle and rerun; if reproducible,
  try attempt escalation (longer settle between restarts) before
  concluding a deeper firmware wedge.
- **Crates.io 0.3.8** is a natural release point once the camera run is
  green; polish: README example for the verified start + telemetry,
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
