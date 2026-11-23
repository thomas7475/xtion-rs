# Consolidated Firmware Inventory - xtion-rs firmware hunt (2026-11-23)

Coordinator-consolidated results of the 4-agent parallel firmware hunt for the
ASUS Xtion / PrimeSense PS1080 (RD108x / Carmine 1.08x) platform, run against
repository https://github.com/thomas7475/xtion-rs (branch `main` and all
`research/idong100*` branches untouched throughout).

Public firmware line: 5.7.x (2012 ship era) -> 5.7.9 (2013-04) -> 5.8.14
(2013-04-26) -> 5.8.22 (2013-07-31, final; already mirrored in the repo).

## 1. Device firmware packages

| Version | File | Size | SHA-256 | Source | Status |
|---|---|---|---|---|---|
| 5.7.9 | FW579-RD1081-112v2.zip | 799,206 B | c5e7e38ba2a2ace4b3c3f019f6a01c826a6f57d02c68d1c720fef8a2937c5cae | github.com/nh2/asus-xtion-fix (originally the Artec3D 'Xtion USB 3.0 hotfix') | **RECOVERED this session** - FLA flash images + UsbUpdate tool |
| 5.8.14 | FWUpdate_RD108x-112_5.8.14.exe | unknown | unknown | http://www.primesense.com/wp-content/uploads/2013/04/ (host dead ~2015) | URL identified via kheresy announcement; not recovered (archive.org unreachable from sandbox) |
| 5.8.22 | FWUpdate_5_8_22.zip | 2,250,298 B | 6f9c0ec93bb77491416c5eeff4f6e37cb55483ad709faad08c95db94bd43caad | https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/FWUpdate_5_8_22.zip (still live) | Already in repo `firmware/`; live-CDN copy re-downloaded and verified byte-identical |

The 5.7.9 package contains `FLA/` flash images (`source.out.bin.fla` 134,876 B main
image, `BootManager.out.bin.fla`, 13 parameter images) plus `UsbUpdate/`
(`UsbUpdate.exe`, `!Update-RD108x!.bat`, `UsbUpdate_RD108.ini`). Zip-internal
timestamps: 2012-09-02. The Artec3D article explicitly excludes Carmine 1.09,
confirming the RD108x (this platform) target. Per the repo's own docs, flashing
5.7.9 would drop the camera back to the OpenNI 1.x protocol generation - it is
archived for preservation and diffing, not for the validation camera.

## 2. All recovered files (deduplicated by SHA-256)

### 2.1 Copied to download/firmware/ (files <= 60 MB)

