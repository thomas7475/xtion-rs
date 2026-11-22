# Agent 01 — Chinese-language web deep dive (idong100/idong200)

Branch: `research/idong100-01-chinese-web` · Date: 2026-11-23
Method: Google-backed web search (z-ai `web_search`, up to 10 results/query,
~25 query rounds) + direct HTTP fetches of every promising page. All quotes
from Chinese sources are given with English translations. "Retrieved
2026-11-23" applies to every URL listed below.

## Executive summary

- **BG0316 is identified**: it is a **CMOS image sensor designed by Brigates
  Microelectronics (锐芯微电子股份有限公司, Kunshan)** — VGA, 3.2 µm pixels,
  120 fps, "global shutter", 40 dB SNR, 65 dB DR — and **Brigates publicly
  stated in 2011 that Taishan/i-dong ordered interactive-gaming image-sensor
  chips from them in 2010**. The toltech "BG0316 模组" is a camera module
  built around this Brigates sensor die.
- **The vendor's own CN "Development Tools" page (ToolsPage.jsp) fetched
  fully** (it renders fine to plain GET, unlike ProductView.jsp): it
  documents **AnySense SDK v1.0.21 (last updated 2013-01-28)** — a
  software+hardware partner kit whose hardware is *exactly our device* and
  whose reference materials include **"2.6-generation sensor BOM, circuit
  diagram, 3D structural drawing, PCB"**. The SDK supports Java, C, C++, C#,
  Flash, Unity3D. A private multi-platform API therefore provably exists.
- **The vendor's Technology Introduction page** (also fetched) confirms the
  product family (爱动感应器 / idong100 模组 / idong200 模组), the binocular
  triangulation principle, **2 mm positioning accuracy at 5 m with passive
  markers**, and the vendor's claim that the tech is **免驱 ("driver-free")**
  on the host — important for the libusb design (no OS driver needed; the
  client app talks to the bulk endpoints directly).
- **Baidu Baike gives the sensor's engineering lineage**: 2008 recognizer =
  dual image sensors, DSP, **USB 1.1**, 10 points @ 45 fps, 3D-coordinate
  output; 2009-2011 recognizer = **three** image sensors (dual stereo + one
  0.3 MP color), **FPGA, USB 2.0**, 20+ points @ 60 fps, 3D coordinates +
  color image output. Our V2.6 device belongs to this FPGA/USB2.0 line.
- **The USB protocol itself is still not published anywhere** — no SDK copy,
  no teardown, no forum thread, no leaked driver found on the open
  Chinese-language web. But three concrete acquisition paths are now
  identified: (1) the vendor (contact **anysense@toltech.cn** — verified live
  on toltech.cn/cn/Contact.jsp); (2) Windows PC client software for the
  2nd-gen i-dong host / PC时尚版 (2011) — it must contain the host-side USB
  implementation (agent 02); (3) Wayback snapshots of ToolsPage.jsp /
  www.i-dong.com / brigates.com upload dirs (agent 05).
- **idong200 is a real, purchasable part**: broker sites list
  **IDONG200-AAC, manufacturer TOLTECH, package BGA144** (digi-electronics,
  ic-components.is, ariat-tech.ee, yic-electronics, allelco, bonchip,
  alibaba, y-ic). No equivalent IDONG100 part listing exists.
- Key people found: Taishan Online CEO **谢峰 (Xie Feng)**; CTO **师丹玮
  (Shi Danwei)**; SIAT engineer **吴迪 (Wu Di)** whose LinkedIn profile
  states he implemented a *"custom exposure mode of the CMOS image sensor
  BG0316"* at Shenzhen Institutes of Advanced Technology (CAS) from 2007 —
  the institute that co-founded i-dong.
- Corporate chain clarified: **Toltech = 深圳泰山体育科技有限公司 (formerly
  深圳泰山体育科技股份有限公司, NEEQ ticker 泰山体育 837739, listed
  2016-07-20)**, wholly owned by Shandong Taishan Sports Industry Group;
  the consumer product company is **深圳泰山科技在线科技有限公司 (Taishan
  Online)**; brand site **www.i-dong.com**; official Weibo e.weibo.com/aidong001.
