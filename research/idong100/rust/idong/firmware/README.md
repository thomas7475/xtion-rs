# Firmware files (user-supplied — not redistributed)

The driver never bundles vendor firmware. Extract the three images from
the official xeyez PC packages (`v2.5.07.zip` / `v2.6.0.6.zip`, fetched
from `download.i-dong.com`, see branch `research/idong100-02`):

| Crate field | File | Role | Notes |
|---|---|---|---|
| `bf527_ldr` | `ens_code.ldr` | Blackfin BF527 loader image | uploaded through the bootloader (0471:2600); triggers re-enumeration to 0471:2500. v2.5.07 size 235,930 B |
| `fpga_bit` | `A_a_v4.4_BG0313.bit` | Xilinx Spartan-3 bitstream | image pipeline; "work_top.ncd" 2010-04-06; staged per `xeyezHal.ini` |
| `mcu_hex` | `app_m8051_2500.hex` / `app_m8051_2611.hex` | 8051 (MCS-51) sensor control | platform-dependent; `xeyezHal.ini` [Version2.5] `HexFileName=app_m8051.hex`, `LastVersion=2700` |

Legacy platforms (v2.0/v2.1, PIDs 0668/2100) use `xeyez_v20.hex` /
`xeyez_v21.hex` from the same runtime directory.

Before uploading, verify your extraction against the recorded sha256
inventory (branch `research/idong100-02`, `artifacts/`), then let
`upload_firmware` dry-run the packet math (count + CRC) against the files.

## Sensor-mode register tables (context, not uploaded by v1)

The runtime dir also carries CMOS/FPGA register programs selected by
`xeyezHal.ini`:

- `Rolling/Global_Blb_0316_Cmos` + `..._Fpga` — blob mode (on-chip
  centroids; the marker-tracking mode)
- `Rolling/Global_Img_0316_...` — image mode (raw 2-sensor difference
  frames for host-side CV)
- `30fps/` variants — global shutter at 30 fps
- `Led_Scale_Fpga_v26.txt` — IR LED scale registers 0x58–0x6B

These document how the device is switched between tracking and raw-image
modes (open item #7).
