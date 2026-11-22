//! High-level device facade: the whole lifecycle behind one type.
//!
//! ```text
//! Idong::open()                device already runtime → start tracking
//! Idong::open_or_boot(&fw)     bootloader → upload → re-enum → runtime
//! ```
//!
//! `FirmwareSet` is deliberately **user-supplied**: the vendor images are
//! not redistributed with the crate. The set is size-checked before any
//! byte is written; recorded sha256 values live in `firmware/README.md`
//! for out-of-band verification.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::calib::{DeviceIdentity, StereoCalib};
use crate::error::{IdongError, Result};
use crate::handle::{HandleConfig, HandleEvent};
use crate::markers::MarkerFrame;
use crate::protocol::boot::{BootInfo, BootUploader};
use crate::stream::{Streamer, StatsSnapshot};
use crate::usb::{DeviceState, Transport};

/// Tracking configuration (v1: mostly informational; the device streams
/// at its fixed 50 fps — `xEyeFrame=50`).
#[derive(Debug, Clone)]
pub struct TrackingConfig {
    /// Report frames whose counter jumped (vendor behaviour: publish + flag).
    pub report_drops: bool,
    /// Quality-filter threshold per slot (vendor default 50).
    pub quality_min: i32,
}

impl Default for TrackingConfig {
    fn default() -> Self {
        TrackingConfig {
            report_drops: true,
            quality_min: crate::protocol::MARKER_QUALITY_MIN,
        }
    }
}

/// The vendor firmware triple (BF527 + Spartan-3 + 8051), user-supplied.
#[derive(Debug, Clone)]
pub struct FirmwareSet {
    /// `ens_code.ldr` — Blackfin BF527 loader image.
    pub bf527_ldr: PathBuf,
    /// `A_a_v4.4_BG0313.bit` — Spartan-3 FPGA bitstream.
    pub fpga_bit: PathBuf,
    /// `app_m8051_2500.hex` (or `_2611.hex` per `xeyezHal.ini` platform).
    pub mcu_hex: PathBuf,
}

impl FirmwareSet {
    /// Collect and size-check all three images. Hash verification against
    /// `firmware/README.md` is done by the caller so that alternative
    /// vendor versions stay usable.
    pub fn load(
        bf527_ldr: impl AsRef<Path>,
        fpga_bit: impl AsRef<Path>,
        mcu_hex: impl AsRef<Path>,
    ) -> Result<FirmwareSet> {
        for p in [bf527_ldr.as_ref(), fpga_bit.as_ref(), mcu_hex.as_ref()] {
            let meta = std::fs::metadata(p).map_err(|e| IdongError::FirmwareIntegrity {
                path: p.to_path_buf(),
                got: e.to_string(),
                want: "readable file".into(),
            })?;
            if meta.len() == 0 {
                return Err(IdongError::FirmwareIntegrity {
                    path: p.to_path_buf(),
                    got: "empty".into(),
                    want: "non-empty firmware".into(),
                });
            }
        }
        Ok(FirmwareSet {
            bf527_ldr: bf527_ldr.as_ref().to_path_buf(),
            fpga_bit: fpga_bit.as_ref().to_path_buf(),
            mcu_hex: mcu_hex.as_ref().to_path_buf(),
        })
    }
}

/// Queued device events (populated from milestone M6).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Handle(HandleEvent),
    LinkDisturbed,
    LinkNominal,
}

/// The device facade.
pub struct Idong {
    transport: Option<Transport>,
    streamer: Option<Streamer>,
    identity: Option<DeviceIdentity>,
    calib: Option<StereoCalib>,
    boot_info: Option<BootInfo>,
}

