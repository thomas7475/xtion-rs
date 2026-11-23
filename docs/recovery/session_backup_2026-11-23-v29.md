# Chat Session Backup — ASUS Xtion Linux research

**Version:** 29 (2026-11-23, turn 29): **0.3.0 — the photogrammetry
milestone: camera geometry + wrap-aware timestamps, plus all three
diagnostics-parse fixes — PUSHED 0ecdc56.** The queue from the 0.2.10
camera run is fully implemented: (A1) `Xtion::depth_intrinsics(res)`
derives the pinhole model from the on-device calibration (ZPD/ZPPS — no
new device I/O), `DepthFrame::to_world` / `point_cloud` convert to
metric world coordinates and `examples/point_cloud.rs` writes an ASCII
PLY; (A2) every decoded frame now carries `timestamp_us` — microseconds
since the session anchor via a full port of the C's
`CreateTimestampFromDevice` (anchor, host-time wrap estimation, wrap-
aware delta, 10 s sanity re-sync; clock rate = ALGORITHM_PARAMS 0x80,
~60 ticks/µs); (C) the firmware-log header is `{u16 size, u32 ts,
u16 type}` — 0.2.8's field-order bug made boot structured entries print
as "Unix epoch" timestamps (0x5F00001F = ts_hi16|log_type splice; real
values: module 0x5F/err 0, module 0x5A/err 2, ~0x1Fxxxx boot ticks) —
fixed + `firmware_log_entries()` raw API; the flash file list is
documented (size in WORDS, 0x8000 = read-only, garbage tail on this fw)
with raw-table stride scans (24/26/28/32) at flash 0x1000 and a
GainControl CRC-16 candidate dump (six algorithms pinned by catalog
check values) — **one camera run names the undocumented flash CRC**;
the emitter BadParams NACK is verified as a build-specific refusal
(request byte-identical to XnHostProtocol.cpp:3420-3445, opcode IS
dispatched) and now reports that context; opcodes 18/27 (InvalidCommand)
and 35 (short reply) documented as fw 5.8.22 quirks. 123 lib + 2 golden
+ 1 doc test green, clippy 0, fmt clean, hardware target compiles
(8 hardware tests now — new `depth_geometry_fov_and_timestamps`).
**State: waiting for the user's on-camera run of
`cargo test --test hardware -- --nocapture` — expected all green (the
two skip cases stay: color no-sensor, emitter NACK-as-skip), plus the
log/table/CRC ground-truth rows to paste back for the final flash
table parser. NEXT (Phase 3): cropping (PARAMs 46-60), registration
end-to-end (ALGORITHM_PARAMS 0x02 tables; moot on this unit for color
but useful for depth/IR pipelines), QVGA@60 mode matrix.**

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
6. **Ignore the `research/idong100*` branches** on the remote:
   `research/idong100`, `-01-chinese-web`, `-02-products-software`,
   `-09-xeyez-analysis`, `-10-rust-driver-draft` — a multi-agent research
   fork. All xtion-rs work goes to `main` only.

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
- **Turn 29 (this turn):** the 0.2.10 camera run was all green (7/7,
  color skipping with the reason) → the user's queue (A1 geometry, A2
  timestamps, C diagnostics fixes) implemented in full as 0.3.0, pushed.
  Sandbox had been recycled again: restored from the v28 zip
  (`/home/z/my-project/download/asus_xtion_workspace_backup_2026-11-23_v28.zip`),
  which remained intact.

## 3. Turn-29 findings (all C-source-verified this turn)

- **The log "timestamp" bug was a field-order bug.** The C's packed
  `XnLogEntryHeader` (XnHostProtocol.cpp:1196-1211) is
  `{ u16 entry_size, u32 timestamp, u16 log_type }` — timestamp at offset
  2, BEFORE log_type. 0.2.8 read `{u16, u16, u32}`. Replaying the two
  observed values through the correct layout: 0x5F00001F → log_type
  0x5F00 (module 0x5F, error 0) + true ts ≈ 0x001Fxxxx; 0x5A02001F →
  log_type 0x5A02 (module 0x5A, error 2) + same boot-tick range. Both are
  boot-time structured entries; the ~2.03M "timestamps" are sane boot
  tick counts, not epochs. The structured payload's `nParam` is a
  **u32** (`XnLogDefaultData`), 0.2.8 read its low u16.
