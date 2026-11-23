# ASUS Xtion Firmware Archive — Agent A (ASUS Official CDN + Support Pages)

Scope: ASUS official CDN live probing + Wayback Machine (Internet Archive).
Date of hunt: 2026-11-23. All sha256 values verified locally after download.

## Verified files

### 1. FWUpdate 5.8.22 (anchor, CDN verification) — MATCH
- Directory: `5.8.22-cdn-verification/`
- Filename: `FWUpdate_5_8_22.zip`
- Size: 2,250,298 bytes
- sha256: `6f9c0ec93bb77491416c5eeff4f6e37cb55483ad709faad08c95db94bd43caad`
- Source (live): `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/FWUpdate_5_8_22.zip`
- Result: byte-identical to the package already mirrored in xtion-rs (sha256 matches exactly).
  This confirms the CDN path scheme `dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/FWUpdate_<ver>.zip`.
- Zip contents: `FWUpdate_RD108x-5.8.22/FWUpdate_RD108x-5.8.22.exe` (2,301,952 bytes, zip dated 2013-11-05).
- Provenance: proprietary ASUS/PrimeSense software, mirrored for preservation and interoperability.

### 2. Firmware 5.7.9 hotfix package (NEW — not previously in repo)
- Directory: `5.7.9-artec-dropbox/`
- Filename: `FW579-RD1081-112v2.zip` (FW 5.7.9, RD108x platform, build 112 v2 — "OpenNI 2.0 / USB 3.0 hotfix")
- Size: 799,206 bytes
- sha256: `c5e7e38ba2a2ace4b3c3f019f6a01c826a6f57d02c68d1c720fef8a2937c5cae`
- Source: `https://raw.githubusercontent.com/nh2/asus-xtion-fix/master/FW579-RD1081-112v2.zip`
  (repo `nh2/asus-xtion-fix`; its README names the original source as the Artec3D KB article
  `https://my.artec3d.com/kb/upgrading_asus_firmware`, mirrored today as
  `https://support.artec3d.com/hc/en-us/articles/201831841-How-to-upgrade-ASUS-Xtion-Pro-ProLive-firmware`,
  which linked `https://www.dropbox.com/s/7wepvnm0glhupey/FW579-RD1081-112v2.zip` — now dead).
- Zip contents (verified): `FLA/` firmware flash images (BootManager.out.bin.fla, source.out.bin.fla
  134,876 bytes, params, internal dates 2012-09-02) + `UsbUpdate/` (!Update-RD108x!.bat, UsbUpdate.exe,
  UsbUpdate_RD108.ini, UsbUpdate_RD108_SWOnly.ini, OpenNI/Xn*.dll, internal dates 2012-06-28).
- This is the pre-5.8 USB 3.0 hotfix package referenced by Artec3D, nh2/asus-xtion-fix and the
  totakke.blog 2012-11-08 post; breaks OpenNI 1.x compatibility (per nh2 README).
- Provenance: proprietary ASUS/PrimeSense software originally distributed via Artec3D support;
  mirrored from the nh2/asus-xtion-fix GitHub mirror for preservation and interoperability.
  NOTE for dedupe: found via a support-page lead, not from an ASUS CDN host — other agents may
  have found the same package through the PrimeSense/OpenNI channel.

### 3. Driver CD V1164_1202 extracts (context evidence, not firmware)
- Directory: `v1164_1202-drivercd-parts/`
- `README.TXT` (612 B, sha256 `fc41ebaefdf0914d4259d4681c5d9da6292aa5be2c3511301d111364909b004c`)
  — confirms CD bundles: OpenNI Framework v1.5.5.0, Sensor DDK v5.1.4.0, NITE v1.5.2.21, USB driver v3.1.3.1.
- `XTIONPRO.EXE` (921,336 B, sha256 `0706568916e1eff5b762529d920ed6b3a8aca7c019ac41a7d694c855e8cfbed6`)
  — PE32 Win32 GUI utility from the CD; no firmware/FWUpdate strings.
