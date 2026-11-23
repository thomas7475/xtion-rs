# PrimeSense / OpenNI Ecosystem Firmware Archive — Agent B

Channel: PrimeSense / OpenNI ecosystem + vendor support articles (Artec3D, kheresy,
MAGICC wiki, PrimeSense site via Wayback, GitHub OpenNI orgs).
Scope: older ASUS Xtion / Xtion Pro / Xtion Pro Live / PrimeSense Carmine (PS1080,
platform RD108x = Carmine 1.08x reference design) firmware: 5.7.x, 5.7.9, 5.8.14,
other 5.x, Carmine lines.

## Headline finding

**No pre-5.8.22 device-firmware package could be retrieved from any live host.**
The only live vendor host is ASUS's download CDN (`dlcdnet.asus.com`), and the
complete ASUS "Xtion PRO" download catalog (11 files, as indexed by rebyte.me
from the ASUS product page, Nov 2013 era) contains exactly ONE firmware file:
`FWUpdate_5_8_22.zip` (already mirrored in this repo). All probed older-version
URLs (`FWUpdate_5_7_9.zip`, `FWUpdate_5_8_14.zip`, 5_7_2/5_7_5/5_7_10/5_8_20/
5_8_26/5_9_1/5_9_2, other folders) return 404. The 5.8.14 and 5.7.9 updates were
distributed via PrimeSense's own site (dead ~2015, only via Wayback — unreachable
during this hunt; see Dead leads).

What WAS recovered and is mirrored below: the era-correct host software stack
(PrimeSense Sensor DDK 5.0.2 and 5.1.4.x, OpenNI 1.2.0.7 and 1.5.5.0) from two
official ASUS driver packages. Neither contains device firmware images or a
flash updater — confirming that Xtion firmware updates were always separate
download artifacts (FWUpdate zips), not part of the SDK/driver CDs.

## Mirrored files

All files proprietary ASUS / PrimeSense; mirrored for preservation and
interoperability research only (no redistribution rights granted).

### asus-xtion-driver-sdk-2011/ — ASUS Xtion Driver SDK v1.0 (2011-05/06, SDK 20.2.0)

Source: `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/Xtion_Driver_SDK.zip`
(98,679,983 bytes, sha256 `e2813579209612a768a3cad32e5ab020da5ace551c32b816668b5bef1614c90a`,
zip contains `SDK/{setup.exe, Setup.bat, Linux-x32/x64-20.2.0.rar}`; zip too large for
direct mirroring, inner files extracted byte-identical with 7-Zip 24.09 / Info-ZIP).

| file | size (B) | sha256 |
|---|---|---|
| setup.exe (NSIS; OpenNI-Win32-1.2.0-Dev.msi 15257621 B, Sensor-Win32-5.0.2.msi 536384 B, NITE-Win32-1.3.1-Dev.msi 38268639 B) | 54,130,310 | `1efaf82871f9737b49b8036dc7f652c17cd75e4d212d27f070d28d3d50bc0ed7` |
| Linux-x64-20.2.0.rar (Sensor-Bin-Linux64-v5.0.2.3.tar.bz2, OpenNI-Bin-Linux64-v1.2.0.7.tar.bz2, NITE-Bin-Linux64-v1.3.1.6.tar.bz2) | 22,364,725 | `c062d2495dcf38abf0f37f4d28bf5fa157c524d4d42919c0999125a4fad7c1a5` |
| Linux-x32-20.2.0.rar (x86 equivalents) | 22,194,686 | `61c59fa7db25ae40a7b9749847a10316f90c2e62120ef1470af72e4d5ed7fe16` |
| Setup.bat | 47 | `52151d72e6afd1512d8bccb01ef6c531bc1fe2d327aae9aa057911d8ed748262` |

Era: pre-release Xtion (2011), OpenNI 1.2.x, Sensor DDK 5.0.2 — oldest ASUS Xtion
software stack found anywhere on the live web.

### asus-driver-cd-v1164-1202-extracts/ — ASUS Xtion Pro driver CD v20.4.4.0 (2012 build, ISO dated 2013-12-03)

Source: `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/V1164_1202.zip`
(653,295,199 bytes, sha256 `8199a8294142a529a76871ca21dc9dd5bf4560d27dcc1b5dc24b6d6c6d8ee720`,
contains single `V1164_1202.iso` 654,246,044 B; both too large to mirror, small files
extracted below — ISO is ISO9660+Joliet, listing script notes in repo history).

CD stack (per `readme.txt`): OpenNI Framework v1.5.5.0, **Sensor DDK v5.1.4.0**,
NITE v1.5.2.21, USB driver v3.1.3.1 — the exact host stack matching the 5.7.x
device-firmware ship era.

