//! Stream infrared frames and print per-frame statistics; saves the
//! first decoded frame as a 16-bit PGM (`ir_frame.pgm`) so the picture
//! can be inspected with any image viewer.
//!
//! The IR stream runs on the IMAGE endpoint (STREAM0 mode 3). The IR
//! CMOS (MT9M001) transmits `height + 8` rows per frame; the driver
//! validates the wire size and crops to `height` visible rows, exactly
//! like OpenNI's `XnIRProcessor` does.
//!
//! Run: `cargo run --example ir_dump -- [--frames N]`

use std::time::Duration;

use xtion::{IrConfig, Xtion, BULK_MULTIPLIER, RING_BUFFERS};

fn main() -> xtion::Result<()> {
    let frames: usize = std::env::args()
        .skip(1)
        .last()
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);

    let mut cam = Xtion::open()?;
    let (_depth_mps, image_mps) = cam.usb_max_packet_sizes();
    println!("opened Xtion, firmware {}", cam.version);
    println!(
        "depth CMOS type {} (0 none, 1 MT9M001, 2 AR130)",
        cam.fixed_params.depth_cmos_type
    );

    let config = IrConfig::default(); // VGA 640x480 @ 30 fps, packed-10
    cam.start_ir(config)?;
    let (width, height) = config.dimensions();
    println!(
        "ir stream started: {}x{} @ {} fps on the image endpoint (max packet {} bytes); \
         ring: {} buffers x {} x {} B",
        width, height, config.fps, image_mps, RING_BUFFERS, BULK_MULTIPLIER, image_mps
    );

    let started = std::time::Instant::now();
    let mut saved = false;
    for i in 0..frames {
        match cam.read_ir_frame(Duration::from_secs(5)) {
            Ok(frame) => {
                let (min, max) = frame.value_range().unwrap_or((0, 0));
                println!(
                    "frame {:>3}: ts {:>10}, values {:>4}..{:>4} (10-bit), {}x{} px",
                    i + 1,
                    frame.timestamp,
                    min,
                    max,
                    frame.width,
                    frame.height
                );
                if !saved {
                    save_pgm("ir_frame.pgm", &frame)?;
                    println!("         saved ir_frame.pgm (10-bit values scaled to 16-bit)");
                    saved = true;
                }
            }
            Err(e) => {
                println!("frame {:>3}: error {}", i + 1, e);
            }
        }
    }

    if let Some(stats) = cam.ir_ring_stats() {
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
        // 640x480 @ 30 fps packed-10 with the 8 extra CMOS rows needs
        // ~11.7 MB/s of payload; high-speed bulk delivers ~40 MB/s.
        println!("note: this stream needs ~11.7 MB/s of USB bandwidth (payload only)");
    }

    cam.stop_ir()?;
    println!("ir stream stopped");
    Ok(())
}

/// Write one IR frame as a big-endian 16-bit PGM (P5). The raw 10-bit
/// sensor values are left-aligned into 16 bits (<< 6) so the full
/// dynamic range is visible in viewers that assume 16-bit PGMs.
fn save_pgm(path: &str, frame: &xtion::IrFrame) -> xtion::Result<()> {
    use std::io::Write;
    let mut out = Vec::with_capacity(frame.data.len() * 2 + 32);
    write!(out, "P5\n{} {}\n65535\n", frame.width, frame.height).unwrap();
    for v in &frame.data {
        let scaled = (*v as u32) << 6;
        out.extend_from_slice(&(scaled as u16).to_be_bytes());
    }
    std::fs::write(path, &out).map_err(|e| xtion::XtionError::Io(e.to_string()))?;
    Ok(())
}
