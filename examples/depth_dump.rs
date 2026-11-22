//! Stream depth frames and print per-frame statistics.
//!
//! Run: `cargo run --example depth_dump -- [--frames N]`

use std::time::Duration;

use xtion::{DepthConfig, Xtion, BULK_MULTIPLIER, RING_BUFFERS};

fn main() -> xtion::Result<()> {
    let frames: usize = std::env::args()
        .skip(1)
        .last()
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);

    let mut cam = Xtion::open()?;
    let (depth_mps, _image_mps) = cam.usb_max_packet_sizes();
    println!("opened Xtion, firmware {}", cam.version);
    println!(
        "usb speed {:?}, depth endpoint max packet {} bytes",
        cam.usb_speed(),
        depth_mps
    );

    let config = DepthConfig::default(); // VGA 640x480 @ 30 fps, packed-11
    cam.start_depth(config)?;
    let (width, height) = config.dimensions();
    let expected_pixels = (width * height) as usize;
    println!(
        "depth stream started: {}x{} @ {} fps ({:?}); ring: {} buffers x {} x {} B",
        width, height, config.fps, config.input_format, RING_BUFFERS, BULK_MULTIPLIER, depth_mps
    );

    let started = std::time::Instant::now();
    for i in 0..frames {
        match cam.read_depth_frame(Duration::from_secs(5)) {
            Ok(frame) => {
                let s = frame.shift_stats;
                println!(
                    "frame {:>3}: ts {:>10}, min {:>5} mm, {:>6}/{} valid pixels (expected {})",
                    i + 1,
                    frame.timestamp,
                    frame.min_depth().unwrap_or(0),
                    frame.valid_pixels(),
                    frame.data.len(),
                    expected_pixels,
                );
                println!(
                    "           shifts: 0:{}  2047:{}  1-1023:{}  1024-2046:{}  min-max {:?}..{:?}",
                    s.zeros, s.markers, s.low, s.high, s.min, s.max
                );
            }
            Err(e) => {
                println!("frame {:>3}: error {}", i + 1, e);
            }
        }
    }

    if let Some(stats) = cam.depth_ring_stats() {
        let secs = started.elapsed().as_secs_f64();
        println!(
            "ring: {} completions, {} bytes in {:.2} s = {:.2} MB/s, {} failures, {} queued",
            stats.completions,
            stats.bytes,
            secs,
            stats.bytes as f64 / 1_048_576.0 / secs.max(f64::EPSILON),
            stats.failures,
            stats.queued
        );
        // 640x480 @ 30 fps packed-11 needs ~12.7 MB/s of payload; high-speed
        // bulk delivers ~40 MB/s, full speed only ~1.2 MB/s.
        println!("note: this stream needs ~12.7 MB/s of USB bandwidth (payload only)");
    }

    cam.stop_depth()?;
    println!("depth stream stopped");
    Ok(())
}
