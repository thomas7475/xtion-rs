# idong100 Sensor — Rust Driver Realization Draft

**Status:** draft v1.0 · 2026-11-23
**Author line:** idong100 research, turn 10 (Rust driver draft)
**Scope:** turn everything the research family has established (branches
`research/idong100-01`, `-02`, `-09`, the EEPROM dump, the xeyez PC stack
decompilation, the Sports King installer file list, and the official chip
specifications recovered from toltech.cn this turn) into a concrete plan and
code-level structure for a **pure-Rust, user-space USB driver** for the
Toltech **idong Sensor V2.6** — in the spirit of, and structurally mirroring,
the proven `xtion-rs` crate (PrimeSense PS1080 driver, same author).

This document is the "what we build and how" contract. The accompanying
skeleton crate lives at `research/idong100/rust/idong/` in this branch.

---

## Part I — Research synthesis (everything the driver must implement)

### 1.1 The physical device (hardware architecture)

```
                    USB 2.0 High-Speed (0471:2600 -> 0471:2500)
                                  |
              +-------------------+---------------------+
              |        idong Sensor V2.6 (识别器)         |
              |                                           |
              |  +------------+   +------------------+    |
              |  | 8051 MCU   |   | Blackfin BF527   |    |
              |  | (MCS-51)   |<->| USB + DSP host   |    |
              |  | sensor ctl |   | (ens_code.ldr)   |    |
              |  +-----+------+   +--------+---------+    |
              |        | I2C/CrossBus       |              |
              |  +-----+----------+   +-----+----------+  |
              |  | Spartan-3 FPGA |   | T24C16A        |  |
              |  | image pipeline |   | EEPROM 2 KiB   |  |
              |  | (A_a_v4.4_     |   | stereo calib   |  |
              |  |  BG0313.bit)   |   | + serial       |  |
              |  +-----+-----+----+   +----------------+  |
              |        |     |                              |
              |  +-----+---+ +--+-------+   +-----------+  |
              |  | BG0316  | | BG0316   |   | 6x IR     |  |
              |  | CMOS L  | | CMOS R   |   | LED 850nm |  |
              |  | 640x480 | | 640x480  |   | + lenses  |  |
              |  | @60fps  | | @60fps   |   +-----------+  |
              |  +--------+ +----------+                  |
              |      + 2.4 GHz RF receiver (handles)      |
              +-------------------------------------------+
```

Verified facts (sources: owner hardware probe + T24C16A dump, Nikkei
Electronics teardown of the i-dong sensor, toltech.cn product pages, and the
recovered Windows driver stack):

- **Three-chip pipeline**: BF527 Blackfin (USB device controller + DSP,
  firmware `ens_code.ldr`), Xilinx Spartan-3 FPGA (image pipeline,
  `A_a_v4.4_BG0313.bit`, "work_top.ncd" 2010-04-06), MCS-51-compatible MCU
  (sensor control, `app_m8051_2500/2611.hex`). All three firmware images are
  **in hand** (v2.5.07/v2.6.0.6 zips, `$_6_` runtime dir).
- **Two Brigates BG0316 CMOS sensors** (640x480 @ 60 fps) behind the FPGA,
  stereo baseline **exactly 100.00 mm** (T24C16A EEPROM: T=(99.5627, 0.4639,
  9.3304) mm, |T|=100.0000; OpenCV-convention intrinsics, cx ~312-313 px ->
  640-px frames, mean f=432.33 px, |k1|<0.05).
- **6 IR LEDs at 850 nm** with collimating lenses; the tracked objects are
  **passive retro-reflectors** (ball on the wand tip, hat strip, wrist/foot
  bands, "airtouch" wristband with an RF handle inside).
- **2.4 GHz GFSK RF link** (2400-2525 MHz, 2 Mbps) from the handheld
  controller(s) into the sensor bar; button/mic data is forwarded over USB.
- **On-device stereo processing**: the idong100 chip kit spec (recovered from
  toltech.cn this turn) — "dual image-processing units, detect and track
  multiple moving targets, high-speed 3D coordinate computation; embedded
  MCS-51 MCU; UVC & UAC support and IR reception; detection distance up to
  5 m; up to 15 simultaneous targets; BG0316 640x480@60 fps; bootloader with
  automatic enumeration and host-side firmware download; CrossBus; USB 2.0
  HS with multiple control endpoints; 6-channel DMA". Marketing spec of the
  finished sensor: 2 mm precision at 5 m, 20+ points @ 60 fps (v2.x claims
  vary by platform generation; `DevicePlatform=2` in the shipped config).
- **The host never does stereo matching at runtime** — it receives computed
  3D marker coordinates. (Host-side OpenCV code exists only for
  calibration/verification tools: `StereoCamera.dll` `Get3DObject`/
  `Get3dSingle`, `MiddleCameraCalib.dll` `UserCalibMiddle` via cvSVD.)

### 1.2 USB lifecycle: bootloader <-> runtime

`xeyez.inf` (DriverInstall/, both 2.5.07 and 2.6.0.6):

