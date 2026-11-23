# Chat Session Backup — ASUS Xtion Linux research

**Version:** 31 (2026-11-23, turn 31): **0.3.2 — flash file table
decoded, per-file CRC identified, READ_FLASH chunking fixed — PUSHED
cf56f7a.** The 0.3.1 camera run (9/9 green) supplied the last missing
ground truth, and the whole flash story closed OFFLINE this turn: (1)
`XnFlashFile` is pinned from the vendor header (Include/PS1080.h:600-609,
`#pragma pack(1)`): a **packed 22-byte** record — the on-flash table at
**word 0x1000 (byte 0x2000)** is a flat array of them, record 0
describing the table itself with the magic 0x12345678 in `version`;
(2) **every flash address on this sensor is a 16-bit WORD** — READ_FLASH
and DOWNLOAD_FILE wire offsets, the table records and the updater's
`UsbUpdate_RD108.ini` `OffsetFactory` values all agree (vendor console:
`nSizeInBytes = nSize * sizeof(XnUInt16)`, PS1080Console.cpp:1164), which
resolves the flash map with zero overlaps (FileTable bytes 0x2000-0x4000,
BootManager at 0x4000, Application 0x20000-0x458A2, ScratchFile = the
last 64 KiB at 0x1F0000); (3) the fw 5.8.22 GET_FILE_LIST "garbage tail"
is a **packed/unpacked struct mismatch** — the firmware walks the packed
22-byte records with its unpacked 24-byte layout, drifting +2 B per row
(every 11th row, lcm(22,24)=264, is a true record), and DOWNLOAD_FILE's
type lookup suffers the same walk: type 0x12 returned the 548-byte
**RegistrationParams** image instead of GainControl while type 0x0A
returned the correct ImageCmos image; (4) the per-file CRC is
**CRC-16/CCITT-FALSE over the payload words, each 16-bit word fed
most-significant byte first** — identified 8/8 against the factory image
bundled in `firmware/5.8.22` (payload = `.fla` minus its 6-byte
[type][build][version] header): GainControl 0xC44B, FixedParams 0x233D,
AlgorithmParams 0xA7F1, DefaultParams 0xE4EC, PrimeProcessor 0x9A35,
ImageCmos 0x1CFA, BootManager 0x9F64, Application 0xDF96; (5) a REAL bug
fixed: `read_flash` advanced its wire offset by BYTES instead of words —
reads > 1024 B silently skipped every second 1 KiB block (the 0.3.1
"4096-byte" table dump was four gapped blocks; the records all lived in
the first one, which is why the decode still worked). New API:
`parse_flash_file_table`, `read_file_table`, `Xtion::flash_file_table`,
`flash_file_crc`, `Xtion::verify_flash_file`; `crc16_candidates`
(identification tooling) removed. Golden tests: the parser against the
real 256-B device capture (11 records) + the CRC against the factory
payloads (8/8). Hardware test 7 now asserts record-0 identity (>= 12
records), verifies every stored crc on-device (GainControl + ImageCmos
are hard gates) and maps the DOWNLOAD_FILE type lookups. 126 lib + 4
golden + 1 doc tests green, clippy 0, fmt clean. **State: waiting for
the on-camera rerun of `cargo test --test hardware -- --nocapture` —
expected 9/9 with the new flash sections all-OK; paste back the parsed
table rows, the CRC verification lines and the DOWNLOAD_FILE mapping.**

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
   NOTE: the user pasted a fresh PAT in the turn-31 chat (stored, works);
   recommend rotating both pasted tokens eventually.
   **Fetch/rebase before pushing** — the user sometimes pushes their own
   commits (8935057 appeared between 0.3.0 and 0.3.1).
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
  wrap-aware host timestamps (`timestamp_us`), diagnostics-parse fixes
  (log header field order, flash-list words/attrs documentation, emitter
  NACK context). Pushed 0ecdc56.
