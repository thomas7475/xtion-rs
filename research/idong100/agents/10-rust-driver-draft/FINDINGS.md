# Agent 10 — Rust Driver Draft (turn 10)

Task ID: 10. Date: 2026-11-23. Inputs: `upload/bin-files.txt` (full file
list of the Sports King installer), the GitHub research branches
(`research/idong100` family), plus fresh vendor-page research.

## What was done

1. **bin-files.txt triage** (21,995 entries ranked into 3 tiers — see
   DRAFT §1.6). Highlights:
   - `Games\pingpang\game\iDongTTCamera.dll` — the game-side camera
     consumer (highest-value next RE target).
   - `Dlls\iDongAction3.0\` — complete motion middleware incl.
     **`iDongSimulator3.exe`** (hardware-free session replay = golden
     test bench) and the analysis-plugin family.
   - `Lobby\` xEye* pipe consumers, Camset/XeYeZ settings UI, RHCC*
     CV family (RHCCEXTV 2.2 MB — unidentified), 6 games shipping
     OpenCV 2.0 (image-mode consumers).
2. **Vendor pages recovered** (toltech.cn): idong100 chip kit official
   specs (dual image-processing units, embedded MCS-51, UVC/UAC+IR,
   bootloader with host-downloaded firmware, CrossBus, USB2.0 HS, 6-ch
   DMA, 5 m / 15 targets, BG0316 640×480@60); idong200 kit; AnySense SDK
   v1.0.21 (Java/C/C++/C#/Flash/Unity3D). GitHub/Gitee searches confirm
   **no existing public RE** of this device.
3. **EEPROM record format corrected**: the annotated hexdump shows
   fixed-length records with ASCII header `ef0123456789abcd`; payloads
   may contain 0xFF so there is no terminator scan. Calibration float
   order pinned: cam1 fx,fy,cx,cy,k1,k2,p1,p2 | cam2 … | R(9) | T(3);
   |T| = 100.0000 mm verified numerically.
4. **The Rust driver draft written** (`docs/RUST_DRIVER_DRAFT.md`):
   research synthesis → data products → crate design → module skeletons
   → milestones M0–M8 with a safety gate (no writes before USBPcap
   byte-confirmation) → open questions table.
5. **Skeleton crate** at `rust/idong/` mirroring the user's xtion-rs
   architecture: `error/usb/device/stream/markers/handle/calib` +
   `protocol/{frames,boot,runtime,records}` + 4 examples + golden tests.
   Not compiled in the sandbox (no toolchain + standing no-build rule) —
   code is reviewed-by-construction against the disassembly notes.

## Key new facts (beyond agent-09)

| Fact | Source |
|---|---|
| idong100 chip = dual image-processing units + MCS-51 MCU + UVC/UAC/IR + host-downloadable firmware, 15 targets, 5 m | toltech.cn ProductInfo.jsp?id=989 |
| AnySense SDK API families (air mouse / gesture / sports DB / face+voice) = facade model for the crate | toltech.cn ToolsPage.jsp |
| xEyeClass pipe record = 0x208 (520) B transformed from xEye3Ddata (72 B header + 10×88 B slots) | disasm_GetADataFromUSB.txt (this turn's read) |
| Sensor has blob vs image mode; global-shutter 30 fps variants; LED-scale FPGA regs 0x58–0x6B | xeyezHal.ini + 30fps/ configs |
| Runtime composite device: MI_00–03 UVC/UAC (usbvideo/USBAUDIO file lists), MI_04 vendor | DriverConfig.ini + xeyez.inf |

## Next steps

1. User: assemble the 16 .bin slices, extract Tier-1 files with
   `inno_bin_extract.py`, PE-inventory `iDongTTCamera.dll` + RHCC*.
2. USBPcap capture of a Windows boot (gate for M3/M4).
3. Disassemble `ens_code.ldr` (BF527) for device-side type tables.
4. Compile the skeleton on a real toolchain, run `cargo test`.