- `ISO-CONTENTS.txt` — full remotely-parsed ISO9660 tree of the driver CD.
- Parent archive (live, 653,295,199 bytes / 623 MB, too large to mirror under the 90 MB rule):
  `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/V1164_1202.zip` (HTTP 200, Range supported,
  last-modified 2024-12-11; contains single deflated member `V1164_1202.iso`, 654,246,044 B).
  ISO tree: MANUAL/ (HELP.PDF, QUICK_ST.PDF, SETUP.EXE), SDK/LINUX/LINUX_X3.RAR, SDK/LINUX/LINUX_X6.RAR,
  SDK/PSWIN32.EXE, SDK/PSWIN64.EXE, XTIONPRO.EXE, ASUSLOGO.ICO, AUTORUN.INF, README.TXT.
  NO FWUpdate tool or firmware image inside the driver CD.
  Extraction method: HTTP Range fetch of zip tail -> central directory -> incremental raw-deflate
  inflation -> ISO9660/Joliet parse (only ~2.5 MB of the 623 MB transferred).
  ASUS download page metadata (via search snippet of asus.com regional support pages):
  "XtionCenter Package ... Version: 20.4.4.0 ... V1164_1202.zip 623.03MB", dated 2013-11-06.

## Dead leads

Live CDN probes (75 HEAD probes; ALL returned 404 on live host dlcdnet.asus.com):
- `FWUpdate_5_{8_15..8_21,8_10..8_14,8_0,7_0..7_9,6_0..6_2,5_0}.zip` under `/pub/ASUS/MM/Xtion_Pro/` — 404 (only 5_8_22 lives).
- Naming variants `FWUpdate_5.8.14.zip`, `FWUpdate_RD108x-5.8.14.zip`, `Xtion_FWUpdate_5_8_14.zip`, `Firmware_5_8_14.zip`, `FWUpdate_5.7.9.zip`, `FWUpdate_5_8_14.exe` — 404.
- Alternate paths `/pub/ASUS/MM/{Xtion_Pro_Live,Xtion,Xtion_PRO}/`, `/pub/ASUS/3D-Sensor/{Xtion_Pro,Xtion,Xtion_Pro_Live,Xtion_PRO_LIVE}/` for 5_8_22/5_8_14/5_7_9 — 404.
- Alternate CDN hosts `dlcdn.asus.com`, `dlsvr01.asus.com`, `dlsvr04.asus.com`, `dlcdnrt.asus.com`, `dlcdnhb.asus.com` — TCP connect fails (dead hosts; 000). `nw-dlcdnet.asus.com` resolves but 404s the firmware paths.
- `https://www.dropbox.com/s/7wepvnm0glhupey/FW579-RD1081-112v2.zip` (original Artec link) — link disabled by Dropbox (returns "disabled" error page).
- Wayback Machine / Internet Archive — **completely unreachable from this sandbox for the entire session** (web.archive.org and archive.org TCP timeouts on every retry over ~40 minutes; CDX sweeps and snapshot browsing impossible). STEP 3/4 (historical captures) could not be executed.
- `https://jurn.link/dazposer/...installing-the-asus-xtion-pro-in-2021/` (blog quoting V1164_1202.zip + FWUpdate_5_8_22.zip) — host unreachable (000) from this sandbox.
- ASUS support API `www.asus.com/support/api/product.asmx/GetPDSoftware` — HTTP 500. Modern supportonly pages are JS-rendered; no firmware downloads listed for Xtion Pro / Pro Live / Xtion.

## Notable observations

1. `dlcdnet.asus.com` is the only live ASUS download host; the final firmware 5.8.22 and the driver CD V1164_1202.zip remain online there, but every older FWUpdate version has been purged from the CDN.
2. The 5.7.9 package used a different naming scheme (`FW579-RD1081-112v2.zip`) than the later CDN packages (`FWUpdate_5_8_22.zip`), and was distributed as a hotfix through partners (Artec3D) rather than via the ASUS download page — which explains why guessing CDN names for 5.7.x fails.
3. Driver CD V1164_1202.zip internal zip date 2013-11-05/06 aligns with FWUpdate_5_8_22.zip's internal date (2013-11-05) — both are from the same final 2013 upload wave; the CD contains no firmware updater.
4. Wayback inaccessibility is an environment/network limitation, not proof that captures are absent; the CDX sweeps (dlcdnet/dlcdn/dlsvr01/dlsvr04 `/pub/ASUS/*Xtion*`) remain to be re-run from a network that can reach web.archive.org.
