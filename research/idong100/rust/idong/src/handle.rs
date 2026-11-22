//! RF handle (wand / airtouch wristband) events.
//!
//! The 2.4 GHz RF link from the handheld controllers terminates inside the
//! sensor bar; pairing and button state arrive over the vendor interface
//! as the TSOL_DEVICE_HANDLE_* sequence (open item #6: exact blob layout).
//! `xEyeUsb.ini`: `NeedHandleMatch=1`, `MonitorList=10`.

use std::time::Duration;

/// Handle-match configuration sent to the device.
///
/// Open item #6: the serialization is the captured vendor blob until the
/// handle-config format is disassembled. Raw passthrough by design.
#[derive(Debug, Clone, Default)]
pub struct HandleConfig {
    pub raw: Vec<u8>,
}

/// Handle lifecycle events surfaced through [`crate::device::Idong::drain_events`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleEvent {
    /// Match round requested (we sent the config).
    Requested,
    /// TSOL_DEVICE_HANDLE_FOUND
    Found,
    /// TSOL_DEVICE_HANDLE_NOT_FOUND
    NotFound,
    /// TSOL_DEVICE_HANDLE_TIMEOUT
    Timeout,
    /// TSOL_DEVICE_HANDLE_FOUND_OVER
    FoundOver,
}

/// Vendor monitor list window (MonitorList = 10).
pub const MONITOR_LIST: usize = 10;
/// Default handle-match round duration.
pub const HANDLE_MATCH_WINDOW: Duration = Duration::from_secs(10);
