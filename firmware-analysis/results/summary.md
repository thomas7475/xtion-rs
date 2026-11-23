# PS1080 firmware 5.7.9 vs 5.8.22 - comparison evidence (2026-11-23)

## 1. Main image headers (first 16 u16 words, little-endian)

| Image | Size | u16[0..7] | u16[8..15] |
|---|---:|---|---|
| fw579 | 134876 | 0006 0009 0507 0d63 eb71 e213 0000 095f | 0000 e213 0000 0000 0000 e213 0000 076d |
| fw5822 | 153704 | 0006 0022 0508 0b33 ef51 c9f3 0000 255f | 0000 c9f3 0000 0000 0000 c9f3 0000 0847 |
| boot579 | 23214 | 0003 0009 0507 01f4 2b5c 0000 0010 0020 | 0030 0040 0050 0060 0070 0081 0091 00a1 |
| boot5822 | 23214 | 0003 0022 0508 01f4 2b5c 0000 0010 0020 | 0030 0040 0050 0060 0070 0081 0091 00a1 |

BootManager identical across versions: **False**

## 2. Flash image inventory

| Image | 5.7.9 (B) | 5.8.22 (B) | Status |
|---|---:|---:|---|
| AlgorithmParams.xml.bin.fla | 736 | 780 | changed |
| BootManager.out.bin.fla | 23214 | 23214 | changed |
| DefaultParams-OvenMode.xml.bin.fla | 38 | 38 | stamp-only (body identical) |
| DefaultParams.xml.bin.fla | 22 | 22 | stamp-only (body identical) |
| DepthCmos.xml.bin.fla | 240 | 278 | changed |
| FixedParamsRD108.xml.bin.fla | 526 | 658 | changed |
| GainControl.xml.bin.fla | 120 | 120 | stamp-only (body identical) |
| IDParams.xml.bin.fla | 302 | 302 | stamp-only (body identical) |
| ImageCmos.xml.bin.fla | 1648 | 2278 | changed |
| PrimeProcessor.xml.bin.fla | 1116 | 1116 | stamp-only (body identical) |
| RegistrationParams.xml.bin.fla | 548 | 548 | stamp-only (body identical) |
| SensorNesaParams.xml.bin.fla | - | 78 | only-5.8.22 |
| SensorSafetyParams.xml.bin.fla | 58 | 58 | stamp-only (body identical) |
| SensorTecParams.xml.bin.fla | 14 | 14 | stamp-only (body identical) |
| UploadInProgress.fla | - | 7 | only-5.8.22 |
| source.out.bin.fla | 134876 | 153704 | changed |

## 3. Parameter image first-16-bytes peek

- AlgorithmParams.xml.bin.fla: 5.7.9=0c00090007051e0100000100c8003c00 5.8.22=0c00220008051e0100000100c8003c00
- FixedParamsRD108.xml.bin.fla: 5.7.9=07000900070500000000404b4c000100 5.8.22=07002200080500000000404b4c000100
- ImageCmos.xml.bin.fla: 5.7.9=0a0009000705000000010400f0000000 5.8.22=0a0022000805000000010400f0000000
- DepthCmos.xml.bin.fla: 5.7.9=0b00090007050000000102000d000100 5.8.22=0b00220008050000000102000d000100

## 4. Updater ini (flash map)

### 579: 13 file entries; kv={"InitFunction": "RepairFW5327ZoneB", "Min": "0x25000", "Max": "0x3C9C0", "NumFiles": "13"}
  - {"idx": 1, "Name": "..\\FLA\\IDParams.xml.bin.fla", "Offset": "0x25000", "OffsetFactory": "0x50000", "SensorSpecific": "1", "Function": "UpgradeSensorID1"}
  - {"idx": 2, "Name": "..\\FLA\\SensorTecParams.xml.bin.fla", "Offset": "0x25200", "OffsetFactory": "0x58000", "SensorSpecific": "1"}
  - {"idx": 3, "Name": "..\\FLA\\SensorSafetyParams.xml.bin.fla", "Offset": "0x25230", "OffsetFactory": "0x68000", "SensorSpecific": "1"}
  - {"idx": 4, "Name": "..\\FLA\\GainControl.xml.bin.fla", "Offset": "0x25260", "OffsetFactory": "0x550", "SensorSpecific": "0"}
  - {"idx": 5, "Name": "..\\FLA\\RegistrationParams.xml.bin.fla", "Offset": "0x25360", "OffsetFactory": "0x70000", "SensorSpecific": "1"}
  - {"idx": 6, "Name": "..\\FLA\\AlgorithmParams.xml.bin.fla", "Offset": "0x25660", "OffsetFactory": "0x950", "SensorSpecific": "0"}
  - {"idx": 7, "Name": "..\\FLA\\PrimeProcessor.xml.bin.fla", "Offset": "0x25A60", "OffsetFactory": "0x6000", "SensorSpecific": "0"}
  - {"idx": 8, "Name": "..\\FLA\\FixedParamsRD108.xml.bin.fla", "Offset": "0x26060", "OffsetFactory": "0x6700", "SensorSpecific": "0"}
  - {"idx": 9, "Name": "..\\FLA\\ImageCmos.xml.bin.fla", "Offset": "0x26460", "OffsetFactory": "0x6B00", "SensorSpecific": "0"}
  - {"idx": 10, "Name": "..\\FLA\\DepthCmos.xml.bin.fla", "Offset": "0x27660", "OffsetFactory": "0x7D00", "SensorSpecific": "0"}
  - {"idx": 11, "Name": "..\\FLA\\DefaultParams.xml.bin.fla", "Offset": "0xFFFFFFF", "OffsetFactory": "0x6600", "SensorSpecific": "2"}
  - {"idx": 12, "Name": "..\\FLA\\BootManager.out.bin.fla", "Offset": "0xFFFFFFF", "OffsetFactory": "0x2000", "SensorSpecific": "2"}
  - {"idx": 13, "Name": "..\\FLA\\source.out.bin.fla", "Offset": "0x27960", "OffsetFactory": "0x10000", "SensorSpecific": "0"}
