//! Runtime marker dump to JSONL (one JSON object per frame).
//!
//! Run: cargo run --example dump_frames -- [--seconds 30] [--eeprom dump.bin]

use std::time::{Duration, Instant};

use idong::device::Idong;

fn main() {
    env_logger::init();
    let seconds: u64 = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--seconds")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(30)
    };

    let mut dev = Idong::open().expect("no runtime idong sensor (boot first)");
    println!("# identity: {:?}", dev.identity().map(|i| i.serial.clone()));

    dev.start_tracking(Default::default()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        match dev.read_frame(Duration::from_millis(500)) {
            Ok(f) => println!("{}", to_jsonl(&f)),
            Err(e) => eprintln!("read error: {e}"),
        }
    }
    if let Some(s) = dev.stream_stats() {
        eprintln!("# stats: {s:?}");
    }
}

fn to_jsonl(f: &idong::MarkerFrame) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(
        s,
        "{{\"counter\":{},\"mode\":{},\"dropped\":{},\"markers\":[",
        f.frame_counter, f.mode, f.dropped
    );
    for (i, m) in f.markers.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{{\"slot\":{},\"x\":{:.2},\"y\":{:.2},\"z\":{:.2},\"q\":{}}}",
            m.slot, m.x_mm, m.y_mm, m.z_mm, m.quality
        );
    }
    s.push_str("]}");
    s
}
