//! Runtime stream: bulk reads → frame assembly → marker frames.
//!
//! The reader loop mirrors `xEyeClass.dll`'s reader thread: bulk chunks
//! feed a [`FrameDecoder`], control frames (0xFFF keep-alive, type-4 link
//! status) are answered inline, and marker records are parsed with the
//! vendor's frame-counter drop check and ≥ 50 quality filter. Stats cover
//! drops, resyncs, and a `RingSize`-style quality window.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use crate::error::Result;
use crate::markers::MarkerFrame;
use crate::protocol::{self, frames::FrameDecoder, records, runtime::RuntimeClient};
use crate::usb::Transport;

/// Stream statistics (all monotonic counters).
#[derive(Debug, Default)]
pub struct StreamStats {
    /// Bulk chunks consumed.
    pub chunks: AtomicU64,
    /// Complete frames decoded.
    pub frames: AtomicU64,
    /// Marker frames published.
    pub marker_frames: AtomicU64,
    /// Frames whose counter jumped (vendor drop check).
    pub drops: AtomicU64,
    /// Keep-alive polls answered.
    pub keepalives: AtomicU64,
    /// Link-status frames handled.
    pub link_status: AtomicU64,
}

impl StreamStats {
    fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            chunks: self.chunks.load(Ordering::Relaxed),
            frames: self.frames.load(Ordering::Relaxed),
            marker_frames: self.marker_frames.load(Ordering::Relaxed),
            drops: self.drops.load(Ordering::Relaxed),
            keepalives: self.keepalives.load(Ordering::Relaxed),
            link_status: self.link_status.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub chunks: u64,
    pub frames: u64,
    pub marker_frames: u64,
    pub drops: u64,
    pub keepalives: u64,
    pub link_status: u64,
}

/// Runtime streamer. Spawn with [`Streamer::start`], then receive
/// [`MarkerFrame`]s through the channel.
pub struct Streamer {
    stats: Arc<StreamStats>,
    rx: Receiver<MarkerFrame>,
}

impl Streamer {
    /// Start the reader thread on an open runtime transport.
    pub fn start(transport: Transport) -> Self {
        let (tx, rx) = mpsc::channel();
        let stats = Arc::new(StreamStats::default());
        let thread_stats = Arc::clone(&stats);
        std::thread::Builder::new()
            .name("idong-stream".into())
            .spawn(move || reader_loop(transport, tx, thread_stats))
            .expect("spawn idong-stream thread");
        Streamer { stats, rx }
    }

    /// Blocking read of the next marker frame with timeout.
    pub fn read_frame(&self, timeout: Duration) -> Result<MarkerFrame> {
        match self.rx.recv_timeout(timeout) {
            Ok(f) => Ok(f),
            Err(RecvTimeoutError::Timeout) => Err(crate::error::IdongError::Protocol(
                crate::error::ProtocolError::ReadTimeout(timeout),
            )),
            Err(RecvTimeoutError::Disconnected) => Err(crate::error::IdongError::NotFound),
        }
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }
}

fn reader_loop(
    transport: Transport,
    tx: mpsc::Sender<MarkerFrame>,
    stats: Arc<StreamStats>,
) {
    let mut decoder = FrameDecoder::new();
    let client = RuntimeClient::new(&transport);
    let mut prev_counter: Option<u32> = None;
    loop {
        let chunk = match transport.read_raw(protocol::RUNTIME_CHUNK, Duration::from_secs(2)) {
            Ok(c) => c,
            Err(e) => {
                log::debug!("stream read error: {e}; retrying");
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        StreamStats::bump(&stats.chunks);
        for frame in decoder.push(&chunk) {
            StreamStats::bump(&stats.frames);
            // Control plane: keep-alive must be answered inline.
            if RuntimeClient::is_keepalive(&frame) {
                if let Err(e) = client.answer_keepalive() {
                    log::warn!("keep-alive answer failed: {e}");
                } else {
                    StreamStats::bump(&stats.keepalives);
                }
                continue;
            }
            if frame.kind == protocol::TYPE_LINK_STATUS {
                match client.handle_link_status(&frame) {
                    Ok(_) => StreamStats::bump(&stats.link_status),
                    Err(e) => log::warn!("link-status handling failed: {e}"),
                }
                continue;
            }
            // Data plane: marker record.
            if let Some(mf) = records::parse_record(&frame.payload, prev_counter) {
                if mf.dropped {
                    StreamStats::bump(&stats.drops);
                }
                prev_counter = Some(mf.frame_counter);
                StreamStats::bump(&stats.marker_frames);
                if tx.send(mf).is_err() {
                    return; // receiver dropped — stop the thread
                }
            }
        }
    }
}