- **Turn 30 (0.3.1):** scene-robust plane-fit gate (RANSAC dominant
  plane + PCA refit + conditional 15 mm RMS gate) + `plane_fit_synthetic`
  regression test; documented the 0.3.0 run's probe answers. Pushed
  a717d85. The 0.3.1 camera run came back **9/9 green** (dominant plane
  92% coverage, 3.04 mm inlier RMS on the user's room).
- **Turn 31 (this turn, 0.3.2):** the 9/9 run's pasted output (full
  GainControl blob + raw table head + opcode-19 rows) closed the flash
  story offline: `XnFlashFile` pinned from PS1080.h, word addressing
  resolved, the packed/unpacked firmware bug explained, the CRC
  identified 8/8 against the factory image, the read_flash chunking bug
  fixed. Pushed cf56f7a.

## 3. Turn-31 findings (all source- or data-verified this turn)

- **`XnFlashFile` (Include/PS1080.h:600-609, inside `#pragma pack(1)`)**
  is **22 bytes packed**: `{u16 id, u16 type, u32 version, u32 offset,
  u32 size, u16 crc, u16 attributes, u16 reserve}` — the natural
  (unpacked) host layout is 24. The on-flash table = flat array of the
  packed records; the FIRMWARE walks it with the unpacked 24-byte
  stride. Decoded records (ids 0-13 from the capture + the realigned
  opcode-19 row 12 + the stride-26 hit at 0x11E): id 0 FileTable
  (magic 0x12345678, word 0x1000, 0x1000 w, crc 0xfffc), 1 ScratchFile
  (word 0xF8000, 64 KiB = the last 64 KiB of the 2 MiB part, no crc),
  2 BootSector (word 0xF800, 39 w, 0x4639), 3 Monitor (0x200, 597 w,
  0x8571), 4 CodeDownloader (0xF000, 48 w, 0x482C), 5 BootManager
  (0x2000, 11604 w, 0x9F64), 6 FixedParams (0x6700, 326 w, 0x233D),
  7 AlgorithmParams (0x950, 387 w, 0xA7F1), 8 DefaultParams (0x6600,
  8 w, 0xE4EC), 9 PrimeProcessor (0x6000, 555 w, 0x9A35), 10 ImageCmos
  (0x6B00, 1136 w, 0x1CFA), 11 DepthCmos (0x7D00, 136 w), 12 GainControl
  (0x550, 57 w, 0xC44B), 13 Application (0x10000, 0x12C31 w, 0xDF96).
- **All flash offsets are 16-bit WORDS.** Evidence: the vendor console's
  ReadFlash (`PS1080Console.cpp:1142-1176`) multiplies the size by
  `sizeof(XnUInt16)`; every decoded record offset equals the updater
  ini's `OffsetFactory` value; the upgrade staging area
  `[0x25000, 0x3F9C0]` only avoids the Application image under the word
  interpretation; the FileTable (word 0x1000, 8 KiB) ends exactly where
  BootManager (word 0x2000) begins. The byte-addressed map (×2):
  Monitor 0x400, GainControl 0xAA0, AlgorithmParams 0x12A0, FileTable
  0x2000-0x3FFF, BootManager 0x4000, PrimeProcessor 0xC000,
  DefaultParams 0xCC00, FixedParams 0xCE00, ImageCmos 0xD600, DepthCmos
  0xFA00, CodeDownloader 0x1E000, BootSector/Scratch 0x1F000/0x1F0000,
  Application 0x20000-0x458A2.
- **The mystery 548-byte "GainControl" download = RegistrationParams.**
  Its head `[13 00 22 00 08 05 ...]` is a `.fla` header with type 0x13
  (RegistrationParams), and the local `RegistrationParams.xml.bin.fla`
  is 548 B matching the blob except **35 per-device calibration bytes**
  around 0x1A8-0x1F1 (registration data is device-specific — the
  `SensorSpecific=1` flag in the updater ini). So DOWNLOAD_FILE replies
  are the file's `.fla` image ([type][build][version] + payload), and
  the fw's type-0x12 lookup served the wrong file — same corrupted-table
  walk as GET_FILE_LIST.
- **The flash CRC = CRC-16/CCITT-FALSE over the payload words fed
  most-significant byte first** (= CCITT-FALSE of the big-endian u16
  serialization; flash stores LE words). Identified by the standard
  catalogue sweep (scripts/flash_crc_sweep.py) — the hit was
  `ccitt-false over content[6:] word-swapped`, 8/8 files. This is why
  the 0.3.1 candidates over the raw blob failed: wrong file AND wrong
  byte order AND wrong region.
