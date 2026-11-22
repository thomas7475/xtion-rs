//! Asynchronous bulk-in URB ring.
//!
//! Port of the reference driver's read thread (`xnUSBInitReadThread` /
//! `XnUSBReadThreadMain` in `ThirdParty/PSCommon/XnLib/Source/Linux/
//! XnLinuxUSB.cpp`): a fixed set of bulk transfers is submitted up front
//! and re-submitted the moment one completes, so one or more URBs are
//! ALWAYS queued at the device. This is not an optimization: the Xtion's
//! small FPGA FIFO drops stream data whenever no URB is ready to receive
//! it. A synchronous read loop (one URB at a time) leaves a window
//! between reaping and resubmitting on every 20 KiB — observed live as
//! frames arriving as fragments (736..45424 of 307200 pixels, ~4% of the
//! stream bytes surviving).
//!
//! The reference allocates `XN_SENSOR_USB_DEPTH_BUFFERS` (= 16 on Linux)
//! buffers of `XN_SENSOR_USB_DEPTH_BUFFER_SIZE_MULTIPLIER_BULK` (= 40) x
//! max packet size each (`XnDeviceSensorInit.h`) and runs its event loop
//! at CRITICAL thread priority, warning that lower priority "might cause
//! loss of data". This port keeps 16 x 40 x maxpacket queued.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use libusb1_sys as ffi;

use crate::error::{Result, XtionError};

/// Number of pre-submitted buffers
/// (`XN_SENSOR_USB_DEPTH_BUFFERS` on Linux).
pub const RING_BUFFERS: usize = 16;

/// Per-read buffer size in max-packet-size units
/// (`XN_SENSOR_USB_DEPTH_BUFFER_SIZE_MULTIPLIER_BULK` on Linux).
pub const BULK_MULTIPLIER: usize = 40;

/// Live counters of a running ring (diagnostics).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RingStats {
    /// Transfers that completed with data.
    pub completions: u64,
    /// Stream bytes delivered to the consumer.
    pub bytes: u64,
    /// Completions with a non-recoverable status or failed resubmits.
    pub failures: u64,
    /// Transfers currently queued at the device.
    pub in_flight: isize,
    /// Completed chunks waiting to be consumed.
    pub queued: usize,
}

struct RingShared {
    queue: Mutex<VecDeque<Vec<u8>>>,
    ready: Condvar,
    kill: AtomicBool,
    /// Number of transfers currently submitted (callback decrements,
    /// resubmit increments; the event thread exits at kill && 0).
    in_flight: AtomicIsize,
    /// Set when the event loop died (unrecoverable error): Drop must not
    /// wait for transfers and must leak instead of freeing.
    events_dead: AtomicBool,
    error: Mutex<Option<XtionError>>,
    completions: AtomicU64,
    bytes: AtomicU64,
    failures: AtomicU64,
}

impl RingShared {
    fn record_error(&self, err: XtionError) {
        log::error!("bulk ring: {err}");
        *lock(&self.error) = Some(err);
        self.ready.notify_all();
    }
}

/// One buffer + its libusb transfer. Boxed so its address is stable for
/// the transfer's `user_data`; never moved or freed while submitted.
struct TransferSlot {
    shared: Arc<RingShared>,
    transfer: *mut ffi::libusb_transfer,
    buffer: Vec<u8>,
    /// True while the transfer is queued at the device (set on submit,
    /// cleared at the top of the callback).
    submitted: AtomicBool,
}