| USB ID | INF name | Role |
|---|---|---|
| `0471:2600` | TOLTech xeyez Bootloader | **the physical unit as plugged in** — BF527 boot ROM state, no strings, bcdDevice 0.00, one vendor interface (class 0xFF), bulk EP **0x02 OUT** + **0x82 IN**, maxpacket 64 (full-speed value at high speed — accepted quirk) |
| `0471:2500` MI_04 | TOLTech xeyez v2.5 | **runtime composite device** — interfaces 0-3 are standard video/audio (DriverConfig.ini ships `usbvideo.sys`/`USBAUDIO.sys` file lists -> UVC chat camera + UAC mic array), **interface 4** is the vendor bulk interface bound by `xeyez.sys` |
| `0471:2100` / `0471:0668` / `0471:0666` | v2.1 / v2.0 / v1.0 | older generations (8051-only platforms; `xeyezHal.ini` carries `xeyez_v20.hex` / `xeyez_v21.hex` for them) |

`xeyez.sys` is the Microsoft **BulkUSB sample** retargeted at the BF527
(strings: "BF527 Bulk USB DriverEntry"), service `XEYEZ`,
`MaximumTransferSize=4096`, exposing `\\.\xeyezUsbDriverDeviceN` via
interface GUID `{77f49320-16ef-11d2-ad51-006097b514dd}`.

**Consequence for the Rust driver:** the lifecycle is *enumerate as
bootloader -> host uploads firmware over bulk EP 0x02 -> device re-enumerates
as 0471:2500 composite -> claim MI_04 and speak the runtime protocol*. The
RGB camera and microphone are then plain UVC/UAC and need **no custom driver
at all** — the Rust driver only owns the vendor bulk interface.

### 1.3 Windows host stack -> how the camera works with the driver

The shipped user-space stack (extracted + decompiled, agent-09):

```
xeyez.exe / xEyeService.exe / xeyezMonitor.exe        app, NT service, watchdog
  +- BasicInterFace.dll                               UI glue
      +- xEyeClass.dll                                reader thread:
          |  GetADataFromUSB(xEye3Ddata&)  @50 fps    (named pipes to apps,
          |  TransUSB2Pipe()  PipeBufferSize=9000      \\.\pipe\*, MaxRead=1)
          +- xEyeUSB.dll                              device manager:
              |  InitxEyeUSB() = firmware upload + boot state machine
              |  GetxEyeUSBID()/GetxEyeUSBState(), handle match
              +- EasyUSB.dll                          transport: ReadData/
                                                         WriteData, SetupAPI
                    |                                  GUID enum, 16 KiB
                    |                                  overlapped chunks,
                    +- xeyez.sys (kernel)              3 retries x 100 ms
```

Runtime data path, end to end:

1. FPGA (Spartan-3) grabs both BG0316 frames, runs the configured mode:
   **blob mode** (`Rolling/Global_Blb_*`: on-chip centroid extraction) or
   **image mode** (`*_Img_*`: 2-sensor difference images streamed raw — the
   "30fps" folder holds global-shutter 30 fps variants).
2. BF527 DSP matches blobs between the two cameras, triangulates with the
   factory EEPROM calibration, assigns track IDs, and emits an `OD` frame
   with the 3D marker record (~50 fps; `xEyeFrame=50`, `USBComFPS=35`).
3. `xEyeClass.dll` validates the record (frame-counter continuity check at
   record +0; per-marker quality filter — slot field must be >= 50), converts
   to the 0x208-byte pipe record and broadcasts over named pipes.
4. Consumers: the Lobby app (`xEye2DEngine/Engine/MsgEngine`), games
   (`iDongTTCamera.dll` in ping-pong, `iDongJump.dll` in jump-sports), the
   `iDongAction3.0` middleware (analysis plugins: 3DMouse, Hitting, Running,
   Hammer, Circle, Fighting, Wave, Reflux, RPGMove, Lining), virtual input
   injection (`vKeyboard.sys`/`vMouse.sys` HID), and quality monitoring
   (green/yellow/red thresholds from `xEyeUsb.ini`).

The **driver replacement** (our crate) occupies the bottom two layers
(EasyUSB+xeyez.sys -> `usb.rs`; xEyeUSB -> `boot.rs`+`runtime.rs`;
xEyeClass -> `stream.rs`+`markers.rs`), and offers the pipe consumers'
functionality through a normal Rust API.

### 1.4 Wire protocol (host side, decoded from xEyeUSB.dll/EasyUSB.dll)

**Frame format** — `[u32 magic][u16 type][payload...]`, little-endian:

| Direction | magic (LE bytes) | value |
|---|---|---|
| host -> device | `"PU\x7f\x7f"` | `0x7f7f_5550` |
| device -> host | `"OD\x7f\x7f"` | `0x7f7f_444f` |

I/O primitives (EasyUSB.dll): bulk read/write on EP 0x82/0x02 in up to
0x4000-byte chunks, 3 retries, GetTickCount timeouts. Known timeout
constants in the boot code: 3000 ms (ready / CRC-result / device-ID poll),
6000 ms (per firmware packet), 1000 ms (DSP-reset ACK, 0xFFF handshake),
300 ms (device-ID re-poll, <=10 retries).

**Boot / firmware upload (`_InitBlobUpload` in xEyeUSB.dll):**

