# xtion-rs Rewrite Plan

This document is the working plan for rewriting the complete PrimeSense
PS1080 driver in Rust: the target file structure, the milestone plan, and
the full test matrix. It is kept in the repository as the source of truth
for what is done, what is next, and how everything is verified.

The port reference is the OpenNI 2.2.0 source (tarball sha256
`08f6842f20d1098ab2ebafadaac0130ffae5abd34cdf464bb6100cbe01ed95a8`),
specifically `Source/Drivers/PS1080/**` and the Linux USB layer of XnLib.
`docs/PROTOCOL.md` records every extracted protocol fact with its source
file.

## Milestones

| Milestone | Scope | Status |
|---|---|---|
| M0 - Protocol extraction | Read the C++ source, document framing, params, calibration | **done** (docs/PROTOCOL.md) |
| M1 - USB + command protocol | Open device, control transfers, version handshake, params | **done** |
| M2 - Calibration | Fixed params, ALGORITHM_DEPTH_INFO, S2D tables | **done, C-verified** |
| M3 - Depth stream | Bulk framing, packed-11 decode, shift-to-depth, VGA@30 | **code done, unit+C-verified; on-camera test pending** |
| M4 - Color stream | YUV422 bulk framing + RGB conversion | **code done, unit-verified; on-camera test pending** |
| M5 - On-camera validation | Run the integration tests + examples on real hardware | **in progress: first run found the split-reply transport quirk (fixed in 0.1.1, regression-tested); re-run pending** |
| M6 - Robustness | Timeouts, error recovery, keep-alive, multi-frame pipelining | pending |
| M7 - Completeness | Registration, MJPEG, misc/GMC packets, LED/emitter controls | pending |
| M8 - Ergonomics | Callback API, `std::error` polish, docs.rs, publish to crates.io | pending |

## File structure

```text
xtion-rs/
├── Cargo.toml              # crate manifest (rusb, thiserror, log)
├── LICENSE                 # Apache-2.0
├── NOTICE.md               # OpenNI 2 / PrimeSense attribution
├── README.md               # usage + status
├── docs/
│   ├── PLAN.md             # this file
│   ├── PROTOCOL.md         # extracted protocol reference (source-cited)
│   └── recovery/           # full project state for session recovery
├── reference/
│   ├── c_golden.c          # verbatim C transcription of the two algorithm
│   │                       # cores (XnShiftToDepthUpdate, Unpack11to16)
│   └── golden.txt          # generated golden vectors (build & compare!)
├── src/
│   ├── lib.rs              # crate root + re-exports
│   ├── error.rs            # XtionError, Nack, ProtocolError (thiserror)
│   ├── usb.rs              # rusb transport: discovery, endpoints, EP0
│   │                       #   vendor control transfers, bulk reads
│   ├── device.rs           # Xtion: handshake, calibration, params,
│   │                       #   stream start/stop, frame read loops
│   ├── stream.rs           # bulk-stream state machine: magic scan ->
│   │                       #   12-byte header -> payload -> frame assembly
│   ├── color.rs            # YUV422 (UYVY) -> RGB888, BT.601 integer path
│   ├── protocol/
│   │   ├── mod.rs          # constants (magic, opcodes, params, formats,
│   │   │                   #   resolutions, endpoints, packet types)
│   │   ├── command.rs      # request framing + reply validation + retries
│   │   ├── fixed_params.rs # GET_FIXED_PARAMS blob parser (V3.0+, 168 B)
│   │   └── algorithm.rs    # ALGORITHM_PARAMS (depth ConstShift)
│   └── depth/
│       ├── packed11.rs     # 4x11-bit -> 4x16-bit unpacker (+ inverse)
│       └── shift_to_depth.rs  # S2D/D2S LUT builder + converters
├── examples/
│   ├── list.rs             # enumerate, version, serial, calibration dump
│   └── depth_dump.rs       # stream depth frames, print statistics
└── tests/
    ├── golden.rs           # Rust output == C reference output (proven)
    └── hardware.rs         # on-camera integration tests (auto-skip)
```

## Test matrix