- **The firmware's own runtime log strings embed the same timestamp**
  as a `%lu` prefix (`%lu Error! Timeout IR! (%d)` etc. at image
  0x4d6-0x808) — on-hardware self-check for the header decode.
- **`XnFlashFile::size` is in WORDS** (vendor console prints
  "SizeInWords", PS1080Console.cpp:503); attributes 0x8000 = read-only
  ("[R]"). GainControl = 57 words = 114 B = the local .fla minus its
  6-byte [type][build][version] header — the on-flash content is known
  offline, so the CRC identification only needs the stored crc value
  from the camera run.
- **The .fla container type IS the flash file type** (GainControl.fla
  starts 0x0012, AlgorithmParams 0x000C, BootManager 0x0003, main image
  0x0006 = Application). The MG/BR protocol magic pair sits at image
  0xc14/0xc16 as u16 constants 0x4d47/0x4252 — verified on the restored
  blob (bytes read "GM"/"RB" because they are little-endian u16 values).
- **The emitter opcode-42 encoding is provably correct**: the C sends a
  single u16 (XnVSetEmitterStateRequest, XnHostProtocol.cpp:3420-3445),
  identical to the driver's request; the C's only gate is fw >= 5.8.15
  (met). BadParams ≠ InvalidCommand → the opcode IS dispatched, the
  handler refuses → build-specific (ASUS 5.8.22) refusal, likely laser
  safety. `set_emitter_enabled` now maps the NACK to a BadConfig with
  that context.
- **Firmware CPU-task names recovered** from the image:
  `c,T,DC,TD,PK,IK,DK,O,SM,HM,TR,DC,PC,TC` @ 0xd1e with the unit list
  `s,ms,c,c,c,c,c,c,i|c|p,i|h|c,cl|rm|wm,c,p,s` @ 0xd6c — the 15 tasks
  GET_CPU_STATS (27) would report; the opcode NACKs InvalidCommand on
  this build ("compiled in, locked").
- **Geometry sources pinned**: FOV = 2·atan(ZPPS·1280/2/ZPD) /
  2·atan(ZPPS·960/2/ZPD) (XnSensorDepthStream.cpp:187-191 — CMOS-native,
  resolution-independent); conversion = OpenNI2 core
  `convertDepthToWorldCoordinates` (Source/Core/OniStream.cpp:399-404,
  normalized x/y × z × 2·tan(fov/2), **Y up** because of `(0.5 - y/h)`).
  VGA closed form: fx = fy = ZPD/(2·ZPPS) (square pixels). ZPD 120 /
  ZPPS 0.1052 → 58.6°/45.7° = the spec 58/45.
- **Timestamp sources pinned**: `CreateTimestampFromDevice`
  (XnDataProcessor.cpp:84-226): global anchor (first frame's ticks +
  host time, shared), per-stream first-frame wrap estimation clipped to
  ±half-wrap around the host-time estimate, normal/wrap branch (the
  branch runs AFTER init in the same call — init's result=0 is always
  overwritten), µs = ticks/fDeviceFrequency, sanity = jump > 10 ms ×
  1000 → re-init. fDeviceFrequency (ALGORITHM_PARAMS 0x80) is in ticks
  per MICROSECOND (~60.0 = 60 MHz) — consistent with the 71.58 s wrap.
- **Dispatcher re-verification on the restored blob did NOT reproduce**
  the turn-15 claim (0x21400-0x22000 cmp chain): capstone at image
  offset 0x21400 decodes garbage on this copy; a full-region scan finds
  only 3-cmp runs at 0x20032. Not blocking (the hardware NACK evidence
  answers the opcode question directly), but the old dispatcher offsets
  in docs/FIRMWARE.md §5 may be file/alignment-specific — re-derive
  before citing them again.

## 4. 0.3.0 changes (commit 0ecdc56, pushed to main)

- `src/geometry.rs` (new): `CameraIntrinsics` (from_calibration, to_world,
  to_pixel) + 8 unit tests (FOV spec, closed form, resolution
  independence, optical axis, Y-up, roundtrip).
- `src/timestamp.rs` (new): `TimestampConverter` + 6 unit tests (zero
  point, 30 fps cadence, single wrap, ~150 wraps / 3 h, duplicate-ts
  resync, backwards clamp).