/// Map a raw libusb return code to the crate error type.
fn libusb_error(rc: i32) -> XtionError {
    use rusb::Error as E;
    let e = match rc {
        x if x == ffi::constants::LIBUSB_ERROR_IO => E::Io,
        x if x == ffi::constants::LIBUSB_ERROR_INVALID_PARAM => E::InvalidParam,
        x if x == ffi::constants::LIBUSB_ERROR_NO_DEVICE => E::NoDevice,
        x if x == ffi::constants::LIBUSB_ERROR_NOT_FOUND => E::NotFound,
        x if x == ffi::constants::LIBUSB_ERROR_BUSY => E::Busy,
        x if x == ffi::constants::LIBUSB_ERROR_TIMEOUT => E::Timeout,
        x if x == ffi::constants::LIBUSB_ERROR_NO_MEM => E::NoMem,
        _ => E::Other,
    };
    XtionError::Usb(e)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// libusb transfer callback. Runs on the event thread.
extern "system" fn on_complete(transfer: *mut ffi::libusb_transfer) {
    unsafe {
        let slot = (*transfer).user_data as *mut TransferSlot;
        let slot = &*slot;
        let shared = &*slot.shared;
        slot.submitted.store(false, Ordering::SeqCst);
        shared.in_flight.fetch_sub(1, Ordering::SeqCst);

        let status = (*transfer).status;
        let len = (*transfer).actual_length as usize;

        match status {
            s if s == ffi::constants::LIBUSB_TRANSFER_COMPLETED => {
                if len > 0 {
                    // Copy out: the buffer is re-used for the next read
                    // as soon as we resubmit.
                    let data = slot.buffer[..len].to_vec();
                    shared.completions.fetch_add(1, Ordering::SeqCst);
                    shared.bytes.fetch_add(len as u64, Ordering::SeqCst);
                    lock(&shared.queue).push_back(data);
                    shared.ready.notify_all();
                }
                // Resubmit immediately (the reference driver re-queues the
                // buffer as long as the thread is not shutting down), so
                // the device never runs out of queued URBs.
                if !shared.kill.load(Ordering::SeqCst) {
                    let rc = ffi::libusb_submit_transfer(slot.transfer);
                    if rc == 0 {
                        slot.submitted.store(true, Ordering::SeqCst);
                        shared.in_flight.fetch_add(1, Ordering::SeqCst);
                    } else {
                        shared.failures.fetch_add(1, Ordering::SeqCst);
                        shared.record_error(libusb_error(rc));
                    }
                } else {
                    shared.ready.notify_all();
                }
            }
            s if s == ffi::constants::LIBUSB_TRANSFER_CANCELLED => {
                // Shutdown path; the reference driver still consumes
                // partial data of cancelled transfers.
                if len > 0 {
                    let data = slot.buffer[..len].to_vec();
                    shared.bytes.fetch_add(len as u64, Ordering::SeqCst);
                    lock(&shared.queue).push_back(data);
                }
                shared.ready.notify_all();
            }
            _ => {
                // ERROR / TIMED_OUT / STALL / NO_DEVICE / OVERFLOW.
                shared.failures.fetch_add(1, Ordering::SeqCst);
                shared.record_error(libusb_error(ffi::constants::LIBUSB_ERROR_IO));
                shared.ready.notify_all();
            }
        }
    }
}

/// Event loop thread: processes transfer completions until shutdown.
fn event_loop(ctx: *mut ffi::libusb_context, shared: Arc<RingShared>) {
    let tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 10_000, // 10 ms tick; only a poll granularity
    };
    loop {
        if shared.kill.load(Ordering::SeqCst) && shared.in_flight.load(Ordering::SeqCst) == 0 {
            break;
        }
        let rc =
            unsafe { ffi::libusb_handle_events_timeout_completed(ctx, &tv, std::ptr::null_mut()) };
        if rc != 0 && rc != ffi::constants::LIBUSB_ERROR_TIMEOUT {
            shared.events_dead.store(true, Ordering::SeqCst);
            shared.record_error(libusb_error(rc));
            shared.ready.notify_all();
            break;
        }
    }
}

/// A running ring of pre-submitted bulk-in transfers for one endpoint.
pub struct BulkRing {
    shared: Arc<RingShared>,
    // Boxed on purpose: each slot's heap address is the user_data stored
    // in its libusb transfer, so slots must never move (a plain Vec
    // would relocate them on growth).
    #[allow(clippy::vec_box)]
    slots: Vec<Box<TransferSlot>>,
    thread: Option<JoinHandle<()>>,
}

