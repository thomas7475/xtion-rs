# Camera Validation Plan

How to prove xtion-rs works as intended **on real hardware**. Everything
that can be checked without a camera already runs in CI (`cargo test`,
84 checks); this document covers the on-camera layer.

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
   Without a camera the four tests print `skipped: no Xtion attached` and
   pass; with one attached they run for real.

## 2. Automated suite (tests/hardware.rs)

| # | Test | What it proves | Pass criteria |
|---|---|---|---|
| 1 | `open_handshake_and_identity` | v2.6 handshake, serial via opcode 37, fixed params sane | non-empty serial; firmware >= 3; reference distance 50-500 mm; pixel size 0.01-1.0 mm |
| 2 | `depth_stream_roundtrip` | full depth pipeline: stream setup, ring, packed-11 decode, shift-to-depth | >= 80% valid pixels (M5 reference: 87.4%); 29-30.1 fps (reference 29.97); ring `failures == 0`; shift histogram inside the table (reference 291-797); min depth 400-500 mm (reference 431 mm) |
| 3 | `phase1_identity_and_options` | 0.2.0 identity + runtime options | platform string non-empty (ASUS Xtion: `RD108x-112`-family); USB core parses (Jango=0 expected on 5.8.22); registration GET/SET round-trips true/false; hole filter, depth mirror, close range accept writes; emitter off/on accepts writes |
| 4 | `ir_stream_roundtrip` | IR stream end-to-end | 640x480, exactly 307,200 values; values not constant (`max > min`) and not black (`max > 0`); `drain_events()` empty on a healthy camera; `stop_ir` succeeds |

Also run the numerical core regression on-camera data:
`cargo test --test golden` (no camera needed; proves packed-11 and
shift-to-depth against the C reference).

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
- `emitter toggle skipped: ...`: expected on some 5.8.x builds — the
  reference driver only issues opcode 42 on fw >= 5.8.15 and even then
  some builds NACK it (BadParams). Runtime state only; the default
  (emitter on) is restored by power-cycling.