- Ecosystem facts useful for other agents: 2nd-gen console ran **Windows**,
  2011 mini console ran **embedded Linux**, software was embedded in IPTV
  STBs (Hangzhou Telecom iTV, 2011); **Skyworth TVs bundled the 爱动感应器**
  (user Q&A evidence); Toltech licensed recognition algorithms to
  **GalaxyCore (格科微)**; CTO says AnySense "occupies only one USB
  interface".

## Key findings

### 1. BG0316 = Brigates (锐芯微) CMOS image sensor; i-dong was Brigates' customer in 2010
- Source: http://image-sensors-world.blogspot.com/2011/09/brigates-published-datasheet-of.html
  (post of 2011-09-19 by Vladimir Koifman; fetched in full, incl. 17
  comments). Artifact `artifacts/01-imagesensorsworld-2011-09-brigates-bg0316.txt`.
- What it says: "Kunshan, China-based company Brigates (Rui Xin in Chinese)
  published datasheet of what appears to be the world's smallest pixel
  sensor having global shutter, the **BG0316. The 3.2um pixel VGA sensor has
  120fps speed and offers 40dB SNR and 65dB DR**." The blog quotes Brigates'
  2011 company profile (auto-translated): "**Kunshan sharp core [Rui Xin /
  Brigates] received an order in 2010 from the nation's largest online
  sporting-goods supplier, i-Dong Taishan somatosensory, for man-machine
  interactive gaming chips**; the product has been deployed in the National
  Olympic Park and the Shanghai World Expo Park… covered by Reuters, CCTV…"
  Brigates had also developed "the world's first high-speed infrared
  three-dimensional image sensor chip" among 9 CIS chips by 2011.
- A comment (Feb 2012, "E.P.") quotes the datasheet: *"BG0316 also
  incorporates an experiment mode of 32 line global exposure mode"* and the
  VSIZE register note *"The window height size. In global shutter mode it
  must be 0x20"* → the global shutter works only for the first 32 lines.
  A comment gives the then-datasheet URL
  `http://www.brigates.com/en/upload/20118514485960458.pdf` (now 404;
  note: the blog author says that particular PDF is a *different* 6 µm-pixel
  sensor — the real BG0316 datasheet was at a similar path).
- Why it matters: identifies the exact sensor silicon inside the BG0316
  camera module of the idong100 chipset; gives datasheet-grade specs; and
  establishes that a **public BG0316 datasheet with a register map once
  existed** — recoverable via Wayback of `brigates.com/en/upload/*`
  (CDX query timed out from here; handed to agent 05). It does NOT describe
  the USB protocol (sensor ↔ idong100 link is internal), but it completes
  the hardware picture and opens a second vendor (Brigates,
  sales@brigates.com) who may share the legacy datasheet.

### 2. Brigates IPO prospectus corroborates BG0316 (product family + IC layout registration)
- Source: 锐芯微电子股份有限公司 首次公开发行股票并在科创板上市招股说明书（申报稿）
  (STAR-market IPO prospectus, draft), filed 2021-07-30, 357 pp.
  URL: http://notice.10jqka.com.cn/api/pdf/83546b381269e439.pdf
  (4,211,241 bytes, sha256
  `3a4782551a8e8b04ce20dbf75a9be7084c3396621edf085e12860c7073e5ff9e`;
  downloaded to /tmp only, NOT committed).
  Artifact `artifacts/02-brigates-prospectus-BG0316-excerpt.txt`.
- What it says: product-evolution table — **BG0311, BG0313, BG0316, BG0323,
  BG0328, BG0108** are the products developed 2008–2011, "mainly applied in
  **mobile phones and game consoles**" (主要应用于手机和游戏机). IC
  layout-design (集成电路布图设计) table: **BG0316 — registration
  BS.115000011, certificate #4128, application 2011-01-07, creation
  completed 2010-10.15, first commercial exploitation 2011-11-29**, rights
  holder 锐芯微有限. (No mention of 泰山/爱动/Toltech by name in the
  prospectus.)
- Why it matters: independent, legally-filed confirmation that BG0316 is a
  Brigates sensor commercialized in Nov 2011 — exactly the idong100 era —
  and that the BG03xx family was aimed at phones/consoles, matching the
  i-dong game-sensor use case.