| Agent channel | File | Size | SHA-256 (first 16) |
|---|---|---|---|
| archives | asus-cd-v1164-1202/biglocal/Sensor-Bin-Linux-x64-v5.1.4.1.tar.bz2 | 528.0 KB | 055bc542d7395529 |
| archives | asus-cd-v1164-1202/biglocal/pswin32/$PLUGINSDIR/OpenNI-Win32-1.5.5-Dev.msi | 17.9 MB | e41158aed1694fd2 |
| archives | asus-cd-v1164-1202/biglocal/pswin32/$PLUGINSDIR/Sensor-Win32-5.1.4-Redist.msi | 832.0 KB | d6e2d374eddad43d |
| archives | asus-cd-v1164-1202/biglocal/pswin32/$PLUGINSDIR/System.dll | 11.0 KB | dc58d8ad81cacb0c |
| archives | asus-cd-v1164-1202/biglocal/pswin32/$PLUGINSDIR/modern-header.bmp | 25.2 KB | 5ee216825beda46f |
| archives | asus-cd-v1164-1202/biglocal/pswin32/$PROGRAMFILES/PrimeSense/SDKUninstall.exe | 58.4 KB | c01227a7a0371f46 |
| archives | asus-cd-v1164-1202/cd-extract/AutoRun.inf | 51 B | 41f1c3e0a70a0385 |
| archives | asus-cd-v1164-1202/cd-extract/Manual/Help.pdf | 436.7 KB | 05f7cc522f18e388 |
| archives | asus-cd-v1164-1202/cd-extract/Manual/QuickStartGuide.pdf | 426.7 KB | ad8604889c542b18 |
| archives | asus-cd-v1164-1202/cd-extract/Setup32.bat | 49 B | 3ff5de32ca12f198 |
| archives | asus-cd-v1164-1202/cd-extract/Setup64.bat | 49 B | 0dd94fc7fdf87d3f |
| archives | asus-cd-v1164-1202/cd-extract/XtionPro.exe | 899.7 KB | 0706568916e1eff5 |
| archives | asus-cd-v1164-1202/cd-extract/readme.txt | 612 B | fc41ebaefdf0914d |
| archives | github-taochenshh/repo.tar.gz | 858 B | ab40fff4cfee15eb |
| asus-cdn | 5.7.9-artec-dropbox/FW579-RD1081-112v2.zip | 780.5 KB | c5e7e38ba2a2ace4 |
| asus-cdn | 5.8.22-cdn-verification/FWUpdate_5_8_22.zip | 2.1 MB | 6f9c0ec93bb77491 |
| asus-cdn | v1164_1202-drivercd-parts/ISO-CONTENTS.txt | 501 B | db3721b699c81d69 |
| pkg-mirrors | 5.7.9-hotfix-RD1081/firmware-flush-hotfix-log.txt | 9.1 KB | 849feb3cadc73718 |
| primesense | asus-driver-cd-v1164-1202-extracts/Sensor-Bin-Linux-x86-v5.1.4.0.tar.bz2 | 534.3 KB | e85fd7dc3aca0a4e |
| primesense | asus-xtion-driver-sdk-2011/Linux-x32-20.2.0.rar | 21.2 MB | 61c59fa7db25ae40 |
| primesense | asus-xtion-driver-sdk-2011/Linux-x64-20.2.0.rar | 21.3 MB | c062d2495dcf38ab |
| primesense | asus-xtion-driver-sdk-2011/Setup.bat | 47 B | 52151d72e6afd151 |
| primesense | asus-xtion-driver-sdk-2011/setup.exe | 51.6 MB | 1efaf82871f9737b |

### 2.2 Large files kept in the workspace only (work/firmware/, not pushed, > 90 MB GitHub cap or > 60 MB local cap)

| Agent channel | File | Size | SHA-256 (first 16) | Still-live source |
|---|---|---|---|---|
| archives | asus-cd-v1164-1202/V1164_1202.iso | 623.9 MB | 21cb02e44a77736a | https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/V1164_1202.iso |
| archives | asus-cd-v1164-1202/V1164_1202.zip | 623.0 MB | 8199a8294142a529 | https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/V1164_1202.zip |
| archives | asus-cd-v1164-1202/biglocal/SDK_Linux_Linux-x64-20.4.4.0.rar;1 | 111.8 MB | 1acc45f9b18eb83d | - |
| archives | asus-cd-v1164-1202/biglocal/SDK_PSWin32.exe;1 | 116.6 MB | 14e707901cc6eed6 | - |
| archives | asus-cd-v1164-1202/biglocal/pswin32/$PLUGINSDIR/NITE-Win32-1.5.2-Dev.msi | 99.1 MB | 9898da382001cab4 | - |
| pkg-mirrors | 2011-xtion-driver-sdk/Xtion_Driver_SDK.zip | 94.1 MB | e2813579209612a7 | https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/Xtion_Driver_SDK.zip |

Note: neither ASUS driver CD (V1164_1202, 2013-12; Xtion_Driver_SDK, 2011) contains
any FWUpdate tool or device firmware image - they are the ship-era HOST stack
(OpenNI 1.x, NITE, PrimeSense Sensor 5.0.x/5.1.4.x), which confirms device firmware
updates were always distributed as separate downloads.

## 3. Branches pushed to thomas7475/xtion-rs

