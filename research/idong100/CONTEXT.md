# idong100 Research — Verified Background (as of 2026-11-23)

This file holds the facts established in earlier research sessions so that
every research agent starts from the same state. Everything here was
verified from the vendor's pages, the physical device, or a full EEPROM
dump. Read this before starting your own searches.

## Vendor and products

- Vendor: **Toltech** — Chinese name 泰山体育科技 (Shenzhen Taishan Sports
  Technology), Shenzhen, China. Website `www.toltech.cn`
  (pages: `/en/ProductList.jsp`, `/cn/ProductList.jsp`,
  `/en/ProductView.jsp?id=42` = idong100 chipset, `?id=38`, Download page
  `id=22`, "Development Tool" pages `id=18`/`id=19`).
- Consumer brand: **i-dong (爱动)**. Created 2007 (investment by Taishan
  Sports Industry Group + Shenzhen Institute of Advanced Technology, CAS);
  operating company founded 2008-01-16. Related entity: **Taishan Online
  (泰山在线)**, which designed the BG0316 camera module.
- Product family (from toltech.cn product list):
  - **idong Sensor V2.6** — finished dual-camera tracking sensor with
    onboard idong100 image processor; "locates marker points in 3D space;
    provides their coordinates and motion trajectories" (a marker-tracking
    sensor, not a webcam).
  - **idong100 chipset** — idong100 processor + **BG0316** camera module.
  - **idong200 chipset** — idong200 processor + **idong201** image sensor +
    optical lens (successor generation).
- Vendor platform statement (CN product page, publicly truncated):
  「基于应用层界面设计，爱动感应器可为基于Windows、Linux...」 — "based on
  application-layer interface design, the idong sensor can serve
  applications built on Windows, Linux ..." → a Linux interface almost
  certainly exists but is not published (private OEM SDK).

## The physical device (idong Sensor V2.6, owner-verified)

- **USB**: VID:PID **0471:2600** (lsusb prints "Philips (or NXP)"; the PID
  is absent from usb.ids and from the public record). High-speed. ONE
  vendor-specific interface (bInterfaceClass 0xff), bulk endpoints
  **0x02 OUT** (commands, presumed) and **0x82 IN** (tracking data,
  presumed), maxpacket 64 (full-speed value — spec violation at high
  speed, logged and accepted by Linux). No string descriptors,
  bcdDevice 0.00, no driver binds. NOT a pwc-family Philips webcam (those
  are 0471:0300–0313, full-speed isochronous).
- **T24C16A EEPROM** (2048 B), full dump decoded,
  MD5 `dc02fce8ca39155135a4b0368b3f33ae`:
  - 0x0000 header tag `ef0123456789abcd`; records at 0x0100 (stereo
    calibration), 0x0180 (`"2059"` version code), 0x0192 (8×int16
    [386,473,215,303,142,221,0,0]), 0x0300 identity (serial
    `AB460DC8BA12C6840B49CD5786C329DD`, codes `"000T"`, `"2613"`); every
    record duplicated +0x400; record format
    `[payload][checksum = sum(payload) & 0xFF][0xFF]`; all 6 checksums
    verified.
  - Calibration (OpenCV convention, 28×float32 LE): cam1 fx 428.761292,
    fy 430.208038, cx 312.787292, cy 212.867691; cam2 fx 435.891205,
    fy 437.767944, cx 312.149200, cy 179.222244; small distortion
    (|k1| < 0.05); R orthonormal (det = 1.000000);
    T = (99.5627, 0.4639, 9.3304) mm with **|T| = 100.0000 mm exactly**
    (designed baseline). cx ≈ 312–313 px → 640-px-wide frames. Mean focal
    length 432.33 px.
  - Consequence: **stereo processing happens on the idong100**; the host
    receives computed marker coordinates over the bulk endpoints.

## Goal of this research

Find ANY software / firmware / documentation for idong100, idong200,
BG0316, or idong201 — or for any product that uses these chips: SDKs,
drivers, PC client software, firmware update packages, datasheets,
application notes, protocol descriptions — and anything that reveals the
USB protocol on endpoints 0x02/0x82, so that a Linux user-space (libusb)
driver can be written.

## Already searched — NEGATIVE (2026-11-18; do not simply repeat, go deeper or different)

- Vendor site EN+CN product pages; EN `ProductView.jsp?id=42` and `?id=38`
  render empty/dynamic to automated access; site intermittently returns
  Error 408 (Chinese timeout page).
- Wayback: direct fetches of ProductList.jsp snapshots (2020 EN, 2023 CN)
  timed out; CN homepage Error 408.
- Web (EN/CN), all negative: "toltech idong100 image processor",
  "idong100 camera driver", "idong100 BG0316", "i-dong sensor driver",
  "爱动 感应器 驱动", "泰山在线 SDK", "idong200 linux library",
  "github idong100", "xeyez", "爱动 驱动 SDK 拆解 逆向",
  "泰山在线 二次开发", "0471:2600" (twice, incl. "linux OR driver OR webcam").
- No repository, no kernel driver, no usb.ids entry, no protocol write-up
  found anywhere as of 2026-11-18.

## Safety rules for the physical device (unchanged, in force)

- Reading endpoint 0x82 is safe. **NEVER write to endpoint 0x02
  speculatively** (possible firmware-update commands).
- **NEVER write to the T24C16A EEPROM** — the dump is the only backup of
  the factory calibration.
- Treat any MD5 change on a future EEPROM read as corruption.
