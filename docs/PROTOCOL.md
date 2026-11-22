# PS1080 USB Protocol Reference

Every fact below was extracted by reading the OpenNI 2.2.0 source (Apache
2.0, PrimeSense Ltd. 2012; tarball sha256 `08f6842f20d1098ab2ebafadaac013
0ffae5abd34cdf464bb6100cbe01ed95a8`), file paths cited. This is the
complete reference the Rust port is built from. The ASUS Xtion (1d27:0601)
runs firmware 5.3.x, which uses the "new USB" layout and the v2.6 command
protocol highlighted below.

## 1. USB transport

| Item | Value | Source |
|---|---|---|
| VID | `0x1d27` | XnDeviceEnumeration.cpp |
| PIDs | `0x0601` (Xtion), `0x0400` (older) | XnDeviceEnumeration.cpp |
| Bulk-in depth | `0x81` (fw >= 3.0/4.0), else `0x82` | XnDeviceSensorIO.cpp |
| Bulk-in image | `0x82` (new), else `0x83` | XnDeviceSensorIO.cpp |
| Bulk-in misc | `0x83` (new), else `0x86` | XnDeviceSensorIO.cpp |
| Control (new fw) | EP0 vendor transfers: OUT `bmRequestType 0x40`, IN `0xC0`, `bRequest 0`, `wValue 0`, `wIndex 0`, `wLength = payload` | XnHostProtocolUSBSend/USBReceive + XnLinuxUSB.cpp |
| Control (fw < 3.0) | bulk OUT `0x04` / bulk IN `0x85` (NOT supported by xtion-rs) | XnDeviceSensorIO.cpp |
| Interface | claim 0; altsetting stays 0 for bulk | XnDeviceSensorIO.cpp |
| Bulk read chunk | `40 * max_packet_size` on Linux (ISO: 32) | XnDeviceSensorInit.h |

Endpoint auto-detection: open 0x81 as bulk-in; if absent, fall back to the
old layout (`XnSensorIO::OpenDataEndPoints`). Per-endpoint isochronous
fallbacks exist for ISO altsettings (not needed on the Xtion, which uses
bulk).

## 2. Command protocol (PsMonitor)

Header layouts (`XnHostProtocol.h`, packed, little-endian):

```text
v2.5 (fw < 1.2):  u16 magic "PS" 0x5053 | u16 size_words | u16 opcode | u16 id | u16 crc16(=0)
v2.6 (fw >= 1.2): u16 magic "MG" 0x4d47 | u16 size_words | u16 opcode | u16 id
```

- `size_words` counts the **payload** in 16-bit words for requests
  (`XnHostProtocolInitHeader`: `nSize / sizeof(XnUInt16)` where nSize is
  the payload size).
- The CRC16 field of v2.5 is always sent as 0 and never verified
  (ValidateReplyV25 contains `// CRC? // ...`).
- `id` increments per request; replies must match id and opcode.
- Replies begin with the firmware magic ("SP" 0x5350 / "BR" 0x4252),
  possibly preceded by garbage - the receiver scans for the magic
  byte-wise (ValidateReplyV26/V25).
- After the reply header: `u16 error` (0 = ACK; otherwise a NACK code 1-16,
  see `XnHostProtocolNacks`), then `size_words - 1` words of data.
- Send retries: 5 attempts on USB timeout/stall, 100 ms apart
  (`XN_USB_HOST_PROTOCOL_SEND_RETRIES`). Receive loops until an overall
  deadline (nTimeOut ms).
- Version handshake quirk: the very first GetVersion after power-up may
  time out - wait 2 s and retry once; then try firmware 5.0 parameters;
  re-configure the link for the real version
  (`XnDeviceSensorConfigureVersion`).
- Firmware >= 5.0 build number quirk: `sprintf("%x", build)` then `atoi()`
  - the build is a hex number re-read as decimal (XnHostProtocolGetVersion).

### Reply transport behavior (verified on hardware)

These behaviors of `XnHostProtocolExecute`/`ReceiveReply` were confirmed by
the first on-camera run (firmware 5.3, Xtion Pro Live):

- **Replies arrive split across several vendor control-IN transfers.**
  The first transfer carries the reply header + the first data chunk;
  Execute's "Get rest of data" loop keeps issuing control reads
  (512 bytes each for fw >= 5.0, `XN_PROTOCOL_MAX_PACKET_SIZE_V5_0`) and
  appending until the promised `size_words - 1` words of data are
  collected. Observed live: GetVersion delivered 22 bytes first (header +
  ACK + 12 of 34 data bytes), the rest in a second transfer.
