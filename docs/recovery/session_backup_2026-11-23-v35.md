# Chat Session Backup — ASUS Xtion Linux research

**Version:** 35 (2026-11-23, turn 35): **0.3.6 — the apply-then-reject
AGC quirk pinned and fixed: readback-verified depth-processing writes —
pushed cf48110.** The 0.3.5 camera run (pasted in full, 11/12 green)
ended with `depth_processing_controls` panicking on "the probe must
leave the factory partition in place": bin0 read `0..512` after the
probe ladder. Root cause (two layers): (1) a FIRMWARE quirk — the
`grow bin0: HIGH <- 512` probe was NACKed `BadParams` yet the value
LANDED anyway (the run's own final partition proves it: the 512 could
only come from that write) — fw 5.8.22 **applies an invariant-
preserving write to params 63-70 and then rejects the reply**
(apply-then-reject), while an invariant-VIOLATING write (bin1 LOW <-
512 while its HIGH is 0) is rejected without applying; (2) a DRIVER
bug exposed by that quirk — `write_agc_bin_via` bailed out on the
first NACK, so the factory restore's LOW write error aborted the
function and the critical `HIGH <- 0` write never went out (the
restore errors were also silently discarded by the test). Fix: all
depth-processing setters are now **readback-verified** — both bounds
are sent transient-safely even under NACK, then read back; Ok means
the value reads back exactly as requested. Hardware test 11 measures
every probe against its immediate readback and hard-gates both
camera-verified quirk facts; close-range and wavelength refusals
became verdicts so no firmware refusal can redden the suite. 148 lib +
4 golden + 1 doc tests green, clippy clean. **State: waiting for the
on-camera rerun — expected 12/12; paste back test 11's AGC section
(readback lines + custom-partition cycle) and the close-range /
wavelength sections (first time they run at all).**

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
   commands — use targeted patterns or the Grep tool.
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
  (C-exact multi-pair SET_PARAM). Pushed 3cb58a1's parent c1a0e3b.
  Camera run: 10/11 — `depth_processing_controls` died on the AGC
  write cycle (NACK BadParams at the LOW-first grow order).
- **Turn 34 (0.3.5):** transient-safe AGC bin write order + probe
  ladder (never-panic design); GMC complete surface (config block,
  mode/debug setters, 0xa200/0xb200/0xb500 decode, `drain_gmc_packets`);
  preset resolution names. Pushed 3cb58a1. Camera run: 11/12 — the
  probe ladder ran (all four probes NACKed) but the partition assert
  fired: bin0 stuck at 0..512 (THIS turn's input).
- **Turn 35 (this turn, 0.3.6):** root-caused the 0.3.5 failure
  (apply-then-reject firmware quirk + the bail-out driver bug),
  reworked all depth-processing writes to readback-verified semantics,
  fixed the test to measure each probe against its immediate readback
  and to treat every firmware refusal as a verdict. Pushed cf48110.
  Also: the sandbox was recycled this turn — repo restored from the
  v33/v34 zips + remote, Rust toolchain reinstalled (see section 7).

## 3. Turn-35 findings (all evidence- or source-verified this turn)

- **The apply-then-reject quirk (the load-bearing discovery).** The
  0.3.5 camera run's failing assert IS the proof: the final partition
  `[bin0 = (0, 512), bins1-3 = (0,0)]` could only contain 512 if the
  NACKed `param 64 <- 512` probe landed (nothing else wrote 512; the
  restore writes zeros). Therefore on fw 5.8.22: SET_PARAM on the AGC
  bin params (63-70) **replies NACK BadParams while applying the value
  when the write preserves the per-bin `min <= max` invariant**
  (camera-observed for `HIGH <- 512` with LOW = 0), and **rejects
  without applying when the write violates the invariant**
  (camera-observed: `bin1 LOW <- 512` with HIGH = 0 left bin1 at
  (0,0)). The NACK's underlying reason is unknown (suspect a
  post-apply state check — e.g. stream state — building the error
  reply after the param store is updated); operationally ONLY the
  readback is trustworthy.
- **The 0.3.5 suite failure was the driver's bail-out, not the
  firmware's refusal.** `write_agc_bin_via` propagated the first
  NACK with `?` (v0.3.5 code): the factory restore's first write
  (param 63 <- 0, LOW-first branch) NACKed → the function returned
  Err → the `HIGH <- 0` write was never SENT → bin0 stayed at (0,512)
  → the partition assert fired. The test also discarded the restore
  error silently (`let _ =`), hiding the bail-out.
