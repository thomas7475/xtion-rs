# Session Backup v38 — xtion-rs restart ladder (health / stuck / reset)

**Date:** 2026-11-23
**Repo:** https://github.com/thomas7475/xtion-rs (PAT-gated, pushed to `main` as `6330165`)
**Working copy:** `/home/z/my-project/xtion-rs` (clean tree, PAT scrubbed from `.git/config`)

## Standing rules (apply every session)

1. All communication in English only.
2. Zip backup of all generated files -> let the user download it.
3. Recurring context/state backup -> let the user download it. Every time.

## Where the work stands

### Version history

- 0.3.5/0.3.6/0.3.7: GMC, AGC quirk, debug telemetry, mode matrix (earlier sessions)
- 0.3.8: `start_depth_verified` + FrameTimeout + ASUS firmware-history research
- **0.3.9 (this session): restart ladder — health/stuck probes + soft reset + USB port reset**

### The 0.3.8 camera run's failures (user-pasted, analyzed this session)

- `depth_processing_controls`, `gmc_config_and_motion_data`,
  `depth_mode_matrix_and_high_fps` -> `BadConfig("depth stream stayed silent
  through 3 start attempts (fw 5.8.22 dead-stream quirk; power-cycle …)")`
- `depth_geometry_fov_and_timestamps` -> panic "timestamp jitter 296.95 ms is
  too large for a wired bus" (mean 36.74 ms OK; ONE outlier ~330 ms = 10-frame gap)
- `ir_stream_roundtrip`, `open_handshake_and_identity` -> "running over 60
  seconds" = blocked on DEVICE_LOCK waiting for earlier tests, then died on
  dead-stream / were fine once lock freed. Lock works; do not "fix" it.
- Root cause chain: refused SXGA/480P probes in the mode walk poison the next
  starts (documented correlation) -> dead-stream state; control plane stays
  ACKing; only a power cycle cures it (soft reset does NOT — hardware evidence).

### 0.3.9 implementation (committed + pushed, commit `6330165`)

Library (`src/device.rs`, `src/usb.rs`, `src/lib.rs`):

- `Xtion::health_check() -> HealthReport { control_alive, mode,
  depth_streaming, depth_ring }` + `is_control_alive()` (2x KEEP_ALIVE, 1 s)
- `Xtion::stream_silent(window) -> Option<bool>` — two ring-counter samples;
  `Some(true)` = stuck (dead-stream verdict)
- `Xtion::restart_camera()` — tier 1: teardown_streams() + soft reset +
  keep-alive proof; port reset ONLY if control plane died
- `Xtion::restart_camera_deep()` — tier 2: teardown + ALWAYS USB port reset
  (`libusb_reset_device` = USBDEVFS_RESET = software unplug/replug — the
  dead-stream cure per hardware evidence) + 10x keep-alive probe + OFF baseline
- `UsbDevice::reset_and_reopen()` — port reset; reopen retries (12 x 250 ms)
  with fresh VID/PID enumeration fallback; re-probe endpoints/mps/speed
- `start_depth_verified` / NEW `start_ir_verified`: attempts x (start + 3 s
  probe), then tier-1 restart + one start, then tier-2 restart + one start,
  then final error naming the ladder
- HealthReport exported from lib.rs

Tests (`tests/hardware.rs`):

- jitter test: 10-frame warm-up discard + median/p95(70 ms)/max(150 ms)
  bounds in frame periods; mean band widened to 28..40
- ir_stream_roundtrip: `start_ir_verified(config, 3)`
- mode walk: `let _ = cam.soft_reset()` after EVERY refused mode (breaks the
  refused-SXGA -> silent-start cluster)
- NEW test 15 `camera_health_restart_and_stuck_probe`: health before ->
  verified start -> stream_silent(500 ms) == false -> restart_camera (soft)
  -> restart_camera_deep (USB port reset) -> health after -> verified start ->
  10 frames >= 8 -> stop -> final health check
- Header doc updated

Host-side prevention:

- `udev/70-xtion.rules` (NEW): power/control=on + MODE 0666 for 1d27:0600/0601
  (both usb and usbfs match). Autosuspend wake-ups = known multi-hundred-ms
  stall source on PS1080.

Docs: `docs/PROTOCOL.md` new section "The restart ladder: health, stuck probe,
in-place reset (0.3.9)". Cargo.toml version 0.3.9.

### Verification status

- `cargo build` + `cargo fmt`: clean, zero warnings
- `cargo test`: 155 unit + 4 golden + 15 hardware (skip w/o device) + 1 doc — ALL PASS
- Hardware validation pending on the USER's machine (sandbox has no camera)

## What the user must do next

```bash
cd ~/Projekte/Photogrammetrie/cameras/xtion-rs
git pull                        # get 0.3.9 (commit 6330165)
sudo cp udev/70-xtion.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
# replug the camera once (or power-cycle it once — clears any stuck state)
RUST_LOG=info RUST_BACKTRACE=full cargo test --test hardware -- --nocapture
```

Expect: every depth/IR start auto-recovers from dead-stream; the restart test
prints the two-tier ladder; jitter test reports median/p95/max.

## Research notes (for future sessions)

- libusb `reset_device()` on Linux = USBDEVFS_RESET = port reset; keeps bus
  address; standard software unplug/replug (askubuntu #645, 01signal.com).
- fw 5.8.22 is the FINAL Xtion firmware (Asus FWUpdate_5_8_22 enables OpenNI
  2.x; jurn.link install notes). No older firmware to roll to — quirks are
  handled driver-side.
- USB autosuspend is a recurring cause of silent/stalling USB cameras
  (Arch bbs, askubuntu #982860) — hence the udev rule.
- User's local repo path: `/home/t/Projekte/Photogrammetrie/cameras/xtion-rs`
  (Linux, stable toolchain). Firmware of their unit: 5.8.22, serial 1408090603,
  depth-only (no color CMOS), USB 2.0 High speed.

## Feature backlog still open (from earlier sessions)

- SXGA 1280x1024@15 advertised by nothing on this unit (firmware NACKs 15 fps;
  matrix advertises SXGA@30 but USB-2 bandwidth caps ~26 MiB/s) — SXGA@15
  stays a documented, firmware-refused probe in the mode walk
- Projector/modulation-frequency: read-only surface done (device_frequency +
  wavelength hop telemetry); no runtime write exists in the protocol
- TEC/NESA channels: parse/decode done; first camera run produced 0 lines
  (verdict, not failure)