impl Idong {
    /// Open a runtime-state device. Errors with [`IdongError::BootloaderState`]
    /// if the sensor is still waiting for firmware (use [`Self::open_or_boot`]).
    pub fn open() -> Result<Self> {
        match Transport::open_runtime() {
            Ok(t) => Ok(Idong {
                transport: Some(t),
                streamer: None,
                identity: None,
                calib: None,
                boot_info: None,
            }),
            Err(IdongError::NotFound) => {
                // Is it sitting in the bootloader instead?
                let states = crate::usb::list_devices();
                if states.iter().any(|d| d.state == DeviceState::Bootloader) {
                    Err(IdongError::BootloaderState)
                } else {
                    Err(IdongError::NotFound)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Full lifecycle: if the device is in the bootloader, upload the
    /// BF527 image (safety-gated — see `protocol::boot::BOOT_CONFIRMED`),
    /// wait for re-enumeration, then open the runtime interface.
    pub fn open_or_boot(fw: &FirmwareSet) -> Result<Self> {
        let mut me = Self::open();
        if matches!(&me, Err(IdongError::BootloaderState)) {
            let ldr = std::fs::read(&fw.bf527_ldr)?;
            let bt = Transport::open_bootloader()?;
            let mut uploader = BootUploader::new(&bt, &ldr);
            let info = uploader.run()?; // gated until M3 confirmation
            let rt = crate::usb::wait_runtime_reenum(Duration::from_secs(10))?;
            me = Ok(Idong {
                transport: Some(rt),
                streamer: None,
                identity: None,
                calib: None,
                boot_info: Some(info),
            });
        }
        me
    }

    /// Device identity (from an attached EEPROM dump — the dump is the
    /// recovery source; the device does not expose the EEPROM directly).
    pub fn identity(&self) -> Result<&DeviceIdentity> {
        self.identity
            .as_ref()
            .ok_or(IdongError::Protocol(crate::error::ProtocolError::Short {
                len: 0,
                need: 40,
            }))
    }

    /// Attach a dumped T24C16A EEPROM (2 KiB) to provide calibration and
    /// identity.
    pub fn attach_eeprom(&mut self, dump: &[u8]) -> Result<()> {
        let (cal, id) = crate::calib::parse_eeprom(dump)?;
        self.identity = Some(id);
        self.calib = Some(cal);
        Ok(())
    }

    /// Factory stereo calibration (from an attached EEPROM dump).
    pub fn calibration(&self) -> Result<&StereoCalib> {
        self.calib
            .as_ref()
            .ok_or(IdongError::Protocol(crate::error::ProtocolError::Short {
                len: 0,
                need: 120,
            }))
    }

    /// Boot info captured during [`Self::open_or_boot`] (device ID / DSP
    /// version), if this process performed the upload.
    pub fn boot_info(&self) -> Option<&BootInfo> {
        self.boot_info.as_ref()
    }

    /// Start marker streaming (consumes the transport; the reader thread
    /// answers keep-alives and link status inline).
    pub fn start_tracking(&mut self, _cfg: TrackingConfig) -> Result<()> {
        let t = self.transport.take().ok_or(IdongError::NotFound)?;
        self.streamer = Some(Streamer::start(t));
        Ok(())
    }

    /// Blocking read of the next marker frame.
    pub fn read_frame(&mut self, timeout: Duration) -> Result<MarkerFrame> {
        self.streamer
            .as_ref()
            .ok_or(IdongError::NotFound)?
            .read_frame(timeout)
    }

    /// Live stream statistics.
    pub fn stream_stats(&self) -> Option<StatsSnapshot> {
        self.streamer.as_ref().map(|s| s.stats())
    }

    /// Drain queued events (handle match, link status). v1: the stream
    /// thread handles control frames inline; queued events arrive with
    /// milestone M6.
    pub fn drain_events(&mut self) -> Vec<Event> {
        Vec::new()
    }

    /// Suspend the device (`setEyeUsbSuspend` analog) — milestone M7.
    pub fn suspend(&self) -> Result<()> {
        Err(IdongError::Protocol(
            crate::error::ProtocolError::KeepaliveTimeout(Duration::ZERO),
        ))
    }

    /// Shut the device down (`setEyeUsbShutDown` analog) — milestone M7.
    pub fn shutdown(&self) -> Result<()> {
        Err(IdongError::Protocol(
            crate::error::ProtocolError::KeepaliveTimeout(Duration::ZERO),
        ))
    }
}

/// Default handle configuration (raw passthrough until open item #6).
pub fn default_handle_config() -> HandleConfig {
    HandleConfig::default()
}