- **Lesson (now encoded in the API docs and test design): a NACK
  reply on this build cannot be treated as "not applied" — every
  depth-processing write is verified by readback, and test probes
  measure the readback immediately after each write.**
- **The probe ladder's four REJECTED lines all NACKed while the
  third one's value landed** — so "REJECTED" prints are the REPLY
  truth, and the readback lines (new in 0.3.6) are the STATE truth;
  both are printed per probe.
- GMC (0.3.5 run, test 12 passed): switches 36/61 write+ACK cleanly
  (no quirk); the misc endpoint carries 0xb200 debug points (376
  packets / 376 points over 20 frames) and NO plain 0xa200 on a
  static scene — verdict printed, not gated.
- The 0.3.5 run also confirmed: emitter toggle ACKs while depth
  streams; crop mode 2 rejected in every form (verdict); the
  geometry/timestamp/plane gates all green; close range + wavelength
  sections NEVER RAN (the panic preceded them) — their first real
  camera exercise is the NEXT run (now refusal-safe).

## 4. 0.3.6 changes (commit cf48110, pushed to main)

- `src/device.rs`:
  - `write_agc_bin_via` reworked: transient-safe bound order kept
    (raise HIGH first when `new.min > current.max`, else lower LOW
    first), BOTH bounds are sent even when a reply is a NACK (only
    non-NACK errors abort), then the bin is read back — Ok iff the
    readback equals the request, else `BadConfig` naming sent-vs-
    read-back values. Doc comments fully rewritten around the quirk.
  - NEW `set_param_verified_via` (transport-generic): single-param
    write with the same NACK-tolerant + readback-verified contract.
  - `set_close_range` (84), `set_wavelength_correction` (73),
    `set_wavelength_correction_debug` (74) now go through it.
    `set_depth_agc_bin` docs updated (Ok = verified).
  - NEW unit tests: `agc_bin_write_lands_despite_nack` (models the
    exact camera quirk: both writes NACK + both land → Ok, both
    bounds sent), `agc_bin_write_truly_refused_is_an_error` (NACK +
    no application → error naming both values),
    `set_param_verified_decides_by_readback` (three arms: NACK+land
    → Ok, NACK+not-land → Err, ACK+echo → Ok). The two existing
    order tests gained their verification GET replies; NEW reply
    builder `setparam_nack_bad_params_reply`.
- `tests/hardware.rs` (test 11 `depth_processing_controls`):
  - `probe_readback` helper — every probe is immediately measured:
    the landed NACK (grow → bin0 `0..512`) and the rejected invariant
    violation (C-order → bin1 stays `(0,0)`) are now HARD-GATED
    camera-verified facts with explanatory messages.
  - Factory restore runs through the verified API with restore
    errors PRINTED (not discarded) before the partition assert.
  - The custom-partition cycle runs UNCONDITIONALLY (its own Result
    decides: Ok → echo/frame/restore asserts; Err → VERDICT line);
    a mid-cycle write failure still restores before propagating.
  - Close-range and wavelength sections: refusals are VERDICT
    prints (verified setters leave the device untouched); the
    wavelength closure restores BOTH switches even on mid-cycle
    refusal. No firmware refusal can redden the suite any more.
- Docs: PROTOCOL.md AGC findings rewritten (both camera runs; the
  readback contract) + wavelength/close-range setter notes; README
  0.3.6 changelog + status row (148 lib tests); VALIDATION.md test 11
  row + 153 total checks; FEATURE_PLAN rows 22/23 + section-5 item
  11 updated. Cargo 0.3.6.
- Note: `src/protocol/presets.rs` + `src/stream.rs` carry a few
  lines whose formatting disagrees between rustfmt versions (the
  sandbox toolchain was reinstalled — 1.100.0 — vs. whatever
  formatted 0.3.5); left untouched to keep the diff focused. My
  files are fmt-clean under 1.100.

## 5. Hardware test inventory (TWELVE tests, tests/hardware.rs)

1. `open_handshake_and_identity` — green (camera-validated).
2. `depth_stream_roundtrip` — green (camera-validated).
3. `phase1_identity_and_options` — green.
4. `ir_stream_roundtrip` — green (camera-validated).
5. `color_stream_roundtrip` — SKIPS on this unit (no color sensor).
6. `color_start_probe` — diagnostic (start_color gate).
7. `system_commands_readonly` — camera-validated 0.3.2 (22-record
   table, 20/20 CRCs on device, DOWNLOAD_FILE mapping).
