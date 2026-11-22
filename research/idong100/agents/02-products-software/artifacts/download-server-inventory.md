# Official Toltech / i-dong download server inventory (retrieved 2026-11-23)

All URLs verified LIVE on 2026-11-23 with HTTP HEAD (Apache, Accept-Ranges: bytes).
Host `download.i-dong.com` resolves to 39.100.15.105 (Alibaba Cloud). Directory
listing is disabled (403). Files are still served from the official brand site's
download page `http://www.idong.com/idong/a/service/ruanjianxiazai/`
("软件下载" = Software download).

## Entry page → file mapping

| Page | Title (CN) | Translation | Files |
|---|---|---|---|
| `/idong/a/service/ruanjianxiazai/2016/0620/43.html` | 运动王PC版安装包 | Sports King (Yundongwang) PC installer + **sensor driver** | A/B zips below |
| `.../44.html` | 运动王主机固件更新到V1.2.40 | Sports King **console firmware** V1.2.40 (Ghost image for SD card) | .GHO + Ghost tool |
| `.../45.html` | PC时尚版安装包V1.9.02下载 | PC "Fashion" edition installer V1.9.02 | A/B zips |
| `.../46.html` | PC大众版安装包1.0.6下载 | PC "Mass/Popular" edition installer 1.0.6 | A/B zips |

## Live files (HTTP 200 on 2026-11-23)

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

Full sha256 was NOT computed for the multi-GB zips (not downloaded in full).
The zips were enumerated remotely via HTTP Range reads of the ZIP central
directory (see FINDINGS.md). Individually extracted files are hashed below.

## Sensor driver/firmware payload extracted from
`idong_1.9.04_windows_yundongwang_A_setup.zip` → `dependent/xeyez_pc_v2.6.0.6/`
(xeyez_setup.exe + xeyez_Lobby.dat + xeyez_alone.dat; the two .dat files are
NSIS installers) and from `xeyez_Lobby.dat` (646 files, incl. the Windows
driver and all firmware):

sha256 (of extracted, post-NSIS, post-4-byte-size-prefix files):

```
28,872   f0d54e2a4f62ae8d7458c22c5026b08cb87ad3ae8a37a3bbff3eb4c5081a2e01  xeyez.sys (Win32 kernel driver, "TOLtech xeyez driver" 2.5.4.9)
 3,092   e9820bf2efe928c1c668332f257ef8e5accd74463fc5d249b69db79172bc4c87  xeyez.inf
73,728   c436cc1923f6cd0430efe6906ee4a7099373210535dd8fc47133cdb5671b8a8c  xeyezCoIns.dll (32-bit co-installer)
35,968   8fbc6714253588b17644fe50e8b37dbe8df78ecabcb6326f80f05d6f63b7c2b6  xeyez.sys (x64, Win7/xeyezDriver64)
80,200   a97651f06994b5268fdce194eb7e0bfebaacc33d1b2cd55bf8dc72f0f18933b2  xeyezCoIns.dll (x64)
61,756   1fad00c1dca7f995e8b58d4ed6c5c080da4005f63682ad7af6288abd4d118f35  app_m8051_2611.hex   (8051 fw, version string "2619")
61,031   ffb8b96274c3b4c4d6400b75917f5a270431a21af6952d67967c1867e3a003e4  app_m8051_2500.hex   (8051 fw, version string "2614")
25,913   3f82b8ccd5a80aef7a476d4de0cb7c03f580eb777521d97aa76f12feadfce61d  xeyez_v20.hex       (8051 fw, version string "2022")
26,304   0298a4d8a6501bcdef22965946f2d5dad11d211673f8915c804f65af9dea6854  xeyez_v21.hex       (8051 fw, version string "2102")
54,420   3a19c68a33352cda9b4c9e8a476c4978e19a5b983e567cff946fe543f51a60dd  ens_code.ldr        (Blackfin BF527 loader image; strings "ep0_handler", "usb_init")
235,930  75df0a38037e1983d270f1003c2303c25a61dcdb554d455b1f963cd4a70b4abd  A_a_v4.4_BG0313.bit (Xilinx Spartan-3 XC3S400 ft256 bitstream, "work_top.ncd", 2010-04-06)
51,568   3abd739a3cb441feaa8182510d772066fada9fb806d11194f5d14bed0709c192  EasyUSB.dll        (exports ReadData/WriteData)
104,816  ed38ce172b810b4191d2061a062716453d86acfae7d55507186bd7b7d500ae9e  StereoCamera.dll   (C++ StereoCameraBridge: Get3DObject/Get3dSingle/SetLens/...)
17,264   9f07f17a4acef834c89583db33941d1a87d0ef30b230f62915a3bd7adfc863ce  BasicInterFace.dll (InitInterFace/GetAnMsg/GetProperty/SetProperty/Update)
92,528   e800ca19ae8a379814001178479f3b48581c9cccbe6499d563c92e4660733889  xeyez.exe
55,664   74a7a9634bf213879f0484ffb6b5df49dd3468b5c05e280fd950a6636796617f  xEyeService.exe
```

Container files (as stored inside the remote zip, CRC-verified via ranged GET):

```
82,816   4252597e98bb4242402b89352b9938e6681c9f5dabe0e43d363c9f630f982711  xeyez_setup.exe  (NSIS 2.44 launcher)
2,132,624 4fe0127c71b2d64d88f1474b81928bc24e96136df8f4919ad51089814db087de  idong_setup.exe (main installer stub)
20,901,912 0b04734f23a61fe5f7b60a8e0205d4bb80bb909a6bed7773c320d809b893a00a  xeyez_Lobby.dat (NSIS installer, renamed)
23,655,864 6a877968deee70e9ec10cacb615703b5af84020677fed655272c84dffe1592be  xeyez_alone.dat (NSIS installer, renamed)
128      250fc22f23e6acc969a6234cb3231b318fba737e17b5edb9759dc3233c50e7c7  AUTORUN.INF (label: 爱动体感运动机（运动王pc版）)
```

## Related i-dong brand domains (probed 2026-11-23)

| Domain | IP | Status |
|---|---|---|
| www.i-dong.com / i-dong.com / idong.com / www.idong.com | 39.98.72.147 | **LIVE** — official consumer brand portal "爱动" (footer: Copyright TOLTECH INC., 粤ICP备11013256号) |
| www.tsproject.cn | — | LIVE — "About us" portal (泰山体育工程) |
| download.i-dong.com | 39.100.15.105 | LIVE — file server (this inventory) |
| u.idong.com | — | login portal (referenced) |
| jf.idong.com | — | points mall (referenced) |
| idong.tmall.com | — | Tmall flagship store (referenced, likely dead) |
| idong.cn | 47.77.197.181 | resolves; serves empty 80-byte page |
| aidong.com.cn | 43.160.235.235 | parked, "domain for sale" |
| aidong.com | 51.195.113.88 | parked, GoDaddy sale page |
| aidong.net | 128.14.163.252 | empty page |
| taishanonline.com | 76.223.54.146 | empty response (114 bytes) |
| i-dong.cn, idong.com.cn, aidong.cn, idong.net, i-dong.net, taishan-online.com, idong.com.tw | — | NXDOMAIN |
