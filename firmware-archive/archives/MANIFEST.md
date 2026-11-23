# Firmware Archive — Agent C (Internet Archive / community mirrors / dead-link recovery)

Date: 2026-11-23. Target: older ASUS Xtion / PrimeSense PS1080 ("RD108x") firmware
(5.7.x, 5.7.9, 5.8.14) and firmware-bearing packages (driver CDs, SDK bundles).

Provenance note: all files below are proprietary ASUS / PrimeSense software,
mirrored for hardware-preservation and driver-development purposes only.

## 1. ASUS Xtion Pro driver CD (V1164_1202) — first-party ASUS CDN

Source: https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/V1164_1202.zip
(live, HTTP 200, Last-Modified 2013-12-11 on CDN). The zip wraps a single
`V1164_1202.iso` (ISO 9660 volume label `V1164`), the official Xtion Pro
installation CD. CD contents per readme: OpenNI Framework v1.5.5.0,
Sensor DDK v5.1.4.0, NITE v1.5.2.21, USB driver v3.1.3.1 (2012 ship era,
OpenNI 1.x only). NOTE: this CD contains NO camera-firmware updater
(FWUpdate tool is not on the CD) — it is the 2012-era SDK/driver bundle.

Local-only (too large to push, >90 MB per-file cap; verify against these hashes):

| filename | size (bytes) | sha256 |
|---|---|---|
| V1164_1202.zip (downloaded) | 653,295,199 | 8199a8294142a529a76871ca21dc9dd5bf4560d27dcc1b5dc24b6d6c6d8ee720 |
| V1164_1202.iso (inner CD image) | 654,246,044 | 21cb02e44a77736ab43859f02be6a7f11d78e8506fe4cc4f5bc1d0296ad7320f |
| SDK_PSWin32.exe (NSIS SDK installer, PrimeSense SDK 20.4.4.0 Win32) | 122,303,030 | 14e707901cc6eed6a4e4a719baf783b65adc179b0ad8bcbd8394338860061b35 |
| SDK_Linux_Linux-x64-20.4.4.0.rar | 117,214,046 | 1acc45f9b18eb83d76a1484e2028bf57a0c4ccb5c541fbe72c1eaee0c311a105 |
| NITE-Win32-1.5.2-Dev.msi (from PSWin32.exe NSIS payload) | 103,903,232 | 9898da382001cab4fd5b4b68c4c4f94db0605e022472afaa3417a8ba8d4ab65f |

Pushed (this branch), extracted from the ISO / NSIS payload, clean filenames:

| filename | version | size | sha256 |
|---|---|---|---|
| cd-extract/readme.txt | — | 612 | fc41ebaefdf0914d4259d4681c5d9da6292aa5be2c3511301d111364909b004c |
| cd-extract/AutoRun.inf | — | 51 | 41f1c3e0a70a0385bacaaad3283be0bd146f979e8ac9bfb920ab2c6f889b8061 |
| cd-extract/XtionPro.exe (ASUS Xtion Pro Windows utility) | — | 921,336 | 0706568916e1eff5b762529d920ed6b3a8aca7c019ac41a7d694c855e8cfbed6 |
| cd-extract/Setup32.bat / Setup64.bat | — | 49 / 49 | 3ff5de32ca12f198db6bfd6694a4ce504e66b1ada522658f0be0b701e971bcf7 / 0dd94fc7fdf87d3f07155b20f35ad453e41bd0d058fe365b39ffd2e760756616 |
| cd-extract/Manual/Help.pdf | — | 447,229 | 05f7cc522f18e38844f6b2bb81da6098a74b6b6674e12766012d255074b93b5f |
| cd-extract/Manual/QuickStartGuide.pdf | — | 436,991 | ad8604889c542b1827008131f27101a3a9ecfb44b515211c22031058242f813e |
| cd-extract/SDK/Sensor-Win32-5.1.4-Redist.msi | Sensor 5.1.4.0 | 851,968 | d6e2d374eddad43d114db1e8b867d6fcd989655b5a12c45c1b8e8e40455a7dfb |
| cd-extract/SDK/OpenNI-Win32-1.5.5-Dev.msi | OpenNI 1.5.5.0 | 18,755,584 | e41158aed1694fd2e29f755195ba07719819be9d3d75595c8eef51427aa3c952 |
| cd-extract/SDK/Sensor-Bin-Linux-x64-v5.1.4.1.tar.bz2 | Sensor 5.1.4.1 | 540,676 | 055bc542d73955291afca115d627a4026773ed0121a8f289e715cf9cee4ec6d1 |

The Windows Sensor 5.1.4.0 redist MSI and the Linux Sensor 5.1.4.1 tarball were
inspected: they contain the XnSensorServer / libXnDeviceSensorV2 driver stack
only — no embedded camera firmware image, no FWUpdate tool.

## 2. Reference: still-live first-party firmware URL

https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/FWUpdate_5_8_22.zip → HTTP 200
(final firmware 5.8.22; already mirrored in this repository, not re-downloaded).
Older-version probes on the same CDN path all returned 404:
FWUpdate_5_8_14, 5_8_10, 5_8_20, 5_7_9, 5_7_26, 5_7_20, 5_7_14, 5_7_30, 5_7.
→ The ASUS CDN never exposed 5.7.x / 5.8.14 under that naming, or they were
purged. dlcdns.asus.com (mirror host) is unreachable (connection refused).

## 3. Dead leads (checked 2026-11-23)

- archive.org (all services: archive.org, web.archive.org incl. CDX API,
  ia*.us.archive.org download nodes) — connection refused / HTTP 429 from this
  sandbox's IP. No item enumeration, metadata listing, Wayback recovery, or
  download possible. **Environmental blocker, not evidence of absence.**
- https://jurn.link/dazposer/index.php/2021/10/23/installing-the-asus-xtion-pro-in-2021 —
  blog post referencing the ASUS drivers ISO V1164_1202.zip + FWUpdate_5_8_22.zip
  (likely archive.org links) — connection timeout.
- https://support.artec3d.com/hc/en-us/articles/201831841-... — "download
  archive here" firmware link is behind JS; static HTML exposes no file URL.
- Scribd "Carmine 1.08x RD FW 5.8.22 release notes v1.1" (2013-07-31) —
  documentation only, paywalled; not chased per instructions.
- github.com/taochenshh/ASUS-Xtion-Pro-Live — README-only repo, no firmware files.
- ASUS support page https://www.asus.com/supportonly/xtion pro/ — JS-driven,
  no static download list; ASUS CDN path probing used instead (see section 2).
- Untrusted (not downloaded, per trust rule): driver aggregators that rank for
  "Xtion driver" queries (driverscape/softpedia family) — no firmware packages
  identified on them anyway.

## 4. Notable observations

- The only still-live first-party source for Xtion software is the ASUS CDN
  (dlcdnet.asus.com): it serves the final FWUpdate_5_8_22.zip AND the complete
  2013 driver CD (V1164_1202) but nothing older.
- No archive.org item identifiers for "ASUS Xtion firmware" / "FWUpdate_RD108x"
  surfaced in web search; whether such items exist could not be confirmed
  because archive.org is unreachable from this sandbox.
- The driver CD's PrimeSense SDK 20.4.4.0 installers (PSWin32/64.exe) bundle
  OpenNI 1.5.5.0, Sensor 5.1.4.0 and NITE 1.5.2.21 MSIs — the last full
  PrimeSense-era software stack, preserved here, but it does NOT include any
  RD108x camera firmware updater.
