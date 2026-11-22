# Feature Plan: Closing the Gap to the OpenNI 2 PS1080 Driver

This plan lists everything the OpenNI 2.2.0 PS1080 driver
(`Source/Drivers/PS1080`, Apache-2.0) implements that xtion-rs 0.1.5 does
not, evaluates each feature by implementation effort and **risk to the
camera**, and schedules them in phases. The guiding safety rule, per the
project owner's constraint:

> **No write that could delete calibration data or overwrite configs.**
> Everything that touches device flash, calibration data, or raw sensor
> buses is permanently excluded (see "Excluded features" below).

All Phase 1-2 features are runtime-only firmware state (RAM, lost on
power-cycle) — the same operations the vendor driver issued for a decade
of field use.

## 0. Hardware-validation fixes (0.2.1)

First real-camera run of the 0.2.0 tests (fw 5.8.22) failed two of the
four hardware tests; both were driver deviations from the reference,
now fixed and covered by unit tests:

1. `usb_core_type` NACK(InvalidCommand) → the driver now falls back to
   `UsbCore::Jango` on a failed probe, mirroring the reference
   ("don't check error code. If this fails, assume JANGO",
   XnDeviceSensorInit.cpp:233). Opcode 40 is evidently not universal
   even within firmware 5.x.
2. `ir_stream_roundtrip` ControlTimeout → `start_ir` was missing the
   CMOS blanking fetch that all three reference streams issue at
   configure time (`SetCmosConfig` → ALGORITHM_PARAMS type 0x06,
   16 bytes, for the requested resolution/fps). Without it the firmware
   ACKs every parameter but never arms the sensor timing, so no IR
   packets ever arrive. 0.2.1 adds `read_cmos_blanking` and issues it in
   `start_depth`, `start_color` and `start_ir` (parity with
   XnSensorDepthStream.cpp / XnSensorImageStream.cpp /
   XnSensorIRStream.cpp).

## 1. Gap analysis (OpenNI 2.2.0 vs xtion-rs 0.1.5)

| # | OpenNI feature | Mechanism | xtion-rs 0.1.5 | Effort | Camera risk |
|---|---|---|---|---|---|
| 1 | Serial number | OPCODE 37 | **present** | - | none (read) |
| 2 | Platform string | OPCODE 39 | missing | trivial | none (read) |
| 3 | USB core type | OPCODE 40 | missing | trivial | none (read) |
| 4 | Overheat event | packet 0xf31f → error state | packet silently skipped | small | none (read) |
| 5 | Projector-fault event | packet 0xdead → error state | packet silently skipped | small | none (read) |
| 6 | Depth registration (depth onto color) | PARAM 2 (hardware, in-firmware) | missing | small | none (runtime param) |
| 7 | Depth mirror | PARAM 23 | missing | trivial | none (runtime param) |
| 8 | Depth hole filter | PARAM 22 | missing | trivial | none (runtime param) |
| 9 | Depth AGC (gain) | PARAM 21 | missing | trivial | none (runtime param) |
| 10 | Close range | PARAM 84 (fw >= 5.6) | missing | trivial | none (runtime param) |
| 11 | GMC mode | PARAM 36 | missing | trivial | none (runtime param) |
| 12 | Emitter (IR projector) on/off | OPCODE 42 | missing | trivial | none (runtime state) |
| 13 | IR stream | STREAM0_MODE=3 + PARAMs 26/27/72, 10-bit packed frames on the image pipe | missing | medium | none (stream config) |
| 14 | Image mirror | PARAM 71 | missing | trivial | none (runtime param) |
| 15 | Image auto exposure | PARAM 80 | missing | trivial | none (runtime param) |
| 16 | Image exposure bar | PARAM 81 | missing | trivial | none (runtime param) |
| 17 | Image auto white balance | PARAM 77 | missing | trivial | none (runtime param) |
| 18 | Image gain (AGC) | PARAM 15 | missing | trivial | none (runtime param) |
| 19 | Image quality | PARAM 16 | missing | trivial | none (runtime param) |
| 20 | Anti-flicker | PARAM 17 | missing | trivial | none (runtime param) |
| 21 | Cropping (all streams) | PARAMs 46-60 + mode | missing | medium | none, but complex bookkeeping |
| 22 | Depth AGC bins | PARAMs 63-70 | missing | small | none (runtime params) |
| 23 | Wavelength correction | PARAMs 73/74 | missing | small | none (runtime params) |
| 24 | GMC data/debug stream | packets 0xa200/0xb200, OPCODE 26 | missing | medium | none (read) |
| 25 | Firmware log | OPCODE 7 + PARAMs 30/31 | missing | medium | low (PARAM 30/31 are runtime log config) |
| 26 | CPU stats | OPCODE 27 | missing | small | none (read) |
| 27 | TEC/emitter status reads | OPCODEs 30/32/38 | missing | small | none (read) |
| 28 | BIST | OPCODE 28 | missing | small | **medium: runs self-test machinery; deferred** |
| 29 | Audio stream | STREAM2_MODE=7, packets 0x9200, PARAMs 8-11/33-35 | missing (disabled in OpenNI 2.2.0 too: `TODO: enable`) | large | none, but big surface |
| 30 | CMOS blanking/presets | OPCODEs 34/35/36 | missing | small | **medium: changes sensor timing** |