- A reply whose **id does not match** the request (stale answer to an
  earlier command) is discarded and the receive is repeated.
- A reply buffer with **no firmware magic** is re-received every 10 ms
  until the receive budget (20 s `XN_RECEIVE_USB_DATA_TIMEOUT`) expires.
- A **NACK_NOT_READY** reply causes Execute to re-send the SAME request
  (same id), up to `XN_HOST_PROTOCOL_NOT_READY_RETRIES` = 3 times, 1 s
  apart.
- Timing delays per command: 1 ms before sending
  (`nUSBDelayExecutePreSend`), 10 ms between send and read
  (`nUSBDelayExecutePostSend`); firmware >= 5.3.16 with a Jango USB core
  reduces these to 0.
- The device **pads odd data lengths up to whole 16-bit words** (e.g. the
  33-byte XnVersions arrives as 17 words = 34 bytes).

Opcodes (`EPsProtocolOpCodes`): GET_VERSION=0, KEEP_ALIVE=1, GET_PARAM=2,
SET_PARAM=3, GET_FIXED_PARAMS=4, GET_MODE=5, SET_MODE=6, GET_LOG=7,
ALGORITHM_PARAMS=22, GET_SERIAL_NUMBER=37, GET_PLATFORM_STRING=39,
GET_USB_CORE_TYPE=40, SET_LED_STATE=41, ENABLE_EMITTER=42.

## 3. Parameters (GET_PARAM / SET_PARAM)

Payload: request `{ u16 param, u16 value }` (SET) or `{ u16 param }` (GET);
reply data: `u16 value`. Param ids from `Sensor/XnParams.h`:

| Param | Id | Values |
|---|---|---|
| REGISTRATION_ENABLE | 2 | 0/1 |
| STREAM0_MODE (color) | 5 | 0=off, 1=color (`XnVideoStreamMode`) |
| STREAM1_MODE (depth) | 6 | 0=off, 2=depth |
| STREAM2_MODE (audio) | 7 | 0=off, 1=on |
| IMAGE_FORMAT | 12 | see image formats |
| IMAGE_RESOLUTION | 13 | resolution id (QVGA=0, VGA=1, SXGA=2, QQVGA=4, ... `XnResolutions`) |
| IMAGE_FPS | 14 | fps |
| DEPTH_FORMAT | 18 | see depth formats |
| DEPTH_RESOLUTION | 19 | resolution id |
| DEPTH_FPS | 20 | fps |
| DEPTH_MIRROR | 23 | 0/1 |

Stream start order (from the C property pipeline): format -> resolution ->
fps -> stream mode on; stop: stream mode off. Changing modes requires
passing through OFF first (`SetStreamMode`).

Depth input formats (`XnIODepthFormats`): UNCOMPRESSED_16_BIT=0,
COMPRESSED_PS=1, UNCOMPRESSED_10_BIT=2, **UNCOMPRESSED_11_BIT=3 (Linux
default - despite the name this is the PACKED 11-bit format on the wire,
handled by XnPacked11DepthProcessor!)**, UNCOMPRESSED_12_BIT=4 (packed 12).
Image input formats (`XnIOImageFormats`): BAYER=0, YUV422=1, JPEG=2,
JPEG_420=3, JPEG_MONO=4, UNCOMPRESSED_YUV422=5 (default), UNCOMPRESSED_BAYER=6,
UNCOMPRESSED_YUYV=7.

## 4. Calibration

- **GET_FIXED_PARAMS** (opcode 4): chunked read of a 168-byte (V3.0+
  layout) struct; each request carries `u16 word_offset = bytes_read / 4`;
  each reply appends `nDataSize * 2` bytes. Geometry block at offset 88:
  `fDCmosEmitterDistance` (LDDIS, f32) at 92, `fDCmosRCmosDistance` at 96,
  `fReferenceDistance` (**ZPD**, f32) at 100, `fReferencePixelSize`
  (**ZPPS**, f32) at 104 (`XnFixedParams`, XnDeviceSensor.h).
- **ALGORITHM_PARAMS** (opcode 22), fw >= 5.1 request: `{ u16 algorithm_id,
  u16 format=0, u16 resolution, u16 fps, u16 offset_words }` (10 bytes,
  `XnAlgorithmParamRequest` in XnHostProtocol.cpp). Algorithm ids:
  DEPTH_INFO=0x00, REGISTRATION=0x02, PADDING=0x03, BLANKING=0x06,
  DEVICE_INFO=0x07, FREQUENCY=0x80. DEPTH_INFO returns
  `XnDepthInformation { u16 const_shift }` (requested at VGA/30).
