# PS1080 Firmware 5.8.22 — Reverse-Engineered Overview

This document describes what the ASUS Xtion's firmware actually is, what it
does, and which of its features the xtion-rs driver does not use yet. It is
based on static analysis of the official FWUpdate 5.8.22 package mirrored in
[`firmware/`](../firmware/): string extraction (the firmware's log strings are
UTF-16LE), Ghidra 11.3 decompilation of the ARM Thumb payload, the
same-vintage Windows driver DLL (`XnDeviceSensorV2.dll`, built June 5 2013,
shipped in the same package), and the OpenNI 2.2.0 PS1080 driver source
(Apache-2.0) as the authoritative protocol reference.

## 1. What the firmware is

| Property | Value | Evidence |
|---|---|---|
| Product | ASUS Xtion / Xtion Pro Live (PrimeSense RD108x reference design) | `RD108x-112` platform string at 0x2a6 |
| Firmware version | 5.8.22, built **Jun 5 2013 23:43:05** | `v%h.%h.%h (%s, %s)` format + date strings at 0xa66 |
| File format | "Packer"-wrapped image (`../Packer/source.out.bin` string at 0xbdc) | header `[u16 type][u16 build 22][u16 version 0x0508][sizes/checksums]` |
| CPU | ARM926-class, **Thumb** instruction set (TI toolchain, `.out` convention, 16-bit `wchar_t`) | 107 push/152 pop/38 bx-lr patterns; capstone Thumb decode over 86% of image; vector entries with bit0 set |
| Main image | `source.out.bin.fla`, 153,704 B, flashed at 0x2A960 (update zone) / 0x10000 (factory) | `UsbUpdate_RD108.ini` flash map |
| Boot manager | `BootManager.out.bin.fla`, 23,214 B, factory offset 0x2000 | ini + its own flash init strings ("Set Fmif Burst: …") |
| Parameter images | 13 further `.fla` files (per-sensor calibration, defaults, CMOS configs) | ini `[File1..File14]` |
| Language/toolchain | C with TI ARM compiler; UTF-16LE strings, `%h`/`%D` format specifiers | strings |

The camera this repo was validated against reports `firmware 5.8.22, chip
0x00212020 (PS1080-A6), FPGA 0x0005 (RD1082)` — exactly this build.

## 2. Boot and self-test (from the boot-time log strings)

The main image's first 1.5 KB is a UTF-16 log-message table. Read in address
order it reconstructs the power-on sequence:

1. **TEC** (thermoelectric cooler): `TEC OR`, `TEC OR2`, `TEC TO`, `TEC CRS`,
   `TEC PRB`, `TEC OK` — overrange, timeout, current-sense and probe checks
   on the TEC control loop.
2. **PRJ** (IR laser projector): `PRJ F`, `PRJ FSL`, `PRJ FSH`, `PRJ OK` —
   projector power verification (fail-slow/fail-shut states).
3. **CLR** (color CMOS): `CLR C %d`, `CLR V %X %X`, `CLR ND`, `CLR W %X %X`,
   `CLR OK` — register count, read-back validation, no-data and write tests.
4. **IR** (IR CMOS): `IR V %d %d`, `IR ND`, `IR TO`, `IR W %x %x`, `IR OK`.
5. **FL** (flash): `FL R/W/B/BR/BW/S/P %d…`, `FLF OK` — block read/write and
   file-system checks.
6. **NESA**: `NESA OK / DISABLED / N-A / INV / ERR / TIME` — a per-sensor
   subsystem (own params image `SensorNesaParams.xml.bin.fla`, own debug
   channel 0xd201, referenced by the updater's `RunBISTNESAUnlimited`
   finish-function). PrimeSense never documented what NESA is; from its
   calibration/test usage it is a safety/monitoring block associated with the
   projector and TEC.
7. Stream watchdogs and quality checks run continuously afterwards:
   `Error! No Data arrived!`, `Error! Timeout IR/depth! (%d)`,
   `Error! Safety!`, `illegal fixed params`, **`depth homogeneity failed`**
   (an in-firmware depth quality check), and a **zero-order (ZO) monitor**
   (`ZO %d: [%d, %d] -> [%d, %d] (exp %d)`, `under/over/badZoArea/goodZo`)
   that validates the projected pattern's zero-order images per frame.