Params already used by 0.1.5: 5, 6 (stream modes), 12-14 (image setup),
18-20 (depth setup). Opcodes already used: 0, 1, 2, 3, 4, 22, 37.

## 2. Phases (ordered by effort and risk, lowest first)

### Phase 1 — identity and safety events (implemented in 0.2.0)

Read-only commands plus parser work; nothing is written to the device at
all beyond the request packets themselves.

1. `platform_string()` — OPCODE 39, reply is UTF-16-ish (one char per
   16-bit word; the C reads every second byte).
2. `usb_core_type()` — OPCODE 40, reply is a single u16
   (0 = Jango, 1 = gadgetfs).
3. `DeviceEvent::Overheat` / `DeviceEvent::ProjectorFault` — the stream
   assembler captures packets 0xf31f/0xdead instead of skipping them
   (OpenNI maps them to the device error state; a following good stream
   packet clears it). The misc endpoint (0x83) is now read while any
   stream runs so events reach the host whichever endpoint carries them.

### Phase 2 — runtime toggles and the IR stream (implemented in 0.2.0)

All single-parameter runtime writes, safe by construction (the firmware
keeps them in RAM; power-cycle restores defaults), plus the IR stream
that reuses the already-proven image pipe:

4. `set_registration(bool)` — PARAM 2. On PS1080 the firmware registers
   depth onto color in hardware (`XnSensorDepthStream::DecideFirmwareRegistration`:
   PS1080 supports hardware registration at all resolutions).
5. `set_depth_mirror`, `set_hole_filter`, `set_depth_agc`, `set_close_range`,
   `set_gmc_mode` — PARAMs 23, 22, 21, 84, 36. Close range uses the
   direct PARAM 84 (firmware >= 5.6; the validated camera runs 5.8.22).
6. `set_emitter_enabled(bool)` — OPCODE 42, payload `{u16 active}`. This
   is the runtime projector toggle; the *calibration* opcodes (31/33) stay
   excluded.
7. Image controls: `set_image_mirror` (71), `set_auto_exposure` (80),
   `set_exposure` (81), `set_auto_white_balance` (77), `set_image_gain`
   (15), `set_image_quality` (16), `set_flicker` (17).
8. IR stream: `start_ir(IrConfig)` sets PARAM_IR_RESOLUTION (26),
   PARAM_IR_FPS (27), PARAM_IR_MIRROR (72) = 0 (the reference driver
   mirrors IR host-side; not ported yet) and STREAM0_MODE = 3. IR frames
   arrive on the **image endpoint** as IMAGE packets (0x8100/0x8500) with
   4x10-bit packed pixels (5 bytes per 4 pixels), decoded to 16-bit
   grayscale (`XnIRProcessor::Unpack10to16`). IR and color are mutually
   exclusive (same firmware stream slot); IR+depth run together fine and
   that is exactly the pairing hardware registration is designed for.

### Phase 3 — medium effort, still zero-risk (next minor release)

9. Cropping (PARAMs 46-60): firmware crop size/offset/mode per stream,
   with the reference driver's "bigger crop first, then shrink" protocol
   for changing crop on the fly.
10. Depth AGC bins (PARAMs 63-70) and wavelength correction (73/74).
11. GMC motion data (packet 0xa200, up to 100 points per packet) and its
    debug channel; `set_gmc_params` (OPCODE 26) configuration.
12. Read-only diagnostics: CPU stats (27), TEC/emitter status (30/32/38),
    firmware log read (7) with log filter/interval params (30/31).

### Phase 4 — large surfaces (only on explicit request)

13. Audio (STREAM2_MODE=7 + PARAMs 8-11/33-35 + packets 0x9200): a full
    USB Audio Class implementation; disabled even in OpenNI 2.2.0.
14. CMOS blanking (34/35) and presets (36): changes sensor timing; needs
    on-camera validation against a known-good configuration.

## 3. Excluded features (never in this driver)

Per the safety rule, these are **not planned**, not merely deferred:

| Feature | Opcodes/params | Why excluded |
|---|---|---|
| Flash file system (upload/download/delete/execute/read/attributes) | 13-19, 23-25, PARAM 85 | writes firmware, parameter images and calibration files to device flash; a bug here can brick the camera or wipe per-sensor calibration |
| Raw I2C bus writes | 10 | direct CMOS register writes can leave a sensor in a bad state |
| Raw AHB bus writes | 21 | writes arbitrary SoC memory |
| TEC calibration | 29 | writes calibration data |
| Emitter calibration | 31 | writes emitter power calibration |
| Projector-fault calibration | 33 | writes safety-threshold calibration |
| Take snapshot | 12 | writes a frame to device flash |
| SET_MODE | 6 | changes firmware mode; the read side (5) may be added later |
| SET_LED_STATE | 41 | harmless but pointless: the Xtion has no LED |
| Reset sensor | XN_MODULE_PROPERTY_RESET | forces a device reset |