- Frames: `DepthFrame`, `ColorFrame`, `IrFrame` gained
  `timestamp_us: Option<u64>` (raw `timestamp` kept);
  `DepthFrame::to_world` / `point_cloud`.
- `src/device.rs`: `depth_intrinsics(res)`, `device_frequency()`,
  `firmware_log_entries()`, private `convert_timestamp` (lazy frequency
  retry), frequency read at open (tolerated optional), emitter NACK
  → contextual BadConfig.
- `src/protocol/system.rs`: log parser fixed to the C layout
  (`FirmwareLogEntry`, `read_firmware_log_entries`,
  `render_firmware_log`); `FlashFile::size_bytes`/`is_read_only`,
  `FILE_ATTRIBUTE_READ_ONLY`, `FILE_TABLE_OFFSET/SIZE`;
  `crc16_candidates` (ccitt-false/xmodem/kermit/arc/modbus/usb —
  pinned by catalog check values 0x29B1/0x31C3/0x2189/0xBB3D/0x4B37/
  0xB4C8; the usb check is MODBUS^0xFFFF, verified independently).
- `tests/hardware.rs`: system test extended (log raw+render, NACK
  trigger + re-read, device clock print, file list words/attrs, raw
  table @0x1000 stride scan, GainControl CRC dump, opcode 18/27/35
  notes); NEW `depth_geometry_fov_and_timestamps` (FOV gates 52-64°/
  38-52°, emitter-while-streaming probe, 90-frame cadence gate 28-38 ms
  ±5 ms jitter, optical-axis center, least-squares plane fit RMS < 20 mm
  — point at a flat wall 1-3 m).
- `examples/point_cloud.rs` (new): intrinsics printout, N-frame capture,
  ASCII PLY (metres, OpenNI axes, documented).
- Docs: PROTOCOL.md (intrinsics + timestamps + log layout + flash
  quirks sections), FEATURE_PLAN.md 0d/0e/0f + status, VALIDATION.md
  test rows 3/7 + new 8 + triage entry, FIRMWARE.md §4.1.1, README
  status rows + 0.3.0 changelog. Cargo 0.3.0.

## 5. Hardware test inventory (EIGHT tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green; emitter toggle prints as the
   documented NACK skip (0.3.0 rewords the error).
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic.
7. `system_commands_readonly` — green; 0.3.0 adds the ground-truth
   probes (log NACK-trigger re-read, raw table @0x1000, GainControl
   CRC candidates, device clock).
8. `depth_geometry_fov_and_timestamps` — NEW: intrinsics/FOV gates,
   30 fps timestamp cadence, plane fit RMS gate (needs a flat wall
   in view; RMS < 20 mm, good fits are a few mm).

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware -- depth_geometry --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v29.md` | This file (v29, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v29.zip` | Turn-29 zip (repo with .git at 0.3.0 + this backup). |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (12 pages; restored from upload/). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (historical, 0.1.5). |

The driver lives in the git repo `/home/z/my-project/xtion-rs/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = 0ecdc56 (0.3.0)**). If a future turn finds the tree missing,
re-clone with the stored token or restore this zip; the remote also
carries the `research/idong100*` branches to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 29)

- `/home/z/my-project/xtion-rs/xtion-rs/` — repo at 0.3.0, clean, pushed.
- OpenNI 2.2.0 reference: extract from
  `/home/z/my-project/firmware_dl/openni_ref/v2.2.0-debian.tar.gz` to
  /tmp/xtion-verify (`--strip-components=1`). NOTE: some files in this
  tarball have mangled identifier names (e.g. `#define n 10` for
  XN_SENSOR_TIMESTAMP_SANITY_DIFF, `m_ln(` for
  m_HorizontalFOV.UnsafeUpdateValue) — cross-check every quote against
  `git`-clean sources or the semantics before relying on names; the
  values and structure are trustworthy.