### 3. toltech.cn CN "Development Tools" page — the AnySense SDK (fetched live)
- Source: http://www.toltech.cn/cn/ToolsPage.jsp (HTTP 200, full HTML).
  Artifact `artifacts/04-toltech-cn-ToolsPage-anysense-sdk.txt`.
- What it says (translation): "**Anysense SDK v1.0.21** (last updated
  2013-01-28). To let third-party developers build computer-vision games and
  applications more easily, Toltech has prepared a powerful development
  toolkit for interested partners, hoping to build an open platform… The
  AnySense SDK is a software+hardware toolkit… supports multiple development
  environments (**Java, C, C++, C#, Flash, Unity3D**)… Hardware: **爱动
  感应器 (idong sensor), airtouch wristband, remote pen or handle, USB
  cable**. Software: air-mouse action library, gesture-recognition action
  library, motion-game database, audio/video multimedia library. Product
  reference materials: **2.6-generation sensor BOM / circuit diagram / 3D
  structural drawing / PCB**. The motion-game database 'provides complete
  3D-spatial channel data… wraps a series of API interfaces… so that
  third-party developers are decoupled from the low level and can switch
  seamlessly across platforms.'"
- Why it matters: (a) proves a **multi-platform, C/C++-addressable SDK with
  USB-attached 爱动感应器 hardware existed and was distributed to partners**;
  (b) the "2.6代感应器" is *our exact device* (idong Sensor V2.6) — **BOM,
  schematic, 3D and PCB files exist in partner hands**; (c) the phrase
  "decoupled from the low level … switch seamlessly across platforms"
  corroborates the vendor's Windows/Linux statement from CONTEXT.md.

### 4. toltech.cn CN "Technology Introduction" page (fetched live)
- Source: http://www.toltech.cn/cn/TechnologyPage.jsp?type=TechnologyIntro.
  Artifact `artifacts/05-toltech-cn-TechnologyPage-intro.txt`.
- What it says (translation): principle = (A) position extraction by
  **binocular vision** (triangulation of two camera views); (B) pose
  recognition from joint points (head, hands, arms, shoulders, thigh, shin,
  feet), trajectory classification into actions → control commands.
  "Our HCI technology mainly includes the **爱动感应器 (idong sensor),
  idong200 module, idong100 module** products, and solutions such as
  AnySense omni-directional HCI, air mouse, gesture control, set-top box,
  3D smart game console, electronic whiteboard, smart toys." Key traits:
  "optical recognition, multi-point, **passive (无源)**, low power, low
  cost, **driver-free (免驱)**" and "**with markers, 2 mm positioning
  accuracy within a 5 m range**". AnySense kit adds an HD camera (with
  compression) and a microphone array (noise reduction, echo cancellation,
  far-field).
- Why it matters: first-party confirmation of the exact product naming
  (idong100/200 as *modules*), the passive-marker stereo architecture, the
  accuracy envelope, and the driver-free host model — i.e. the host talks to
  the sensor over bulk endpoints from user space, exactly the libusb case.

### 5. Baidu Baike 爱动体感运动机 — recognizer engineering history (USB, DSP/FPGA)
- Source: https://baike.baidu.com/item/爱动体感运动机/6717251 (fetched in
  full). Artifact `artifacts/03-baidu-baike-idong-motion-machine.txt`.
- What it says (translation of the 研发历程/R&D-history section):
  - 2008 "motion-type recognizer": **dual image-sensor system (dual-sensor
    stereo 3D tracking); 10 points simultaneously at 45 fps; DSP processing;
    standard USB 1.1 interface; multi-point 3D coordinate output**; air
    mouse; controller with vibration + heart-rate; low cost.
  - 2009–2011 "interactive recognizer": **three image-sensor system (dual
    stereo + single 0.3 MP color capture); 20+ points at 60 fps; FPGA
    processing; USB 2.0 interface; multi-point 3D coordinates + color image
    output**; also video comms, face recognition, contour recognition.
  - Core hardware = "**xeyez computer-vision recognizer**" (60 fps, 1 cm
    tracking resolution, 20+ patents). 2012: rated National Key New Product
    with "100+ patents and self-developed chips".
  - Console models: 大众版 uses **Trident PNX8473** STB SoC; 礼品装 is a
    Great-Wall Atom N450 laptop; recognizer dimensions 250×35×25 mm /
    131 g; recognizer generations "2.0代"/"2.5代" shipped with different
    bundles; "provides **standardized API interface and human-motion action
    model library** so third parties can develop motion games/applications".
