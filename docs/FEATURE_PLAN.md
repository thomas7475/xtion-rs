# Feature Plan: Closing the Gap to the OpenNI 2 PS1080 Driver

This plan lists everything the OpenNI 2.2.0 PS1080 driver
(`Source/Drivers/PS1080`, Apache-2.0) implements that xtion-rs does
not, evaluates each feature by implementation effort and **risk to the
camera**, and schedules them in phases. The guiding safety rule, per the
project owner's constraint:

> **No write that could delete calibration data or overwrite configs.**
> Everything that WRITES device flash, calibration data, or raw sensor
> buses is permanently excluded (see "Excluded features" below). READS
> of the same surfaces are safe and implemented as of 0.2.8.

**Status after 0.2.9 (2026-11-23):** Phases 1-2 complete and hardware-
validated (depth, IR); Phase 3 items 9-11 still open; the read-only
diagnostics tier (old item 12) shipped early in 0.2.8 together with the
flash file-system READS, the init soft reset (GET_MODE/SET_MODE) and
the PS-compressed color path. **0.2.9 adds stream hygiene + the full
image-open C parity** (see section 0b): the on-camera color run still
NACKed BadParams on the stream-on write after 0.2.8, and the re-audit
found the reference guards every stream-mode write with an OFF
transition, kills all streams at open/shutdown, and writes four more
params in the image-open sequence. 0.2.9 is green on CI (106 lib +
2 golden tests, clippy clean); the on-camera run of the new color path
+ `color_start_probe` (now with image-CMOS-type and boot-log dumps) is
the immediate next action.

## 0b. Stream hygiene + image-open parity (0.2.9)

The 0.2.8 hardware run (camera attached, depth + IR passing) failed
`color_stream_roundtrip` with NACK(BadParams) on the stream0=COLOR
write, exactly like the pre-0.2.8 uncompressed attempts. Write-for-write
re-audit of the reference image path (XnSensorImageStream.cpp,
XnSensorStreamHelper.cpp, XnSensorFirmwareParams.cpp, XnSensor.cpp)
found five C behaviours xtion-rs lacked; all are now ported:

1. **Stream-mode writes go through OFF.** The reference guard
   `SetStreamMode` rejects any mode change that does not pass through
   OFF ("we require that every change to mode will go through OFF",
   XnSensorFirmwareParams.cpp:585-600). The firmware enforces the same
   rule server-side with NACK(BadParams), so a stream left in IR/COLOR
   mode by an abnormal session poisons the next stream-on.
   `start_depth`/`start_ir`/`start_color` now write OFF first.
2. **Every clean session ends with all streams OFF** — the reference
   writes all three stream modes to OFF in the pre-5.2 reset path
   (XnHostProtocol.cpp:1804-1813) and at device shutdown
   (XnSensor.cpp:315-317). Ported: `firmware_init_reset` step 7 (so
   every `open()`/`soft_reset` starts from the OFF baseline) and a new
   `Drop` for `Xtion` (best-effort kill on close, panic-proof).
3. **The image-open sequence has steps xtion-rs skipped**: flicker
   (param 17) and firmware mirror (param 71) are written on every
   configure (ConfigureStreamImpl), and OpenStreamImpl always writes
   crop mode (50, disabled) plus auto exposure (80) and auto white
   balance (77) with their reference defaults TRUE on fw >= 5.8.9
   (bImageAdjustmentsSupported). All five writes are now in
   `configure_color_via`, order-verified by a unit test.
4. **The device-frequency read** (ALGORITHM_PARAMS 0x80, 4-byte f32,
   resolution 0 / fps 0) happens in `XnSensorStreamHelper::Open` on
   EVERY stream open, between configure and stream-on. Ported as
   `read_device_frequency`, issued by `start_color`.
5. **bImageSupported** — the reference refuses to create the image
   stream at all when fixed params report image CMOS type 0
   (XnSensorFirmware.cpp:128-140). Not yet enforced host-side; the
   hardware tests now print the value (0 = no color sensor detected)
   plus the boot self-test log (CLR/PRJ/TEC lines) so the next run
   decides between "stale-state/sequence" and "no working color CMOS".

If the color start STILL NACKs after 0.2.9, `color_start_probe` output
now pinpoints the state: per-step readbacks of the whole sequence, the
image CMOS type, the firmware mode and the boot log.

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
3. `ir_stream_roundtrip` "unusable frames: 390,400 bytes (expected
   384,000)" (0.2.2 diagnostics run) → the MT9M001 IR CMOS transmits
   **`height + 8` rows** per frame (488 rows at VGA); the 0.2.3 fix
   mirrors the reference exactly: expect the extra-rows wire size
   (`ir::ir_wire_size`, XnSensorIRStream.cpp:411-422) and crop the
   trailing 8 rows off the decoded frame (XnIRProcessor.cpp:244-282,
   "don't report additional rows out"). The CMOS type comes from the
   fixed params (`depth_cmos_type`, already parsed since 0.1.2);
   0.2.2's fail-fast diagnostics + assembler/ring counters pinpointed
   the size mismatch in one run.

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

