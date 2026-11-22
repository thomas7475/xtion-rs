# Xtion / PS1080 firmware — FWUpdate 5.8.22

This folder mirrors the **last firmware ASUS ever released for the Xtion Pro /
Pro Live** (PrimeSense PS1080-based sensors, internally codenamed *RD108x*),
version **5.8.22**, released in 2013. It is the version the xtion-rs
reference/validation camera runs (reported as `firmware 5.8.22, chip 0x00212020,
FPGA 0x0005`).

## Why this firmware exists

Early Xtion units shipped with 5.7.x firmware that fails on USB 3.0 ports with
`xhci_hcd: Not enough bandwidth` errors. The 5.8.22 update fixes that. It also
switches the sensor to the newer protocol generation: **after this update the
camera speaks the OpenNI2 / XnDeviceSensorV2 protocol only** — which is exactly
the protocol xtion-rs implements (endpoint layout 0x81/0x82/0x83, protocol
v2.5/v2.6 command framing).

## Contents

| File | What it is |
|---|---|
| `FWUpdate_5_8_22.zip` | The original official ASUS update package, unmodified (contains the Windows updater `FWUpdate_RD108x-5.8.22.exe`, a CAB self-extractor) |
| `5.8.22/*.fla` | The 15 flash images extracted from that package: `source.out.bin.fla` is the main firmware, `BootManager.out.bin.fla` the boot manager, the rest are parameter images (calibration/CMOS/registration data) |
| `5.8.22/UsbUpdate_RD108.ini` | Flash-layout configuration used by the updater: the offset every image is written to |

The Windows updater and its OpenNI 1.x DLLs stay inside the zip — this folder
extracts only the firmware images themselves (the CAB payload is LZX-compressed;
extraction requires `cabextract`/`7z` or libarchive).

Every image begins with a small header `[u16 image-type][22 00][08 05]…` whose
trailing words match the version (5.8.22). The parameter images are the factory
defaults for what the driver reads back at runtime through
`GET_FIXED_PARAMS` / `ALGORITHM_PARAMS` — e.g. `AlgorithmParams.xml.bin.fla`
starts with `c8 00` = 200, the ConstShift xtion-rs reads from a running camera.

## Flash map (from `UsbUpdate_RD108.ini`)

| Image | Size | Update offset | Factory offset | Sensor-specific |
|---|---:|---:|---:|---|
| IDParams.xml.bin.fla | 302 | 0x28000 | 0x50000 | yes |
| SensorTecParams.xml.bin.fla | 14 | 0x28200 | 0x58000 | yes |
| SensorNesaParams.xml.bin.fla | 78 | 0x3F9C0 | 0x60000 | yes |
| SensorSafetyParams.xml.bin.fla | 58 | 0x28230 | 0x68000 | yes |
| GainControl.xml.bin.fla | 120 | 0x28260 | 0x550 | no |
| RegistrationParams.xml.bin.fla | 548 | 0x28360 | 0x70000 | yes |
| AlgorithmParams.xml.bin.fla | 780 | 0x28660 | 0x950 | no |
| PrimeProcessor.xml.bin.fla | 1116 | 0x28A60 | 0x6000 | no |
| FixedParamsRD108.xml.bin.fla | 658 | 0x29060 | 0x6700 | no |
| ImageCmos.xml.bin.fla | 2278 | 0x29460 | 0x6B00 | no |
| DepthCmos.xml.bin.fla | 278 | 0x2A660 | 0x7D00 | no |
| DefaultParams.xml.bin.fla | 22 | — | 0x6600 | factory |
| BootManager.out.bin.fla | 23214 | — | 0x2000 | factory |
| source.out.bin.fla | 153704 | 0x2A960 | 0x10000 | no |

(Sizes in bytes; "sensor-specific" images carry per-unit calibration and must
not be flashed across units; `UploadInProgress.fla` is a 7-byte marker written
at 0x3F9C0 to make power loss during an update recoverable.)

## How to flash

- **Windows (official path):** unzip `FWUpdate_5_8_22.zip` and run
  `FWUpdate_RD108x-5.8.22.exe` with the camera attached. This is the only
  official tool.
- **Linux:** no official tool exists. Flashing requires the PS1080's
  firmware-upload protocol, which xtion-rs does not implement (and does not
  need to — a driver talks to the running firmware, not the flash).
  **Warning:** the sensor-specific images above are defaults, not your unit's
  calibration — do not blind-flash them over a working camera.

## Sources

- Official ASUS CDN (still live, 2026-11):
  `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/FWUpdate_5_8_22.zip`
- ASUS support page for the Xtion PRO LIVE (driver/firmware downloads)
- MAGICC wiki (BYU) documenting the update and its USB 3.0 fix:
  `https://byu-magicc.github.io/wiki/hw_guides/asus_xtion_pro_live`
- Artec3D support article on upgrading Xtion Pro / Pro Live firmware

## Copyright

The firmware is © ASUS / PrimeSense (now Apple). It is mirrored here for
hardware preservation and interoperability with an open-source driver, from
ASUS's official public download. These files are **not** covered by xtion-rs's
open-source license; no source code for the firmware exists publicly.
