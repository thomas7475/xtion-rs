# xtion-rs

A from-scratch, pure-Rust userspace driver for the **ASUS Xtion /
PrimeSense PS1080** coded-light depth camera (USB `1d27:0601` /
`1d27:0400`). No OpenNI, no PS1080 C++ code, no system packages - the
crate talks the device's USB protocol directly.

This is the pure-Rust driver (Option D) from the ASUS Xtion Linux research
series; the alternative Rust bindings over the C OpenNI2 stack live in the
`rust-xtion-check` workspace of that series.

## Status

| Area | State |
|---|---|
| USB transport (rusb), endpoint probing | done, unit-tested |
| Command protocol (v2.5/v2.6, retry quirks, split-reply accumulation) | done, unit-tested incl. first-run hardware regression |
| Version handshake + identity (serial, versions) | done |
| Calibration (fixed params, depth info) | done |
| Shift-to-depth tables | done, **byte-identical to the C reference** (golden vectors) |
| Packed-11 depth decode | done, **byte-identical to the C reference** (golden vectors) |
| Bulk-stream frame assembly | done, unit-tested incl. chunk splits |
| Depth stream (VGA@30, packed-11 or raw 16-bit) | implemented; **on-camera validation pending** |
| Color stream (YUV422 -> RGB888) | implemented; on-camera validation pending |
| Registration, MJPEG, misc/GMC, keep-alive | not yet (see docs/PLAN.md) |
| Pre-3.0 firmware (bulk control endpoints) | not supported (explicit error) |

Everything that can be verified without the camera **is** verified: 59
tests pass on a bare CI machine (`cargo test`), and the two numerical
cores are proven equal to the original C++ implementation by golden
vectors generated from a verbatim C transcription
(`reference/c_golden.c` -> `reference/golden.txt` -> `tests/golden.rs`).
The remaining step is running `cargo test --test hardware` and the
examples on a real Xtion plugged into a **USB 2.0** port (the known USB
3.0 bandwidth quirk applies).

## Usage

```rust
use std::time::Duration;
use xtion::{DepthConfig, Xtion};

let mut cam = Xtion::open()?;                    // handshake + calibration
println!("{} serial {}", cam.version, cam.serial_number()?);

cam.start_depth(DepthConfig::default())?;        // VGA 640x480 @ 30 fps
let frame = cam.read_depth_frame(Duration::from_secs(2))?;
println!("min depth {} mm, {} valid pixels",
         frame.min_depth().unwrap_or(0), frame.valid_pixels());
```

Examples:

```bash
cargo run --example list          # enumerate + dump identity/calibration
cargo run --example depth_dump -- 30
```

Linux permissions: the camera is accessed via libusb; install the OpenNI2
udev rule (or run as root) to allow user access to `1d27:0601`.

## Documentation

- `docs/PLAN.md` - the rewrite plan: milestones, file structure, full test
  matrix, known gaps.
- `docs/PROTOCOL.md` - the extracted PS1080 USB protocol reference, every
  constant cited to its OpenNI 2.2.0 source file.
- `docs/recovery/` - snapshot of the research workspace for session
  recovery.

## Attribution

The USB protocol implemented here was derived from the OpenNI 2 PS1080
driver (Apache-2.0, PrimeSense Ltd.); see `NOTICE.md`. The decode cores
are verified byte-identical to that implementation via golden vectors.

## Changelog

- **0.1.3** - fixed the third on-hardware failure: after the EOVERFLOW
  fix the depth stream ran, but every assembled frame was a fragment
  (736..45424 of 307200 pixels, ~4% of the stream bytes surviving).
  Root cause: a synchronous read loop (one bulk URB at a time) leaves
  gaps between reads in which the device's FPGA FIFO drops data. The
  C driver avoids this with a ring of 16 pre-submitted async URBs that
  are re-submitted the moment they complete; this release ports that
  design (`src/ring.rs`, via `libusb1-sys` since rusb has no async
  API). The examples now also print USB bus speed, endpoint max packet
  size, ring throughput stats and expected-vs-got pixel counts, so a
  mis-negotiated bus speed shows up immediately.

- **0.1.2** - fixed the second on-hardware failure: every depth bulk read
  returned `USB error: Overflow` because the read buffer was resized to
  the whole frame's wire size (422,411 bytes - not a multiple of the
  512-byte max packet size; the Linux USB stack completes such bulk-IN
  URBs with EOVERFLOW). Reads now always use 40 x maxpacket chunks like
  the C reference, read timeouts are retried until the caller's deadline
  instead of failing, parallel hardware tests are serialized (the
  `Usb(Busy)` collision), and a busy device now reports a dedicated
  `DeviceBusy` error with guidance.

- **0.1.1** - fixed the first on-hardware failure: replies can arrive split
  across multiple vendor control-IN transfers (the first run of
  `tests/hardware.rs` got "version reply too short: 12 bytes"); the
  exchange now accumulates follow-up reads like the C reference. Also
  ported stale-reply re-receive, NACK_NOT_READY re-send, the 1 ms/10 ms
  command delays, 512-byte control reads, and the XnVersions wire quirks
  (minor/major byte order, 12-byte base struct, word padding).
- **0.1.0** - initial port: USB transport, command protocol, calibration,
  packed-11 + shift-to-depth decode (C-verified), bulk-stream framing,
  depth/color streaming API.