- Why it matters: the 2009–2011 FPGA+USB2.0 recognizer **is the design
  generation of our idong Sensor V2.6**; the numbers (20+ points, 60 fps,
  USB 2.0 bulk, 3D coordinate output) bound the expected endpoint-0x82
  stream (≈20 points × (id+xyz) × 60 Hz ≈ a few kB/s, well within one
  64-byte-packet bulk stream — consistent with what we see). Also confirms
  the STB SoC lineage (Trident/NXP PNX) which resonates with the Philips
  USB VID 0471 (agent 06).

### 6. "xeyez" technology entries (Baidu Baike + Douyin Baike)
- Sources:
  - https://baike.baidu.com/item/xeyez三维光学识别与追踪技术/252359 (direct
    fetch blocked 403 "百度安全验证"; content from search snippets):
    "xeyez 3D optical recognition & tracking technology — for motion-sensing
    sports, developed by Taishan Sports Industry Group with CAS
    participation; captures target position/motion in 3D space in real time
    via optical tracking. The core device is the **xeyez 3D motion
    recognizer**, shaped like a laptop battery, fixed on top of the
    monitor/TV, adjustable viewing angle."
  - https://m.baike.com/wikiid/1560208261714173306 (fetched; artifact
    `artifacts/11-douyin-baike-xeyez.txt`): "…co-developed by **Taishan
    Online and CAS**; via the xeyez 3D motion recognizer uses **optical
    binocular stereo recognition and tracking**… achieves **passive
    (无源) multi-target fast tracking**, high interference immunity, low
    price, true 3D motion capture."
- Why it matters: "xeyez" (searched standalone before, negative) is now
  anchored to two encyclopedic entries — the technology brand of the
  recognizer family; gives the passive-marker, binocular-stereo description
  again from an independent source.

### 7. 2014 elecinfo interview with Taishan Online CTO 师丹玮 (Shi Danwei) — AnySense deployment model
- Source: https://www.elecinfo.com/index.php?do=tech_article_show&id=10714
  (2014-03-10; fetched in full). Repost:
  http://www.diybcq.com/forum.php?mod=viewthread&tid=125089.
  Artifact `artifacts/10-elecinfo-2014-03-10-anysense-interview.txt`.
- What it says (translation): AnySense = Taishan Online's long-range HCI
  tech, **mass-produced 2013**, ramping Q1 2014; "similar to what Microsoft
  uses in Kinect 2 but low cost". Deployment: TV/box maker adds **one
  AnySense chip in the host** plus a slightly modified remote; camera
  captures motion trajectories; **algorithms simulate gyroscope and
  accelerometer**; touch-screen-style operation can be simulated (≈90% of
  operations). "Why is our cost lower than Taiwanese dual-mic solutions?
  **Integration** — many functions in one module, system cost halved, and it
  **occupies only ONE USB interface**." Patents allow "recognizing a
  **button-sized target in complex environments**" (vs. Samsung TVs needing
  distinct background). "We provide algorithms to camera-sensor vendors such
  as **GalaxyCore (格科微)**." Talks with QVOD/LeTV/Xiaomi OTT boxes;
  cooperation with Coship (同洲); Taishan Online positions itself as a
  **solution provider, not a chip vendor — IP and chips serve the overall
  solution**.
- Why it matters: describes exactly how the sensor+chip go into third-party
  hosts (one USB port!), the algorithmic feature set (simulated IMU from
  vision), and the GalaxyCore relationship — relevant both to the USB
  protocol surface and to the idong201 supply question.

### 8. PR Newswire 2011 — corporate/product facts incl. Windows→Linux host switch
- Source: https://www.prnasia.com/story/46083-1.shtml (2011-05-24, issued by
  Shenzhen Taishan Online; fetched in full).
  Artifact `artifacts/09-prnasia-2011-05-24-idong-times-square.txt`.
- What it says (translation): CEO 谢峰 (Xie Feng, ex-CEO of China Mobile's
  Aspire/卓望, founder of Monternet). New **i-dong mini console** (~0.4 kg,
  <RMB 1000) **uses an embedded Linux OS**, while "the second-generation
  i-dong console previously used **Windows**". Next-gen would go
  **markerless**. 2011-05-17: i-dong online fitness hall launched on
  **Hangzhou Telecom's iTV IPTV platform** (software embedded in STBs;
  Taishan Online operates the iTV game channel). Backers: Taishan Sports
  Industry Group + CAS.
