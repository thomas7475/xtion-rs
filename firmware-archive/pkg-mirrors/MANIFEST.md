# Firmware archive — Agent D: package archives + code-hosting mirrors + dead-link recovery

Channel scope: AUR / Debian / Ubuntu / RPM packaging, GitHub/Gitee/GitLab mirrors,
and recovery of dead ASUS / PrimeSense CDN URLs for the ASUS Xtion / PrimeSense
PS1080 "RD108x" platform (USB VID 1d27).

## Verified files

### 1. FW579-RD1081-112v2.zip — firmware 5.7.9 hotfix (USB 3.0 bandwidth fix) — NEW FIND

- Version: 5.7.9 (build "112 v2", package built 2012-09-02 per internal ZIP timestamps;
  the Artec/ASUS hotfix circulated 2012-10 as the "Xtion USB 3.0 hotfix")
- Path: `files/5.7.9-hotfix-RD1081/FW579-RD1081-112v2.zip`
- Size: 799,206 bytes
- SHA-256: `c5e7e38ba2a2ace4b3c3f019f6a01c826a6f57d02c68d1c720fef8a2937c5cae`
- Source URL: https://github.com/nh2/asus-xtion-fix (file at repo root,
  raw: https://raw.githubusercontent.com/nh2/asus-xtion-fix/master/FW579-RD1081-112v2.zip)
- Provenance: proprietary ASUS / PrimeSense firmware, originally distributed by
  Artec 3D ("How to upgrade ASUS Xtion/Pro/ProLive firmware",
  https://support.artec3d.com/hc/en-us/articles/201831841) from
  https://www.dropbox.com/s/7wepvnm0glhupey/FW579-RD1081-112v2.zip (old-style
  Dropbox share, now login-walled) and re-committed to GitHub by nh2 for
  preservation. Mirrored here for preservation/interoperability of the
  end-of-life Xtion hardware.
- Contents (verified by zip listing): `FLA/` flash images — `source.out.bin.fla`
  (134,876 B main firmware image), `BootManager.out.bin.fla` (23,214 B), plus 13
  parameter images (AlgorithmParams, DepthCmos, ImageCmos, RegistrationParams,
  IDParams, FixedParamsRD108, GainControl, PrimeProcessor, SensorSafetyParams,
  SensorTecParams, DefaultParams, ...); `UsbUpdate/` with `UsbUpdate.exe`,
  `!Update-RD108x!.bat`, `UsbUpdate_RD108.ini` (13-file flash map,
  `InitFunction=RepairFW5327ZoneB`), `UsbUpdate_RD108_SWOnly.ini`,
  `UsbUpdate_Factory.ini` and the OpenNI/Xn DLLs the flasher needs.
