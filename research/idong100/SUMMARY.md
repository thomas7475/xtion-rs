# idong100 Research — Consolidated Summary

Status as of 2026-11-23, after research agents 01, 02, 09 and the Rust
driver draft (10). Branches: `research/idong100-NN-*` (see README).

## Where we stand

The project set out to write a Linux/Rust user-space driver for the
Toltech **idong Sensor V2.6** (USB `0471:2600`, vendor-specific bulk
interface, onboard stereo processing). It is now past the
"unknown protocol" stage:

1. **Hardware understood** (agent 09 + owner probe + teardowns): BF527 +
   Spartan-3 + MCS-51 three-chip pipeline, 2× BG0316 640×480@60 CMOS,
   6× 850 nm IR LEDs, passive reflective markers, 2.4 GHz RF handles,
   T24C16A EEPROM with factory stereo calibration (baseline exactly
   100.00 mm, OpenCV convention). Official chip specs recovered from
   toltech.cn (agent 10): 15 targets, 5 m, UVC/UAC/IR, host-downloaded
   firmware.
2. **USB lifecycle mapped**: the physical device sits in the BF527
   bootloader (PID 2600); after firmware upload it re-enumerates as the
   runtime composite device (PID 2500) whose MI_00-03 are standard
   UVC/UAC and MI_04 is the vendor bulk interface (xeyez.sys = MS
   BulkUSB sample).
3. **Wire protocol decoded on the host side** (agent 09 disassembly):
   `[u32 magic][u16 type][payload]`, `PU\x7f\x7f` / `OD\x7f\x7f`; full
   boot-upload state machine (DSP-start → ready → count+CRC → 64-B
   packets → CRC ack → device-ID → DSP reset → handle cfg); runtime
   keep-alive 0xFFF/0xFFE, link status type 4, TSOL handle events;
   xEye3Ddata record = 72-B header + 10×88-B slots (quality ≥ 50
   filter), pipe record 0x208 B.
4. **All vendor firmware in hand** (ens_code.ldr, A_a_v4.4_BG0313.bit,
   app_m8051_2500/2611.hex + legacy) and the complete PC stack extracted
   (EasyUSB/xEyeUSB/xEyeClass/StereoCamera/MiddleCameraCalib/
   classifying + SVM data + virtual HID drivers).
5. **Sports King installer fully listed & extractable** (agent 10
   triage): game-side camera consumers (iDongTTCamera.dll), the
   iDongAction3.0 middleware with the iDongSimulator3 replay tool, the
   RHCC* CV family, 6 OpenCV games. No public RE of this device exists
   anywhere else.
6. **Rust driver draft + skeleton crate written** (agent 10, branch
   `research/idong100-10-rust-driver-draft`): `docs/RUST_DRIVER_DRAFT.md`
   is the design contract; `rust/idong/` mirrors the proven xtion-rs
   architecture with a safety gate (no writes before USBPcap
   byte-confirmation).

## Recommended next steps

1. **User**: assemble the 16 .bin slices, extract Tier-1 files
   (`iDongTTCamera.dll`, RHCC*, iDongAction3.0) with
   `inno_bin_extract.py`; PE-inventory them.
2. **User**: capture one Windows boot session with USBPcap (device
   replug → vendor stack upload → first minutes of streaming). This
   transcript is the M3 gate for every write path.
3. **Agent task**: disassemble `ens_code.ldr` (Blackfin BF527) for the
   device-side type tables and CRC variant; cross-check the 8051 hex
   for the sensor-mode register protocol.
4. **Code**: compile the skeleton on a real toolchain, run
   `cargo test`, then milestone M1 hardware validation with the
   read-only examples.
