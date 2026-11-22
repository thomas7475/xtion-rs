# idong100 / idong200 Research

Web research into the **Toltech idong100 / idong200** dual-camera tracking
processors — vendor **Toltech / Shenzhen Taishan Sports Technology**
(Chinese: 泰山体育科技), consumer brand **i-dong (爱动)**, Shenzhen, China.

> Storage note: this repository (xtion-rs) hosts an unrelated Rust driver
> project for ASUS Xtion cameras on `main`. The idong100 research lives only
> on the `research/idong100` branch family and only inside the
> `research/idong100/` tree. Do not mix the two.

## Goal

Locate any software, firmware, binary, or documentation for the
idong100 / idong200 processors, the BG0316 camera module, or the idong201
image sensor — or for any product that uses them — to support writing a
Linux user-space (libusb) driver for the **idong Sensor V2.6**
(USB `0471:2600`, vendor-specific bulk interface, EP `0x02` OUT / `0x82` IN,
onboard stereo processing, marker-coordinate output).

## Layout

| Path | Content |
|---|---|
| `CONTEXT.md` | Verified background: device, USB enumeration, EEPROM decode, prior search status, safety rules |
| `agents/NN-slug/FINDINGS.md` | One directory per research agent (one git branch each) |
| `agents/NN-slug/artifacts/` | Recovered text snapshots / excerpts (text files only) |
| `SUMMARY.md` | Consolidated cross-agent findings and recommended next steps |

## Branch map

| Branch | Agent | Topic |
|---|---|---|
| `research/idong100` | — | Parent branch (this tree + SUMMARY.md) |
| `research/idong100/01-chinese-web` | 01 | Chinese-language web deep dive (tech communities, job ads, supply mentions) |
| `research/idong100/02-products-software` | 02 | i-dong consumer products and their host software (PC clients, drivers, downloads) |
| `research/idong100/03-code-hosting` | 03 | Code hosting: GitHub, Gitee, GitCode, SourceForge, kernel/usb.ids/linux-hardware |
| `research/idong100/04-chip-docs` | 04 | Chip documentation & IC supply chain (datasheets, part listings, teardowns) |
| `research/idong100/05-web-archives` | 05 | Wayback / archive.today deep dive (CDX enumeration, recovered snapshots) |
| `research/idong100/06-usb-vid` | 06 | USB VID 0471 / PID 2600 angle (NXP reference-firmware theory, device databases) |
| `research/idong100/07-ecosystem` | 07 | Product ecosystem: games, TV/STB integrations, press, manuals, procurement |
| `research/idong100/08-patents-fcc` | 08 | Patents, academic papers, FCC certifications, trademarks, software copyrights |

## Status

Round 1 launched 2026-11-23 (8 parallel agents). Results: see `SUMMARY.md`.
