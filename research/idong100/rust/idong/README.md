# idong — Rust driver for the Toltech idong Sensor V2.6

Pure-Rust, user-space USB driver for the **idong Sensor V2.6** stereo
infrared tracking bar (Toltech / Taishan Sports, consumer brand *i-dong /
爱动*) — status: **skeleton (milestone M1)**.

- Device: USB `0471:2600` (bootloader) → `0471:2500` (runtime composite;
  vendor bulk interface MI_04, EP `0x02`/`0x82`).
- Data product: computed **3D marker coordinates** at ~50 fps (up to 10
  slots observed; chip documents 15), plus RF handle events, factory
  stereo calibration (T24C16A EEPROM), and standard UVC/UAC video/audio
  via the composite interfaces.
- Full design + protocol documentation:
  [`../../docs/RUST_DRIVER_DRAFT.md`](../../docs/RUST_DRIVER_DRAFT.md)
  (branch `research/idong100-10-rust-driver-draft`).

## Status

| Milestone | State |
|---|---|
| M0 protocol consolidation | done (DRAFT Part I) |
| M1 skeleton + read-only examples | **this branch** |
| M2 codec/EEPROM tests | code present; `cargo test` needs a toolchain with the deps vendored |
| M3 USBPcap byte-confirmation | pending (user-side capture) |
| M4 real firmware upload | gated on M3 (`protocol::boot::BOOT_CONFIRMED`) |
| M5–M8 runtime streaming → robustness → ergonomics | pending |

## Safety

No code path writes to the device except the vendor-exact boot upload
(state machine refuses until M3 confirmation) and the documented runtime
replies (0xFFE keep-alive, type-4 word 0x11, type-7 DSP reset).
`probe_bootloader` and `list` are strictly read-only.

udev rule for unprivileged access:

```
SUBSYSTEM=="usb", ATTRS{idVendor}=="0471", ATTRS{idProduct}=="2600", MODE="0660", TAG+="uaccess"
SUBSYSTEM=="usb", ATTRS{idVendor}=="0471", ATTRS{idProduct}=="2500", MODE="0660", TAG+="uaccess"
```

## Examples

```sh
cargo run --example list                    # enumerate + state
cargo run --example probe_bootloader -- 20  # read-only EP 0x82 probe
cargo run --example upload_firmware -- --fw-ldr ens_code.ldr \
    --fw-bit A_a_v4.4_BG0313.bit --fw-hex app_m8051_2500.hex   # dry run
cargo run --example dump_frames -- --seconds 30               # JSONL dump
```

## Firmware

Not redistributed. Supply the vendor images from the official packages
(`v2.5.07.zip` / `v2.6.0.6.zip`, see `firmware/README.md`).