| file | size (B) | sha256 | origin inside CD |
|---|---|---|---|
| readme.txt | 612 | `fc41ebaefdf0914d4259d4681c5d9da6292aa5be2c3511301d111364909b004c` | /readme.txt |
| XtionPro.exe | 921,336 | `0706568916e1eff5b762529d920ed6b3a8aca7c019ac41a7d694c855e8cfbed6` | /XtionPro.exe (autorun installer stub) |
| Sensor-Win32-5.1.4-Redist.msi | 851,968 | `d6e2d374eddad43d114db1e8b867d6fcd989655b5a12c45c1b8e8e40455a7dfb` | from SDK/PSWin32.exe (NSIS `$PLUGINSDIR`) |
| Sensor-Bin-Linux-x64-v5.1.4.1.tar.bz2 | 540,676 | `055bc542d73955291afca115d627a4026773ed0121a8f289e715cf9cee4ec6d1` | from SDK/Linux/Linux-x64-20.4.4.0.rar |
| Sensor-Bin-Linux-x86-v5.1.4.0.tar.bz2 | 547,093 | `e85fd7dc3aca0a4eff82f78fbf2a8132fbacadd47c7c421954f1578eeddd8f4e` | from SDK/Linux/Linux-x32-20.4.4.0.rar |
| Setup32.bat / Setup64.bat | 49 each | `0dd94fc7fdf87d3f07155b20f35ad453e41bd0d058fe365b39ffd2e760756616` / `3ff5de32ca12f198db6bfd6694a4ce504e66b1ada522658f0be0b701e971bcf7` | /SDK/ |

## Complete ASUS CDN "Xtion Pro" catalog (live, all under `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/`)

Not mirrored (oversize / not firmware): `FWUpdate_5_8_22.zip` 2,250,298 B (already
in repo); `V1049_0430.zip` 632,823,982 B (ASUS SDK **20.4.2.20**, 2012-era driver CD —
not downloaded, over 300 MB cap); `V1164_1202.zip` 653,295,199 B (20.4.4.0, see above);
`XtionStore_installer.zip` 299,820,802 B; `BeatBooster_1_0_5_setup.rar` 469,530,067 B;
`Xtion_Center_Package_Win8_8_1.rar`, `BoxingTrainer_Patch.rar`, `Mayafit_Patch.rar`,
`Guidelines_For_Developers.zip` 986,127 B, `Quick_Start_Guide.zip` 800,809 B.

## Dead leads

- `https://kheresy.wordpress.com/2013/04/26/asus-xtion-pro-firmware-update/` —
  live site serves hashcash JS challenge (curl) / HTTP 403 (WebFetch). Via search
  snippets: the 2013-04-26 5.8.14 update was downloaded as a **single executable**
  (「下載後是一個執行檔」), consistent with the official naming
  `FWUpdate_RD108x-111_5.8.14.exe` (the 5.8.22 release notes on Scribd name the final
  updater `FWUpdate_RD108x-112_5.8.22.exe`, so 5.8.14 = build 111, 5.7.9 presumably
  build 110). Exact URL unreachable.
- `https://support.artec3d.com/hc/en-us/articles/201831841` — Cloudflare-protected;
  its "download archive here" firmware link target could not be resolved.
- `https://jurn.link/dazposer/index.php/2021/10/23/installing-the-asus-xtion-pro-in-2021/`
  — host unreachable (connect timeout) from this sandbox; snippet confirms it links
  the official drivers ISO (V1164_1202.zip, found independently above) and
  FWUpdate_5_8_22.zip only.
- Wayback Machine (`web.archive.org`, `archive.org`) — connect timeout (HTTP 000)
  for the entire session from both sandbox curl and the WebFetch backend; all
  PrimeSense-site CDX sweeps, kheresy/Artec3D snapshots blocked. Re-try later:
  CDX patterns `primesense.com/*` and `www.primesense.com/*` filtered for
  `FWUpdate|firmware|carmine|sensor`.
- `api.github.com` — rate limit exceeded (shared egress IP); release assets of
  OpenNI/OpenNI, OpenNI/OpenNI2, OpenNI/Sensor, avin2/SensorKinect,
  occipital/OpenNI2 could not be enumerated (these repos ship host drivers /
  SDKs only; no device firmware expected in their assets).
- `https://rebyte.me/en/asus/77165` — ASUS mirror index (source of the 11-file
  catalog above); Cloudflare-protected, downloads not directly accessible.
- ASUS CDN firmware variants `FWUpdate_{5_7_2,5_7_5,5_7_9,5_7_10,5_8_14,5_8_20,5_8_26,
  5_9_1,5_9_2}.zip` under `MM/Xtion_Pro/` (and sibling folders) — all 404.

## Provenance / license note

Everything above is proprietary ASUS / PrimeSense Ltd. software (PrimeSense assets
were acquired by Apple in 2013; the site went dark ~2015). It is mirrored here
without permission, strictly for hardware preservation and interoperability
research of end-of-life sensors (USB VID 1d27, PS1080 / RD108x). No warranty.
