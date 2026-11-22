//! Stream depth frames and print per-frame statistics.
//!
//! Run: `cargo run --example depth_dump -- [--frames N]`

use std::time::Duration;

use xtion::{DepthConfig, Xtion};

fn main() -> xtion::Result<()> {
    let frames: usize = std::env::args()
        .skip(1)
        .last()
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);

    let mut cam = Xtion::open()?;
    println!("opened Xtion, firmware {}", cam.version);

    let config = DepthConfig::default(); // VGA 640x480 @ 30 fps, packed-11
    cam.start_depth(config)?;
    println!(
        "depth stream started: {}x{} @ {} fps ({:?})",
        config.dimensions().0,
        config.dimensions().1,
        config.fps,
        config.input_format
    );

    for i in 0..frames {
        match cam.read_depth_frame(Duration::from_secs(5)) {
            Ok(frame) => {
                println!(
                    "frame {:>3}: ts {:>10}, min {:>5} mm, {:>6}/{} valid pixels",
                    i + 1,
                    frame.timestamp,
                    frame.min_depth().unwrap_or(0),
                    frame.valid_pixels(),
                    frame.data.len()
                );
            }
            Err(e) => {
                println!("frame {:>3}: error {}", i + 1, e);
            }
        }
    }

    cam.stop_depth()?;
    println!("depth stream stopped");
    Ok(())
}
