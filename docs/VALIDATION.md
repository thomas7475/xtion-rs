# Camera Validation Plan

How to prove xtion-rs works as intended **on real hardware**. Everything
that can be checked without a camera already runs in CI (`cargo test`,
149 checks); this document covers the on-camera layer.

Reference hardware: ASUS Xtion Pro Live, firmware 5.8.22, USB 2.0
(see the USB 3.0 caveat in section 4).

## 1. Preparation

1. Plug the camera into a **USB 2.0 port/hub**. On USB 3.0 controllers the
   5.8.22 firmware can miss bulk packets (the known bandwidth quirk from
   the research series); if a test fails, retry on USB 2.0 before
   investigating code.
2. Confirm enumeration:
   `lsusb` must show `1d27:0601` (or `1d27:0400` for the non-Live model).
3. Make sure no other driver (OpenNI/NiViewer, libfreenect2) holds the
   device; `sudo fuser -v /dev/bus/usb/...` helps to find holders.
4. Build and run the hardware suite:
   ```
   cargo test --test hardware
   ```
   Without a camera the twelve tests print `skipped: no Xtion attached` and
   pass; with one attached they run for real.

## 2. Automated suite (tests/hardware.rs)

| # | Test | What it proves | Pass criteria |
|---|---|---|---|
| 1 | `open_handshake_and_identity` | v2.6 handshake, serial via opcode 37, fixed params sane | non-empty serial; firmware >= 3; reference distance 50-500 mm; pixel size 0.01-1.0 mm |
| 2 | `depth_stream_roundtrip` | full depth pipeline: stream setup, ring, packed-11 decode, shift-to-depth | >= 80% valid pixels (M5 reference: 87.4%); 29-30.1 fps (reference 29.97); ring `failures == 0`; shift histogram inside the table (reference 291-797); min depth 400-500 mm (reference 431 mm) |
| 3 | `phase1_identity_and_options` | 0.2.0 identity + runtime options | platform string non-empty (ASUS Xtion: `RD108x-112`-family); USB core parses (Jango=0 expected on 5.8.22); registration GET/SET round-trips true/false; hole filter, depth mirror, close range accept writes; emitter toggle reported — NACK(BadParams) is the documented behaviour of this fw build (0.3.0) and prints as a skip, an ACK is a bonus |
| 4 | `ir_stream_roundtrip` | IR stream end-to-end | 640x480, exactly 307,200 values; values not constant (`max > min`) and not black (`max > 0`); `drain_events()` empty on a healthy camera; `stop_ir` succeeds |
| 5 | `color_stream_roundtrip` | color stream end-to-end in the **PS-compressed YUV422 mode** (format 1, the C's non-Windows/bulk default) | prints the 25-entry preset table + endianness guard + the two capability gates (image_cmos_type, firmware mode); **on units reporting image_cmos_type = 0 (no color sensor — our fw 5.8.22 unit) SKIPS with the reason**; otherwise `start_color` ACKed including `STREAM0_MODE=COLOR`, one frame decodes to exactly width x height x 3 RGB, non-constant; IR start refused while color runs; stop clean |
| 6 | `color_start_probe` | diagnostic (always REPORTS, never gates): mode, image-pipe params before/after the init soft reset, C-parity write sequence with readbacks (compressed mode), one driver `start_color` | informational — paste the output when color misbehaves |
| 7 | `system_commands_readonly` | the 0.2.8 read-only command surface end-to-end, extended in 0.3.0/0.3.2 with the diagnostics ground-truth probes. **Camera-validated 2026-11-23 (0.3.2 run): 22-record table, 20/20 CRCs verified on device** | firmware log (raw entries + rendering, then a triggered known-NACK re-read), device clock rate (~60 ticks/us expected), flash file list (opcode 19, the firmware's drift-corrupted view), **on-flash file table via opcode 25 + packed-record parse (0.3.2 ground truth): record 0 must be the FileTable self-entry (offset 0x1000 words, size 0x1000 words, crc 0xfffc), >= 12 records**, **on-device CRC verification: every file's payload read at its word offset must reproduce its stored crc under CRC-16/CCITT-FALSE over big-endian words; GainControl (0xc44b) and ImageCmos (0x1cfa) are hard gates**, DOWNLOAD_FILE type-lookup mapping (fw 5.8.22 serves the wrong file for some types — 0x12 returned RegistrationParams), flash map, CPU stats, TEC + emitter telemetry, direct CMOS blanking, CMOS register reads (I2C), AHB read, raw flash read — each section may report `unavailable: ...` without failing (firmware-dependent surface), but nothing may hang or corrupt the session |
| 8 | `depth_geometry_fov_and_timestamps` | the 0.3.0 photogrammetry surface: intrinsics from the on-device calibration, FOV vs the Xtion spec, wrap-aware host-referenced timestamps, dominant-plane fit (0.3.1: RANSAC + PCA refit — camera-validated FOV 58.59/45.64, 33.37 ms cadence, on-axis center; the 0.3.0 global least-squares fit failed 49 mm on a two-plane room) | hfov 52-64 deg and vfov 38-52 deg (Xtion spec 58/45); center pixel maps to (x,y) ~ 0 on the optical axis; 90 frames with mean inter-frame delta 28-38 ms (30 fps) and < 5 ms jitter, strictly monotonic; when the dominant plane covers >= 40% of the cloud its inlier RMS must be <= 15 mm (point at a flat wall 1-3 m away; a good fit is a few mm) — below that coverage the gate reports the numbers and skips, so ordinary non-planar rooms no longer fail the test |
| 9 | `plane_fit_synthetic` | camera-independent coverage of `fit_dominant_plane` (no Xtion needed) | synthetic noisy wall: coverage > 95%, RMS < 6 mm, distance within 5 mm of ground truth, tilt < 4 deg; two-plane room: dominant plane found, coverage ~53%, RMS < 6 mm; uniform noise cloud: coverage stays under the 40% gate |
| 10 | `depth_and_ir_crop_windows` (0.3.3, mode-2 probe reworked 0.3.4) | firmware crop windows end-to-end: mode-1 crop (now via the C-exact multi-pair batch), on-the-fly shrink, disable, geometry via `to_world`, IR crop (no extra CMOS rows while cropped), mode-2 increased-fps probe | full-frame baseline 640x480 with `crop == None`; 320x240 centered crop: frame is 320x240, `crop == Some(...)`, data len 76,800, valid pixels > 1000 and <= 105% of the baseline; crop centre maps to world (x,y) within 50 mm of the optical axis; on-the-fly 160x120 accepted with matching dims; disable returns 640x480/`None`; IR crop 320x240 decodes exactly 76,800 values and the full frame returns after disable; the mode-2 probe reports THREE forms (single write while streaming, C-exact batch, stream-stopped batch) and NEVER gates on a rejection — the 0.3.3 camera run already showed this fw build NACKs the single write with BadParams, so a full rejection prints the documented verdict and re-verifies the mode-1 window instead; only an unexpected error type (or a broken mode-1 window afterwards) fails the test |
| 11 | `depth_processing_controls` (0.3.4; AGC write probe reworked 0.3.5) | the depth processing params end-to-end while streaming: gain (21), the four AGC shift bins (63-70), close range (84), wavelength correction + debug (73/74). Every phase follows read-factory -> write-known-good -> read-back -> RESTORE -> assert, so a panic can never leave modified processing on the device | factory inventory printed (gain, four bins as shift ranges, three booleans) with structural bin sanity (ordered, <= 2047); gain 42 read back while set and the factory value after restore; **AGC bin write probe (0.3.5): four ladder writes — no-op LOW/HIGH at the factory values, a HIGH raised above its LOW (invariant-preserving), and the C's LOW-first order at the v0.3.4 failure point — pin exactly which encodings this build accepts, then the factory partition is verified. If writes are accepted at all, the contiguous custom partition (0-512/512-1024/1024-1536/1536-2047) must echo exactly with depth still flowing and the factory bins verified afterwards; a refusal prints a VERDICT line (like crop mode 2) instead of failing — the 0.3.4 camera run showed the reference's LOW-first order NACKs BadParams on the first bin that grows upward**; close range reads back `true` while set, factory value after restore (the frame under close range is printed, not gated — near-field re-bias legitimately changes the depth distribution); wavelength + debug read back on and restored; final frame > 1000 valid pixels (scene back to baseline) |
| 12 | `gmc_config_and_motion_data` (0.3.5) | the complete GMC surface: config block (params 36-44, 61, 75), the two reference-writable switches (mode 36, debug 61) toggled while depth streams, and the misc endpoint decoded for plain GMC motion data (0xa200) plus the debug channels (0xb200 points, 0xb500 summary) | config block printed (mode/ref-offset/ricc/dx-correction terms/debug/correction-mode); over 20 frames the GMC packet census is printed (plain / debug points / debug summaries / total points, with a sample of the first plain packet) — **a silent GMC stream prints a VERDICT line and still passes** (nothing in the protocol guarantees points on a static scene); mode + debug switches must be back at the factory values afterwards (exact restore, even a non-0/1 factory mode); final frame > 1000 valid pixels |

Also run the numerical core regression on-camera data:
`cargo test --test golden` (no camera needed; proves packed-11 and
shift-to-depth against the C reference, and — since 0.3.2 — the
on-flash file-table parser against a real device capture plus the
flash CRC against the 5.8.22 factory image payloads, 8/8 files).

## 3. Feature-by-feature manual checks (0.2.0 surface)

Run each with the built-in example or a small scratch program using the
documented API. Every check lists the expected observation.

1. **Identity values** — `platform_string()` on an ASUS Xtion returns a
   `RD108x`-family string; `usb_core_type()` returns `Jango` (fw >= 5.3.16).
   *Fail* if empty or unknown-core error on 5.8.22.
2. **Depth mirror** — `set_depth_mirror(true)` then read a frame while
   pointing the camera at an asymmetric scene (e.g. hand to the left):
   the valid-pixel side must flip compared to mirror off. Frame size and
   fps unchanged. Mirror off restores the original orientation.
3. **Hole filter** — aim at a distant dark edge (produces single-pixel
   holes): with `set_hole_filter(true)` the no-depth marker ratio must
   drop or stay equal vs off, never rise.
4. **Close range** — hold a textured target at ~30 cm:
   off = 0 valid pixels (below minimum range), on = plausible values
   (< 500 mm). Do not forget the ConstShift table changes meaning; the
   shift-to-depth LUT stays valid because it is read after the toggle.
5. **Registration** — start color + depth, `set_registration(true)`, point
   at a doorframe: depth-object edges must align with the color edges
   within a few pixels at image center; toggle off and the offset
   reappears. (`registration_enabled()` must agree each time.)
6. **Image controls** — `set_auto_exposure(true)` in a dark room then
   bright lamp: mean frame brightness must adapt. With auto exposure off,
   `set_exposure(v)` must change mean brightness monotonically.
   `set_flicker(50)` in 50 Hz-mains lighting must reduce the banding
   visible in `raw_dump` output.
7. **Emitter + IR interplay** (the laser-safety check) —
   `set_emitter_enabled(false)`: IR frame mean value must collapse toward
   ambient (roughly < 10% of the projector-on mean in a dark room);
   re-enable: mean restores. Depth stream keeps producing (sunlight-
   insensitive) but with degraded/no validity in the dark.
8. **Events path** — `drain_events()` returns empty on a healthy camera;
   the misc ring shows zero errors in `depth_ring_stats`-style counters
   and `other_packets` may grow slowly (debug channel packets are
   counted). *Fail* only on stream errors, not on events being absent.
9. **Mutual exclusion guards** — with color running, `start_ir` must fail
   with the image-pipe conflict error and vice versa; after `stop_color`,
   `start_ir` must succeed. No device lockup in either case.
10. **Clean stop** — after `stop_depth`/`stop_color`/`stop_ir` the rings
    are gone (`depth_ring_stats() == None`) and `start_*` works again
    immediately (stream-mode OFF write accepted).

## 4. Regression matrix

| Change type | Minimum re-run |
|---|---|
| any protocol/param edit | section 2 fully (4 tests) |
| stream/decoder changes | section 2 + manual 2/5/7 |
| USB stack changes | section 2 on USB 2.0 AND USB 3.0 ports |
| release tag | section 2 + section 3 items 1/5/7/9 |

## 5. Triage

- **Firmware-log "Unix epoch" timestamps SOLVED (0.3.0, parse bug):**
  0.2.8 printed values like 1593835551 (0x5F00001F) and 1510080543
  (0x5A02001F) as log timestamps. The packed `XnLogEntryHeader`
  (XnHostProtocol.cpp:1196-1211) is `{u16 entry_size, u32 timestamp,
  u16 log_type}` — the timestamp PRECEDES the log type — but the parser
  read `{u16, u16, u32}`, so what got printed was `[timestamp_hi16 |
  log_type]` of boot-time STRUCTURED entries: module 0x5F/error 0 and
  module 0x5A/error 2 with ~0x001Fxxxx boot ticks. The structured
  payload's param is also a u32 (`XnLogDefaultData`), not a u16. Fixed
  in 0.3.0 with unit tests against the C layout;
  `firmware_log_entries()` exposes the raw fields and the hardware
  test aligns a triggered known-NACK re-read as ground truth.

- `DeviceBusy` on open: another process holds the interface (close
  NiViewer, unload any vendor kernel module).
- Timestamp/fps far from 30: hub or USB 3.0 port — retest on USB 2.0.
- Valid-pixel ratio suddenly < 50%: emitter disabled by a previous test —
  power-cycle the camera (all 0.2.0 options are runtime state) and re-run.
- `ControlTimeout` on the first command: the 2 s first-command quirk is
  built in; persistent timeouts indicate a control-ep problem — replug.
- `usb core: Jango` without a real probe: the firmware refused opcode 40
  (some 5.x builds answer NACK(InvalidCommand) even though the platform
  string works); the driver falls back to Jango exactly like the
  reference driver (XnDeviceSensorInit.cpp:233). Not a failure.
- IR: `completions == 0` in the ring stats = the device never sent a
  byte (sensor timing not armed — pre-0.2.1 behaviour, blanking fetch
  missing). `completions > 0` with the read failing fast on "unusable
  frames ... N bytes (expected M)" = framing or wire-size problem; the
  error message now carries the actual frame size plus assembler
  counters (starts/buffers/ends seen), which distinguishes "frames
  close at the wrong size" from "framing never completes".
- **IR SOLVED (0.2.3):** the 0.2.2 run showed frames closing at
  390,400 bytes = 488 rows x 800 B — the MT9M001 IR CMOS transmits
  `height + 8` rows (XnIRProcessor.cpp:244-282; see PROTOCOL.md
  section 8). The driver now expects the 488-row wire size and crops to
  the first 480 rows like the reference. If a future run still fails
  with a DIFFERENT size, the message now prints the depth CMOS type and
  the row math — compare against `ir_wire_size` and wire_extra_rows.
- **IR CONFIRMED on hardware (2026-11-22):** with 0.2.3 all four
  hardware tests passed — open_handshake_and_identity,
  phase1_identity_and_options, depth_stream_roundtrip,
  ir_stream_roundtrip. The `emitter toggle skipped: ...` line is
  benign (see above).
- Color (since 0.2.4): the same fail-fast contract as IR — an error of
  the form `frame #N <bytes> (expected M for WxH YUV422)` means a
  wire-size deviation (color has NO extra CMOS rows; expected M =
  width x height x 2), `completions == 0` in `color_ring_stats()` means
  the color CMOS was never armed (blanking fetch missing / wrong image
  params), and 5 unusable frames fail fast with assembler + ring
  counters in the message.
- **Color NACK FINAL ROOT CAUSE (0.2.10, hardware-validated 2026-11-23):
  this unit has NO color sensor.** The 0.2.9 on-camera run printed
  `image cmos type: 0` — the fixed-params block reports
  XN_IMAGE_CMOS_NONE. That field IS the reference driver's
  bImageSupported gate (XnSensorFirmware.cpp:128-140, fw >= 5.6.9);
  the C never creates the image stream when it is 0
  (XnSensor.cpp:274-281), so OpenNI lists no color node on this device
  either. Every stream-on NACK since 0.2.4 (compressed/uncompressed x
  VGA/QVGA) was the firmware refusing to route COLOR from an absent
  sensor; IR — the other stream0 source, fed by the depth CMOS —
  starts fine on the same pipe. Since 0.2.10 `start_color` fails fast
  with this explanation (nothing written) and the color test skips on
  such units. Not a driver bug: color is impossible on this hardware;
  the full compressed-color path awaits a unit with an RGB sensor.
- **Color NACK ROOT CAUSE (0.2.8):** two firmware facts from the
  OpenNI 2.2.0 cross-read explain every 0.2.4-0.2.7 symptom:
  (1) the reference driver SOFT-RESETS the firmware at EVERY init
  (XnSensorFirmware::Init, bResetOnStartup=TRUE) and xtion-rs never
  did — after a failed stream start the device rejects even plain
  mode-parameter writes (the "redundant-write" pattern was dirty
  state, not redundancy); (2) the C's image default on non-Windows is
  the PS-COMPRESSED format 1 (XnSensorImageStream.h:36-41, "non win32
  works with BULK, so it's better to compress the image") — the
  uncompressed mode (5) that the preset table lists is the Windows/ISO
  path, and its stream start is what this bulk device rejected (the
  presets describe CMOS capability; the active USB interface filters
  them). Fix: `open()` ports the init reset (1 ms delay on Jango
  fw >= 5.3.16, not 800), `start_color`/`read_color_frame` speak the
  compressed format, and the ported `XnStreamUncompressYUVImagePS`
  decodes it. If a future run still NACKs: run `color_start_probe`
  and check whether the compressed C-parity sequence ACKs — if even
  that fails, grab a usbmon capture of a real OpenNI color session.
- **Color BadParams at start (0.2.4 run, solved in 0.2.5):** `start
  color: Nack(BadParams)` — the requested mode was outside the
  firmware's CMOS preset list (and the PS-compressed Yuv422 format was
  requested, which this driver could not decode then). Since 0.2.5 the
  driver queries the presets (opcode 36) first and fails fast with the
  full supported list before writing anything; since 0.2.8 the hardware
  test picks the compressed (1, VGA@30) mode.
- **Stream0Mode=COLOR NACK with valid params (0.2.6 run, probing in
  0.2.7):** the preset table decodes correctly now and (5,1,30) is
  picked, but the firmware rejects `STREAM0_MODE=1` (COLOR) —
  sometimes, right after a failed attempt, even the first
  PARAM_IMAGE_FORMAT write (suspected redundant-write rejection: the
  values are already stored from the previous attempt). The C open path
  is write-for-write identical, so the difference is device state; the
  one C behavior we never ported is the INIT-TIME SOFT RESET
  (XnSensorFirmware::Init, bResetOnStartup=TRUE: GetMode, KeepAlive,
  SET_MODE(SOFT_RESET), 800 ms, KeepAlive x10). Run the
  `color_start_probe` hardware test — it dumps GET_MODE + image-pipe
  params before/after a soft reset and walks the write sequence with
  readbacks; paste its output.
- **Preset table byte-swapped (0.2.5 run, solved in 0.2.6):** the
  0.2.5 hardware run printed `format 256 @ res 0 @ 7680 fps`-style
  presets and aborted with "no uncompressed-YUV422 color mode" — every
  field was shifted by 256 because the parser read the triples
  big-endian. Ground truth: `XN_PREPARE_VAR16_IN_BUFFER` is the
  IDENTITY macro on x86/Windows, so the opcode-36 fields are plain
  little-endian (Nios II firmware; same as the packet header). After
  the fix the expected on-camera table is: QVGA(0)/VGA(1) in formats
  1 (PS-compressed Yuv422), 5 (uncompressed Yuv422), 7 (uncompressed
  Yuyv) at 15/30/60 fps; SXGA(2)/720p(14)/1280x960(15) Bayer-only
  (formats 0/6) at 30 fps; one extra (9, 1, 30) PS-compressed VGA@30
  entry with a format id OpenNI never exposes. The color test selects
  (5, 1, 30) = uncompressed UYVY VGA@30 and asserts every field is
  <= 60/15/15 so a future byte-swap cannot masquerade as "no matching
  mode" again.
- `emitter toggle skipped: ...`: expected on some 5.8.x builds — the
  reference driver only issues opcode 42 on fw >= 5.8.15 and even then
  some builds NACK it (BadParams). Runtime state only; the default
  (emitter on) is restored by power-cycling.