- **The `.fla` inventory matches the table exactly** (all sizes =
  table words × 2 + 6): BootManager 23214, FixedParamsRD108 658,
  AlgorithmParams 780, DefaultParams 22 (OvenMode 38 is the factory
  variant, not on flash), PrimeProcessor 1116, ImageCmos 2278,
  DepthCmos 278, GainControl 120, source.out 153704 — and the type
  enum from PS1080.h names everything (0x0D ReferenceQVGA, 0x0E
  ReferenceVGA, 0x13 RegistrationParams, 0x14 IDParams, 0x15
  SensorTECParams, 0x17 SensorProjectorFaultParams, 0x19
  UpgradeInProgress, 0x1C SensorNESAParams ...).
- **`read_flash` chunking bug (0.3.1 and earlier):** the wire offset
  advanced by BYTES (`loop_offset += data.len()`) while the device
  addresses words — every read > 1024 B returned gapped blocks (chunk =
  512 words = 1024 B). All camera reads ≤ 1024 B were unaffected, and
  the 0.3.1 table dump survived because the records live in the first
  1024 B of the region.
- **The 0.3.1 run's plane-fit numbers** (user's room): dominant plane
  normal (-0.665, -0.011, -0.747), tilt 41.7 deg, distance 570 mm,
  243700 inliers = 92% of the cloud, inlier RMS 3.04 mm — the RANSAC
  gate design (coverage as the discriminator) worked exactly as
  intended on a tilted two-plane scene.

## 4. 0.3.2 changes (commit cf56f7a, pushed to main)

- `src/protocol/system.rs`:
  - `read_flash(offset_words, size_bytes)` — word-offset semantics
    documented, chunk loop advances by WORDS (the fix), odd trailing
    byte dropped.
  - NEW `parse_flash_file_table(&[u8]) -> Vec<FlashFile>` (22-byte
    packed records from offset 0; stops at all-0x00/all-0xFF or
    implausible fields: id <= 0x0FFF, type <= 0xFF, offset <= 4 MiB in
    words, size <= 1 Mi words) + `read_file_table()` (reads
    `FILE_TABLE_SIZE_WORDS` at `FILE_TABLE_OFFSET_WORDS`, parses).
  - Constants renamed to explicit units: `FILE_TABLE_OFFSET_WORDS`
    (0x1000), `FILE_TABLE_SIZE_WORDS` (0x1000), NEW
    `FILE_TYPE_FILE_TABLE`, `FLASH_FILE_RECORD_SIZE` (22); `FlashFile`
    gained `offset_bytes()`; docs corrected (packed 22 vs unpacked 24,
    word offsets everywhere, the fw drift bug on `get_file_list` /
    `download_file`).
  - CRC: `crc16_candidates` REMOVED; NEW private `crc16_ccitt_false`
    (catalog check 0x29B1 pinned) + public `flash_file_crc` (per-word
    big-endian feed). New unit tests: engine check value, word-order
    equivalence, parser terminator/plausibility/decode paths.
