//! Capture RAW bytes from the depth stream and decode the packet headers
//! independently of the library's assembler (diagnostics).
//!
//! Run: `cargo run --example raw_dump -- [bytes] [file]`
//! (defaults: 131072 bytes to /tmp/xtion_raw.bin)

use std::time::Duration;

use xtion::{protocol::packet_type, DepthConfig, Xtion};

fn type_name(t: u16) -> &'static str {
    match t {
        packet_type::DEPTH_START => "DEPTH_START",
        packet_type::DEPTH_END => "DEPTH_END",
        packet_type::IMAGE_START => "IMAGE_START",
        packet_type::IMAGE_END => "IMAGE_END",
        packet_type::AUDIO_BUFFER => "AUDIO_BUFFER",
        packet_type::GMC => "GMC",
        packet_type::OVERHEAT => "OVERHEAT",
        _ => "OTHER",
    }
}

fn main() -> xtion::Result<()> {
    let bytes: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(128 * 1024);
    let path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/xtion_raw.bin".into());

    let mut cam = Xtion::open()?;
    println!("firmware {}, usb speed {:?}", cam.version, cam.usb_speed());
    cam.start_depth(DepthConfig::default())?;
    println!("capturing {bytes} raw bytes ...");

    let written =
        cam.tap_depth_stream(std::path::Path::new(&path), bytes, Duration::from_secs(5))?;
    cam.stop_depth()?;
    println!("captured {written} bytes to {path}");

    let data = std::fs::read(&path).map_err(|e| xtion::XtionError::Io(e.to_string()))?;
    let magic = cam.fw_magic();
    println!("scanning for fw magic 0x{magic:04x} ...\n");

    // Standalone packet walk: at each magic, decode the 12-byte header.
    // buf_size is shown BOTH ways: BE (the reference driver's read) and
    // LE (the misread that produced 24560-pixel junk frames in 0.1.3).
    let [m_lo, m_hi] = magic.to_le_bytes();
    let mut packets = 0usize;
    let mut payload_total = 0usize;
    let mut by_type = std::collections::BTreeMap::new();
    let mut i = 0usize;
    let mut truncated = false;
    while i + 12 <= data.len() {
        if data[i] != m_lo || data[i + 1] != m_hi {
            i += 1;
            continue;
        }
        let packet_type = u16::from_le_bytes([data[i + 2], data[i + 3]]);
        let packet_id = u16::from_le_bytes([data[i + 4], data[i + 5]]);
        let buf_be = u16::from_be_bytes([data[i + 6], data[i + 7]]);
        let buf_le = u16::from_le_bytes([data[i + 6], data[i + 7]]);
        let ts = u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]);
        let payload = buf_be.saturating_sub(12) as usize;
        if i + 12 + payload > data.len() {
            truncated = true;
            break;
        }
        let head: Vec<String> = data[i + 12..(i + 12 + payload.min(8))]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if packets < 25 {
            println!(
                "pkt {:>3} off 0x{:06x}: type 0x{:04x} ({:<12}) id {:>3} size {:>4} (BE; LE misread {:>5}) ts {:>10} payload {}",
                packets,
                i,
                packet_type,
                type_name(packet_type),
                packet_id,
                buf_be,
                buf_le,
                ts,
                head.join(" ")
            );
        }
        *by_type
            .entry((packet_type, type_name(packet_type)))
            .or_insert(0usize) += 1;
        payload_total += payload;
        packets += 1;
        i += 12 + payload;
    }
    if truncated {
        println!("... capture ends mid-packet");
    }
    println!("\n{packets} packets walked, {payload_total} payload bytes");
    for ((t, name), n) in &by_type {
        println!("  type 0x{t:04x} {name:<12} x{n}");
    }
    println!(
        "packed-11 pixel estimate from payload: {} (a 640x480 frame is 307200)",
        payload_total * 8 / 11
    );
    Ok(())
}