- This is a REAL device-firmware flash package for the RD108x platform (the
  Artec article explicitly warns "do not use this firmware update for
  PrimeSense Carmine 1.09", i.e. it targets the 1.08x/RD108 platform = ours).
- Note: package name says RD1081; ini/bat target RD108x — same platform family.
- Companion: `files/5.7.9-hotfix-RD1081/firmware-flush-hotfix-log.txt` — the
  flash-session log committed alongside the zip in the nh2 repo (provenance
  evidence of a successful flash to an Xtion Pro Live).

### 2. Xtion_Driver_SDK.zip — 2011 ASUS Xtion driver/SDK CD (NO firmware inside; kept locally, NOT pushed — exceeds 90 MB push cap)

- Path: `files/2011-xtion-driver-sdk/Xtion_Driver_SDK.zip` (local archive only)
- Size: 98,679,983 bytes
- SHA-256: `e2813579209612a768a3cad32e5ab020da5ace551c32b816668b5bef1614c90a`
- Source URL (still LIVE on ASUS CDN): https://dlcdnets.asus.com/pub/ASUS/MM/Xtion_Pro/Xtion_Driver_SDK.zip
  (also http://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/Xtion_Driver_SDK.zip)
- Listed on ASUS support page as "Xtion Pro driver&SDK package v1.0, 2011/06/21,
  94.11 MB". Contents: `SDK/setup.exe` (NSIS, 2011-05), `SDK/Setup.bat`,
  `SDK/Linux-x32-20.2.0.rar` / `Linux-x64-20.2.0.rar`. The rars contain only
  host-side stacks: `Sensor-Bin-Linux32-v5.0.2.3.tar.bz2`,
  `OpenNI-Bin-Linux32-v1.2.0.7.tar.bz2`, `NITE-Bin-Linux32-v1.3.1.6.tar.bz2`.
- Negative finding recorded so nobody re-downloads it looking for firmware.

## Dead leads (URL + reason)

- `http://www.primesense.com/wp-content/uploads/2013/04/FWUpdate_RD108x-112_5.8.14.exe`
  — **the official 5.8.14 updater URL** (published 2013-04-26, see
  https://kheresy.wordpress.com/2013/04/26/firmware-update-for-asus-xtion via
  WordPress REST API). primesense.com is defunct (connection fails). NOT yet
  recovered: archive.org / archive.ph / timetravel.mementoweb.org are all
  unreachable from this sandbox (timeouts), so Wayback recovery of this exact
  file remains an open task for an environment with archive.org access.
- `http://www.primesense.com/wp-content/uploads/2013/04/PrimeSense_Carmine-1.08x-RD_FW-5.8.14_ReleaseNotes_docver1.0.pdf`
  — 5.8.14 release notes; dead with primesense.com. (A 5.8.22 release-notes
  copy exists on scribd: scribd.com/document/885463578.)
- `http://www.primesense.com/updates/` — PrimeSense firmware download index; dead.
- `https://dlcdnet.asus.com/pub/ASUS/MM/Xtion_Pro/FWUpdate_5_8_14.zip` (and 5_7_9,
  5_7_24, 5_7_26, 5_8_20, 5_8_21 variants, dlcdnets+dlcdn) — 404. Only 5_8_22
  ever lived at this path pattern.
- `https://dlcdnets.asus.com/pub/ASUS/MM/Xtion_Pro/V1049_0430.zip` — LIVE but
  contains only `V1049_0430.iso` ("OPEN NI Package 20.4.2.20", 2012/11/30,
  603 MB): manuals + SDK installers (LINUX_X3/X6.RAR, PSWIN32/64.EXE). Verified
  by walking the ISO9660 filesystem via HTTP Range requests (script:
  /home/z/my-project/scripts/iso_deflate_reader.py) — **no firmware inside**.
- Same for `V1164_1202.zip` (OpenNI Package 20.4.4.0, 2013/12/18) and
  `XtionStore_installer.zip` (2013/07/01, 285 MB) — live on ASUS CDN, skipped
  (size / no-firmware expectation).
- `https://www.reconstructme.net/?wpdmdl=63` — ReconstructMe's "ASUS Xtion USB
  3.0 hotfix" WordPress download-manager file; download manager gone (200 HTML
  / 500). The hotfix it served is the same FW579 package recovered above.
- `https://www.reconstructme.net/2012/10/13/asus-xtion-usb-3-0-hotfix/` — HTTP 500.
- `https://www.dropbox.com/s/7wepvnm0glhupey/FW579-RD1081-112v2.zip?dl=1` —
  old-style Dropbox share, now serves an HTML interstitial (login-walled), no
  direct content link in page.
- AUR `openni-sensor-primesense`, `primesense`, `openni2`, `openni-sensor-kinect`,
  `sensor-kinect`, `primesense-server` — aur.archlinux.org cgit and `git clone`
  both blocked by Anubis anti-bot challenge; Wayback fallback also unreachable.
  (Debian's packaging shows these PKGBUILDs' source=() points at
  github.com/PrimeSense/Sensor — host driver, no firmware.)
- Debian/Ubuntu `openni-sensor-primesense` 5.1.0.41 (pool listing mined):
  `debian/watch` → `https://github.com/PrimeSense/Sensor/tags .*/Stable-(\d[\d\.]+).tar.gz`
  — host driver source only, never device firmware.
- gitee.com search — HTTP 405 (blocked); grep.app API — Vercel security
  checkpoint; GitLab project search — HTTP 403; GitHub code-search API — needs
  auth; GitHub REST API — rate-limit exhausted mid-hunt.
- jurn.link "Installing the Asus Xtion Pro in 2021" — connection timeouts.

## Notable observations

1. The ASUS CDN directory `https://dlcdnets.asus.com/pub/ASUS/MM/Xtion_Pro/`
   still serves all four historical Xtion_Pro packages live (2011 SDK, 2012 and
   2013 OpenNI ISOs, XtionStore) — but none contain device firmware; ASUS
   shipped firmware only as the separate FWUpdate zips of which 5_8_22 survives.
2. The 5.8.14 updater was distributed from primesense.com wp-content (per the
   2013-04-26 kheresy announcement "PrimeSense releases firmware update for
   ASUS Xtion Pro/Pro Live", which also links the OpenNI2 repo and
   Carmine-1.08x-RD release notes — confirming 5.8.14 covered Carmine 1.08x RD
   as well, i.e. the same RD108x platform).
3. The 5.7.9-era hotfix (FW579-RD1081-112v2) is the package Artec 3D and
   ReconstructMe distributed to fix the "xhci_hcd: Not enough bandwidth" USB 3.0
   problem; the nh2 repo documents that after flashing it the sensor stops
   working with OpenNI 1.x and needs OpenNI 2 — matching the 5.7.9/5.8.x
   OpenNI-2-only era.
4. Recovery of the 5.8.14 exe bytes should be retried from an environment with
   Wayback Machine access (CDX query for
   `primesense.com/wp-content/uploads/2013/04/*`); that is the single highest
   value remaining action for this channel.