- Firmware blobs: `xtion-rs/firmware/5.8.22/*.fla` (restored with the
  tree). New sandbox scripts (survive under /home/z/my-project/scripts):
  `fw_dispatcher_scan.py` (cmp-chain scan — found the 0x21400 claim NOT
  reproducible on this copy), `fw_magic_find.py` (locates the u16 magic
  constants), `fw_strings.py` (UTF-16 string dump — found the CPU-task
  names + %lu log formats), `fw_fla_header.py` (.fla container dump —
  proved container type == flash file type).
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **XnLogEntryHeader field order: {u16 size, u32 ts, u16 type}** — the
  timestamp precedes the log type; XnLogDefaultData.nParam is a u32.
- **fDeviceFrequency is ticks per MICROSECOND (~60.0)** — the C divides
  ticks by it to get µs; wrap = 2^32/60 = 71.58 s. If a camera ever
  reports ~60e6, the C's math (and ours) would break — the hardware test
  prints the value to confirm (~60.0 expected).
- **TimestampConverter first frame**: the C runs the normal/wrap branch
  AFTER the init block in the same call (init's result=0 is always
  overwritten); anchor==ts==0 degenerates to a +71.6 s first result in
  the C (preserved in the port); duplicate ts → wrap branch → sanity
  trip → resync to ~0 (C behaviour).
- **XnFlashFile.size is WORDS**; attributes 0x8000 = read-only; the .fla
  container's first u16 IS the flash file type; GainControl on flash =
  the .fla minus its 6-byte header (114 B).
- **CRC-16/USB check = MODBUS ^ 0xFFFF** (0x4B37 → 0xB4C8) — the 0x906E
  value floating in my memory was wrong; always recompute check values
  (scripts do this in one line).
- The reference tarball's source files contain mangled identifiers
  (token-replacement corruption) — verify names, trust structure.
- The old dispatcher offsets (FIRMWARE.md §5, 0x21400-0x22000) do not
  reproduce on the restored blob; do not cite them without re-deriving.
- bImageSupported = `image_cmos_type != 0` (fw >= 5.6.9) — this unit
  has NO color sensor; the opcode-36 preset table is a platform list.
- Stream-mode params are a state machine: changes go through OFF; every
  C session ends with all streams OFF; the init soft reset does NOT
  clear stream modes.
- The wire fixed-params layout is +1 word vs the C struct; offsets
  hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- SET_PARAM retries 5x on timeout only, BadParams is final.
- GET_LOG entries: text 0x1200 (UTF-16, low byte), overflow 0x1201,
  structured = module<<8|error with {u16 line, u32 param}; the firmware
  embeds the timestamp in runtime texts as %lu too.
- READ_FLASH takes BYTE offsets; DOWNLOAD_FILE takes WORD offsets.
- Opcode 42 NACKs BadParams on this build (dispatched, payload refused);
  opcodes 18/27 NACK InvalidCommand; 35's direct reply is short —
  ALGORITHM_PARAMS 0x06 is the working blanking path.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker; partial first
  frames dropped; buf_size big-endian; high-speed USB required; split
  replies; v2.6 replies have no CRC; one garbage chunk per endpoint on
  fw < 5.0; bulk reads multiple of wMaxPacketSize.

## 9. Open threads (if the user continues)

- **Immediate:** run the 8-test suite on 0.3.0:
  `cargo test --test hardware -- --nocapture` — expected: all green,
  color + emitter sections printing their documented skips, and the new
  test 8 needing a flat wall 1-3 m in view. PASTE BACK from test 7: the
  raw log entries before/after the NACK trigger, the raw 0x1000 table
  hex + stride-scan counts, the GainControl CRC candidates + file-list
  GainControl/FileTable crc values, and the device-frequency line —
  that closes the flash-table parser (stride + record layout) and names
  the CRC algorithm.
- **Then (Phase 3):** cropping (PARAMs 46-60, bigger-crop-first
  protocol), depth write-read sweep, QVGA@60 mode matrix. Registration
  end-to-end only matters for RGB units (this unit: depth/IR only).
- **Crates.io 0.3.0** is the natural release point (geometry + timestamps
  are the photogrammetry API); remaining pre-release polish: README
  example section for point_cloud, docs.rs check (`cargo doc --open`).
- Phase 4 backlog: AGC bins 63-70, wavelength 73/74, GMC 26/0xa200,
  audio, COMPRESSED_PS depth (SXGA@30 needs it; ~78.6 MB/s uncompressed
  exceeds bulk), M6 robustness (keep-alive thread, hotplug, soak).
- Golden vectors: `cargo test --test golden`.
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token).
