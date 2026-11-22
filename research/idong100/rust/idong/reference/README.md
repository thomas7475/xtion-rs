# Golden references (placeholder until milestone M3)

- `golden_boot.txt` — the expected boot transcript (TX/RX frames with
  types and timeouts), to be filled from the USBPcap capture of the
  vendor stack performing one firmware upload. The state machine in
  `src/protocol/boot.rs` is written against this transcript.
- `golden_frames.bin` — raw captured bulk chunks from a runtime session
  (OD frames: keep-alive 0xFFF, link status 4, marker records), replayed
  by `tests/frames_golden.rs`.

Until then, the tests use the disassembly-derived vectors embedded in
`src/protocol/frames.rs` and `tests/frames_golden.rs`.