// SAFETY: all shared state is behind atomics or mutexes; libusb device
// handles are thread-safe for concurrent submit/cancel/callback use (the
// reference driver uses the same cross-thread pattern).
unsafe impl Send for BulkRing {}

impl BulkRing {
    /// Allocate `buffers` transfers of `buffer_len` bytes each and submit
    /// them all on `endpoint` (must have the IN bit set).
    ///
    /// # Safety contract (enforced by the crate, not the type system)
    ///
    /// `ctx` and `dev_handle` must stay valid until the returned ring is
    /// dropped; the ring must be dropped before the owning UsbDevice.
    pub(crate) fn start(
        ctx: *mut ffi::libusb_context,
        dev_handle: *mut ffi::libusb_device_handle,
        endpoint: u8,
        max_packet_size: u32,
        buffers: usize,
    ) -> Result<BulkRing> {
        assert!(endpoint & 0x80 != 0, "bulk ring needs an IN endpoint");
        let buffer_len = BULK_MULTIPLIER * max_packet_size as usize;
        let shared = Arc::new(RingShared {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            kill: AtomicBool::new(false),
            in_flight: AtomicIsize::new(0),
            events_dead: AtomicBool::new(false),
            error: Mutex::new(None),
            completions: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
        });

        let mut slots: Vec<Box<TransferSlot>> = Vec::with_capacity(buffers);
        for _ in 0..buffers {
            let transfer = unsafe { ffi::libusb_alloc_transfer(0) };
            if transfer.is_null() {
                for s in &slots {
                    unsafe { ffi::libusb_free_transfer(s.transfer) };
                }
                return Err(XtionError::Usb(rusb::Error::NoMem));
            }
            let mut slot = Box::new(TransferSlot {
                shared: shared.clone(),
                transfer,
                buffer: vec![0u8; buffer_len],
                submitted: AtomicBool::new(false),
            });
            unsafe {
                ffi::libusb_fill_bulk_transfer(
                    transfer,
                    dev_handle,
                    endpoint,
                    slot.buffer.as_mut_ptr(),
                    buffer_len as libc::c_int,
                    on_complete,
                    &mut *slot as *mut TransferSlot as *mut c_void,
                    0, // no timeout: transfers live until cancelled
                );
            }
            slots.push(slot);
        }

        // Submit every buffer up front.
        let mut submitted = 0usize;
        for slot in &slots {
            let rc = unsafe { ffi::libusb_submit_transfer(slot.transfer) };
            if rc == 0 {
                slot.submitted.store(true, Ordering::SeqCst);
                shared.in_flight.fetch_add(1, Ordering::SeqCst);
                submitted += 1;
            } else {
                shared.failures.fetch_add(1, Ordering::SeqCst);
                log::error!("bulk ring: initial submit failed: {}", libusb_error(rc));
            }
        }
        if submitted == 0 {
            // Nothing in flight, no thread: free directly.
            for slot in &slots {
                unsafe { ffi::libusb_free_transfer(slot.transfer) };
            }
            let err = lock(&shared.error)
                .take()
                .unwrap_or(XtionError::Usb(rusb::Error::Other));
            return Err(err);
        }
        if submitted < buffers {
            // Some buffers failed to queue; keep the ones that did.
            shared.record_error(XtionError::Usb(rusb::Error::Other));
        }

        let loop_shared = shared.clone();
        // Pass the raw context pointer as an address across the thread
        // boundary (raw pointers are not Send; libusb contexts are
        // thread-safe and the UsbDevice keeps this one alive).
        let ctx_addr = ctx as usize;
        let thread = thread::Builder::new()
            .name("xtion-bulk-ring".into())
            .spawn(move || event_loop(ctx_addr as *mut ffi::libusb_context, loop_shared))
            .map_err(|e| XtionError::BadConfig(format!("ring thread spawn failed: {e}")))?;

        Ok(BulkRing {
            shared,
            slots,
            thread: Some(thread),
        })
    }

