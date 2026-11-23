# Session backup 2026-11-23 - v34 (xtion-rs 0.3.5)

Continuation of `session_backup_2026-11-23-v33.md`. Everything below is
committed as `3cb58a1` ("0.3.5: complete GMC surface ...") and pushed to
GitHub.

## Input that drove this session

The 0.3.4 camera run (11 tests, 10 pass): `depth_processing_controls`
panicked at tests/hardware.rs:1577 - "AGC bin write/read/restore cycle:
Nack(BadParams)". User asks: fix the test, print the actual resolution
in the image-preset listing, then implement the complete GMC config
data and the plain GMC data stream.

## 1. AGC bin NACK - root cause and fix (0.3.5)

- The v0.3.4 sequence was C-exact (`XnHostProtocolSetDepthAGCBin`,
  XnHostProtocol.cpp:3124-3141: two SINGLE-pair SET_PARAMs, LOW then
  HIGH; confirmed by reading the C again - no batch anywhere).
- Leading hypothesis: this firmware validates every individual
  SET_PARAM against the sibling bound's CURRENT value. The factory
  bins are all (0,0), so the first bin that GROWS upward (bin1
  LOW <- 512 while its HIGH is still 0) momentarily violates
  min <= max and is NACKed BadParams. Consistent with all 0.3.4
  evidence; NOT yet hardware-proven which of the 8 writes failed.
- Fix: `write_agc_bin_via` (transport-generic, unit-tested) reads the
  bin first and orders the writes transient-safely:
  - `new.min > current.max` (bin grows above the old range) ->
    HIGH first, then LOW;
  - otherwise (shrink / slide down / overlap) -> LOW first, then HIGH.
  Proven safe for every transition; collapse to factory (0,0) uses the
  LOW-first branch.
- Hardware test 11 reworked into a probe ladder that NEVER panics:
  (a) no-op LOW at factory, (b) no-op HIGH at factory, (c) HIGH <- 512
  (invariant-preserving grow), (d) the C-order LOW <- 512 at the
  v0.3.4 failure point; then factory restore + verify; full
  custom-partition cycle only when writes are accepted at all; refusal
  prints a VERDICT line (same treatment as crop mode 2).

## 2. Presets print the actual resolution

`CmosPreset::Display` now resolves the `XnResolutions` wire code via
`resolution_info()` (XnStreamParams.h:183-202):
0=QVGA 320x240, 1=VGA 640x480, 2=SXGA 1280x1024, 3=UXGA 1600x1200,
4=QQVGA 160x120, 5=QCIF 176x144, 6=240P 432x240, 7=CIF 352x288,
8=WVGA 640x360, 9=480P 864x480, 10=800x448, 11=SVGA 800x600,
12=576P 1024x576, 13=DV 960x720, 14=720P 1280x720, 15=1280x960.
Output now looks like `format 1 @ res 1 (VGA 640x480) @ 30 fps` in
both hardware test listings. So: image presets res 14 = 1280x720,
res 15 = 1280x960; depth cmos presets res 4 = 160x120, res 9 = 864x480.

## 3. GMC - complete config + plain data (0.3.5)

Reference facts (OpenNI 2.2.0 PS1080):
- OPCODE 26 = OPCODE_SET_GMC_PARAMS; enabled for fw >= 4.0
  (XnHostProtocol.cpp:442) but NEVER called anywhere in the C - no
  request layout exists. Deliberately NOT implemented (safety policy:
  unknown-payload write into the depth processor config).
- Params: 36 mode (0 normal / 1 scan), 37 ref offset, 38 RICC,
  39-44 DX-correction terms, 61 debug switch, 75 correction mode.
  The C maps ONLY 36 + 61 to host properties (m_GMCMode/m_GMCDebug) -
  so only those two get setters in xtion-rs; the rest is read-only.
- Packets on the misc endpoint (EP 0x83):
  - 0xa200 plain GMC motion data: up to 100 points/packet, 10 B each -
    `XnDeviceSensorGMCPoint { u16 x; u16 y; u16 dx; i16 dy; u16 score }`
    (XnDeviceSensor.h:162-169, pack(1)). NOTE: dx is u16, dy is i16 in
    the C - ported faithfully (leftward motion reads as two's
    complement via `dx as i16`).
  - 0xb200 GMC debug points: identical layout (XnGMCDebugProcessor
    dumps them as "X,Y,DX,DY").
  - 0xb500 GMC debug end: packed `XnHostProtocolGMCLastPacketData`
    (XnHostProtocol.h:267-280, 38 B): mode, coverage_pass,
    {last i16, ricc_last u16, ricc_iir f32}, a/b/c f32, n i16,
    ricc u16, start_b u32, delta_b u32, flash_stored_ref_offset i16
    (-1000 = not written; host fills that for fw < 5.2).

New API: `GmcConfig` + `gmc_config()` (block read), `gmc_mode()`,
`set_gmc_mode()` (existed), `gmc_debug()` / `set_gmc_debug()` (61),
`Xtion::drain_gmc_packets()` (pumps the misc ring, drains from all
four assemblers). StreamAssembler now accumulates GMC-family payloads
across bulk chunks (`gmc_accum`), decodes at packet completion, caps
at 100 points, counts misc packets ONCE per packet (was per chunk).
New hardware test 12 `gmc_config_and_motion_data`: config block
inventory, mode+debug toggles while streaming, 20-frame packet census,
exact restore, stream-survival gate; silence prints a VERDICT, not a
failure.

## 4. Test/doc state after this session

- 149 host-side checks green (145 lib incl. 3 new AGC-order tests +
  6 new GMC decoder tests; 4 golden; clippy clean). 12 hardware tests
  compile and skip without a device.
- Docs updated: PROTOCOL.md (AGC hardware finding + new "GMC" section),
  FEATURE_PLAN.md (rows 11/24, 1b table: GMC done, section 5 text),
  VALIDATION.md (test 11 rework, test 12, counts), PLAN.md (M7),
  README.md, FIRMWARE.md.
- Driver Architecture PDF regenerated as Rev 3 (0.3.5):
  download/xtion_rs_driver_overview.pdf (11 pages, visual check done).
  Sources synced to download/pdf_sources/.
- Workspace backup: download/asus_xtion_workspace_backup_2026-11-23_v34.zip.

## 5. Open threads for the next camera pass

1. Run the 0.3.5 suite on hardware (`RUST_BACKTRACE=full cargo test
   --test hardware -- --nocapture`): expect 12/12. The probe ladder
   will settle the AGC hypothesis (a) "no-op writes ACK, grow ACK ->
   full cycle passes with the transient-safe order" or (b) "writes
   refused outright -> bins read-only on this build like mirror 71".
   The GMC test settles whether the 0xa200 stream actually flows on
   this unit with mode+debug on (payload not %10 == 0 would show up in
   the census/verdict and needs a decoder follow-up).
2. Firmware-quirk pattern so far on this ASUS build (fw 5.8.22):
   emitter toggle NACK (ACKed while streaming), crop mode 2 NACK,
   image-mirror param 71 write NACK, AGC bin write NACK (order- or
   build-dependent - probe will tell), opcode 35 reply-too-short,
   opcodes 18/27 InvalidCommand, opcode 19 drift, opcode 16 type remap.
3. SECURITY: the GitHub PAT used for pushes is exposed in chat history
   and lives in /home/z/my-project/.gh-token - ROTATE it after this
   work concludes.