- Why it matters: confirms a **Windows host client stack existed (2nd-gen
  console / PC versions)** — that software contains the USB protocol
  implementation; the embedded-Linux mini host shows Linux-side code exists
  too. IPTV STB integrations are a second source of host-side binaries.

### 9. IDONG200-AAC — real orderable part number (BGA144)
- Sources (search snippets; direct fetches blocked by Cloudflare/403):
  - https://www.digi-electronics.com/en/products/detail/toltech/IDONG200AAC/16447081.html
    — "IDONG200-AAC TOLTECH | Datasheet PDF … Manufacturer: TOLTECH;
    Description: **TOLTECH BGA144**; Inventory: 3676 Pcs New Original".
  - https://www.ic-components.is/products/IDONG200-AAC.jsp
  - https://www.ariat-tech.ee/parts/IDONG200-AAC
  - https://lu.yic-electronics.com/pdf/IDONG200-AAC.html — "TOLTECH ·
    TOLTECH BGA144 · Lead free / RoHS Compliant · 3376 pcs Stock · Hong
    Kong"
  - https://iw.allelcoelec.com/productdetails/idong200-aac.html (15,960 pcs)
  - https://ne.bonchip.com/product/NA/IDONG200-AAC.html
  - https://portuguese.alibaba.com/product-detail/IDONG200-AAC-IC-Electronic-Components-Chip-1601813627275.html
  - https://www.y-ic.kr/pdf/IDONG200-AAC.html
- Why it matters: proves idong200 was put into open distribution as
  **IDONG200-AAC in a BGA144 package** — the first physical-package data
  point for any chip in the family, and a way to buy silicon (agent 04:
  "datasheet" links on these pages are typically fake, but worth one check).
  No IDONG100-x part number appears on any broker (searched).

### 10. Wu Di (吴迪) LinkedIn — SIAT engineer who customized BG0316 exposure
- Source: https://cn.linkedin.com/in/wuvic/zh-cn (profile "吴迪 - 算法
  算法工程师"; direct fetch blocked by LinkedIn bot wall; content from
  Google snippet, retrieved 2026-11-23): "Co-Founder & Engineering Director
  跨维智能 (DexForce) … **Custom exposure mode of the CMOS image sensor
  BG0316**. Software Engineer. **Shenzhen Institutes of Advanced Technology,
  Chinese Academy of Sciences. 2007年5月** …"
- Why it matters: ties the BG0316 sensor directly to SIAT — the CAS
  institute that co-founded i-dong — and reveals that i-dong's stereo
  pipeline needed a **custom exposure mode** implemented at SIAT (likely
  short/strobe exposure for IR passive-marker tracking). Corroborates the
  Brigates–i-dong supply chain from the engineering side.

### 11. toltech.cn recruitment (fetched live) — current tech stack
- Sources:
  - http://www.toltech.cn/cn/HumanList.jsp (artifact
    `artifacts/06-toltech-cn-HumanList-jobs.txt`): jobs posted 2016-10-24 —
    "**图像算法工程师（双目/机器视觉/深度图像）**" (Image Algorithm Engineer,
    binocular/machine-vision/depth-image, MSc, 2 openings, Shenzhen), APP
    product manager, Java engineer.
  - http://www.toltech.cn/cn/HumanDetail.jsp?id=1022 (artifact
    `artifacts/08-toltech-cn-HumanDetail-1022.txt`): duties = 3D modeling,
    recognition, HCI, algorithm design; C/C++; point-cloud post-processing,
    gesture/mocap experience preferred; office 南山科技园方大大厦; contact
    liuwm@toltech.cn.
- Why it matters: confirms Toltech still ran binocular/depth-image algorithm
  development in 2016+ (i.e. the idong line was alive), and gives a second
  vendor e-mail for SDK requests.