- **Shift-to-depth** (`DDK/XnShiftToDepth.cpp`), defaults ParamCoeff=4,
  ShiftScale=10 (XnSensorDepthStream.cpp:44-45), PixelSizeFactor=1:

```text
const_shift_adj = (param_coeff * const_shift) / pixel_size_factor   [u32 math]
plane_pixel_size = zero_plane_pixel_size * pixel_size_factor
for shift in 1 .. device_max_shift (EXCLUSIVE):
    fixed_ref_x = (shift - const_shift_adj) / param_coeff - 0.375
    metric = fixed_ref_x * plane_pixel_size
    depth = shift_scale * (metric * zpd / (emitter_distance - metric) + zpd)
    if min_cutoff < depth < min(device_max_depth, max_cutoff):
        s2d[shift] = depth (truncated to u16)
        d2s[last_depth .. depth) = last_index
d2s[last_depth ..= device_max_depth] = last_index
```

- Horizontal FOV: `2 * atan(ZPPS * 1280 / 2 / ZPD)` (SXGA width;
  XnSensorDepthStream.cpp).

## 5. Bulk stream framing

Every bulk read on the depth/image/misc endpoints yields a byte stream of
**packets**, each prefixed with a 12-byte header
(`XnSensorProtocolResponseHeader`):

```text
u16 magic      = firmware magic ("SP" 0x5350 / "BR" 0x4252)  <- NOT a per-packet magic
u16 type       = packet type (see below)
u16 packet_id
u16 buf_size   = payload + 12 (header included)
u32 timestamp  = device ticks (see quirk below)
```

Packet types (`XnDeviceSensorProtocol.h`): DEPTH_START 0x7100,
DEPTH_BUFFER 0x7200, DEPTH_END 0x7500, IMAGE_START 0x8100,
IMAGE_BUFFER 0x8200, IMAGE_END 0x8500, AUDIO_BUFFER 0x9200, GMC 0xa200,
OVERHEAT 0xf31f, PROJECTOR_FAULT 0xdead.

Parser state machine (`XnDeviceSensorProtocolUsbEpCb`):
`LOOKING_FOR_MAGIC` (byte scan, with a partial-magic carry across reads;
the 12-byte header INCLUDES the two magic bytes) -> `PACKET_HEADER`
(accumulate 12 bytes, fix endianness, `buf_size -= 12`) -> `PACKET_DATA`
(accumulate payload) -> back to magic scan.

Frame assembly (`XnFrameStreamProcessor`):
- SOF packet with `nDataOffset == 0` starts a frame (its payload is part
  of the frame); data packets append; EOF packet appends and completes.
- Frame timestamp = **EOF packet timestamp** (when using device
  timestamps; `OnEndOfFrame` -> `CreateTimestampFromDevice`).
- Quirk, fw >= 5.1: the SOF packet's timestamp field is NOT a timestamp
  but padding pixels: `start = ts >> 16`, `end = ts & 0xffff`
  (`XnDepthProcessor::OnStartOfFrame`).
- Firmware < 5.0: skip the first chunk of garbage on each endpoint when
  streaming starts (`nIgnoreBytes = nChunkReadBytes`).
- Firmware <= 5.1: depth and image endpoints are swapped relative to the
  probe order (`XnDeviceSensorOpenInputThreads`).
- Low-bandwidth ISO "double SOF" (consecutive SOF packets with sequential
  ids continue the same frame) is ISO-mode only; bulk always treats an SOF
  as a new frame.

## 6. Depth decoding

- Packed 11-bit (COMPRESSED_PS-style packing, used for the default
  UNCOMPRESSED_11_BIT input): 4 values x 11 bits per 11 bytes. Bit layout
  from `XnPacked11DepthProcessor.cpp` (input byte -> bit allocation):

```text
input:    0,  1,  2,3,  4,  5,  6,7,  8,  9,10
bits:     8,3,5,6,2,8,1,7,4,4,7,1,8,2,6,5,3,8
output:     0,  1,    2,  3,  4,    5,  6,  7
```

- Raw shift values then go through the S2D table (section 4) to become
  millimeters.

## 7. Color decoding

- Default input: uncompressed YUV422, byte order `[U, Y1, V, Y2]`
  (`YUV.h`: `YUV422_U=0, YUV422_Y1=1, YUV422_V=2, YUV422_Y2=3`).