1. TX *DSP-start* -> RX **type 2** `"Ready to load program!"` (3000 ms).
   (Error string "Tx DSP Start Err" / "Failed to receive: Ready to load
   program!")
2. TX **type 2** = packet count + CRC ("Tx packet count & crc!").
3. Stream the firmware blob in **64-byte packets**, each as **type 3**
   (6000 ms timeout each; on failure: "Error transfer: send program code
   error"). Which of the three firmware files goes first is confirmed for
   `ens_code.ldr` (BF527); `xeyezHal.ini`/`xEyeUsb.ini` show the full set:
   `ens_code.ldr` (BF527) + `A_a_v4.4_BG0313.bit` (Spartan-3) +
   `app_m8051_*.hex` (8051). Exact ordering/staging: to be confirmed via
   USBPcap or `ens_code.ldr` disassembly (open item, section 2.7).
4. RX **type 3** = CRC result; payload status word 0xF at frame offset +8 ->
   "Error: crc result is not correct!" (CRC variant still to identify).
5. Device-ID exchange: RX **type 2** ("Device ID is received!", logs
   `DEVICE ID =` + DSP version), 300 ms poll x <=10.
6. "Get DSP Reset ACK OK" -> TX **type 7** (1000 ms).
7. Handle-config read/write + backup ("Read Handle Cfg" — airtouch
   wristband pairing) and lens-parameter stage.
8. Startup complete ("启动流程结束OK") -> 50 fps coordinate streaming.

**Runtime protocol (types seen in disassembly):**

- Handshake pair: RX **type 0xFFF** -> host answers TX **type 0xFFE**
  (1000 ms) — periodic keep-alive/link check.
- RX **type 4** (link status): payload word at +8 == 1 -> reply TX type 4
  with word **0x11**; payload words at +0xC/+0x10 == -1 -> internal state 3 /
  command 2, else state 4 / command 3; word at +8 == 0xF -> another TX type 4
  / 0x11 path.
- Handle match (wristband pairing): TSOL_DEVICE_HANDLE_FOUND /
  NOT_FOUND / TIMEOUT / FOUND_OVER ack types; `NeedHandleMatch=1` and
  `MonitorList=10` in `xEyeUsb.ini`.
- DSP reset: TX **type 7**.
- Coordinate records: device->host frames carrying the xEye3Ddata payload.

**xEye3Ddata record** (structural layout, from `GetADataFromUSB` @
xEyeClass.dll RVA 0x20f0):

```
offset  size  field (decoded semantics; confidence noted)
+0x000  u32   frame counter, read at [arg+0] (continuity: delta ≤ 1)
+0x000  10 ×  slot, 88 bytes each (0x58) — addressed directly at arg+i*0x58:
   +0x18  f64   per-marker value compared against a constant (timestamp or
                distance gate)
   +0x24  i32   per-marker quality/age — must be ≥ 50 to pass the filter
+0x370  72 B  trailer (aggregates — noise/disturbance/timestamps; fields
               not yet attributed)
+0x3B8  u32   mode flag (==1 → frame skipped by the pipe writer)
minimum 0x3BC (956) bytes; the pipe record published to apps is 0x208
(520) bytes after transformation (method RVA 0x25e0) — field-level mapping
of the transform is an open item. (Note: the slots start at +0, so the
frame counter overlays slot 0's first u32; there is no separate header.)
```

### 1.5 What information we can get from the camera

The driver's data products (each = a crate API):

| # | Product | Source | Notes |
|---|---|---|---|
| 1 | **3D marker coordinates** (X,Y,Z, device frame, mm) | vendor bulk EP 0x82, OD frames | up to 10 slots/record observed (chip spec: 15 targets on idong100, 20+ on idong200); 50 fps; 2 mm @ 5 m claimed |
| 2 | **Frame counter + timestamps** | record +0 / slot +0x18 | continuity/drop detection built into the host stack ("丢帧" path) |
| 3 | **Marker quality/confidence** | slot +0x24 (>=50 pass) | drives the green/yellow/red link-quality indicator (thresholds 20/35/35, NoiseLevel 50, RingSize 200) |
| 4 | **RF handle events** (wristband/wand buttons, pairing) | TSOL handle-match types | 2.4 GHz RF terminates in the sensor bar; TSOL_*_ACK sequence |
| 5 | **RGB video** | UVC interface of the runtime composite device | 72° chat camera, VGA@30 fps — standard UVC, no custom code |
| 6 | **Microphone array audio** | UAC interface | mic array with AEC/beam options (AnySense claims) |
| 7 | **IR illumination state** | FPGA LED-scale registers 0x58-0x6B (`Led_Scale_Fpga_v26.txt`) | LED drive level control |
| 8 | **Sensor mode** (blob vs raw-image, rolling vs global shutter, 30/60 fps) | CMOS/FPGA register tables (`*_0316_Cmos*.txt`, `*.dat`) | image mode streams 2-sensor difference frames for host-side CV (the OpenCV games use this) |
| 9 | **Factory calibration + identity** | T24C16A EEPROM over the link (or dumped) | stereo intrinsics/extrinsics (OpenCV convention), serial `AB460DC8BA12C6840B49CD5786C329DD`, version codes "000T"/"2613" |
| 10 | **Device/DSP identity at runtime** | device-ID exchange, DSP version string | boot step 5 |
| 11 | **Raw blob/image data** | image mode + `classifying.dll`-style host post-processing | SVM ball/head classifiers ship in the runtime dir — replayable offline |

### 1.6 Useful files in the Sports King installer (bin-files.txt, 21,995 files)

The installer `idong_setup.exe` (爱动体感运动机 运动王PC版 v1.9.04, 4.58 GB,
16 slices) is fully listed in `upload/bin-files.txt`; extraction with
`inno_bin_extract.py` is verified. Ranked RE targets once the .bin slices are
assembled:

**Tier 1 — the camera/driver interface:**

| Path | Size | Why |
|---|---|---|
| `{app}\Games\pingpang\game\iDongTTCamera.dll` | 108,872 B | The only in-tree *game-side* camera client: shows how a consumer subscribes to marker data and maps it to table-tennis physics (complements the pipe-producer side we already decoded) |
| `{app}\Games\pingpang\iDongAction.ini` / `iDongData.ini` / `iDongMarkManager.ini` / `iDongTableTennis.ini` / `iDongTTInterFace.ini` / `iDongTTLogo.ini` | 273-5,225 B | Per-game runtime configuration: action-model selection, marker management, TT interface tuning — the *semantics* of the marker stream fields |
| `{app}\Dlls\iDongAction3.0\` (complete middleware) | ~30 files | `iDongAction/ActionFrame/Data/List/Log/Platform` + `iDongSimulator3.exe` (158 KB) + `iDongMarkManager.dll` + analysis plugins `iDong3DMouse/Circle/Fighting/Hammer/Hitting/Lining/Signal/Wave` (+ game copies `Reflux/RPGMove/Running`) |
| `{app}\Dlls\iDongAction3.0\iDongSimulator3.exe` + `iDongSimulator3.ini` (wajinkuang) | 158 KB / 917 B | **Session simulator** — replays motion input without hardware; the ideal golden-source test bench for our driver's output format |
| `{app}\Lobby\xEye2DEngine/Engine/MsgEngine/UserParamRebuild.dll` | 13-33 KB | Pipe *consumers* inside the Lobby — the downstream API contract |
| `{app}\Lobby\Camset.dll`, `CapStill.dll/.exe`, `ExeXeYeZSetting.exe`, `XeYeZSetting.dll`, `AboutXeYeZDlg.dll` | 223-350 KB | Camera settings UI (brightness/color/saturation/exposure sliders confirmed by `images\CameraSetting\*`); maps which UVC controls the vendor actually sets |
| `{app}\Lobby\conf\camera_set\settings.sol` (+`#local`) | 550/75 B | Persisted camera settings (Flash LSO) — real-world value ranges |

**Tier 2 — host-side CV and analysis:**

| Path | Why |
|---|---|
| `{app}\Lobby\RHCCA/RHCCEXTV/RHCCP/RHCCR/RHCCS/RHCSVE.dll` (RHCCEXTV = **2,197,832 B**) | Unidentified "RHCC" family — the largest CV binary in the Lobby (2.2 MB); likely the extended-vision sports-analysis engine. PE inventory pending extraction |
| `{app}\Lobby\HandSignal.dll`, `IdentifyModule.dll`, `DxDetect.dll` | Gesture recognition (hand signals), module identification, DirectX/environment detection |
| Games shipping OpenCV 2.0 (`cvaux200/highgui200/ml200/opencv_ffmpeg200.dll`): shuiguoaidong, naolifengbao, shuiguoshuangren, courageoftheroad, jiangshikuaipao, tiaoyueyundong | Proof of host-side CV consumers (image mode); `iDongJump.dll` (121 KB) + `iDongAnlysis.xml` in tiaoyueyundong |
| `{app}\Games\szjc\script\game\camera.lua`, `{app}\Games\mouse\hitmouse\cfg\camera.cfg` | Game-script camera parameters |

**Tier 3 — context (not needed for the driver):** the 36 game folders
(Torque/Ogre/Flash engines), Lobby Flash assets, `WinIo.sys` kernel drivers
(games' direct port I/O — security-relevant, unrelated to the camera),
video-chat stack, sqlite/lobby plumbing.

### 1.7 New facts recovered this turn (toltech.cn + press)

- **idong100 chip kit** official page (ProductInfo.jsp?id=989): chip + BG0316
  module; dual image-processing units; MCS-51 MCU; UVC & UAC + IR reception;
  bootloader auto-enumeration with **host-downloaded MCU program**; CrossBus;
  USB 2.0 HS multiple control endpoints; generic camera interface to VGA;
  6-channel DMA (hardware/software trigger); **5 m range, 15 targets**;
  BG0316 = 640x480@60 fps. This corroborates the "no kernel driver needed
  after upload" design and explains the 0471:2600->2500 transition.
- **idong200 chip kit** (id=975): successor, megapixel input, on-chip
  binocular algorithm, directional audio processing — out of scope, but the
  protocol family likely shares framing.
- **AnySense SDK v1.0.21** (ToolsPage.jsp, 2013): official third-party SDK —
  Java/C/C++/C#/Flash/Unity3D bindings; hardware = sensor + airtouch
  wristband + wand; API families: air-mouse library, gesture library, sports
  DB with "perfect 3D-channel data ... precisely matched to video image
  positions", face/voice modules. 2.6-gen sensor BOM/schematic/PCB/3D
  offered behind a contact form. -> the SDK's API shape is a good v1 facade
  model for our crate.
- **No public GitHub/Gitee reverse-engineering** of this device exists
  (searches across idong/toltech/xeyez/RHCC/iDongAction tokens: nothing
  relevant). Our research family is the only public record.

---

## Part II — Driver design

### 2.1 Goals and non-goals

**Goals (v1.0):**

1. Pure-Rust user-space driver (libusb via `rusb`; async ring via
   `libusb1-sys` exactly like xtion-rs) — no kernel code.
2. Full device lifecycle: bootloader discovery -> firmware upload ->
   re-enumeration -> runtime claim.
3. Marker streaming: frames with per-marker id/position/quality, drop
   detection, quality/disturbance level.
4. Handle events (RF wand/wristband) surfaced as an event queue.
5. Calibration/identity: parse the T24C16A layout; expose the factory
   stereo parameters.
6. Read-only tooling first: enumerate/probe examples that never write.

**Non-goals (documented, deliberate):**

- No firmware patching/replacement; only the exact vendor upload sequence.
- No UVC/UAC reimplementation (point users at the kernel's standard
  drivers; expose the interface numbers).
- No host-side stereo vision in v1 (the device already does it); raw image
  mode is a stretch goal.
- No Windows kernel driver; Windows works via WinUSB/libusb with the same
  bulk interface.

**Safety rules (hard constraints, carried over from CONTEXT.md):**

1. **Never write speculative bytes to EP 0x02.** The only permitted TX
   sequences are the ones decoded from xEyeUSB.dll (boot upload steps and
   the documented runtime types 4/7/0xFFE).
2. Read-only until the boot upload is byte-confirmed against a USBPcap
   capture of the vendor stack (or the `ens_code.ldr` disassembly).
3. All firmware files are checksummed before upload; CRC is announced
   before streaming (vendor flow) — a mismatch aborts, never retries
   blindly.
4. udev rule for unprivileged access; no root requirement in the library.

### 2.2 Crate structure (the draft)

Mirrors xtion-rs, adapted to a two-stage (bootloader/runtime) device:

```text
idong/                               crate `idong` (lib name)
+-- Cargo.toml                       rusb, libusb1-sys, thiserror, log
+-- README.md                        usage + status
+-- docs/
|   +-- PLAN.md                      this plan (milestones, test matrix)
|   +-- PROTOCOL.md                  every protocol fact + source citation
|   +-- RECOVERY/                    session backups
+-- firmware/                        NOT redistributed in the crate; README
|   |                                explains which vendor files must be
|   |                                supplied by the user and their sha256
|   +-- verify.rs                    (dev) firmware set checker
+-- reference/
|   +-- golden_boot.txt              captured/expected boot transcript
|   +-- golden_frames.bin            recorded OD frames for decoder tests
+-- src/
|   +-- lib.rs                       crate root + re-exports
|   +-- error.rs                     IdongError, BootError, ProtocolError
|   +-- usb.rs                       discovery (bootloader+runtime), claim,
|   |                                bulk transport, re-enumeration wait
|   +-- device.rs                    Idong: lifecycle facade
|   +-- protocol/
|   |   +-- mod.rs                   constants (magics, types, timeouts,
|   |   |                            endpoints, PIDs)
|   |   +-- frames.rs                Frame codec: encode/decode + magic
|   |   |                            scanner + fuzz target
|   |   +-- boot.rs                  BootUploader state machine
|   |   |                            (DSP-start -> count/CRC -> 64B packets ->
|   |   |                            CRC ack -> device ID -> DSP reset)
|   |   +-- runtime.rs               RuntimeClient: keep-alive 0xFFF/0xFFE,
|   |   |                            link status type 4, handle match,
|   |   |                            device identity queries
|   |   +-- records.rs               xEye3Ddata parser (72B header + N x 88B
|   |                                slots) + pipe-record (0x208) decoder
|   +-- stream.rs                    bulk-stream state machine: magic scan
|   |                                -> 6-byte header -> payload -> frame
|   |                                assembly, ring buffer, drop stats
|   +-- markers.rs                   MarkerFrame / Marker / quality levels
|   +-- handle.rs                    RF handle (wand/wristband) events
|   +-- calib.rs                     T24C16A EEPROM record parser (OpenCV
|                                    stereo params, serial, checksums)
+-- examples/
|   +-- list.rs                      enumerate devices, report lifecycle state
|   +-- probe_bootloader.rs          read-only EP 0x82 probe (Rust port of
|   |                                idong_probe.py — NEVER writes)
|   +-- upload_firmware.rs           guarded full boot (--i-know... flag)
|   +-- dump_frames.rs               runtime marker dump to JSONL
+-- tests/
    +-- frames_golden.rs             codec vs golden vectors
    +-- eeprom_decode.rs             T24C16A checksums + calibration values
```

### 2.3 Module designs (key types — skeleton code exists in this branch)

#### `error.rs` — error model

```rust
#[derive(Debug, thiserror::Error)]
pub enum IdongError {
    #[error("no idong sensor found (looked for 0471:2600 bootloader or 0471:2500 runtime)")]
    NotFound,
    #[error("device is in bootloader state; firmware upload required first")]
    BootloaderState,
    #[error("boot failed at step {step:?}: {reason}")]
    Boot { step: BootStep, reason: String },
    #[error("protocol error: {0}")] Protocol(#[from] ProtocolError),
    #[error("bad frame: magic {magic:#010x}, expected {expected:#010x}")]
    BadMagic { magic: u32, expected: u32 },
    #[error("CRC mismatch after upload: announced {announced:#010x}, device reported {device:#010x}")]
    CrcMismatch { announced: u32, device: u32 },
    #[error("USB: {0}")] Usb(#[from] rusb::Error),
    #[error("firmware file {path} failed integrity check")]
    FirmwareIntegrity { path: PathBuf },
}
```

`BootStep` mirrors section 1.4 step-for-step (`DspStart`, `ReadyToLoad`,
`CountAndCrc`, `PacketStream`, `CrcAck`, `DeviceId`, `DspReset`,
`HandleConfig`) so a failure pins the exact state — same philosophy as the
xtion-rs milestone-1 error paths.

#### `usb.rs` — transport and the two faces of the device

```rust
pub const VID: u16 = 0x0471;
pub const PID_BOOTLOADER: u16 = 0x2600;
pub const PID_RUNTIME: u16 = 0x2500;      // composite; vendor iface = 4
pub const IFACE_VENDOR: u8 = 4;
pub const EP_OUT: u8 = 0x02;
pub const EP_IN: u8 = 0x82;

pub enum DeviceState { Bootloader, Runtime }

pub fn list_devices() -> Vec<UsbDeviceInfo>;      // both PIDs, state, serial
pub struct Transport { /* rusb DeviceHandle, claimed iface */ }
impl Transport {
    pub fn open_bootloader() -> Result<Self>;     // 0471:2600, iface 0
    pub fn open_runtime() -> Result<Self>;        // 0471:2500, iface 4
    pub fn write_frame(&self, f: &Frame, t: Duration) -> Result<()>;
    pub fn read_frame(&self, t: Duration) -> Result<Frame>;   // 64..16384
    pub fn read_raw(&self, n: usize, t: Duration) -> Result<Vec<u8>>;
}
/// After firmware upload the device re-enumerates. Wait for PID 2500
/// (hotplug if available, else poll with backoff), then open_runtime().
pub fn wait_runtime_reenum(timeout: Duration) -> Result<Transport>;
```

Details that matter: the bootloader declares maxpacket 64 — all bootloader
I/O is 64-byte framed; the runtime path (xeyez.sys `MaxTransferSize` 4096,
EasyUSB 16 KiB accumulations) reads in 4096-byte chunks and reassembles.
Detach any kernel driver defensively (Linux: none binds; Windows:
WinUSB/libusb filter).

#### `protocol/frames.rs` — frame codec

```rust
pub const MAGIC_H2D: u32 = 0x7f7f_5550;   // "PU\x7f\x7f" (LE)
pub const MAGIC_D2H: u32 = 0x7f7f_444f;   // "OD\x7f\x7f" (LE)

pub struct Frame { pub magic: u32, pub kind: u16, pub payload: Vec<u8> }

impl Frame {
    pub fn h2d(kind: u16, payload: &[u8]) -> Frame;
    pub fn encode(&self, out: &mut Vec<u8>);
}
/// Streaming decoder: scan for magic, read u16 kind, length is implicit
/// (transport-framed) — exactly the xtion-rs stream approach, adapted:
/// the xeyez framing has no explicit length field, so the assembler uses
/// bulk-packet boundaries + the known record types (see stream.rs).
pub struct FrameDecoder { /* ring + state */ }
impl FrameDecoder { pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame>; }
```

Unit-tested against golden bytes; a `#[cfg(fuzz)]` entry point wraps
`FrameDecoder::push` for `cargo fuzz` (never on hardware).

#### `protocol/boot.rs` — the upload state machine

```rust
pub struct BootUploader<'a> { t: &'a Transport, blob: &'a [u8] }

pub enum BootStep { DspStart, ReadyToLoad, CountAndCrc, PacketStream,
                    CrcAck, DeviceId, DspReset, HandleConfig }

impl BootUploader<'_> {
    /// Replicates _InitBlobUpload (xEyeUSB.dll). 64-byte type-3 packets,
    /// count+CRC announced first, per-packet 6 s timeout, 3 s ready
    /// timeout, CRC abort on mismatch, device-ID poll 300 ms x 10.
    pub fn run(&mut self) -> Result<DeviceIdentity, IdongError>;
}
```

Sequencing constants (from disassembly) live in `protocol/mod.rs` as
`pub const TIMEOUT_READY: Duration = Duration::from_secs(3)` etc. The
multi-file staging (BF527 `.ldr` first — the re-enumeration trigger —
then FPGA `.bit` / 8051 `.hex` possibly via runtime vendor requests) is
**open item #1**; the state machine is written so each file upload is a
`run()` call, letting a USBPcap capture clarify the order without
restructuring.

#### `protocol/runtime.rs` — runtime client

```rust
pub struct RuntimeClient<'a> { t: &'a Transport }

impl RuntimeClient<'_> {
    pub fn device_id(&self) -> Result<DeviceIdentity>;
    /// 0xFFF -> 0xFFE keep-alive (1 s). Must be answered or the device
    /// treats the link as lost (LinkErrCnt = 100 in xEyeUsb.ini).
    pub fn answer_keepalive(&self) -> Result<()>;
    /// Type-4 link status: parse +0x8 subcode (1 / 0xF), reply word 0x11,
    /// map +0xC/+0x10 to state/command as the vendor stack does.
    pub fn handle_link_status(&self, f: &Frame) -> Result<()>;
    pub fn handle_match(&self, cfg: HandleConfig) -> Result<HandleEvent>;
}
```

#### `stream.rs` + `markers.rs` — the data plane

`stream.rs` owns a reader thread (like xtion-rs): bulk reads -> frame
assembler -> `MarkerFrame` channel, with counters for drops, magic
resyncs, and quality-window ring (vendor `RingSize=200`). `markers.rs`
defines the public data model:

```rust
pub struct MarkerFrame {
    pub frame_counter: u32,        // record +0x00 (continuity checked)
    pub mode: u32,                 // record +0x3B8
    pub markers: Vec<Marker>,      // slots that passed the >=50 filter
    pub link_quality: Quality,     // Green/Yellow/Red (thresholds 20/35/35)
    pub dropped: bool,
}
pub struct Marker {
    pub slot: u8,                  // 0..=9 observed (chip: up to 15)
    pub x_mm: f32, pub y_mm: f32, pub z_mm: f32,
    pub quality: i32,              // slot +0x24 (>=50 = tracked)
    pub t: f64,                    // slot +0x18 (gate value)
}
```

#### `calib.rs` — EEPROM layout (already fully decoded)

```rust
pub struct StereoCalib {   // 28 x f32 LE, OpenCV convention
    pub fx: [f64; 2], pub fy: [f64; 2], pub cx: [f64; 2], pub cy: [f64; 2],
    pub dist: [[f64; 5]; 2], pub r: [[f64; 3]; 3], pub t: [f64; 3], // mm
}
pub struct DeviceIdentity { pub serial: [u8; 32], pub code_a: String, // "000T"
                            pub code_b: String }                     // "2613"
/// records at 0x0100/0x0180/0x0192/0x0300, mirrored +0x400;
/// record = [payload][checksum=sum&0xFF][0xFF]; header tag ef0123456789abcd
pub fn parse_eeprom(dump: &[u8; 2048]) -> Result<(StereoCalib, DeviceIdentity)>;
```

#### `device.rs` — the facade

```rust
pub struct Idong { /* transport + runtime client + stream handle */ }

impl Idong {
    pub fn open() -> Result<Self>;               // runtime, else error with hint
    pub fn open_or_boot(fw: &FirmwareSet) -> Result<Self>;  // full lifecycle
    pub fn identity(&self) -> Result<DeviceIdentity>;
    pub fn calibration(&self) -> Result<StereoCalib>;
    pub fn start_tracking(&mut self, cfg: TrackingConfig) -> Result<()>;
    pub fn read_frame(&mut self, timeout: Duration) -> Result<MarkerFrame>;
    pub fn drain_events(&mut self) -> Vec<Event>;  // HandleFound/Timeout/Link
    pub fn suspend(&self) -> Result<()>;            // setEyeUsbSuspend analog
    pub fn shutdown(&self) -> Result<()>;           // setEyeUsbShutDown analog
}
```

`FirmwareSet` points at user-supplied vendor files (never bundled),
verifies sha256 against the recorded hashes (v2.5.07/v2.6.0.6), and refuses
mismatches before the first write.

### 2.4 Concurrency model

- Reader thread per device (xEyeClass analog): bulk read -> decode ->
  bounded channel. `read_frame` is blocking-with-timeout; an iterator
  adapter and a callback registration API are M8 ergonomics (xtion-rs
  pattern).
- Keep-alive responder runs on the reader thread inline (RX 0xFFF -> TX
  0xFFE) to bound latency at 1 s.
- No shared mutable state across threads beyond the channel + atomics
  (drop counters, link quality ring).

### 2.5 Milestones

| Milestone | Scope | Acceptance |
|---|---|---|
| **M0** | PROTOCOL.md: consolidate Part I of this draft with source citations; record all constants | reviewed |
| **M1** | Crate skeleton (this branch) compiles; `list` + `probe_bootloader` run on the real device (read-only) | probe output matches `idong_probe.py` (timeouts, no data — device waits in bootloader) |
| **M2** | Frame codec + golden tests; EEPROM parser + tests vs the real dump (already in hand) | unit tests green; parsed calibration == CONTEXT.md values (f~=432.33, \|T\|=100.0000) |
| **M3** | USBPcap capture of one Windows boot (user-side task) -> byte-confirm `_InitBlobUpload` (ordering, CRC variant, count/CRC framing) | transcript matches disassembly-derived sequence |
| **M4** | `upload_firmware` (guarded): boot state machine on real device -> re-enumeration to 0471:2500 | `lsusb` shows 2500; UVC/UAC interfaces appear; **no writes before M3 sign-off** |
| **M5** | Runtime claim (MI_04), device-ID query, keep-alive answered, first OD record decoded | stable >=60 s, zero magic resyncs, frame-counter deltas <=1 |
| **M6** | Marker stream API + quality/drop stats + handle events | 50 fps sustained; reflective ball tracked at 1-4 m; JSONL dump example |
| **M7** | Robustness: re-plug, suspend/resume, error recovery, `RingSize`-style noise window | 24 h soak without leak/lockup |
| **M8** | Ergonomics: callbacks, docs.rs, publish readiness | docs build |

M3 is the gate: no write-capable example runs on hardware before the
USBPcap transcript confirms the upload bytes (safety rule #2).

### 2.6 Validation & instrumentation

- **Golden sources**: `iDongSimulator3.exe` sessions (no-hardware replay),
  the recorded PE inventories + disassemblies (already in the repo), the
  real EEPROM dump, and (after M3) the USBPcap transcript.
- **Decoder tests**: `frames_golden.rs` replays captured OD frames;
  `eeprom_decode.rs` asserts the six checksums and calibration values.
- **Hardware tests** (`tests/hardware.rs`, `#[ignore]` by default) mirror
  xtion-rs: enumerate -> boot -> 60 s stream -> stats.
- **Fuzz**: `FrameDecoder` and `parse_eeprom` as libFuzzer targets.
- **BF527 disassembly** (`ens_code.ldr`) is the definitive device-side
  cross-check for type tables; radare2/libopcodes bfin or a Blackfin-aware
  Ghidra plugin — separate research agent task, not blocking M3.

### 2.7 Open questions (tracked, non-blocking for M1-M2)

| # | Question | Resolution path |
|---|---|---|
| 1 | Multi-firmware staging order (ldr/bit/hex) and which stage triggers re-enumeration | USBPcap (M3) or `ens_code.ldr` disasm |
| 2 | CRC variant announced in "count & crc" (CRC16-CCITT? CRC32?) + endianness | same |
| 3 | Exact per-type payload layouts beyond types 2/3/4/7/0xFFE/0xFFF | `ens_code.ldr` + capture corpus |
| 4 | xEye3Ddata slot field mapping (which of the 88 bytes are x/y/z vs gate/time) — section 1.4 structural layout | disasm of xEyeClass RVA 0x25e0 transform + `iDongTTCamera.dll` |
| 5 | 0x208-byte pipe record field semantics | same |
| 6 | Handle-config blob format ("Read Handle Cfg") | disasm + AnySense SDK docs if recoverable |
| 7 | Whether image mode (raw 2-sensor diff) is reachable at runtime via a type or only via 8051 register tables | 8051 hex disasm |
| 8 | Runtime UVC interface numbering on the composite 2500 device (expect MI_00/01 video, MI_02/03 audio, MI_04 vendor) | M4 enumeration dump |

### 2.8 What this buys us (the payoff)

With the driver done, the sensor becomes a general-purpose **sub-mm-class
3D tracking bar**: motion-capture input for games and robotics (10-15
passive markers, 50 fps), a 100 mm-baseline calibrated stereo head, an IR
beacon localizer, plus a standard UVC camera and mic array — all from a
~2011 consumer device that currently has zero open drivers. The
`iDongSimulator3` replay pipeline additionally gives us a vendor-generated
ground truth to validate our decoder semantics against.

---

## Appendix A — Known protocol constants (quick reference)

```text
USB VID/PID        0471:2600 bootloader - 0471:2500 runtime (vendor MI_04)
Endpoints          0x02 OUT (commands/upload) - 0x82 IN (data)
Frame              [u32 magic LE][u16 type LE][payload]
Magic H2D          0x7f7f5550  "PU\x7f\x7f"
Magic D2H          0x7f7f444f  "OD\x7f\x7f"
Boot types         TX DSP-start -> RX 2 "Ready to load program!"
                   TX 2 count+CRC -> TX 3 x 64 B packets -> RX 3 CRC result
                   RX 2 device-ID (300 ms x 10) -> TX 7 DSP reset
Runtime types      RX 0xFFF <-> TX 0xFFE keep-alive; type 4 link status
                   (reply word 0x11); TSOL_DEVICE_HANDLE_* events
Timeouts           3 s ready/CRC-ID - 6 s per packet - 1 s reset/keepalive
Transport          bootloader: 64 B frames - runtime: 4096 B chunks,
                   16 KiB accumulation, 3 retries x 100 ms
Streaming          50 fps marker records (xEyeFrame=50, USBComFPS=35)
Record             10 × 88 B slots at +0 (frame counter overlays slot 0's
                   first u32; gate f64 @ slot+0x18; quality i32 @ slot+0x24,
                   ≥ 50 = tracked); 72 B trailer; mode flag @ +0x3B8;
                   pipe copy 0x208 B
Firmware           ens_code.ldr (BF527) - A_a_v4.4_BG0313.bit (Spartan-3)
                   - app_m8051_2500/2611.hex (8051) - xeyez_v20/v21.hex (old)
EEPROM T24C16A     tag ef0123456789abcd; recs 0x0100/0x0180/0x0192/0x0300,
                   mirror +0x400; [payload][sum&0xFF][0xFF]; calib 28 x f32
                   OpenCV; serial AB460DC8BA12C6840B49CD5786C329DD
```

## Appendix B — Sources

- Repo branches: `research/idong100` (parent, CONTEXT.md), `-01` (Chinese
  web), `-02` (products/software, download.i-dong.com inventory),
  `-09` (xeyez stack decompilation + inno_bin_extract.py — FINDINGS.md is
  the primary protocol source).
- Local analysis tree: `analysis/nsis/v2.5.07/$_6_` (runtime configs, all
  firmware), `analysis/xeyez_*.txt` (disassembly notes),
  `upload/24c16-dump.bin` (EEPROM), `upload/idong_probe.py` (read-only
  probe), `upload/bin-files.txt` (installer inventory).
- Vendor: toltech.cn product pages (idong100 kit id=989, idong200 kit
  id=975, sensor id=977, AnySense SDK ToolsPage.jsp), xeyez.inf,
  DriverConfig.ini, xEyeUsb.ini, xeyezHal.ini, xEye.ini, Co2D.ini.
- Press/teardown: Nikkei Electronics i-dong teardown (sensor bar: center
  RGB cam + 2 IR cams + 6 LEDs, passive reflectors); PCOnline product
  specs (72°/30 fps chat cam, 84° 0.5-4 m recognition, 850 nm, 2.4 G GFSK).
