# Agent 02 — i-dong consumer products and host software (retry, 2026-11-23)

Branch: `research/idong100-02-products-software`. This file consolidates the
work of the first (timed-out) attempt — whose artifacts were re-verified live
by this retry — plus new material gathered by the retry (toltech.cn news
archive, FAQ pages, EN ToolsPage, Ghost tool download, download-server
probing).

## Executive summary

- **JACKPOT (verified live 2026-11-23): the official i-dong download server
  `download.i-dong.com` is still up and serving the complete Windows PC
  client software, the sensor driver, and console firmware** — 8 files,
  ~13.7 GB total, all HTTP 200 with `Accept-Ranges: bytes`, linked from the
  still-live consumer portal `www.idong.com` / `www.i-dong.com`
  ("软件下载" section). URL/size/date table below; all 8 re-verified with
  HEAD by this retry.
- **The "Sports King PC" installer contains the entire missing driver stack
  for the idong Sensor**: `dependent/xeyez_pc_v2.6.0.6/` inside
  `idong_1.9.04_windows_yundongwang_A_setup.zip` holds
  `xeyez.sys` (Win32 + x64 kernel drivers), `xeyez.inf`,
  `xeyezCoIns.dll`, `EasyUSB.dll` (exports `ReadData`/`WriteData`),
  `StereoCamera.dll` (`Get3DObject`/`Get3dSingle`/`SetLens`), service/tray
  exes, **and the sensor firmware set** (8051 `app_m8051_2500/2611.hex`,
  Blackfin BF527 `ens_code.ldr`, Xilinx Spartan-3 `A_a_v4.4_BG0313.bit`,
  CMOS/FPGA register-init text blobs). I re-verified the chain end-to-end:
  ranged-GET extracted `xeyez_Lobby.dat` (20,901,912 B) from the live remote
  1.5-GB zip and its sha256 matches the first attempt's inventory exactly
  (`0b04734f…`), as does `AUTORUN.INF` (`250fc22f…`, label
  「爱动体感运动机（运动王pc版）」 = "i-dong motion machine, Sports King PC
  edition").
- **Protocol-relevant insight: USB 0471:2600 is the BOOTLOADER PID.**
  `xeyez.inf` maps `USB\VID_0471&PID_2600` = "TOLTech xeyez Bootloader";
  the runtime device is `PID_2500` ("TOLTech xeyez v2.5", interface
  `MI_04`); older gens: PID 2100 (v2.1), 0668 (v2.0), 0666 (v1.0). This
  explains why the physical unit (0471:2600) has no string descriptors and
  bcdDevice 0.00: it is waiting for firmware upload via EP 0x02.
- The Windows kernel driver is the classic WDM **BulkUSB sample** (strings:
  "BF527 Bulk USB DriverEntry", `IRP_MJ_READ/WRITE` = `BulkUsb_Dispatch*`,
  registry `BulkUsbEnable`, `MaximumTransferSize` 4096) — a thin bulk
  read/write pipe, i.e. **the entire vendor protocol is visible in
  user-mode DLLs** (`EasyUSB.dll`, `StereoCamera.dll`, `BasicInterFace.dll`,
  `xEyeService.exe`), all inside the verified container, ready for RE.
- Host-side config INIs recovered from the same payload give the streaming
  model: 50 fps frame limit, `PipeBufferSize 9000`, `MaxRead 1` reads per
  frame, `USBComFPS 35`, `DevicePlatform 2` (2nd-gen hardware),
  `BitFile A_a_v4.4_BG0313.bit`, firmware selector `xeyezHal.ini` with
  per-PID hex versions (2500→"2700", 2611→"2619") — matching the
  "2059"/"2613" code strings found in the sensor's EEPROM dump.
- **Console firmware also downloadable**: 运动王 (Sports King) console
  firmware V1.2.40 as a 4.2-GB Ghost disk image for SD-card flashing
  (`ce550_1.2.40.GHO`, 2013-02-19) + `Ghost11.5.zip` (13,092,098 B,
  sha256 `8753d6ec0fc9538732cb58d5db496c80c214fa6c9d8cd16f05792561e1724350`,
  downloaded to /tmp by this retry; contains Ghost 11.5 tools + a Chinese
  how-to txt — a Windows-style disk image, so the Sports King console is
  Windows-hosted, consistent with the 2011 2nd-gen console).
- Vendor `toltech.cn` pages fetched live (CN+EN ToolsPage, CN
  TechnologyPage, CN/EN Download.jsp): **AnySense SDK v1.0.21** (2013-01-28,
  Java/C/C++/C#/Flash/Unity3D) with hardware kit 爱动感应器 + airtouch
  wristband + remote pen/handle + USB cable; **reference docs for the 2.6-gen
  sensor (BOM, circuit diagram, 3D structure, PCB) exist but link only to
  `FeedBack.jsp`** → must be requested (anysense@toltech.cn).
  `Download.jsp` is a stub ("敬请期待" = "coming soon").
- Product generations named from vendor news archive (fetched live): family
  爱动体感运动机 (i-dong motion machine); PC editions 时尚版 "Fashion"
  V1.9.02 (2011-12-08), 大众版 "Mass" 1.0.6 (2013-05-24), 运动王 "Sports
  King" PC 1.9.04 (2012-09-12); console 运动王 (firmware V1.2.40, 2013);
  Reuters 2011: "third generation of i-dong" home console, 3-camera sensor
  bar, IR-refracting markers, RMB 1680, "can be used for gaming with a PC
  or a set-top box". PC install creates two desktop shortcuts:
  「爱动在线运动」 (i-dong Online Sports) and 「三维动作识别器」 (3D Motion
  Recognizer — the sensor tray/mouse app).
- **Skyworth tie-in confirmed on vendor news** (2012-07-25): joint
  "Champion Ping Pong" tournament with 创维云电视 (Skyworth cloud TV)
  discount vouchers; i-dong shown inside the Skyworth booth at the 2012
  HIGH-FAIR; 2012-11-26: i-dong flagship stores in Shenzhen Hai'an City /
  KK MALL.
- Environment limits (negative): from this sandbox Bing returns canned
  unrelated results, Baidu/Sogou 302 to captchas, DDG/Mojeek blocked,
  archive.org/timetravel/archive.ph all unreachable (no Wayback mining
  possible in this session), manualslib.com 403. All "search-engine"
  trails (百度网盘 shares, AnySense SDK mirrors, archived JD/Taobao
  listings) must be re-run from an unrestricted host — they are queued as
  follow-ups, not done.

## Key findings

### 1. Official download server `download.i-dong.com` — 8 live files (the payload mother lode)
- Source (list page): http://www.idong.com/idong/a/service/ruanjianxiazai/ (retrieved 2026-11-23, live)
- Item pages: `…/2016/0620/{43,44,45,46}.html` (titles: 运动王PC版安装包 /
  运动王主机固件更新到V1.2.40 / PC时尚版安装包V1.9.02下载 / PC大众版安装包1.0.6下载)
- All 8 files re-verified HTTP 200 on 2026-11-23 (HEAD, exact sizes):

| URL | Bytes | Last-Modified |
|---|---|---|
| http://download.i-dong.com/setup/yundongwang-ver/idong_1.9.04_windows_yundongwang_A_setup.zip | 1,506,761,140 | 2012-09-12 |
| http://download.i-dong.com/setup/yundongwang-ver/idong_1.9.04_windows_yundongwang_B_setup.zip | 1,706,860,901 | 2012-09-12 |
| http://download.i-dong.com/download/windows1.2.40/ce550_1.2.40.GHO | 4,207,555,727 | 2013-02-19 |
| http://download.i-dong.com/download/Ghost11.5.zip | 13,092,098 | 2012-08-15 |
| http://download.i-dong.com/setup/fashion-ver/idong_1.9.02_windows_shishang_A_setup.zip | 1,705,269,409 | 2011-12-08 |
| http://download.i-dong.com/setup/fashion-ver/idong_1.9.02_windows_shishang_B_setup.zip | 1,509,243,844 | 2011-12-08 |
| http://download.i-dong.com/setup/dazhong-ver/windows_dazong_1.0.6_A.zip | 1,308,120,784 | 2013-05-24 |
| http://download.i-dong.com/setup/dazhong-ver/windows_dazong_1.0.6_B.zip | 1,001,642,009 | 2013-05-24 |

- Page 43 (translated): "This driver must be used with the i-dong Sports
  King sensor (识别器). Connect the Sports King sensor to the computer and
  install this driver to use it on a PC. Note: due to different system
  platforms, some items may be unavailable when the sensor is connected to
  a computer. Download A and B archives completely, extract both into the
  same folder, then install."
- Page 44 (translated): "i-dong Sports King latest package, technical
  personnel only. Latest version V1.2.40 … image file can update online
  after install. Ghost image (needs Ghost tool for SD-card flashing)…
  warning: do not attempt if unfamiliar with flashing tools."
- Why it matters: this is downloadable, redistributable-in-analysis,
  official host software for the exact sensor hardware (V2.6, PID family
  0471:2xxx) — everything needed to reconstruct the EP 0x02/0x82 protocol.

### 2. End-to-end verification of the remote-zip extraction chain
- Method (this retry): HTTP `Range` reads against the live 1.5-GB A-zip;
  parsed ZIP central directory (99 entries), then pulled the DEFLATE
  member `dependent/xeyez_pc_v2.6.0.6/xeyez_Lobby.dat` (compressed
  20,878,467 B → 20,901,912 B) and `AUTORUN.INF`.
- Results: sha256(xeyez_Lobby.dat) =
  `0b04734f23a61fe5f7b60a8e0205d4bb80bb909a6bed7773c320d809b893a00a`,
  sha256(AUTORUN.INF) =
  `250fc22f23e6acc969a6234cb3231b318fba737e17b5edb9759dc3233c50e7c7` —
  both identical to the first attempt's `download-server-inventory.md`, so
  the whole inner extraction (file list, per-file hashes below) is
  trustworthy.
- Outer-zip layout confirmed: `idong_setup.exe` (2,132,624 B) +
  `idong_setup-{1..7}.bin` (200 MB each, installer payload split),
  `dependent/` (DirectX 9.0c, Flash, VC runtimes, Xvid) and
  `dependent/xeyez_pc_v2.6.0.6/` = the sensor driver+firmware payload.
- AUTORUN.INF label: 「爱动体感运动机（运动王pc版）」 — "i-dong
  motion-sensing exercise machine (Sports King PC edition)" (product-name
  confirmation).

### 3. `xeyez.inf` — the PID map; 0471:2600 is the bootloader
- Source: extracted from `xeyez_Lobby.dat` (NSIS installer, 646 files; see
  artifacts `xeyez.inf.txt`, `nsis-file-manifest-lobby.txt`).
- `USB\VID_0471&PID_2600` = "TOLTech xeyez **Bootloader**";
  `PID_2500&MI_04` = "TOLTech xeyez v2.5" (runtime, interface #4);
  `PID_2100` v2.1; `PID_0668` v2.0; `PID_0666` v1.0. Driver
  `xeyez.sys` 2.5.4.9 / INF DriverVer 2009-07-30, provider "TOLTech",
  installs into `System32\Drivers` (Win2k/XP/Vista/Win7, x86/ia64/amd64
  sections; also Win7 x64 `xeyezDriver64`).
- Why it matters: the physical device enumerates as 0471:2600 → it is in
  bootloader state; the Windows stack uploads firmware (hex/ldr listed in
  `xeyezHal.ini`) and the device then re-enumerates (likely as 2500 with
  MI_04). A Linux driver must replicate: (a) bootloader protocol on
  2600, then (b) runtime protocol. The ROM bootloader on Blackfin BF527
  has a known (Analog Devices) format — `ens_code.ldr` is a `.ldr` loader
  image — see finding 5.

### 4. Host software stack (from the NSIS payload, extracted by first attempt, hashes in `download-server-inventory.md`)
- `xeyez.sys` 28,872 B — Win32 kernel driver built from the MS **BulkUSB**
  sample; strings: "BF527 Bulk USB DriverEntry", `BulkUsb_DispatchRead/
  Write`, `BulkUsb_ResetPipe`, PDB path `d:\xeyez\objfre_wxp_x86\i386\xeyez.pdb`;
  x64 twin 35,968 B. Signed WoSign 2010 (cert CN=www.i-dong.com).
- `EasyUSB.dll` 51,568 B — user-mode wrapper, exports `ReadData`/`WriteData`.
- `StereoCamera.dll` 104,816 B — C++ class `StereoCameraBridge` with
  `Get3DObject`/`Get3dSingle`/`SetLens` … (the 3D-coordinate API).
- `BasicInterFace.dll` 17,264 B — `InitInterFace/GetAnMsg/GetProperty/
  SetProperty/Update`.
- `xEyeService.exe` 55,664 B (service; strings reference
  `xeyezPCUpdate.exe`, `xeyezBoxUpdate.exe`, the four PIDs, PDB
  `d:\BuildSystem\DalilyBuild\xeyez\…`) + `xeyez.exe` (tray, 92,528 B).
- Config INIs: `xEye.ini` (queues, `PipeBufferSize 9000`, `PipeTimeOut 10`,
  `MaxRead 1`, `xEyeFrame 50` fps, mode 1=PC/2=console),
  `xEyeUsb.ini` (handle matching, `USBComFPS 35`, `BitFile
  A_a_v4.4_BG0313.bit`, `DevicePlatform 2`), `xeyezHal.ini` (per-PID
  firmware: v2.0 `xeyez_v20.hex` "2022", v2.1 `xeyez_v21.hex` "2102",
  v2.5 `app_m8051_2500.hex` "2700", v2.6 `app_m8051_2611.hex`), plus
  `DriverConfig.ini` (hard-coded hardware IDs 0666/0668 for XP-era
  video/audio driver installs), `InterFaceList.txt` (UI plugins),
  `Calculator.txt` (mouse algorithm plugins `xEyeMouse`/`xEyeTS`),
  `mouse_contrl_config.ini` (Kalman/3D-cursor parameters).
- Firmware in same payload: `app_m8051_2500.hex` (61,031 B, ver "2614"),
  `app_m8051_2611.hex` (61,756 B, ver "2619"), `xeyez_v20.hex`/`v21.hex`,
  `ens_code.ldr` (54,424 B, Blackfin BF527, strings `ep0_handler`,
  `usb_init`), `A_a_v4.4_BG0313.bit` (235,930 B, Xilinx Spartan-3
  XC3S400 bitstream, `work_top.ncd`, 2010-04-06), CMOS/FPGA register
  blobs (`30fps_Global_0316_Cmos.txt`, `Global/Rolling_RW_0316_Cmos_v26.txt`,
  `Led_Scale_Fpga_v26.txt`).
- Why it matters: `EasyUSB.dll` + `StereoCamera.dll` + `xeyez.sys` are the
  three layers that define every byte crossing EP 0x02/0x82. All three are
  now in hand (inside the verified container) for disassembly.

### 5. Console firmware V1.2.40 (Ghost image) + Ghost tool verified
- http://download.i-dong.com/download/windows1.2.40/ce550_1.2.40.GHO
  (4,207,555,727 B, 2013-02-19) — "ce550" disk image for SD-card
  flashing of the Sports King console (Windows-based per Ghost usage;
  embedded-Linux "mini console" of 2011 may be a different SKU — inference).
- http://download.i-dong.com/download/Ghost11.5.zip downloaded to /tmp by
  this retry (not committed): 13,092,098 B, sha256
  `8753d6ec0fc9538732cb58d5db496c80c214fa6c9d8cd16f05792561e1724350`,
  contents = Ghost/Gdisk/Ghostexp executables + GBK manual
  「Ghost 使用详解」 (generic Symantec how-to, no vendor protocol info).
- Why it matters: the GHO would contain the console-side sensor service and
  updater (`xeyezBoxUpdate.exe` analog) — a second, independent
  implementation of the USB protocol; also the console OS image itself.

### 6. toltech.cn live JSP pages (CN+EN fetched 2026-11-23; artifacts saved)
- `http://www.toltech.cn/cn/ToolsPage.jsp` + `/en/ToolsPage.jsp?type=Tools`:
  AnySense SDK v1.0.21 (latest update 2013-01-28), API environments
  Java/C/C++/C#/Flash/Unity3D; hardware: 爱动感应器 (idong sensor),
  airtouch 手环 (wristband), 遥控笔或手柄 (remote pen or handle), USB
  cable; software: air-mouse gesture library, hand-gesture library,
  motion-game data library, A/V multimedia library (face+voice). CN page
  links "产品资料参考: 2.6代感应器 BOM / 电路图 / 结构3D图 / PCB" — all
  four link to `FeedBack.jsp` (request-only; contact anysense@toltech.cn).
- `http://www.toltech.cn/cn/TechnologyPage.jsp`: binocular-stereo position
  extraction + joint-point pose recognition; "爱动感应器、idong200模组、
  idong100模组" product line; "免驱" (driver-free host model); markers give
  2 mm accuracy at 5 m; AnySense kit: HD camera w/ compression, array mic
  with noise/echo cancellation, ≥30 % cheaper than MEMS.
- `http://www.toltech.cn/cn/Download.jsp`: stub — "敬请期待" (coming soon);
  `/en/Download.jsp` = "页面没有找到" (page not found).
- Why it matters: confirms SDK + hardware docs exist as requestable
  material; nothing downloadable on toltech.cn itself.

### 7. Product generations & host software naming (vendor news archive, fetched live 2026-11-23)
- News index: `http://www.toltech.cn/cn/NewsList.jsp` (CompanyNews pages
  1-9, ids 1078-1458) and `?newstype=MediaNews` (ids 965-974, 1364).
- Reuters/PRN 2011-05-17 (id 973, full text in artifacts): Toltech launched
  the "**third generation of i-dong**, the Chinese brand of home video game
  console"; sensor bar with **three cameras** + inertial wand; hardware at
  launch: sensor bar, main motion controller, supplementary controller,
  spherical IR-refracting markers (ping-pong paddle, racket, wrist bands);
  price RMB 1680; "the i-dong can be used for gaming with a PC or a
  set-top box, besides the dedicated i-dong box"; i-dong launched in
  mainland China 2010-05-20; CEO Xie Feng; CAS + Taishan Sports Group
  backing.
- 爱动·网络健身馆 (idong network fitness center; id 972, 2011): laptop as
  host + 三维识别器 (3D recognizer = sensor) + 运动手柄 (motion
  controller) + 运动帽 (cap) + 运动脚标 (foot markers), TV connected to
  PC, log in to 爱动在线 (i-dong Online).
- 运动王 (Sports King): flagship console; Donnie Yen brand ambassador
  2012-04-30 (id 1079), gifted 「爱动体感运动机"运动王"」; Shenzhen TV
  promo 2012-07 (id 1083) prizes "爱动运动王". Game titles: 家庭健身操
  (Family Aerobics), 冠军乒乓 (Champion Ping Pong), 弥雅瑜伽 (Miya Yoga),
  爱动网球 (i-dong Tennis), 阿拉法的奇妙世界 (Alafa), 拳击/搏击操 (boxing).
- PC software FAQ (idong.com, artifacts `idong-faq-40/41`): install halts
  if sensor not plugged in; after install two shortcuts: 「爱动在线运动」
  (i-dong Online Sports) and 「三维动作识别器」 (3D Motion Recognizer —
  "click this to use the handheld as a mouse") — i.e. the xEyeService/
  tray app is separately launchable.
- 爱动光感空鼠 (id 1364): "light-sensing air mouse" — reflective sticker
  on any remote upgrades it to an air mouse (the airtouch/AnySense tech).
- Why it matters: names the exact SKUs whose host software we found
  (Sports King PC = the sensor driver package), and gives the component
  vocabulary (识别器/三维识别器 = the sensor) used in all Chinese
  documentation.

### 8. Skyworth integration confirmed (vendor news)
- id 1084 (2012-07-25): i-dong × 创维 (Skyworth) "Champion Ping Pong"
  offline tournament across all i-dong stores; prizes include vouchers
  for both 爱动 and 创维云电视 (Skyworth cloud TVs).
- id 1087 (2012-11-26): at the 14th HIGH-FAIR the i-dong network fitness
  station appeared both in the Skyworth booth (hall 1) and CAS booth.
- No Skyworth-side firmware/game downloads found (skyworth.com reachable
  but no 2012-era content; news.skyworth.com unreachable) — follow-up
  from unsandboxed host / Wayback.

### 9. Consumer portal structure (www.i-dong.com = www.idong.com, live)
- Full site map in artifact `idong-com-site-map.md` (DedeCMS, 2016-era
  B2B line: smart gymnasium, fitness-test kiosks; service section holds
  the four download items; FAQ 40/41 live).
- Related domains probed: `u.idong.com` (login), `jf.idong.com` (points
  mall), `idong.tmall.com` (dead), `bbs.i-dong.com` → 301 to
  www.idong.com homepage (forum gone), `idong.cn` empty page,
  aidong.* parked. Official Weibo: e.weibo.com/aidong001.

## Artifacts

(All under `research/idong100/agents/02-products-software/artifacts/`)

- `download-server-inventory.md` — the 8 live URLs + per-file sha256 of all
  driver/firmware files extracted from the Sports King A-zip (first
  attempt; container hashes re-verified by this retry).
- `idong-com-site-map.md` — consumer portal map + download-section translation.
- `nsis-file-manifest-lobby.txt` — full 646-file manifest of xeyez_Lobby.dat.
- `xeyez.inf.txt` — full INF text (PID map, service config).
- `xeyez.sys-strings.txt`, `xEyeService.exe-strings-usb.txt`,
  `ens_code.ldr-strings.txt` — strings of driver/service/loader.
- `xEye.ini.txt`, `xEyeUsb.ini.txt`, `xeyezHal.ini.txt`, `DriverConfig.ini.txt`,
  `InterFaceList.txt.gbk.txt`, `Calculator.txt.gbk.txt`,
  `mouse_contrl_config.ini.txt` — host config files (GBK comments).
- `30fps_Global_0316_Cmos.txt`, `Global_RW_0316_Cmos_v26.txt`,
  `Rolling_RW_0316_Cmos_v26.txt`, `Led_Scale_Fpga_v26.txt` — sensor
  register-init blobs shipped with the host app.
- `toltech-cn-ToolsPage.jsp.{html,txt}`, `toltech-cn-TechnologyPage.jsp.{html,txt}`,
  `toltech-cn-Download.jsp.{html,txt}` — live vendor pages (retrieved 2026-11-23).
- `toltech-news-{971,972,973,1078,1079,1083,1084,1087,1364}.txt` — key
  vendor news articles (Reuters 2011 console, Skyworth, Sports King, etc.).
- `idong-faq-40-install-halts.txt`, `idong-faq-41-shortcuts.txt` — PC
  install FAQ.
- `idong-dl{43..46}-*.txt` — the four download item pages (text).

## Negative results

| Query / action | Outcome |
|---|---|
| Bing web search (any query, EN/CN) | canned unrelated results from sandbox proxy — unusable |
| DuckDuckGo html/lite, Mojeek | blocked (connection timeout) |
| Baidu `/s`, Sogou `/web` | 302 to anti-bot/captcha pages |
| web.archive.org CDX (i-dong.com, idong.com) | unreachable from sandbox (timeout) — Wayback mining NOT done |
| archive.org / timetravel / archive.ph | unreachable |
| manualslib.com | 403 on homepage (blocked) — no i-dong/爱动 manual search possible |
| toltech.cn `Download.jsp` (CN) | live but stub: "敬请期待" (nothing) |
| toltech.cn `/en/Download.jsp` | 404-style error page ("泰山在线" branded) |
| ToolsPage sensor docs (BOM/电路图/3D/PCB) links | all point to FeedBack.jsp — no direct download |
| download.i-dong.com directory listing | 403 (listing disabled) |
| download.i-dong.com `/AnySense.zip`, `/anysense/…`, `/setup/anysense.zip`, `/download/anysense.zip`, `/download/driver.zip`, `/setup/driver/` | all 404 |
| i-dong.com subdomains: dl, update, soft, help, ftp, game(s), shop, mall, news, service, support, api, open, user | no DNS/no service (000) |
| bbs.i-dong.com | 301 → www.idong.com homepage (forum dead) |
| idong.com service FAQ items 38, 39 (usage FAQ) | soft-404 ("页面走丢了") |
| idong.com product pages | only 2016-era B2B fitness kiosks; no consumer-console pages remain |
| news.skyworth.com | unreachable; no Skyworth-side i-dong downloads found |
| Full-file sha256 of the multi-GB zips/GHO | not computed (not downloaded; only HEAD + ranged reads) |

## Assessment for the Linux-driver goal

**The goal of finding software that reveals the USB protocol on EP
0x02/0x82 is now substantially met.** The complete official Windows host
stack for the sensor family is downloadable today from the vendor's own
servers:

1. `xeyez.sys` is a BulkUSB-sample kernel driver → the wire protocol is
   fully defined in user mode. `EasyUSB.dll` (ReadData/WriteData) and
   `StereoCamera.dll` (Get3DObject/…) are small (51 KB / 105 KB),
   unobfuscated MSVC binaries — disassembly will yield the exact
   command/response framing on the bulk endpoints.
2. The device under study (0471:2600) is in **bootloader** state; runtime
   is 0471:2500 (MI_04). The firmware upload flow is recoverable from
   `xEyeService.exe`/`xeyezPCUpdate.exe` + `xeyezHal.ini` + the shipped
   hex/ldr images (`app_m8051_2611.hex` for V2.6 hardware, BF527
   `ens_code.ldr`, Spartan-3 bitstream). This also finally explains the
   odd full-speed-style maxpacket 64 at high speed and missing string
   descriptors on the physical unit.
3. Practical path for the libusb driver: (a) enumerate as 2600, replicate
   the bootloader handshake to upload `app_m8051_2611.hex` + `ens_code.ldr`
   + `.bit`, (b) re-enumerate as 2500, (c) implement the ReadData/WriteData
   framing recovered from the DLLs. **Safety**: the CONTEXT.md rule
   "never write to EP 0x02 speculatively" remains in force until the
   bootloader protocol is confirmed from `xEyeService.exe`/`ens_code.ldr`
   analysis — the 2600 PID makes firmware-update commands on that
   endpoint a certainty, not a hypothesis.
4. Secondary sources: the 4.2-GB console Ghost image likely contains a
   second implementation (`xeyezBoxUpdate.exe` analog) plus the console
   OS; the request-only AnySense SDK + V2.6 hardware docs (BOM, circuit,
   PCB) would give the electrical/connector ground truth.

## Recommended follow-ups

1. **Download the two Sports King zips (A 1.5 GB + B 1.7 GB) on a machine
   with disk space, extract `dependent/xeyez_pc_v2.6.0.6/`, and reverse
   `EasyUSB.dll`, `StereoCamera.dll`, `BasicInterFace.dll` and
   `xEyeService.exe`** (Ghidra/radare2). Record the full EP 0x02/0x82
   protocol (framing, opcodes, frame layout vs. `PipeBufferSize 9000`,
   50 fps, `MaxRead 1`).
2. Analyze the bootloader path: `ens_code.ldr` (BF527 `.ldr` format),
   `app_m8051_*.hex`, and `xeyezPCUpdate.exe` — determine the exact
   firmware-upload sequence on PID 2600 before any write test.
3. Compare with the console image: fetch `ce550_1.2.40.GHO` (4.2 GB),
   mount/inspect for the box-side sensor service (independent protocol
   reference + the console OS build).
4. Email anysense@toltech.cn requesting the AnySense SDK v1.0.21 and the
   four V2.6-sensor reference documents (BOM, circuit diagram, 3D
   structure, PCB) — the ToolsPage links make clear these exist.
5. **Archive the download server** (Wayback "Save Page Now" for the 8
   URLs, plus the idong.com pages) — the server is 2012-era and could
   vanish; record sha256 of every full file when downloaded.
6. From an unrestricted host (not this sandbox): run the blocked search
   trails — Bing/Google/Baidu for 爱动 驱动 下载, 爱动 运动机 京东/淘宝
   archived listings, 爱动 客户端 百度网盘, "AnySense SDK" 下载, manualslib
   爱动/i-dong manuals, and Wayback CDX mining of i-dong.com, idong.com,
   download.i-dong.com, toltech.cn (2008-2015 snapshots, incl. old
   i-dong.com download pages and the Weibo aidong001).
7. Investigate "ce550" (Sports King console board/SoC name) and the 2011
   "mini console with embedded Linux" SKU as separate targets for console
   firmware images.
8. Cross-check the recovered firmware version codes (hex "2614"/"2619",
   hal "2700") against the physical device's EEPROM codes "2059"/"2613"
   to pin which firmware revision shipped in the owner's unit.