### 12. Corporate identity, NEEQ listing, contacts (verified on vendor + registry pages)
- Sources:
  - http://www.toltech.cn/cn/Contact.jsp (fetched live):
    **深圳泰山体育科技有限公司** (Shenzhen Taishan Sports Technology =
    Toltech), address 深圳市南山区粤海街道沙河西路深圳湾科技生态园2栋A702
    (Shenzhen Bay Science & Technology Ecological Park Bldg 2 A702,
    Nanshan, Shenzhen 518057); customer hotline 400-1818-520; **tech hotline
    +86-755-26993663**; tel +86-755-26993188; **Mail: anysense@toltech.cn**.
  - http://349293439.dayinmao.com (company profile; fetched): 2016-05-31
    NEEQ listing approval, ticker "泰山体育", code **837739**; "integrates
    face recognition, VR, motion-sensing sports tech, fitness testing,
    cloud fitness guidance"; online 爱动科学健身云服务平台 + offline 爱动
    科学健身馆; parent = Taishan Group (founded 1978, Shandong).
  - https://www.tianyancha.com/company/2352417123 (snippet): 深圳泰山体育
    科技有限公司, formerly 深圳泰山体育科技股份有限公司, founded 2008-01-16,
    legal rep 卞国峰, wholly-owned subsidiary of 山东省体育产业集团有限公司.
  - https://www.cnpp.cn/pinpai/44676.html (snippet): brand 爱动idong,
    **brand website http://www.i-dong.com**, email anysense@toltech.cn.
  - toltech.cn site header: official Weibo http://e.weibo.com/aidong001.
- Why it matters: gives the exact corporate tree (Toltech ↔ Taishan Online
  ↔ Taishan Sports Industry Group ↔ SIAT), the **verified contact channel
  for SDK requests**, and the consumer domain **www.i-dong.com** (never
  mined before — likely hosted downloads/drivers; agent 02/05).

### 13. Skyworth TV bundle + console-history context (ecosystem)
- Sources:
  - Baidu Zhidao Q&A (via search snippet): "买创维电视赠了一个爱动感应器，
    不知道为啥不能用了" — "I got an 爱动感应器 as a gift with a Skyworth TV;
    why doesn't it work?" → **Skyworth (创维) TVs bundled the idong sensor**.
  - http://www.iheima.com/article-157175.html (2016-07-05, fetched;
    artifact `artifacts/12-iheima-2016-07-05-chinese-consoles.txt`):
    "In 2010 Shenzhen Taishan Technology launched the 爱动体感运动机…
    recognition like a combination of Kinect and Wii, captures spatial
    position and trajectory… endorsed by CAS… failed commercially (price,
    games)."
  - Sogou WeChat search surfaced: "vSport携手深圳泰山体育科技共推区块链应用
    落地" (vSport blockchain partnership with Taishan, WeChat article; link
    redirects blocked by Sogou anti-spider) — minor, 2018-era.
- Why it matters: TV-vendor bundles (Skyworth) and STB integrations are the
  most likely places host-side software/driver discs survived; agent 02/07
  should hunt for Skyworth/创维 smart-TV accessory pages and iTV/同洲
  materials.

## Artifacts

All under `artifacts/` (text only; the 4.2 MB prospectus PDF was NOT
committed — URL + size + sha256 recorded in finding 2):

1. `01-imagesensorsworld-2011-09-brigates-bg0316.txt` — ISW blog post + the
   i-dong order quote + datasheet-comment quotes.
2. `02-brigates-prospectus-BG0316-excerpt.txt` — raw BG0316 contexts from
   the Brigates IPO prospectus (product table + layout-design table).
3. `03-baidu-baike-idong-motion-machine.txt` — full Baike entry text.
4. `04-toltech-cn-ToolsPage-anysense-sdk.txt` — vendor AnySense SDK page.
5. `05-toltech-cn-TechnologyPage-intro.txt` — vendor technology page.
6. `06-toltech-cn-HumanList-jobs.txt` — vendor jobs list.
7. `07-toltech-cn-Devolopment-Download-empty.txt` — Devolopment.jsp
   ("under construction") + Download.jsp ("coming soon") = no public
   downloads on the live site.
8. `08-toltech-cn-HumanDetail-1022.txt` — image-algorithm-engineer job.
9. `09-prnasia-2011-05-24-idong-times-square.txt` — press release.
10. `10-elecinfo-2014-03-10-anysense-interview.txt` — CTO interview.
11. `11-douyin-baike-xeyez.txt` — xeyez entry text.
12. `12-iheima-2016-07-05-chinese-consoles.txt` — console-history excerpt.