8. `depth_geometry_fov_and_timestamps` — camera-validated (RANSAC
   plane gate; 92% coverage, 3.05 mm RMS).
9. `plane_fit_synthetic` — camera-independent RANSAC regression.
10. `depth_and_ir_crop_windows` — camera-validated 0.3.3/0.3.4 (mode-1
    crop works, mode-2 rejected in every form = verdict).
11. `depth_processing_controls` — 0.3.6 rework, awaiting camera run:
    gain cycle + AGC probe ladder with per-probe readbacks (two hard
    quirk gates) + verified restore + custom-partition cycle
    (verdict on true refusal) + close-range + wavelength (verdict on
    refusal) + final recovery gate.
12. `gmc_config_and_motion_data` — camera-validated 0.3.5 (config
    block + switches + 0xb200 census; 0xa200 silent on a static
    scene = verdict).

Run all: `RUST_BACKTRACE=full cargo test --test hardware -- --nocapture`
Run one: `cargo test --test hardware depth_processing -- --nocapture`

## 6. Deliverables (all under /home/z/my-project/download/)

| File | What it is |
|---|---|
| `session_backup_2026-11-23-v35.md` | This file (v35, CURRENT). |
| `asus_xtion_workspace_backup_2026-11-23_v35.zip` | Turn-35 zip (repo with .git at 0.3.6 incl. this backup). |
| `xtion_rs_driver_overview.pdf` | Rev 3 driver overview (0.3.5, historical; the older PDFs were lost in the sandbox recycle). |

The driver lives in the git repo `/home/z/my-project/xtion-rs`
(remote origin = https://github.com/thomas7475/xtion-rs.git, branch
main, **HEAD = cf48110 (0.3.6)**; also committed at
docs/recovery/session_backup_2026-11-23-v35.md). If a future turn
finds the tree missing, re-clone with the stored token or restore this
zip; the remote also carries the `research/idong100*` branches to
IGNORE (rule 6).

## 7. Workspace state (2026-11-23, after turn 35)

- `/home/z/my-project/xtion-rs/` — repo at 0.3.6, clean, pushed.
- **The sandbox was RECYCLED before this turn** (repo, toolchain and
  download/ artifacts were gone). Recovery procedure that worked
  (reuse if it happens again):
  1. Repo: extract the newest full zip (v33: repo + .git at 0.3.4),
     overlay the newest light zip (v34: working tree at 0.3.5, no
     .git), then `git fetch origin main` + `git reset --hard
     origin/main` (the remote had 0.3.5 + the v34 backup commit
     6320c47).
  2. Rust toolchain: `RUSTUP_HOME=/home/z/my-project/.rustup
     CARGO_HOME=/home/z/my-project/.cargo`; install rustup-init by
     direct download (piping `curl | sh` is blocked): `curl -sSfL
     https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
     -o /tmp/rustup-init && chmod +x && /tmp/rustup-init -y
     --default-toolchain stable --profile minimal --no-modify-path`
     (got 1.100.0) + `rustup component add clippy rustfmt`.
  3. libusb dev (no root in the sandbox): `cd /tmp && apt-get
     download libusb-1.0-0-dev libusb-1.0-0 && dpkg -x <dev.deb>
     /tmp/libusb-dev && dpkg -x <runtime.deb> /tmp/libusb-dev`, then
     sed the extracted `libusb-1.0.pc` `libdir`/`includedir` to the
     /tmp paths, and build with `PKG_CONFIG_PATH=/tmp/libusb-dev/
     usr/lib/x86_64-linux-gnu/pkgconfig RUSTFLAGS="-L /tmp/libusb-dev/
     usr/lib/x86_64-linux-gnu"`.
- OpenNI 2.2.0 reference: LOST with the recycle (was under
  firmware_dl/) — re-extract from the `FWUpdate` mirrors or
  re-download if a turn needs to cite it again; the key facts are
  baked into docs/PROTOCOL.md with citations.
- Firmware blobs: intact at `xtion-rs/firmware/5.8.22/*.fla` +
  `UsbUpdate_RD108.ini` (they ride inside the repo/zips).
- Sandbox scripts: `/home/z/my-project/scripts/fw_*.py` +
  `driver_*.py` (restored from the v34 zip).