- `src/device.rs`: `read_flash` doc/params updated; NEW
  `flash_file_table()` and `verify_flash_file(&FlashFile) -> bool`
  (reads the payload at the record's word offset, checks the crc).
- `tests/golden.rs`: NEW `flash_file_table_parses_device_capture` (the
  real 256-B capture from the 0.3.1 run, 11 records, exact fields) and
  `flash_file_crc_matches_factory_image` (8 `.fla` payloads vs the
  stored crcs + size cross-checks; skips gracefully if the firmware
  dir is absent).
- `tests/hardware.rs` (test 7): opcode-19 listing kept with the drift
  note; NEW ground-truth section (`flash_file_table`, record-0 identity
  assert + >= 12 records assert); NEW on-device CRC verification loop
  over every record with a stored crc (GainControl 0xc44b and
  ImageCmos 0x1cfa are hard asserts); NEW DOWNLOAD_FILE type-lookup
  mapping (0x0A, 0x0B, 0x12, 0x13 — head type/build/ver + length);
  removed the old stride scans, the GainControl full dump and the
  candidates printout.
- Docs: PROTOCOL.md flash section rewritten (word addressing, 22-byte
  table, fw bug, CRC algorithm, read_flash fix); VALIDATION.md test 7
  row + golden note; FEATURE_PLAN.md flash item CLOSED + emitter
  idle-state wording; README status row + 0.3.2 changelog + test count
  (126 lib). Cargo 0.3.2.

## 5. Hardware test inventory (NINE tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green; idle emitter toggle prints
   the documented NACK skip (ACKs while streaming — see test 8).
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic.
7. `system_commands_readonly` — green; 0.3.2: ground-truth table parse
   (record-0 + count asserts), on-device CRC verification (hard gates:
   GainControl, ImageCmos), DOWNLOAD_FILE mapping probes.
8. `depth_geometry_fov_and_timestamps` — camera-validated 0.3.1
   (conditional RANSAC plane gate; the user's room: 92% coverage,
   3.04 mm RMS).
9. `plane_fit_synthetic` — camera-independent RANSAC regression.

Run all: `cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware system_commands -- --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v31.md` | This file (v31, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v31.zip` | Turn-31 zip (repo with .git at 0.3.2 + this backup). |
| `xtion_rs_firmware_analysis.pdf` | Rev 2 firmware report (12 pages; historical). |
| `xtion_rs_driver_overview.pdf` | Turn-13 driver overview (historical, 0.1.5). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch main,
**HEAD = cf56f7a (0.3.2)**; also committed at
docs/recovery/session_backup_2026-11-23-v31.md). If a future turn finds
the tree missing, re-clone with the stored token or restore this zip; the
remote also carries the `research/idong100*` branches to IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 31)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.2, clean, pushed.
- OpenNI 2.2.0 reference: ALREADY EXTRACTED at
  `/home/z/my-project/firmware_dl/openni_ref/OpenNI2-2.2.0-debian/`
  (Include/PS1080.h = the flash structs; Sensor/XnHostProtocol.cpp;
  PS1080Console/PS1080Console.cpp). NOTE: some files have mangled
  identifier names (token-replacement corruption) — verify names,
  trust structure.
- Firmware blobs: `xtion-rs/firmware/5.8.22/*.fla` (16 files) +
  `UsbUpdate_RD108.ini` (the vendor flash map — `OffsetFactory` values
  = the table record offsets) + `FWUpdate_5_8_22.zip`.
- Sandbox scripts (survive under /home/z/my-project/scripts):
  `flash_crc_sweep.py` (NEW this turn — the CRC identification sweep +
  blob-vs-.fla diff), `flash_fla_inventory.py` (NEW — .fla inventory vs
  decoded records), plus the earlier fw_*.py analysis scripts.
- Rust toolchain: `.cargo/` + `.rustup/` in my-project root (use
  `export RUSTUP_HOME=/home/z/my-project/.rustup
  CARGO_HOME=/home/z/my-project/.cargo
  PATH=/home/z/my-project/.cargo/bin:$PATH`). Verified working this turn.
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **All flash addresses on this sensor are 16-bit WORDS** (byte = ×2):
  READ_FLASH wire offsets AND chunk sizes, DOWNLOAD_FILE offsets,
  `XnFlashFile.offset/size`, the updater ini. READ_FLASH chunk loop must
  advance the offset by WORDS RECEIVED, not bytes.
- **`XnFlashFile` is 22 bytes PACKED, 24 unpacked** — fw 5.8.22 walks
  the on-flash table with the unpacked stride: GET_FILE_LIST rows drift
  +2 B per record (every 11th row real, lcm(22,24)=264) and
  DOWNLOAD_FILE can serve the wrong file (0x12 → RegistrationParams
  instead of GainControl; 0x0A worked). Ground truth = READ_FLASH the
  table + parse the packed records.
- **Flash CRC = CRC-16/CCITT-FALSE over the payload words fed
  big-endian** (per-word MSB first). The 6-B `.fla` header is NOT part
  of the CRC'd payload; RegistrationParams carries per-device bytes
  (35 around 0x1A8-0x1F1) so only build-generic files match the local
  mirror byte-for-byte.
- **DOWNLOAD_FILE replies = the file's `.fla` image**: 6-byte
  `[type u16][build u16][version u16]` + `size` words of payload.
- The fw's file-table self-entry (record 0) carries the magic
  0x12345678 in the `version` field; regular files carry 0x05080022
  (fw 5.8, build 34).
- CRC-16 catalogue check values from memory are unreliable — USB =
  MODBUS^0xFFFF (0xB4C8), MAXIM = ARC^0xFFFF (0x44C2), GENIBUS =
  CCITT-FALSE^0xFFFF (0xD64E) are safe by construction; assert only
  those and recompute the rest.
- Plane-fit gate design (0.3.1): RANSAC inlier RMS is bounded by the
  inlier band BY CONSTRUCTION — coverage is the discriminator; the
  15 mm RMS gate is the quality check once coverage >= 40%.
- Emitter opcode 42: BadParams while IDLE, ACK while depth streams.
- fw 5.8.22 does not log NACKs — GET_LOG returns boot history only.
- XnLogEntryHeader field order: {u16 size, u32 ts, u16 type}; the
  structured-entry param is a u32.
- fDeviceFrequency is ticks per MICROSECOND (~60.0); wrap = 71.58 s.
- TimestampConverter first frame: the C runs the wrap branch AFTER
  init in the same call; anchor==ts==0 degenerates to +71.6 s
  (preserved); duplicate ts -> resync to ~0.
- The .fla container's first u16 IS the flash file type; on-flash
  payload = .fla minus the 6-byte header; `size` counts WORDS;
  attributes 0x8000 = read-only.
- The reference tarball's sources contain mangled identifiers —
  verify names, trust structure.
- The old dispatcher offsets (FIRMWARE.md §5, 0x21400-0x22000) do not
  reproduce on the restored blob; do not cite without re-deriving.
- bImageSupported = `image_cmos_type != 0` — this unit has NO color
  sensor; the opcode-36 preset table is a platform list.
- Stream-mode params are a state machine: changes go through OFF;
  every C session ends with all streams OFF; the init soft reset does
  NOT clear stream modes.
- Wire fixed-params layout is +1 word vs the C struct; offsets
  hardware-verified: serial@0, depth cmos@56, image cmos@76,
  geometry@92..108, pll@108.
- SET_PARAM retries 5x on timeout only, BadParams is final.
- GET_LOG entries: text 0x1200 (UTF-16, low byte), overflow 0x1201,
  structured = module<<8|error with {u16 line, u32 param}.
- Opcodes 18/27 NACK InvalidCommand; 35's direct reply is short —
  ALGORITHM_PARAMS 0x06 is the working blanking path.
- XN_SHIFTS_MAX_SHIFT = 2047; 2047 = no-depth marker; partial first
  frames dropped; buf_size big-endian; high-speed USB required; split
  replies; v2.6 replies have no CRC; one garbage chunk per endpoint on
  fw < 5.0; bulk reads multiple of wMaxPacketSize.
- Git: fetch + rebase before push; Cargo.lock gitignored.

## 9. Open threads (if the user continues)

- **Immediate:** rerun the suite on 0.3.2:
  `cargo test --test hardware -- --nocapture` — expected **9/9 green**
  with the new flash sections. PASTE BACK (a) the parsed on-flash
  table rows (ids + types + offsets + crcs — the region past id 13 has
  never been seen), (b) the on-device CRC verification lines (any
  MISMATCH matters), (c) the DOWNLOAD_FILE mapping (which types the fw
  serves correctly). That confirms the parser + CRC + chunking fix on
  hardware and completes the flash map (ids 14+ = the per-device
  sensor files: RegistrationParams, IDParams, SensorTecParams,
  SensorNesaParams, SensorSafetyParams, UpgradeInProgress...).
- **Then (Phase 3):** cropping (PARAMs 46-60, bigger-crop-first
  protocol), depth write-read sweep, QVGA@60 mode matrix. Registration
  end-to-end only matters for RGB units (this unit: depth/IR only).
- **Crates.io 0.3.2** is the natural release point (flash ground truth
  + CRC + geometry + timestamps); remaining pre-release polish: README
  example section for point_cloud, docs.rs check.
- Phase 4 backlog: AGC bins 63-70, wavelength 73/74, GMC 26/0xa200,
  audio, COMPRESSED_PS depth (SXGA@30 needs it; ~78.6 MB/s uncompressed
  exceeds bulk), M6 robustness (keep-alive thread, hotplug, soak).
- Golden vectors: `cargo test --test golden` (now includes the flash
  table + CRC vectors).
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/.gh-token
  (never print or commit the token); fetch + rebase first (rule 5).
