# Chat Session Backup — ASUS Xtion Linux research

**Version:** 30 (2026-11-23, turn 30): **0.3.1 — scene-robust plane-fit
gate — PUSHED a717d85.** The first 0.3.0 camera run came back 7/8 green;
the only failure was the new geometry test's plane-fit gate, and the
failure was the TEST's assumption, not the driver: 0.3.0 fitted one plane
to the WHOLE cloud by least squares, so the user's ordinary room (a
tilted two-plane scene, global RMS 49.23 mm over 266,584 points) failed
while every geometric gate passed — intrinsics 58.59/45.64 deg (spec
58/45), timestamp cadence 33.37 ms with 0.00 ms max deviation, center
pixel exactly on the optical axis (0, 0, 759). 0.3.1 replaces the fit
with a RANSAC dominant-plane search (400 three-point hypotheses on a
deterministic 8k subsample) + PCA refit (smallest-eigenvector normal) of
the full-cloud inliers; the RMS gate (15 mm, inlier band 20 mm) applies
ONLY when the dominant plane covers >= 40% of the cloud — anything else
reports its numbers and skips, so the test no longer demands a staged
scene. New camera-independent `plane_fit_synthetic` test pins the fit on
wall / two-plane / noise-cloud scenes (9 tests total now). **The same
camera run also settled the open 0.3.0 probe questions: device clock =
60.0 ticks/us (the timestamp port's assumption, confirmed); emitter
toggle ACKed WHILE depth streams (the idle-state BadParams NACK is an
idle-state refusal — hypothesis confirmed); the known-NACK trigger adds
0 new firmware-log entries on fw 5.8.22 (the log is boot-history, NACKs
are not logged — the fixed header parse renders sane module/error/line
values, closing that thread); and NO CRC-16 candidate matched
GainControl's stored 0xc44b over the whole 548-byte download — test 7
now dumps the full blob so the CRC region can be found offline.**
123 lib + 2 golden + 1 doc test green, clippy 0, fmt clean. **State:
waiting for the user's on-camera rerun of
`cargo test --test hardware -- --nocapture` on 0.3.1 — expected 9/9
green; PASTE BACK the dominant-plane lines from test 8 and the FULL
GainControl hex dump from test 7 (that names the flash CRC algorithm
and pins the on-flash table record layout).**

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
   all-target builds, no release builds.
4. **Produce a downloadable session backup** (this file, refreshed with a new
   version/date each turn) so the chat can be continued/resumed later.
5. GitHub token: `/home/z/my-project/.gh-token` (chmod 600, NEVER printed or
   committed). Push via `umask 077 && printf
   'https://thomas7475:%s@github.com\n' "$(cat /home/z/my-project/.gh-token)"
   > /tmp/gh-creds` then
   `git -c credential.helper='store --file=/tmp/gh-creds' push origin main`.
   NOTE: the user pasted a fresh PAT in the turn-30 chat — the stored file
   still works; recommend the user rotate the pasted one eventually.
   **Fetch/rebase before pushing** — the user sometimes pushes their own
   commits (8935057 "docs: recovery session backup v29" appeared between
   0.3.0 and 0.3.1).
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
- **Turn 29 (0.3.0):** the photogrammetry milestone — A1 geometry
  (`depth_intrinsics`, `to_world`, `point_cloud`, PLY example), A2
  wrap-aware host timestamps (`timestamp_us`), C diagnostics-parse fixes
  (log header field order, flash-list words/attrs documentation, emitter
  NACK context). Pushed 0ecdc56.
- **Turn 30 (this turn, 0.3.1):** the user's 0.3.0 camera run was 7/8 —
  only the new plane-fit gate failed, on a non-flat scene (user: "the
  scene it looks at is not a complete flat at the moment"). Fixed by
  making the fit scene-robust (RANSAC dominant plane + PCA refit +
  conditional gate) + synthetic regression test; documented the run's
  probe answers (clock, emitter-while-streaming, NACK-not-logged) and
  extended test 7's GainControl section to a full hex dump for the
  offline CRC-region sweep. Pushed a717d85 (rebased onto the user's
  8935057 backup commit).

## 3. Turn-30 findings (from the user's on-camera 0.3.0 run)

- **The 49.23 mm plane-fit failure was a test-scene artifact.** Global
  least squares over the whole cloud: mean distance 840.2 mm, slope
  (-0.8481, 0.0920) — a steeply tilted/multi-plane room, not a frontal
  wall. All actual geometry gates PASSED in the same test: intrinsics
  fx=fy=570.34 (ZPD 120.000 / ZPPS 0.10520), hfov 58.59 / vfov 45.64
  (spec 58/45), 90-frame timestamp deltas mean 33.37 ms, max deviation
  0.00 ms, monotonic, center pixel -> world (0.0, 0.0, 759.0) mm.
  Conclusion: A1 + A2 are camera-validated; only the acceptance-scene
  assumption was wrong. Fixed in 0.3.1 (see §4).
- **Device clock = Ok(60.0) ticks/us** (ALGORITHM_PARAMS 0x80) — exactly
  the ticks-per-MICROSECOND value the `CreateTimestampFromDevice` port
  assumes; wrap = 2^32/60 = 71.58 s. A2's clock model is confirmed.
- **Emitter toggle ACKed WHILE the depth stream runs** (geometry test's
  streaming probe): "emitter toggle: ACKed while depth streams
  (idle-state hypothesis confirmed)". The opcode-42 BadParams NACK
  (idle) is an IDLE-STATE refusal, not a build-wide ban — set_emitter
  needs a streaming-aware note, and any future emitter API should
  document "toggle while streaming" as the reliable path.
- **Known-NACK trigger adds 0 new firmware-log entries** on fw 5.8.22:
  the log is boot-history (68 entries, repeating module 0x5F/0x5A
  patterns with sane lines 176-312), NACKs are not logged. The log-parse
  thread is CLOSED (the fixed `{u16 size, u32 ts, u16 type}` header
  renders plausible values); the NACK-trigger probe stays in the test as
  a harmless data point.
- **Boot/self-test log lines: 0** — the text-log section is empty on
  this build (no runtime strings buffered at read time).
- **Flash CRC: still open, but narrowed.** The six pinned CRC-16
  candidates over the WHOLE 548-byte GainControl download
  (ccitt-false 0xdcac, xmodem 0x2737, kermit 0x83e8, arc 0x3bd4,
  modbus 0xc6f1, usb 0x390e) do NOT match the stored 0xc44b. The
  download's head `13 00 22 00 08 05 1e 01 00 00 3a 00 20 0b 0c b4
  ad 01 00 00 fa ff ff ff` does not start with the .fla type header
  either — DOWNLOAD_FILE replies carry their own framing (548 = 0x224
  bytes vs 57 words = 114 B of GainControl content; likely [record +
  params][sector-sized block]). Test 7 now hex-dumps the full blob; the
  next paste-back feeds an offline region + parameter sweep (all CRC-16
  poly/init/refin/refout/xorout combinations x sub-ranges).
- **On-flash file-table lead: record stride 26.** The raw 0x1000 sector
  shows the FileTable self-entry at 0x0000 (with the 0x12345678 magic,
  offset 0x1000, size 0x1000 words, crc 0xfffc — 20 bytes), then
  record-like structures repeating at 0x2c, 0x46, 0x60, 0x7a, 0x94...
  (stride 26 = 0x1a), each containing the `22 00 08 05` pattern. The
  stride scans scored: 24 -> 1 plausible, 26 -> 2 (id 13 type 0x06
  Application offset 0x10000 size 76849 words crc 0xdf96), 28 -> 2,
  32 -> 2. The bytes between 0x14-0x2b contain the 0x8000000f pattern
  that GET_FILE_LIST renders as garbage entry 2. **Next step: pin the
  exact `XnFlashFile` struct from the reference C
  (`firmware_dl/openni_ref/v2.2.0-debian.tar.gz`,
  Source/Drivers/PS1080/Sensor/XnHostProtocol.{cpp,h} — tarball verified
  intact) and write the real on-flash table parser.**
- Other telemetry from the run: TEC set point = measured = 37.24 C,
  duty 7%, mode 2; emitter state 1, setpoint 987 mV, pd 63, apc 1;
  depth CMOS I2C reg 0x00 = 33841 (0x8431, MT9M001 re-confirmed);
  AHB read 0x0 = 4042322160 (0xF0F0F0F0-ish pattern); IR 640x480
  value range (12, 1021); depth min 525 mm; USB High speed, 512 B
  max packet. Color tests skip with the no-sensor reason as designed.

## 4. 0.3.1 changes (commit a717d85, pushed to main; rebased onto 8935057)

- `tests/hardware.rs`:
  - NEW module-level fit: `fit_dominant_plane(cloud, tau)` — RANSAC (400
    3-point hypotheses, deterministic xorshift64* sampling on a ~8k
    stride subsample) + PCA refit on full-cloud inliers (power iteration
    + deflation for the two in-plane eigenvectors; normal = cross
    product) + final inlier/RMS pass. Helpers: `xorshift64`, `dot3`,
    `cross3`, `mat_vec`, `normalize3`, `dominant_eigenvector`;
    `PlaneFit { normal, distance, inliers, total, rms }` +
    `coverage()`.
  - Gate constants: `PLANE_TAU` 20 mm (inlier band), `PLANE_MIN_COVERAGE`
    0.40, `PLANE_MAX_RMS` 15 mm. Gate applies only when coverage >= 0.40;
    otherwise prints "plane-fit gate skipped: ..." — non-planar scenes
    no longer fail. Design note: inlier RMS is bounded by tau by
    construction (<= 20 mm), so COVERAGE is the real discriminator;
    the RMS gate is the quality check once a plane dominates.
  - Geometry test now prints the dominant plane (normal, tilt from the
    optical axis, distance) + inliers/coverage/inlier RMS, and asserts
    a >= 10k-point cloud (stream health) before fitting.
  - NEW `plane_fit_synthetic` test (no camera): synthetic wall at 1 m
    (coverage > 95%, rms < 6 mm, distance within 5 mm, tilt < 4 deg),
    two-plane + clutter room (dominant plane found, coverage ~53%,
    rms < 6 mm, tilt < 3 deg), uniform noise cloud (coverage stays under
    the 40% gate). Runs in every `cargo test --test hardware` — the
    RANSAC logic is now regression-covered without hardware.
  - Test 7 GainControl section: full hex dump of the downloaded blob
    (32 bytes/line) on top of the six candidates — feeds the offline
    CRC-region sweep.
- `Cargo.toml` 0.3.1; README (status row + 0.3.1 changelog entry),
  `docs/VALIDATION.md` (row 8 rewritten as conditional gate + new row 9
  for `plane_fit_synthetic`), `docs/FEATURE_PLAN.md` (0d hardware
  acceptance bullet).

## 5. Hardware test inventory (NINE tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green; idle emitter toggle prints the
   documented NACK skip (build refuses while idle — see the streaming
   ACK in test 8).
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic.
7. `system_commands_readonly` — green; 0.3.1 adds the FULL GainControl
   hex dump (paste back for the CRC sweep).
8. `depth_geometry_fov_and_timestamps` — geometry camera-validated
   0.3.0; 0.3.1 makes the plane-fit gate conditional (RANSAC dominant
   plane >= 40% coverage -> inlier RMS <= 15 mm; else report + skip).
9. `plane_fit_synthetic` — NEW, camera-independent, pins
   `fit_dominant_plane` on wall / two-plane / noise scenes.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- depth_geometry --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v30.md` | This file (v30, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v30.zip` | Turn-30 zip (repo with .git at 0.3.1 + this backup). |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (12 pages; historical). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (historical, 0.1.5). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = a717d85 (0.3.1)**; also committed at
docs/recovery/session_backup_2026-11-23-v30.md). If a future turn finds
the tree missing, re-clone with the stored token or restore this zip; the
remote also carries the `research/idong100*` branches to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 30)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.1, clean, pushed. (The
  v29 backup said `/home/z/my-project/xtion-rs/xtion-rs` — the v29 zip
  restores to `/home/z/my-project/xtion-rs/`; same repo, one level up.)
- OpenNI 2.2.0 reference: extract
  `/home/z/my-project/firmware_dl/openni_ref/v2.2.0-debian.tar.gz` to
  /tmp/xtion-verify (`--strip-components=1`). Tarball VERIFIED intact
  this turn (XnHostProtocol.cpp/.h present — needed for the flash-table
  struct). NOTE: some files have mangled identifier names (token-
  replacement corruption) — verify names, trust structure.
- Firmware blobs: `xtion-rs/firmware/5.8.22/*.fla` (restored with the
  tree). Sandbox scripts survive under /home/z/my-project/scripts:
  `fw_dispatcher_scan.py`, `fw_magic_find.py`, `fw_strings.py`,
  `fw_fla_header.py`.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`). Verified working this turn.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **Plane-fit gate design (0.3.1):** RANSAC inlier RMS is bounded by the
  inlier band (tau = 20 mm) BY CONSTRUCTION — the coverage fraction is
  what actually discriminates flat-wall scenes from clutter; the 15 mm
  RMS gate is the quality check once coverage >= 40%. A curved surface
  loses coverage below the gate and skips rather than fails.
- **Emitter opcode 42:** BadParams while IDLE, ACK while depth streams —
  an idle-state refusal on this build. Probe emitter state changes with
  a stream running.
- **fw 5.8.22 does not log NACKs** — GET_LOG returns boot history only
  (68 structured entries, modules 0x5F/0x5A, lines 176-312); runtime
  text-log section reads empty.
- **The flash CRC is not over the whole DOWNLOAD_FILE blob** (548 B for
  GainControl vs 114 B content; reply head `13 00 22 00 08 05 ...` is
  reply framing, not the .fla type header). Identify the region from the
  full dump before sweeping CRC parameters.
- **On-flash table records repeat at stride 26 from 0x2c** in the 0x1000
  sector; the FileTable self-entry (with 0x12345678 magic) is 20 bytes
  at 0x0000; bytes 0x14-0x2b hold the 0x8000000f pattern GET_FILE_LIST
  renders as garbage entry 2. Pin against XnFlashFile in
  XnHostProtocol.h before writing the parser.
- **Git: fetch + rebase before push** — the user pushes their own
  commits between turns (8935057 appeared this turn); Cargo.lock is
  gitignored (never `git add` it).
- XnLogEntryHeader field order: {u16 size, u32 ts, u16 type}; the
  structured-entry param is a u32.
- fDeviceFrequency is ticks per MICROSECOND (~60.0); wrap = 71.58 s.
- TimestampConverter first frame: the C runs the wrap branch AFTER init
  in the same call; anchor==ts==0 degenerates to +71.6 s (preserved);
  duplicate ts -> resync to ~0.
- XnFlashFile.size is WORDS; attributes 0x8000 = read-only; the .fla
  container's first u16 IS the flash file type; GainControl on flash =
  the .fla minus its 6-byte header (114 B).
- CRC-16/USB check = MODBUS ^ 0xFFFF (0x4B37 -> 0xB4C8).
- The reference tarball's sources contain mangled identifiers — verify
  names, trust structure.
- The old dispatcher offsets (FIRMWARE.md §5, 0x21400-0x22000) do not
  reproduce on the restored blob; do not cite without re-deriving.
- bImageSupported = `image_cmos_type != 0` — this unit has NO color
  sensor; the opcode-36 preset table is a platform list.
- Stream-mode params are a state machine: changes go through OFF; every
  C session ends with all streams OFF; the init soft reset does NOT
  clear stream modes.
- Wire fixed-params layout is +1 word vs the C struct; offsets
  hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- SET_PARAM retries 5x on timeout only, BadParams is final.
- GET_LOG entries: text 0x1200 (UTF-16, low byte), overflow 0x1201,
  structured = module<<8|error with {u16 line, u32 param}.
- READ_FLASH takes BYTE offsets; DOWNLOAD_FILE takes WORD offsets.
- Opcodes 18/27 NACK InvalidCommand; 35's direct reply is short —
  ALGORITHM_PARAMS 0x06 is the working blanking path.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker; partial first
  frames dropped; buf_size big-endian; high-speed USB required; split
  replies; v2.6 replies have no CRC; one garbage chunk per endpoint on
  fw < 5.0; bulk reads multiple of wMaxPacketSize.

## 9. Open threads (if the user continues)

- **Immediate:** rerun the suite on 0.3.1:
  `cargo test --test hardware -- --nocapture` — expected **9/9 green**
  (the user's current non-flat scene should print the dominant-plane
  lines and either gate or skip cleanly). PASTE BACK: (a) the
  `dominant plane:` + `plane fit:` lines from test 8, (b) the FULL
  GainControl hex dump from test 7 — (b) feeds the offline CRC-region
  sweep that names the flash CRC algorithm and closes the file-table
  parser.
- **Flash-table parser (C.2 finish):** pin `XnFlashFile` from
  XnHostProtocol.h (tarball path in §7), decode the 0x1000 sector
  (stride-26 lead), verify GainControl's stored 0xc44b against the
  dumped blob regions, then write `parse_flash_table()` into
  `src/protocol/system.rs` with the CRC check.
- **Emitter API polish:** document/encode the idle-vs-streaming behaviour
  (BadParams idle, ACK streaming) in `set_emitter_enabled`'s docs and
  error context.
- **Then (Phase 3):** cropping (PARAMs 46-60, bigger-crop-first
  protocol), depth write-read sweep, QVGA@60 mode matrix. Registration
  end-to-end only matters for RGB units (this unit: depth/IR only).
- **Crates.io 0.3.1** release polish: README example section for
  point_cloud, docs.rs check.
- Phase 4 backlog: AGC bins 63-70, wavelength 73/74, GMC 26/0xa200,
  audio, COMPRESSED_PS depth (SXGA@30 needs it; ~78.6 MB/s uncompressed
  exceeds bulk), M6 robustness (keep-alive thread, hotplug, soak).
- Golden vectors: `cargo test --test golden`.
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token); fetch + rebase first (rule 5).
