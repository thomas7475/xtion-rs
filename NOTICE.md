# NOTICE

This product includes software developed as part of a from-scratch Rust
port of the PrimeSense PS1080 USB protocol, as implemented by the OpenNI 2
project (https://github.com/occipital/OpenNI2), Copyright 2012 PrimeSense
Ltd., licensed under the Apache License, Version 2.0.

Protocol constants, framing, calibration math and decoding logic were
derived by reading the OpenNI 2.2.0 source (tarball sha256
08f6842f20d1098ab2ebafadaac0130ffae5abd34cdf464bb6100cbe01ed95a8) and are
reproduced in Rust in this crate under the terms of the Apache License,
Version 2.0. No OpenNI 2 source code is included in or linked by this
repository; the USB transport is provided by the `rusb` crate (libusb).

The detailed per-constant provenance is documented in
`docs/PROTOCOL.md`, which cites the originating file of every ported
constant and algorithm.