The updater ini confirms the machinery: `InitFunction=RepairFW5327ZoneB,
RepairFW570114ZoneA,ReviveSensorFault,NESI`, `FinishFunction=
RunBISTNESAUnlimited` — the firmware can repair its own flash zones, revive a
faulted sensor, and re-run BIST after an update.

## 3. Runtime architecture

```
USB device (VID 1d27)
├── EP0 control: "PsMonitor" command protocol (vendor requests 0x40/0xC0)
│     16-bit opcodes, 86 params, version handshake, log, file system, BIST,
│     TEC/emitter calibration, raw I2C/AHB access
├── EP 0x81 bulk-in: depth stream  (packets 0x7100/0x7200/0x7500)
├── EP 0x82 bulk-in: image stream  (packets 0x8100/0x8200/0x8500)
├── EP 0x83 bulk-in: misc stream   (AUDIO 0x9200, GMC 0xa200,
│     GMC_DEBUG 0xb200/0xb500, WAVELENGTH_CORRECTION_DEBUG 0xc200,
│     TEC_DEBUG 0xd200, NESA_DEBUG 0xd201, PROJECTOR_FAULT 0xdead,
│     OVERHEAT 0xf31f)
└── USB Audio Class interface "Microphone" with a Feature Unit
      (volume/mute) — the Xtion Pro Live's dual mics, 16-bit PCM,
      8–48 kHz, stereo, per-channel volume/mute over the same protocol
```

- **Command protocol ("PsMonitor")**: requests `{u16 magic "MG", u16 size_words,
  u16 opcode, u16 id}`; replies `{u16 magic "BR", …, u16 error, data}`. The
  firmware implements **43 opcodes plus KILL (999)** — see §4.
- **Streaming**: the firmware chunks frames into 12-byte-header packets
  (`buf_size` big-endian — the quirk xtion-rs verified live). Depth pixels are
  shift values; the shift-to-depth conversion constants come from
  GET_FIXED_PARAMS + ALGORITHM_PARAMS(DEPTH_INFO).
- **Depth pipeline**: the PS1080's FPGA does pattern correlation; the ARM
  firmware orchestrates the CMOS sensors over I2C (`I2C_READ/WRITE` opcodes),
  manages blanking (CMOS_BLANKING opcodes + presets), monitors homogeneity
  and ZO quality, and can change the projector modulation frequency
  (ALGORITHM_PARAMS FREQUENCY — used for multi-camera interference avoidance).
- **Audio**: a full USB Audio Class microphone implementation with Feature
  Unit; sampling rate, stereo mode, per-channel volume and mute are params
  8–11/33–35.
- **Accelerometer**: log strings `ACC LR %D %D %d %D %d %d`, `ACCOR`, `ACCTEC`,
  `ACCM`, `ACCI`, `ACCOP` — the Xtion's accelerometer is sampled (left/right
  and at least two more axes), used for orientation detection and TEC control.
- **Log system**: hundreds of UTF-16 log strings + `GET_LOG` opcode, log
  filter/interval params, and a "Log Overflow" NACK path in the host driver.
- **Flash file system**: opcodes 13–19 + 23–25 (upload/download/delete/list/
  attributes/execute), `FILE_SYSTEM_LOCK` param, and file-related NACK codes.
  This is how parameter images and firmware are updated — with the
  `UploadInProgress.fla` marker at 0x3F9C0 making updates power-loss safe.
- **Thermal/projection control loop**: TEC set point + calibration opcodes,
  emitter set point + calibration opcodes, projector-fault calibration and
  the PROJECTOR_FAULT event packet, OVERHEAT packets, and `OT %d` /
  `FRQ %d` (temperature/frequency) telemetry strings.
- **GMC** (Global Motion Compensation): a depth-processing mode (params
  36–44, 61, 75) with a point stream on the misc endpoint (up to 100 points
  per packet) and debug channels. Decoded since 0.3.5: plain 0xa200
  points (`XnDeviceSensorGMCPoint`), debug 0xb200, summary 0xb500.

## 4. The complete command surface vs xtion-rs

Full tables with per-item notes: `firmware/analysis` artifacts; summary:

| Surface | Total | xtion-rs uses |
|---|---|---|
| Command opcodes (0–42 + 999) | 41 (+KILL) | **7** (0,1,2,3,4,22,37); 0.2.0 adds 39/40/42 — see [`FEATURE_PLAN.md`](FEATURE_PLAN.md) |
| Firmware parameters (0–85) | 85 | **8** (5,6,12,13,14,18,19,20); 0.2.0 adds 13 more |
| Bulk packet types | 15 | **6** (depth+image); 0.2.0 handles OVERHEAT/PROJECTOR_FAULT events |
| ALGORITHM_PARAMS data | 6 | **1** (DEPTH_INFO) |

### 4.1 Opcodes not used by xtion-rs 0.1.x (grouped)

> 0.2.0 implements most of groups 1–6 below; see
> [`FEATURE_PLAN.md`](FEATURE_PLAN.md) for what landed and what is next.

- **Identity / trivial wins**: `GET_PLATFORM_STRING` (39), `GET_USB_CORE_TYPE`
  (40 — the Jango-core delay optimization), `GET_MODE` (5), `SET_MODE` (6).
  (`GET_SERIAL_NUMBER` 37 was already wired up in 0.1.x.)
- **Events ignored by the stream parser**: OVERHEAT (0xf31f),
  PROJECTOR_FAULT (0xdead) — the firmware actively reports thermal and
  projector-health events; the driver skipped them.
- **Audio**: STREAM2_MODE param 7 + AUDIO_BUFFER 0x9200 + params 8–11, 33–35.
- **IR stream**: STREAM0_MODE=3 (the image pipe, not the depth pipe — the
  frames arrive as IMAGE packets with 4x10-bit packed pixels) + params
  25–29, 72, 56–60 (a full parallel stream configuration).
- **Image (color camera) controls**: params 15–17, 71, 76–82 (AGC, quality,
  flicker detection, mirror, sharpness, auto white balance, color temperature,
  backlight compensation, auto/manual exposure, low-light compensation) and
  image crop params 46–50.
- **Depth extras**: AGC 21 (+ bins 63–70), hole filter 22, mirror 23,
  decimation 24, white balance 45, crop 51–55, **close range 84**, GMC
  36–44/61/75, wavelength correction 73–74.
- **Registration**: param 2 (hardware, in-firmware on PS1080) + ALGORITHM
  REGISTRATION (0x02) tables — depth-to-color alignment.
- **Diagnostics**: GET_LOG (7) + log filter/interval params 30–31,
  GET_CPU_STATS (27), BIST (28), TEC data (29/30/38), emitter data (31/32),
  projector-fault calibration (33), CMOS blanking/presets (34–36).
- **System / factory**: I2C (10/11), AHB bus access (20/21), flash file
  system (13–19, 23–25), TAKE_SNAPSHOT (12), ENABLE_EMITTER (42),
  SET_LED_STATE (41 — reference designs only; the Xtion has no LED),
  FRAME_SYNC/TRIGGER_ACTION/STREAM_PRIORITY params (1/3/4).

### 4.1.1 Hardware-verified opcode behaviour on THIS build (fw 5.8.22)

The 2026-11-23 hardware runs settled four opcodes of the diagnostics
surface (details in docs/PROTOCOL.md, "Flash file system ... quirks"):

- **ENABLE_EMITTER (42)**: NACKs **BadParams** — not InvalidCommand —
  although the request is byte-identical to the reference's single-u16
  `XnVSetEmitterStateRequest` (XnHostProtocol.cpp:3420-3445) and the
  firmware (5.8.22) meets the C's own >= 5.8.15 gate. The opcode IS
  dispatched; its handler refuses the payload. Likely a build-specific
  (laser-safety) refusal.
- **GET_CPU_STATS (27) / GET_FLASH_MAP (18)**: NACK InvalidCommand on
  this build — yet the image carries the 15-task name list the stats
  WOULD report (`c,T,DC,TD,PK,IK,DK,O,SM,HM,TR,DC,PC,TC` at offset
  0xd1e, units at 0xd6c: `s,ms,c,c,c,c,c,c,...`): features compiled
  in, opcodes locked.
- **GET_CMOS_BLANKING (35)**: the direct reply is shorter than the u32
  the C parses; the ALGORITHM_PARAMS(0x06) route is the working path
  (and the one every stream start uses).
