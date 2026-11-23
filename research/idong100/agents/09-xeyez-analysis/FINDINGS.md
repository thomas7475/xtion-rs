# Agent 09 — xeyez PC Software Analysis (decompiled) + Inno .bin Extractor

Task ID: 5-i (user-directed follow-up to Agent 02's download-server discovery).
Date: 2026-11-23. All analysis performed locally on user-supplied files; one
live verification against download.i-dong.com.

## Executive summary

- The user downloaded the official i-dong software from the still-live
  `download.i-dong.com` server (Agent 02's find) and supplied
  `v2.5.07.zip`, `v2.6.0.6.zip` (the **xeyez PC driver stack** in two
  versions) and `i-dong_setup.exe` / `idong_setup.exe` (Inno Setup
  installers that came with "huge .bin files" not included).
- Full extraction of every NSIS container recovered the **complete Windows
  driver stack**: `xeyez.sys` (kernel driver), `EasyUSB.dll`
  (ReadData/WriteData transport), `xEyeUSB.dll` (device state machine),
  `xEyeClass.dll` (coordinate records), `StereoCamera.dll` (3D triangulation),
  plus **all sensor firmware** (8051 `app_m8051_*.hex`, Blackfin BF527
  `ens_code.ldr`, Xilinx Spartan-3 `A_a_v4.4_BG0313.bit`) and the SVM
  ball/head classifiers.
- Disassembly (capstone) decoded the **host↔device wire protocol**:
  messages `"PU\x7f\x7f"` (host→device) / `"OD\x7f\x7f"` (device→host),
  u16 type at +4, **64-byte firmware upload packets**, boot handshake types
  (2 = ready-to-load, 3 = CRC result, 0xFFF/0xFFE handle-match pair), and
  the boot sequence: DSP start → "Ready to load program!" → packet stream
  with per-packet count/CRC → CRC verify → device-ID → handle-cfg → lens
  params → stream start.
- **The physical device (USB 0471:2600) is in BOOTLOADER state** —
  `xeyez.inf` names PID 2600 "TOLTech xeyez Bootloader"; runtime is PID 2500.
  A Linux driver must first replicate the firmware upload via EP 0x02.
- `i-dong_setup.exe` = "爱动在线运动时尚版" (Fashion PC) v1.9.02, 21,999 files;
  `idong_setup.exe` = "爱动体感运动机(运动王PC版)" (Sports King PC) v1.9.04,
  21,995 files, 4.58 GB uncompressed across **16 external slices**
  (`idong_setup-1.bin`…`-16.bin`; A-zip has 1-7, B-zip has 8-16).
- **`inno_bin_extract.py` written and verified end-to-end**: parses these
  custom-build Inno 5.3.6 installers (two format quirks the stock
  `innoextract` 1.9 cannot handle — it errors on these files), lists all
  entries without the .bins, and extracts + MD5-verifies files when the
  .bins are present. Live test: fetched a 900 KB prefix of slice 1 from
  download.i-dong.com and extracted 6 DLLs, **all MD5-verified**.

## Key findings

### 1. The xeyez PC stack (both versions, structure identical)

`v2.5.07.zip` / `v2.6.0.6.zip` → `pingbisetup.exe` + `xeyez_pc_vX/` with
`xeyez_setup.exe` + `xeyez_Lobby.dat` + `xeyez_alone.dat` (all NSIS
installers, extracted with 7zz). Runtime dir (`$_6_`, ~570 files) contains
the complete PC client: MFC80/VC80 runtime, OpenCV `cxcore200.dll` (the
vendor used OpenCV 2.0 — matches the OpenCV-convention stereo calibration
found in the T24C16A EEPROM), the Lobby app + 40+ game components
({app}\Games\...), and the driver stack in `DriverInstall/` (WinXP + Win7,
x86 + x64): `xeyez.sys`, `xeyez.inf`, `xeyezCoIns.dll`, and **virtual
keyboard/mouse drivers** (`vKeyboard.sys`, `vMouse.sys`) — the sensor injects
HID input on console hosts.

### 2. USB identity of the device family (xeyez.inf)

| PID | Name (INF string) | Role |
|---|---|---|
| 0471:2600 | TOLTech xeyez Bootloader | **the physical unit's current state** |
| 0471:2500 (MI_04) | TOLTech xeyez v2.5 | runtime, composite interface 4 |
| 0471:2100 | TOLTech xeyez v2.1 | older gen |
| 0471:0668 | TOLTech xeyez v2.0 | older gen |
| 0471:0666 | TOLTech xeyez v1.0 | oldest gen |

Driver service "XEYEZ", `MaximumTransferSize=4096`, `BulkUsbEnable=1` —
xeyez.sys is the MS BulkUSB sample retargeted (strings: "BF527 Bulk USB
DriverEntry"). The user's bare enumeration (no strings, maxpacket 64,
bcdDevice 0.00) is exactly a device sitting in its BF527 bootloader.

### 3. Host-side architecture (from exports/imports + disassembly)

```
xeyez.exe / xEyeService.exe          apps / NT service (driver install, watchdog)
  └─ BasicInterFace.dll              glue UI layer
      └─ xEyeClass.dll               xEyeClassManager: reader thread,
          │                          GetADataFromUSB(xEye3Ddata&), TransUSB2Pipe(),
          │                          named pipes \\.\pipe\* to apps
          └─ xEyeUSB.dll             xEyeUSBManager: InitxEyeUSB (firmware upload
              │                      orchestration), GetxEyeUSBID/State, handle match
              └─ EasyUSB.dll         exports ReadData/WriteData; SetupAPI enum via
                                     interface GUID {77f49320-16ef-11d2-ad51-006097b514dd},
                                     device path \\.\xeyezUsbDriverDeviceN, overlapped
                                     bulk I/O in 16 KiB chunks, 3 retries × 100 ms
```
- `EasyUSB.dll` keeps an 8-slot device table (32 B/slot); reads accumulate
  up to 0x4000-byte chunks with GetTickCount-based timeouts.
- `StereoCamera.dll` exports `Get3DObject`, `Get3dSingle(x,y,z,fx…)` —
  on-host triangulation for calibration/setup; runtime coordinates come
  from the device.
- `MiddleCameraCalib.dll` exports `UserCalibMiddle` (cvSolve/cvSVD — the
  factory/user calibration tool using OpenCV).
- `classifying.dll` (ClassifyAndTrack/SetLensParam/SetStaticPoint_Param) +
  SVM_Class_ball/head_*.dat — blob classification on host.

### 4. Wire protocol (decoded from xEyeUSB.dll/xEyeClass.dll)

- Message frame: `[u32 magic][u16 type][payload…]`
  - host→device magic `0x7f7f5550` = `"PU\x7f\x7f"`, device→host
    `0x7f7f444f` = `"OD\x7f\x7f"`.
- Boot / firmware upload (`_InitBlobUpload`, "Blob" = firmware blob):
  1. host sends DSP-start; device replies type 2 = "Ready to load
     program!" (3000 ms timeout)
  2. host streams the blob in **64-byte packets** (fread 64 B → TX type 3,
     6000 ms per-packet timeout) — packet count + CRC announced first
     ("Tx packet count & crc!")
  3. device returns CRC result (type 3) — mismatch aborts
  4. "Get DSP Reset ACK OK" → device-ID exchange ("Device ID is
     received!", `DEVICE ID =`), DSP version string
  5. handle-config read/write + backup ("Read Handle Cfg", airtouch
     wristband pairing), lens-parameter stage, optional "Hot Start"
  6. startup ends ("启动流程结束OK") → coordinate streaming at 50 fps
     (xEye.ini: xEyeFrame=50, USBComFPS=35, PipeBufferSize=9000, MaxRead=1)
- Handle-match (wristband pairing) protocol: TSOL_DEVICE_HANDLE_FOUND/
  NOT_FOUND/TIMEOUT/FOUND_OVER ACK types; runtime types in 0xFF0-0xFFF
  range; handshake pair RX 0xFFF → TX 0xFFE.
- Firmware set loaded per xEyeUsb.ini: `BitFile=A_a_v4.4_BG0313.bit`
  (Spartan-3, "work_top.ncd" 2010-04-06), `ens_code.ldr` (BF527,
  "ep0_handler"/"usb_init" strings), `app_m8051_2500.hex`/`_2611.hex`
  (8051, versions "2614"/"2619"), plus `xeyez_v20/v21.hex` for old gens.
- Config: `DevicePlatform=2` (2nd hw platform), `NeedHandleMatch=1`,
  `BootConfigCurr=1`, environment-disturbance grades, `UsbNoice=330`.
- Co2D.ini: 8 calibration corner points spanning x=44..715, y=53.5..506.5
  (2D camera-calibration overlay; cx≈313 in the EEPROM ⇒ 640-wide frames).

### 5. The Inno installers and the .bin files

- Both exes are Inno Setup 5.3.6 ANSI, **built with a custom compiler
  variant**: (a) one extra byte after the signed-uninstaller checksum in
  the header, (b) 28-byte version data in item entries (standard Inno
  writes 20). Stock innoextract 1.9 fails on them ("Stream error");
  innoextract master (2020) has the same issue.
- Layout: setup.exe (2.1 MB, headers only) + `idong_setup-{1..16}.bin`
  slices (12-byte header `idska32\x1a` + u32 size, then `zlb\x1a`-magic
  zlib chunks; 200 MB per slice; data entries carry per-file MD5,
  chunk offsets absolute in the slice file).
- Content: `{app}\Lobby\` (the PC client — iDongAction.dll,
  iDongMarkManager.dll, IdentifyModule.dll, Camset.dll, CapStill.dll,
  …) and `{app}\Games\…` (manpao, boling, tjhc, szjc, dance, dagu/Drum…).
- A-zip (1.5 GB) holds slices 1-7 + the xeyez_pc_v2.6.0.6 dir +
  vcredist/Xvid/pingbisetup; B-zip (1.7 GB) holds slices 8-16. Both are
  needed for a full extraction.

### 6. The extractor tool (`inno_bin_extract.py`)

Pure-Python (stdlib only), verified against the real files:
- Parses the Inno loader offset table (12-byte magic scan), the 64-byte
  version ID, the LZMA1-compressed primary header block (chunked CRC32
  framing), languages/messages/types/components/tasks/directories/files/
  icons/ini/registry/delete/run entries, wizard images, then the SECOND
  LZMA1 block with the data entries.
- Auto-detects the two custom-build quirks by validation (parse must
  consume both blocks exactly and yield a sane language LCID).
- `--extract DIR` reads slices `<stem>-N.bin` (and
  `<base_filename>-N.bin`), decodes `zlb` chunks (zlib/bzip2/LZMA1/stored),
  undoes Inno's x86 call-instruction optimization (>= 5.2.0 variant,
  ported from innoextract), writes files, verifies MD5.
- Live end-to-end proof: ranged-GET of a 900 KB prefix of
  `idong_setup-1.bin` from download.i-dong.com → 6 DLLs extracted,
  **all MD5-verified** (AboutSystem.dll, AboutXeYeZDlg.dll, ACE.dll,
  amconn.dll, borlndmm.dll, Camset.dll).

## Artifacts

- `artifacts/inno_bin_extract.py` — the extractor (also delivered to the
  user in download/)
- `artifacts/pe_inventory_runtime.txt`, `artifacts/pe_inventory_drivers.txt`
  — PE exports/imports of every vendor binary
- `artifacts/disasm_EasyUSB_*.txt`, `artifacts/disasm_GetADataFromUSB.txt`,
  `artifacts/xeyez_boot_protocol.txt`, `artifacts/xeyez_msg_format.txt`,
  `artifacts/xeyez_tx_builder.txt`, `artifacts/xeyez_command_protocol.txt`,
  `artifacts/disasm_EasyUSB_open.txt`, `artifacts/xeyez_msg_wrappers.txt`,
  `artifacts/xeyez_extractor_debug_log.txt` — disassembly/protocol notes
- `artifacts/installer_inventory.md` — both installers' full stats
- `artifacts/live_test_result.txt` — the MD5-verified live extraction log

## Negative results / limits

- No ELF/PE for Linux anywhere in the packages (expected — Windows only).
- The 8051 hex + BF527 ldr + Spartan-3 bitstream are present but not
  disassembled here (no Blackfin/8051 disassembler in this environment);
  they are the definitive device-side protocol reference for the next step.
- `pingbisetup.exe` (屏蔽 = "block") not analyzed in depth; likely a USB
  device-blocking utility for kiosk-style setups.
- Archive.org unreachable from this sandbox (Agent 05's Wayback work
  remains blocked).

## Assessment for the Linux-driver goal

This analysis converts the Linux project from "unknown protocol" to
"protocol documented at the byte level on the host side":
1. The device at 0471:2600 is a **bootloader** — firmware upload is the
   mandatory first step. The exact upload sequence (64-byte packets,
   count+CRC framing, handshake types) is now known from xEyeUSB.dll.
2. The full firmware images are in hand (8051 + BF527 + FPGA) — flashing
   them via libusb replicating the Windows flow is feasible, and the
   BF527 loader image can be disassembled later for the device-side view.
3. The runtime coordinate stream framing (OD/PU magics, type field) is
   known; record layout (xEye3Ddata: frame counter at +0, 3D positions)
   is known structurally; remaining detail = per-type payload layouts,
   best obtained by disassembling ens_code.ldr or by capturing one
   Windows session with USBPcap.
4. The Windows client itself (Lobby + iDong*.dll) is now extractable
   from the .bin files for further reverse engineering (user names the
   files; extraction verified).

## Recommended follow-ups

1. User downloads A+B zips, places all 16 .bin files next to
   idong_setup.exe, runs `python3 inno_bin_extract.py idong_setup.exe
   --extract out` → complete 4.58 GB content incl. the Lobby client.
2. Disassemble `ens_code.ldr` (Blackfin BF527) — device-side USB command
   handlers, the definitive EP 0x02/0x82 protocol tables.
3. Cross-check `app_m8051_2611.hex` (8051) for the sensor-control I2C
   protocol (CMOS register tables `*_0316_Cmos*.txt` already in hand).
4. On the Linux side: libusb sketch — claim 0471:2600, replicate
   _InitBlobUpload (ens_code.ldr first), expect re-enumeration to
   0471:2500, then speak the OD/PU message protocol. Never write
   speculative bytes before the upload sequence is confirmed against a
   USBPcap capture of the Windows flow.