| Test | File / target | Needs hardware | Status |
|---|---|---|---|
| Request header framing (V25/V26, id counter) | `protocol::command` unit | no | pass |
| Reply validation (magic scan, id/opcode, NACK map, garbage skip) | `protocol::command` unit | no | pass |
| Split reply accumulation (first on-hardware bug: 12 of 34 data bytes in the first control read) | `protocol::command` unit | no | pass |
| Stale wrong-id reply discarded; NOT_READY re-send (same id) | `protocol::command` unit | no | pass |
| Version parse quirks: minor/major wire swap, hex build, 12-byte base struct | `device` unit | no | pass |
| Handshake with split version reply (exact hardware scenario) + v2.5 fallback | `device` unit | no | pass |
| Exchange over fake transport | `protocol::command` unit | no | pass |
| Fixed-params blob parsing at documented offsets | `protocol::fixed_params` unit | no | pass |
| Algorithm-param request layout | `protocol::algorithm` unit | no | pass |
| Packed-11: zeros/ones/known vectors/partial groups/VGA frame size | `depth::packed11` unit | no | pass |
| Packed-11: pack/unpack roundtrip | `depth::packed11` unit | no | pass |
| **Packed-11: 512 random groups == C reference** | `tests/golden.rs` | no | **pass** |
| S2D: table sizes, monotonicity, cutoffs, inverse consistency | `depth::shift_to_depth` unit | no | pass |
| **S2D: full 1025-entry tables == C reference, 2 configs** | `tests/golden.rs` | no | **pass** |
| Stream assembler: SOF/buffer/EOF, garbage, chunk splits, partial magic straddle, garbage-skip, padding quirk, orphan drop, misc counting | `stream` unit | no | pass |
| YUV: black/white/gray endpoints, UYVY order, sizes | `color` unit | no | pass |
| Device enumeration graceful without device/USB subsystem | `usb` unit | no | pass |
| Open + handshake + serial + calibration sanity | `tests/hardware.rs` | **yes** | auto-skip |
| Depth stream roundtrip (VGA@30, valid pixels) | `tests/hardware.rs` | **yes** | auto-skip |
| Doc example compiles | `cargo test --doc` | no | pass |

Run everything without hardware:

```bash
cargo test          # 53 tests, all green without a camera attached
cargo clippy --all-targets   # 0 warnings
cargo fmt --check
```

On a machine with the Xtion attached (USB 2.0 port!):

```bash
cargo test --test hardware -- --nocapture
cargo run --example list
cargo run --example depth_dump -- 30
```

## Verification strategy without hardware

1. **Golden vectors** (`tests/golden.rs`): the two numerical cores (S2D
   table construction, packed-11 unpacking) are transcribed verbatim into
   `reference/c_golden.c`, compiled, and their outputs for two calibration
   configs and 512 pseudo-random input groups are committed as
   `reference/golden.txt`. The Rust tests assert byte-identical output.
   This proves the ported math matches the C++ original exactly.
2. **Protocol framing tests**: request/reply bytes are constructed and
   validated against the formats extracted from the source, including the
   documented quirks (word-counted sizes, garbage before magic, NACK
   codes, the >= 5.1 SOF-timestamp-is-padding behavior).
3. **Stream assembler fuzz-ish tests**: byte streams are split at every
   alignment (including inside the magic) and must assemble to the same
   frames.
4. **Hardware tests auto-skip**, so CI stays green everywhere and runs the
   full on-camera suite only when a device is present.

## Known gaps (by design, tracked for M5-M8)

- On-camera validation of the full stack (the sandbox has no Xtion).
- Registration (color-to-depth alignment) - the REGISTRATION algorithm
  tables are documented in PROTOCOL.md but not yet requested.
- MJPEG input format for SXGA color (needs a JPEG decoder crate).
- `COMPRESSED_PS` depth (low-bandwidth ISO mode only; bulk uses the
  packed-11 format which is implemented).
- Keep-alive thread (the reference driver sends OPCODE_KEEP_ALIVE
  periodically; long sessions may need it).
- Device timestamp -> host clock conversion (`CreateTimestampFromDevice`
  needs the FREQUENCY algorithm params).
- Pre-3.0 firmware (bulk-endpoint control at 0x04/0x85) is explicitly
  rejected as unsupported.