- The user clones to `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`.

## 8. Gotchas learned (cumulative, new first)

- **fw 5.8.22 apply-then-reject (params 63-70, camera-proven):** a
  SET_PARAM can be APPLIED while the reply is NACK BadParams. The
  invariant-preserving write lands; the invariant-violating one does
  not. NEVER infer "not applied" from a NACK on this build — verify
  by readback. `write_agc_bin_via` and `set_param_verified_via`
  encode this; the public setters return Ok only when verified.
- **Multi-write sequences must not bail between steps on a NACK** —
  the 0.3.5 failure was exactly that (restore's HIGH write never
  sent). Send all writes (tolerating NACKs), then verify once.
- **Test hygiene: restore errors are evidence** — discard them
  silently and a bail-out hides (the 0.3.5 `let _ =` line). Print,
  then assert the final state.
- rustfmt version skew: 1.100.0 vs. the pre-recycle toolchain
  formats a few constructs differently (presets.rs/stream.rs are
  pre-existing examples); do not "fix" untouched files with a newer
  fmt — it pollutes the diff and ping-pongs across machines.
- The sandbox has NO root: system packages can't be installed; use
  `apt-get download` + `dpkg -x` into /tmp + PKG_CONFIG_PATH +
  RUSTFLAGS (recipe in section 7). `ln -s` is blocked; `dpkg -x`
  materializes deb-internal symlinks fine.
- Emitter opcode 42: BadParams while IDLE, ACK while depth streams.
- Crop: while mode != 0 the firmware transmits ONLY the window (any
  mode); IR has no +8 CMOS rows while cropped; geometry pairs first,
  MODE last, one multi-pair batch; mode 2 NACKed in every form on this
  build.
- FileTable record 0's crc 0xfffc does not self-verify (expected
  MISMATCH); every other stored crc verified on device (0.3.2 run).
- DOWNLOAD_FILE on fw 5.8.22 serves wrong files for some types
  (0x12→RegistrationParams, 0x13→ProductionFile) — same packed/
  unpacked table-walk bug as GET_FILE_LIST. Ground truth = READ_FLASH.
- All flash addresses are 16-bit WORDS (byte = ×2); `XnFlashFile`
  is 22 B packed (fw walks unpacked → opcode-19 rows drift +2 B per
  record); flash CRC = CRC-16/CCITT-FALSE over big-endian words.
- Plane-fit gate: RANSAC inlier RMS is bounded by the inlier band by
  construction — coverage is the discriminator; 15 mm RMS gate once
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

- **Immediate:** rerun the suite on 0.3.6:
  `RUST_BACKTRACE=full cargo test --test hardware -- --nocapture` —
  expected **12/12**. PASTE BACK test 11's full section: (a) the AGC
  probe readbacks (grow write should land 0..512 despite the NACK;
  C-order should stay (0,0)), (b) the custom-partition cycle line
  (echo exact + valid-pixel count under the 4-zone partition + bins
  restored), (c) the close-range section (did the write land? what
  did the near-field frame look like — valid pixels, min depth), (d)
  the wavelength section (73/74 landed or verdict). These decide the
  0.3.7 follow-ups: if close-range/wavelength write cleanly, add
  mm-level recipe notes (e.g. the reference's close-range gain
  stepping) to the docs.
- **If the restore assert fails** ("the probe must leave the factory
  partition in place"): the firmware refused the zero-write restore —
  power-cycle the device (runtime params do not persist) and paste
  the restore warning line; the readback-verified restore path will
  need a rethink (likely: accept the unrestorable bin and document).
- **Crates.io 0.3.6** is the natural release point (flash ground
  truth + crop + geometry + depth-processing writes + GMC); polish:
  README example for crop + point_cloud, docs.rs check.
- Phase 4 backlog: QVGA@60 mode matrix (trivially combinable with
  crop), ALGORITHM 0x80 frequency WRITE (multi-camera interference),
  audio, COMPRESSED_PS depth (SXGA@30 needs it), M6 robustness
  (keep-alive thread, hotplug, soak), registration end-to-end
  (RGB units only).
- Golden vectors: `cargo test --test golden` (flash table + CRC
  vectors included).
- Pushing again: recreate /tmp/gh-creds from /home/z/my-project/
  .gh-token (never print or commit the token); fetch + rebase first
  (rule 5).
- SECURITY: rotate the four pasted PATs when this work concludes.