### Phase 3 — medium effort, still zero-risk (NEXT)

9. **Cropping (PARAMs 46-60)** — firmware crop size/offset/mode per
   stream, with the reference driver's "bigger crop first, then shrink"
   protocol for changing crop on the fly (SizeX -> SizeY -> OffsetX ->
   OffsetY -> Mode last). The next feature to implement.
10. **Full C parity at image open** — the reference `OpenStreamImpl`
    also writes crop-mode 50 = disabled and, on fw >= 5.8.9
    (bImageAdjustmentsSupported), auto-exposure (80) + auto-white-balance
    (77) at every image stream open, after the stream0-mode write. Port
    these three writes for byte-level parity (harmless RAM params).
11. Depth AGC bins (PARAMs 63-70) and wavelength correction (73/74,
    ALGORITHM_PARAMS 0x80 FREQUENCY for multi-camera interference
    avoidance).
12. GMC motion data (packet 0xa200, up to 100 points per packet) and its
    debug channel; `set_gmc_params` (OPCODE 26) configuration.
13. **Depth-to-color registration end-to-end**: `set_registration(true)`
    (PARAM 2) exists since 0.2.0, but the host-side side — the
    REGISTRATION algorithm tables (ALGORITHM_PARAMS 0x02) and the
    on-device frame-size/padding implications — is unported. For a
    photogrammetry pipeline this is the highest-value remaining feature:
    it aligns depth onto the color image in firmware, so RGB + depth
    pixels correspond 1:1.

### Phase 3a — read-only diagnostics (implemented in 0.2.8)

The owner authorized every command that does not write flash, so the
READ side of surfaces that were once excluded wholesale is now
implemented (`src/protocol/system.rs`, one `Xtion` method each):

- GET_LOG 7 (text/overflow/structured entries), GET_CPU_STATS 27,
- GET_TEC_DATA 30 / GET_FAST_CONVERGENCE_TEC 38 (post-5.4 layout),
- GET_EMITTER_DATA 32, GET_CMOS_BLANKING 35 (direct, distinct from
  ALGORITHM_PARAMS 0x06),
- I2C_READ 11 + CMOS register reads via the fixed-params bus/slave
  (`XnHostProtocolGetCMOSRegisterI2C` parity), READ_AHB 20,
- flash file-system READS: DOWNLOAD_FILE 16, GET_FLASH_MAP 18,
  GET_FILE_LIST 19, READ_FLASH 25,
- BIST 28 (the updater runs it continuously; 300 s-class timeout).

Covered by the `system_commands_readonly` hardware test (reports, does
not gate). Still excluded: every WRITE on these buses (see section 3).

### Phase 4 — large surfaces (only on explicit request)

14. Audio (STREAM2_MODE=7 + PARAMs 8-11/33-35 + packets 0x9200): a full
    USB Audio Class implementation; disabled even in OpenNI 2.2.0.
15. Remaining stream modes as convenience surface: QVGA/60 fps image,
    SXGA/720p/1280x960 Bayer image, QVGA depth, 25 fps depth/image
    modes, YUYV/Bayer image inputs — configuration only, the machinery
    (ring, assembler, decoders) already exists; each needs a preset-
    validated mode + a hardware test.
16. NESA debug channel (packet 0xd201) and TEC debug (0xd200) capture —
    diagnostic curiosity, no product value known.

## 3. Excluded features (never in this driver)

Per the safety rule, these are **not planned**, not merely deferred.
Since 0.2.8 the exclusion list covers only the WRITE side; the matching
reads are implemented (Phase 3a).

| Feature | Opcodes/params | Why excluded |
|---|---|---|
| Flash file system WRITES (upload/delete/execute/attributes) | 13, 14, 15, 17, 23, 24, PARAM 85 | writes firmware, parameter images and calibration files to device flash; a bug here can brick the camera or wipe per-sensor calibration |
| TAKE_SNAPSHOT | 12 | writes a frame to device flash |
| Raw I2C bus writes | 10 | direct CMOS register writes can leave a sensor in a bad state |
| Raw AHB bus writes | 21 | writes arbitrary SoC memory |
| TEC calibration | 29 | writes calibration data |
| Emitter calibration | 31 | writes emitter power calibration |
| Projector-fault calibration | 33 | writes safety-threshold calibration |
| SET_MODE (all modes except SOFT_RESET) | 6 | REBOOT/MAINTENANCE/SYSTEM_RESTORE change persistent device state; only the fire-and-forget SOFT_RESET (mode 3) of the C's init path is issued (0.2.7) |
| SET_LED_STATE | 41 | harmless but pointless: the Xtion has no LED |
| Reset sensor (power/reboot property) | XN_MODULE_PROPERTY_RESET | forces a device reset; the soft reset covers the need |

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