| Branch | Commit | Channel / contents |
|---|---|---|
| agent/firmware-asus-cdn | c68e76b | ASUS official CDN probing + support-page leads; 5.8.22 CDN anchor verification; 5.7.9 hotfix; driver-CD extracts |
| agent/firmware-primesense | 35f4354 | PrimeSense/OpenNI ecosystem: ASUS SDK / driver-CD host-stack artifacts (12 files) |
| agent/firmware-archives | 1d4f03f | Internet-archive & community-mirror channel: driver-CD extracts incl. OpenNI 1.5.5 MSI (12 files) |
| agent/firmware-pkg-mirrors | 04a751b | Package-archive & code-mirror channel: 5.7.9 hotfix package + provenance log |
| agent/firmware-summary | (this push) | This consolidated INVENTORY.md + all four agent MANIFEST.md files |

`main` verified unchanged (9baffbb) after all pushes; `research/idong100*` untouched.

## 4. Dead leads

| Lead | Outcome |
|---|---|
| dlcdnet.asus.com FWUpdate_5_8_14.zip and 75+ version/path/host variants (FWUpdate_5_7_*, 5_8_15..5_8_21, alternate paths /MM/Xtion_Pro_Live/, /3D-Sensor/*, hosts dlcdn/dlsvr01/dlsvr04/dlcdnrt) | HTTP 404 / TCP dead - ASUS purged all pre-5.8.22 firmware from its CDN; only dlcdnet.asus.com still serves Xtion_Pro files |
| http://www.primesense.com/wp-content/uploads/2013/04/FWUpdate_RD108x-112_5.8.14.exe | Host defunct (~2015). The OFFICIAL 5.8.14 updater URL - identified via the kheresy 2013-04-26 announcement; top recovery target via Wayback |
| web.archive.org + archive.org (CDX API and download nodes), all four agents, whole session | Unreachable from this sandbox (TCP timeouts / HTTP 429) - environmental blocker, NOT evidence that captures are absent |
| Artec3D support article 201831841 'download archive here' link (Dropbox share) | Login-walled by Dropbox; original 5.7.9 hotfix distribution point |
| kheresy.wordpress.com 2013-04-26 firmware report | Hashcash challenge / 403 (key facts still recovered via WordPress REST API: 5.7.9 filename, 5.8.14 announcement) |
| jurn.link 2021 Xtion install notes | Connection timeout |
| AUR archlinux.org package pages | Anubis anti-bot protection |
| gitee.com search / GitLab search / grep.app | HTTP 405 / 403 / Vercel checkpoint |
| api.github.com OpenNI release asset enumeration | Rate-limited (unauthenticated) |
| Scribd 'Carmine 1.08x RD FW 5.8.22 release notes v1.1' | Paywalled - documentation lead only |

## 5. Recommended next steps

1. Retry Wayback CDX from an unblocked network for:
   `primesense.com/wp-content/uploads/2013/04/*`,
   `dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/*`, `dlsvr04.asus.com/pub/ASUS/*Xtion*`.
2. Authenticated GitHub code search for `FWUpdate_RD108x-111_5.8.14`,
   `FWUpdate_RD108x`, `UsbUpdate_RD108` (repos mirroring old ASUS packages).
3. Ask Artec3D support for the original hotfix archives (they distributed 5.7.9).
4. Optionally expand scope to the Carmine 1.09 firmware line (different platform).
5. Offline value already available: diff the 5.7.9 flash images against the
   5.8.22 images in `firmware/5.8.22/` (main image 134,876 B vs 153,704 B) to
   document the protocol-generation change.

## 6. Verification discipline

Every file above was checked with `file` (no HTML error pages), byte-size and
SHA-256, and archive listings (`unzip -l`) where applicable. The live-CDN 5.8.22
copy matches the repository's `firmware/FWUpdate_5_8_22.zip` hash exactly. The
5.7.9 package was found identically by two independent agent channels (same SHA-256).