### 5822: 14 file entries; kv={"InitFunction": "RepairFW5327ZoneB,RepairFW570114ZoneA,ReviveSensorFault,NESI", "FinishFunction": "RunBISTNESAUnlimited", "Min": "0x25000", "Max": "0x3F9C0", "SkipFastClear": "0", "Name": ".\\UploadInProgress.fla", "Offset": "0x3F9C0", "NumFiles": "14"}
  - {"idx": 1, "Name": ".\\IDParams.xml.bin.fla", "Offset": "0x28000", "OffsetFactory": "0x50000", "SensorSpecific": "1", "Function": "UpgradeSensorID1"}
  - {"idx": 2, "Name": ".\\SensorTecParams.xml.bin.fla", "Offset": "0x28200", "OffsetFactory": "0x58000", "SensorSpecific": "1"}
  - {"idx": 3, "Name": ".\\SensorNesaParams.xml.bin.fla", "Offset": "0x3F9C0", "OffsetFactory": "0x60000", "SensorSpecific": "1"}
  - {"idx": 4, "Name": ".\\SensorSafetyParams.xml.bin.fla", "Offset": "0x28230", "OffsetFactory": "0x68000", "SensorSpecific": "1"}
  - {"idx": 5, "Name": ".\\GainControl.xml.bin.fla", "Offset": "0x28260", "OffsetFactory": "0x550", "SensorSpecific": "0"}
  - {"idx": 6, "Name": ".\\RegistrationParams.xml.bin.fla", "Offset": "0x28360", "OffsetFactory": "0x70000", "SensorSpecific": "1"}
  - {"idx": 7, "Name": ".\\AlgorithmParams.xml.bin.fla", "Offset": "0x28660", "OffsetFactory": "0x950", "SensorSpecific": "0"}
  - {"idx": 8, "Name": ".\\PrimeProcessor.xml.bin.fla", "Offset": "0x28A60", "OffsetFactory": "0x6000", "SensorSpecific": "0"}
  - {"idx": 9, "Name": ".\\FixedParamsRD108.xml.bin.fla", "Offset": "0x29060", "OffsetFactory": "0x6700", "SensorSpecific": "0"}
  - {"idx": 10, "Name": ".\\ImageCmos.xml.bin.fla", "Offset": "0x29460", "OffsetFactory": "0x6B00", "SensorSpecific": "0"}
  - {"idx": 11, "Name": ".\\DepthCmos.xml.bin.fla", "Offset": "0x2A660", "OffsetFactory": "0x7D00", "SensorSpecific": "0"}
  - {"idx": 12, "Name": ".\\DefaultParams.xml.bin.fla", "Offset": "0xFFFFFFF", "OffsetFactory": "0x6600", "SensorSpecific": "2"}
  - {"idx": 13, "Name": ".\\BootManager.out.bin.fla", "Offset": "0xFFFFFFF", "OffsetFactory": "0x2000", "SensorSpecific": "2"}
  - {"idx": 14, "Name": ".\\source.out.bin.fla", "Offset": "0x2A960", "OffsetFactory": "0x10000", "SensorSpecific": "0"}

## 5. Constant markers

| Marker | 5.7.9 | 5.8.22 |
|---|---|---|
| MG_BR_pair | 0xd38 | 0xc14 |
| dead_u32 | 0x133e | 0x12e8 |
| dead_u16 | 0x133c (+1) | 0x12e6 (+1) |
| f31f_u16 | - | - |
| f31f_u32 | - | - |
| task_list_ascii | - | 0xd1e |
| packer_ascii | 0xd06 | 0xbe2 |

## 6. Strings

| Encoding | 5.7.9 count | 5.8.22 count | shared | only-5.7.9 | only-5.8.22 |
|---|---:|---:|---:|---:|---:|
| utf16 | 71 | 93 | 21 | 50 | 72 |
| ascii | 2625 | 2957 | 1557 | 1068 | 1400 |

Full lists: results/strings_*_only*.txt

## 7. Ghidra (ARM:LE:32:v5t, Thumb forced, AIF re-analysis)

### 5.7.9 - instructions=66589 functions=10 definedData=49 (meta={'VEC_SEEDS': '1'})
- call graph: 75 bl/blx instructions, 21 distinct in-range call targets (function starts)
- dispatcher: 0x4a8e-0x59de, 89 cmp-imm, 29 distinct opcodes: [0, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 30, 36, 38]
### 5.8.22 - instructions=76030 functions=16 definedData=6 (meta={'VEC_SEEDS': '1'})
- call graph: 85 bl/blx instructions, 31 distinct in-range call targets (function starts)
- dispatcher: 0x21410-0x2239c, 103 cmp-imm, 36 distinct opcodes: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 16, 17, 18, 20, 21, 22, 23, 24, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 41]

## 8. Opcode dispatch diff

- in 5.8.22 only: [1, 3, 28, 29, 31, 32, 33, 34, 35, 37, 39, 41]
- in 5.7.9 only: [11, 13, 15, 19, 25]
- shared count: 24

## 9. 5.8.22 package zip entries

FWUpdate_RD108x-5.8.22/, FWUpdate_RD108x-5.8.22/FWUpdate_RD108x-5.8.22.exe