- **GET_FILE_LIST (19)**: first entry sane (the FileTable itself:
  offset 0x1000, 4096 bytes), the reply tail misparses into flag-pattern
  values — the on-flash table at 0x1000 is the ground truth (the
  hardware test dumps it).
- The log system's runtime messages embed their own timestamps as the
  `%lu` prefix (`%lu Error! Timeout IR! (%d)`, `%lu ZO %d TIME %lu`,
  ... at 0x4d6-0x808): the same tick count the log-entry header
  carries, which makes on-hardware log decoding self-checking.

### 4.2 Highest-value additions for xtion-rs (recommended order)

1. **OVERHEAT + PROJECTOR_FAULT handling** — safety-relevant, tiny effort.
   *Landed in 0.2.0 (`DeviceEvent`).*
2. **GET_PLATFORM_STRING + GET_USB_CORE_TYPE** — device identity, trivial
   request/reply commands. *Landed in 0.2.0.*
3. **IR streaming** (STREAM0_MODE=3 + IR params, 10-bit packed decode) — a
   whole second use of the sensor. *Landed in 0.2.0 (`start_ir`).*
4. **Registration** (param 2, done in firmware on PS1080) — depth↔color
   alignment, M7 roadmap item. *Landed in 0.2.0 (`set_registration`).*
5. **ENABLE_EMITTER** — IR projector on/off: power saving, laser safety, and
   required for capturing IR-illuminated scenes. *Landed in 0.2.0.*
6. **Mirror + CloseRange + hole filter + AGC + image controls** — one-line
   SET_PARAM features with visible user value. *Landed in 0.2.0.*
7. **Audio** — mic capture over the misc endpoint + volume/mute params.
8. **GET_LOG / TEC / CPU stats** — field diagnostics and thermal monitoring.

## 5. Method and caveats

- Binary-level confirmation: the opcode dispatcher sits at image offsets
  **0x21400–0x22000** — a ~3 KB chain of 78 Thumb `cmp rN, #imm8`
  instructions covering 31 distinct opcode values that match the vendor
  enum exactly (`0x1e`=30 GET_TEC_DATA … `0x26`=38 GET_FAST_CONVERGENCE_TEC,
  plus 41 SET_LED_STATE at 0x21fd2); the v2.6 protocol magic pair
  `MG`/`BR` sits as adjacent u16 constants at 0xc14/0xc16. Ghidra 11.3
  (ARM:LE:32:v5t, Thumb) recovered 165 functions / ~45,000 instructions
  (149 decompiled); the dispatcher region itself does not decompile cleanly
  (jump tables on a base-0 raw image), so it was verified by direct
  capstone disassembly instead.
- The opcode/param tables are the vendor's own (OpenNI 2.2.0 PS1080 driver
  source and the June-2013 `XnDeviceSensorV2.dll` from the same package);
  the firmware strings confirm the subsystems exist in
  5.8.22 (TEC, emitter, projector fault, BIST, NESA, GMC, audio, accelerometer,
  file system, CPU stats, log system).
- The Ghidra project loads the image at base 0; absolute addresses in
  decompiled code are image-relative. The Packer header (first 12 bytes +
  size/checksum records) is metadata, not loaded content.
- The decompiled C dump is a derived work of proprietary firmware and is
  therefore kept out of this repository (analysis artifacts stay in the
  research workspace).
- `SET_LED_STATE` exists in the protocol but the Xtion has no LED hardware;
  expect it to be a no-op or NACK.

## 6. Sources

- `firmware/FWUpdate_5_8_22.zip` — official ASUS package (dlcdnet.asus.com,
  SHA-256 `6f9c0ec9…43caad`), extracted with libarchive (LZX CAB).
- OpenNI 2.2.0 source, `Source/Drivers/PS1080/**` (occipital/OpenNI2
  v2.2.0-debian tarball) — `XnHostProtocol.h` (opcodes, NACKs),
  `XnParams.h` (params), `XnDeviceSensorProtocol.h` (packet types),
  `XnSensorAudioStream.cpp` (audio formats), `PS1080.h` (module properties).
- `XnDeviceSensorV2.dll` strings — property names and subsystem classes of
  the same-vintage Windows driver.
- Ghidra 11.3.2 headless, ARM:LE:32:v5t (Thumb) — function discovery and
  decompilation of `source.out.bin.fla`.