    /// Block until a completed chunk is available or `deadline` passes
    /// (None). Wakes early on ring errors.
    pub fn pop(&self, deadline: Instant) -> Option<Vec<u8>> {
        let mut queue = lock(&self.shared.queue);
        loop {
            if let Some(data) = queue.pop_front() {
                return Some(data);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _timeout) = self
                .shared
                .ready
                .wait_timeout(queue, deadline - now)
                .unwrap_or_else(|e| e.into_inner());
            queue = guard;
        }
    }

    /// Drain buffered chunks without waiting.
    pub fn drain(&self, out: &mut Vec<Vec<u8>>) {
        let mut queue = lock(&self.shared.queue);
        while let Some(data) = queue.pop_front() {
            out.push(data);
        }
    }

    /// First fatal error recorded by the ring, if any.
    pub fn error(&self) -> Option<XtionError> {
        (*lock(&self.shared.error)).clone()
    }

    /// Snapshot of the ring counters (diagnostics).
    pub fn stats(&self) -> RingStats {
        RingStats {
            completions: self.shared.completions.load(Ordering::SeqCst),
            bytes: self.shared.bytes.load(Ordering::SeqCst),
            failures: self.shared.failures.load(Ordering::SeqCst),
            in_flight: self.shared.in_flight.load(Ordering::SeqCst),
            queued: lock(&self.shared.queue).len(),
        }
    }
}

impl Drop for BulkRing {
    fn drop(&mut self) {
        self.shared.kill.store(true, Ordering::SeqCst);

        // Cancel everything still queued; a callback that was already
        // running when `kill` was set may resubmit once, so loop until
        // the in-flight count drains. The event thread keeps processing
        // cancellations while we poll.
        while self.shared.in_flight.load(Ordering::SeqCst) > 0
            && !self.shared.events_dead.load(Ordering::SeqCst)
        {
            for slot in &self.slots {
                if slot.submitted.load(Ordering::SeqCst) {
                    unsafe { ffi::libusb_cancel_transfer(slot.transfer) };
                }
            }
            thread::sleep(Duration::from_millis(5));
        }

        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }

        if self.shared.events_dead.load(Ordering::SeqCst)
            && self.shared.in_flight.load(Ordering::SeqCst) > 0
        {
            // The event loop died with transfers still submitted (device
            // gone). Freeing a submitted transfer is not allowed; leak
            // them like the reference driver does on disconnect.
            log::warn!(
                "bulk ring: leaking {} transfers after event-loop death",
                self.shared.in_flight.load(Ordering::SeqCst)
            );
            std::mem::forget(std::mem::take(&mut self.slots));
            return;
        }

        for slot in &self.slots {
            unsafe { ffi::libusb_free_transfer(slot.transfer) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_constants_match_reference() {
        // XnDeviceSensorInit.h (Linux block): 16 buffers, multiplier 40.
        assert_eq!(RING_BUFFERS, 16);
        assert_eq!(BULK_MULTIPLIER, 40);
        // Total queued bytes for a 512-byte endpoint.
        assert_eq!(RING_BUFFERS * BULK_MULTIPLIER * 512, 327_680);
    }

    #[test]
    fn buffer_len_is_multiple_of_max_packet() {
        for mps in [64u32, 512, 1024] {
            assert_eq!((BULK_MULTIPLIER * mps as usize) % mps as usize, 0);
        }
    }

    #[test]
    fn libusb_error_maps_codes() {
        assert!(matches!(
            libusb_error(ffi::constants::LIBUSB_ERROR_NO_DEVICE),
            XtionError::Usb(rusb::Error::NoDevice)
        ));
        assert!(matches!(
            libusb_error(9999),
            XtionError::Usb(rusb::Error::Other)
        ));
    }
}