- BT.601 integer conversion (`YUV.cpp::YUV444ToRGB888`):

```text
C = (Y - 16) * 298 + 128;  D = U - 128;  E = V - 128
R = clamp((C + 409*E) >> 8);  G = clamp((C - 100*D - 208*E) >> 8);  B = clamp((C + 516*D) >> 8)
```

## 8. Identity

- GET_SERIAL_NUMBER (opcode 37): no request payload; reply data is an
  ASCII string, `nDataSize * 2` bytes (XnHostProtocolGetSerialNumber).
- GET_VERSION (opcode 0): reply is a 33-byte packed `XnVersions`:
  `u8 major, u8 minor, u16 build, u32 chip, u16 fpga, u16 system_version,
  SDK {u8,u8,u8,u16}, u32 HWVer, u32 FWVer, u32 SensorVer, u32 ChipVer`.
  FPGA values identify the hardware revision (RD1082=0x5 for the Xtion).
  Wire quirks (XnHostProtocolGetVersion):
  - The first two bytes on the wire are **minor, major** - the C
    byte-swaps the struct's first u16 after memcpy
    (`*((XnUInt16*)&Version) = xnOSEndianSwapUINT16(...)`).
  - Only the first **12 bytes** carry fields the driver actually uses;
    SDK/HWVer/FWVer/SensorVer/ChipVer are recomputed host-side (from
    nFPGA, nChip and the version triple), and the C memcpy's the full
    struct out of a partially-filled buffer - a reply with only the
    12-byte base struct is valid.
  - The device pads the struct to a whole word (33 -> 34 bytes).

### Bulk read buffer sizing (verified on hardware)

- The reference read thread reads in chunks of **40 x max packet size**
  (`XN_SENSOR_USB_DEPTH_BUFFER_SIZE_MULTIPLIER_BULK` = 40 on Linux,
  `XnDeviceSensorInit.h`) into a ring of pre-submitted async URBs
  (`xnUSBInitReadThread`).
- **Every bulk-IN read length must be a multiple of the endpoint's
  wMaxPacketSize.** The Linux USB stack completes a bulk-IN URB whose
  length is not a multiple with `EOVERFLOW` — observed live: a
  whole-frame-sized buffer of 422,411 bytes (640x480 packed-11 + headers,
  422,411 % 512 = 11) failed instantly on every frame read, while the
  regular 40 x 512 = 20,480-byte chunks work.
- A read timeout with partial data still returns the received bytes
  (libusb/rusb behavior); an empty timeout must be retried until the
  caller's deadline — data paces at ~12 MB/s for 640x480@30 packed-11.

## 9. Reference file map

| Topic | File in OpenNI 2.2.0 |
|---|---|
| Endpoints / open / probe | `Source/Drivers/PS1080/Sensor/XnDeviceSensorIO.cpp` |
| Command framing + exchange | `Source/Drivers/PS1080/Sensor/XnHostProtocol.cpp` |
| Opcodes / NACK / header structs | `Source/Drivers/PS1080/Sensor/XnHostProtocol.h` |
| Param ids | `Source/Drivers/PS1080/Sensor/XnParams.h` |
| Buffer sizes / timeouts | `Source/Drivers/PS1080/Sensor/XnDeviceSensorInit.h/.cpp` |
| Fixed params layout | `Source/Drivers/PS1080/Sensor/XnDeviceSensor.h` (XnFixedParams) |
| Version enum / formats | `Include/PS1080.h` |
| Resolutions / stream modes | `Source/Drivers/PS1080/Include/XnStreamParams.h` |
| Bulk packet types / state machine | `Source/Drivers/PS1080/Sensor/XnDeviceSensorProtocol.h/.cpp` |
| Frame assembly (SOF/EOF semantics) | `Source/Drivers/PS1080/Sensor/XnFrameStreamProcessor.cpp` |
| Depth processing (padding quirk) | `Source/Drivers/PS1080/Sensor/XnDepthProcessor.cpp` |
| Processor selection per input format | `Source/Drivers/PS1080/Sensor/XnSensorDepthStream.cpp` |
| Packed-11 unpacker | `Source/Drivers/PS1080/Sensor/XnPacked11DepthProcessor.cpp` |
| Shift-to-depth math | `Source/Drivers/PS1080/DDK/XnShiftToDepth.cpp/.h` |
| YUV conversion | `Source/Drivers/PS1080/Sensor/YUV.cpp/.h` |
| libusb usage (control transfers) | `ThirdParty/PSCommon/XnLib/Source/Linux/XnLinuxUSB.cpp` |