The vendor's own updater is the right tool for anything in this table.

## 4. Test strategy

Every new code path is covered by tests that run **without hardware**:

- Wire-format unit tests against `FakeTransport` (the established
  pattern in `protocol/command.rs` and `device.rs`): request bytes and
  reply parsing for opcodes 39/40/42, the exact SET_PARAM sequences for
  every new toggle, and the IR start sequence (params 26/27/72 + stream
  mode 3).
- Decoder golden tests: the 10-bit IR unpacker is ported instruction by
  instruction from `XnIRProcessor::Unpack10to16` and tested against
  hand-computed vectors plus the all-zero/all-ones corner cases.
- Assembler event tests: synthetic OVERHEAT/PROJECTOR_FAULT packets
  interleaved into a depth stream must produce exactly one event each
  and must not disturb frame assembly (regression guard for the resync
  path).
- The on-camera integration tests (`tests/hardware.rs`, run only when a
  device is attached) gain identity + IR + registration checks.

## 5. Firmware capabilities beyond the OpenNI 2.2.0 driver (verified)

Diff of the firmware 5.8.22 command surface against what the OpenNI
PS1080 driver actually calls (verified by grepping every
`XnHostProtocol*` function for call sites, not just opcode defines):

| Firmware capability | Surface | OpenNI 2.2.0 status | Evidence |
|---|---|---|---|
| Audio capture | STREAM2_MODE=7, params 8-11/33-35, packet 0x9200 | **disabled** — the stream code exists but `CreateStreamModule` refuses audio (`TODO: enable` + `XN_ASSERT(FALSE)`) | XnSensor.cpp ~470 |
| GMC motion data | packet 0xa200 (up to 100 points/packet), params 37-44, 75 | **unhandled** — only GMC *debug* (0xb200) is routed; plain GMC falls into the default "Unknown packet type" warning | XnFirmwareStreams.cpp switch |
| File **upload** to flash | opcodes 13/14/15 (INIT/WRITE/FINISH_FILE_UPLOAD) | **never called** (download/read/delete/execute *are* used) | zero call sites |
| Firmware kill | opcode 999 (`KILL`) | **never called** | zero call sites |
| Take snapshot to flash | opcode 12 | **never called** — defined, has a body, no caller | XnHostProtocol.cpp:2440 |
| Params 0/3/4/31/32/85 | CURRENT_MODE, STREAM_PRIORITY, TRIGGER_ACTION, packet/session timeouts, FILE_SYSTEM_LOCK | **never mapped/written** (mode is read via GET_MODE opcode; 85 only appears inside the flash-lock flow) | zero uses outside XnHostProtocol.cpp:2036 |
| CMOS register access | GET/SET_CMOS_REGISTER (V110-dialect opcodes 8/9) | present in `FWInfo` but unused on modern firmware (5.x configures CMOS via params/presets) | XnHostProtocol.h V110 enum |

Also confirmed: the `EPsProtocolOpCodes_V300`/`_V110`/`_V017` enums in
the driver are ancient-firmware dialects; firmware 5.8.22 uses the main
0-42+999 table (matches the reverse-engineered dispatcher in
`docs/FIRMWARE.md`). Everything else the firmware implements — all 15
packet types, TEC/emitter/projector calibration, CMOS blanking/presets,
BIST, CPU stats, the log channel — OpenNI does call.

What xtion-rs should do with these: the **audio runtime** and the
**plain GMC data stream** are the two genuinely valuable firmware
capabilities that neither the vendor driver nor this crate exposes yet
(Phases 3-4). The flash-upload, KILL and TAKE_SNAPSHOT surfaces stay
excluded per section 3 — OpenNI not calling them is itself a signal.

## 6. Sources

- OpenNI 2.2.0, `Source/Drivers/PS1080`: `XnHostProtocol.cpp` (opcode
  request/reply formats), `XnSensorDepthStream.cpp` (registration,
  close range, AGC, hole filter, mirror), `XnSensorImageStream.cpp`
  (exposure/WB/gain/quality/flicker mappings), `XnSensorIRStream.cpp`
  (IR configuration: params 26/27/72, STREAM0 mode 3, host-side mirror),
  `XnIRProcessor.cpp` (10-bit unpack), `XnFirmwareStreams.cpp` (event
  packets → error state), `XnSensorFirmwareParams.cpp` (param ids),
  `XnParams.h` (numeric param ids 0-85).
- `docs/FIRMWARE.md` — which of these the 5.8.22 firmware really
  implements (all of Phase 1-2 verified present in the binary).
- `docs/VALIDATION.md` — the on-camera test plan for validating every
  implemented feature.