## Negative results

| Query / source | Outcome |
|---|---|
| `"idong100" 芯片` | Only toltech.cn itself + lnjindao.com spam mirror; no third-party info |
| `"idong100" 处理器` / `idong100 图像` | Same — vendor pages only |
| `"IDONG100-AAC"`, `"IDONG100" TOLTECH BGA`, `IDONG100 chip TOLTECH BGA` | No IDONG100 part listing on any broker (AAC = speech-therapy noise) |
| `"idong201"` | Only toltech.cn / lnjindao mirror; no broker, no datasheet |
| `"BG0316"` (quoted) | Only ISW blog + vendor page + unrelated (SCHOTT glass, hotel) |
| `BG0316 datasheet` / `"BG0316" 数据手册` | No live datasheet PDF anywhere (2011 URL now 404) |
| `BG0316 采购 图像传感器` / `泰山 摄像头 模组` | Generic CIS market noise |
| `爱动 SDK 二次开发`, `爱动 开发文档 感应器`, `爱动 感应器 SDK 下载` (prior) | Noise (AidLearning/Aidlux etc.); SDK not publicly downloadable |
| `"基于应用层界面设计" 爱动` (exact phrase from truncated vendor text) | No mirror carries the full sentence — full text still unknown |
| `"idong100芯片套件"` | Only toltech.cn ProductList.jsp |
| `爱动 感应器 拆解` | No teardown of the sensor on any CN site |
| `泰山体育科技 体感 摄像头` | Vendor + corporate profiles only |
| `泰山在线 爱动 PC时尚版` | Article not refound (Tarzan-movie noise); only Baike echo |
| `泰山在线 招聘 图像 算法` | Tool crash; `深圳泰山体育科技 招聘` found only vendor jobs page + generic job boards |
| `知乎 爱动 体感运动机`, `CSDN 爱动 体感`, `爱动 感应器 elecfans OR eeworld OR 21ic` | Zero i-dong content on Zhihu/CSDN/elecfans/EEWorld/21ic |
| `"爱动" 双目 红外 定位 传感器` | Noise (ADAS, patents by others) |
| `泰山 在线 双目 视觉 论文 中科院 先进` | No SIAT/Taishan paper surfaced via web search (agent 08: try CNKI directly) |
| `airtouch 手环 泰山` | Only the vendor ToolsPage itself |
| `吴迪 跨维智能 锐芯` | LinkedIn profile found but direct fetch blocked (bot wall); snippet used |
| Baidu web search (`baidu.com/s`) | HTTP 403 "百度安全验证" (CAPTCHA) for all queries |
| Bing (`bing.com` CN market) | Returns JS wall; no parseable results via curl |
| Sogou WeChat article redirect links | Blocked by Sogou anti-spider (antispider page) |
| `baike.baidu.com` second entry (xeyez) | 403 security verification (content recovered from snippets instead) |
| `en.brigates.com` | Connection failure (000) via curl; CN site used instead |
| Wayback CDX `brigates.com/en/upload/*` | Two attempts timed out (>120 s) — retry needed (agent 05) |
| `http://www.lnjindao.com` (spam mirror of toltech product list seen in snippets) | Live fetch returns domain-parking page; full mirrored text not recoverable |
| `digi-electronics.com` / `lu.yic-electronics.com` IDONG200-AAC pages | Cloudflare 403 / openresty 403 to curl — snippets recorded instead |
| `v.zhipin.com/detail/a195696591fe8b1a0nNy39q9` (Boss-Zhipin video, surfaced under Taishan query) | "No info for this page" — dead end |
| `349293439.dayinmao.com` "最新供应" (latest supply) section | Empty ("暂无" = none) — no parts sold via that storefront |

## Assessment for the Linux-driver goal

1. **No public protocol documentation exists on the Chinese-language web.**
   The AnySense SDK, Windows clients, IPTV STB builds and hardware design
   files all existed but were distributed privately to partners; nothing
   leaked to forums, code hosts, or file mirrors that Google indexes.
2. **The strongest single lead is the AnySense SDK v1.0.21 + "2.6代感应器
   BOM/电路图/PCB" package** documented on the vendor's own ToolsPage. Two
   concrete acquisition routes: (a) ask the vendor — **anysense@toltech.cn**
   / tech hotline +86-755-26993663 (verified live); (b) find partner copies
   (2013-2015 smart-TV/STB makers: Skyworth, Coship, Hangzhou Telecom iTV,
   QVOD/LeTV/Xiaomi talks) or Wayback captures of ToolsPage.jsp / i-dong.com
   download sections.
3. **Host-side Windows binaries are the best protocol oracle**: the
   2nd-gen console/PC clients (Windows, per 2011 press release) and the PC
   时尚版 (999 RMB, Sept 2011) installed from 光盘 (install discs) — agent
   02 should hunt installers/ISOs on Chinese download sites and auction
   listings (识别器+安装光盘 bundles are documented in Baike packaging
   lists).
4. **Engineering constraints now known** (from Baike + vendor tech page):
   USB 2.0 bulk, FPGA-based recognizer, ≥20 markers @ 60 Hz, 3D coordinates
   (+ optional color image in 3-sensor variant), passive IR-reflective
   markers, 2 mm @ 5 m, "driver-free" host model. A libusb implementation
   should expect a small binary frame stream on EP 0x82 (~20 points ×
   ~10-16 bytes × 60 Hz) and a command protocol on EP 0x02 — consistent
   with maxpacket 64.
5. **Sensor-side datasheet is recoverable**: Brigates published the BG0316
   datasheet in 2011 at `www.brigates.com/en/upload/<id>.pdf`; Wayback
   almost certainly holds it (CDX timed out from this sandbox). Brigates
  (sales@brigates.com, +86-512-55118685) may also share the legacy file.
   This gives the sensor register map (incl. the 32-line global-shutter
   mode and custom exposure mode built by SIAT) — useful context, though
   not the USB protocol.
6. **idong200 silicon is buyable** (IDONG200-AAC, BGA144, claimed in stock
   by several brokers) — a teardown/decap option if all else fails (low
   priority; without SDK docs a bare BGA helps little).
7. **The "免驱" claim + vendor-specific bulk class** strongly suggest the
   Windows client used a generic/user-space driver (WinUSB/libusb-style) —
   reinforcing that a Linux libusb driver is the intended integration path
   (the vendor's own statement mentions Linux support).

## Recommended follow-ups

1. **Agent 05 (archives)**: Wayback CDX for `toltech.cn/*` (especially
   `cn/ToolsPage.jsp`, `cn/Download.jsp`, `en/ToolsPage.jsp` — older
   snapshots may carry actual SDK download links), `i-dong.com/*` (consumer
   site — drivers/downloads), and `brigates.com/en/upload/*` (BG0316
   datasheet). Retry CDX with pagination/timeout budget.
2. **Agent 02 (products/software)**: hunt Windows PC clients for 爱动体感
   运动机 PC时尚版 / 精英版 / 礼品装 (Great-Wall laptop bundle), and
   "爱动运动平台" install discs; check Chinese software-download archives
   (pc6, xp510, 当下载, greenxf …) and auction sites for 识别器+光盘.
3. **Contact the vendor**: e-mail **anysense@toltech.cn** (and/or
   liuwm@toltech.cn) requesting AnySense SDK v1.0.21 and the Linux
   interface referenced in their platform statement.
4. **Agent 08 (patents/papers)**: search CNIPA/Google Patents for assignees
   泰山在线 / 深圳泰山体育科技 / 泰山体育产业集团 and inventors 师丹玮 /
   谢峰 / 卞国峰 — patents will enumerate the USB frame format ("multi-point
   3D coordinate output"); also check 中国版权保护中心 software-copyright
   registrations ("AnySense", "爱动") and SIAT (中科院深圳先进院) papers by
   吴迪 on the BG0316 custom exposure mode.
5. **Brigates outreach / archive**: request the 2011 BG0316 datasheet from
   sales@brigates.com; also mine Brigates' news page for the 2010 i-dong
   order PR.
6. **Agent 07 (ecosystem)**: Skyworth (创维) TV-bundle accessory pages,
   Coship (同洲) and Hangzhou Telecom iTV materials — any surviving
   partner-integration SDKs or firmware images.
7. **Agent 04**: verify whether any IDONG200-AAC "datasheet" link on broker
   sites is real (likely fake, but one check is cheap); note BGA144.
