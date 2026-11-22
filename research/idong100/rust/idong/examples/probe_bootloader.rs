//! Read-only probe of a bootloader-state sensor — the Rust port of
//! `idong_probe.py`. Claims the interface and reads EP 0x82 only.
//! **NEVER writes** (safety rule #1: no speculative bytes to EP 0x02).
//!
//! Expected: timeouts in the bootloader (the device waits for the upload
//! sequence). If data ever appears unprompted, the hex dump is protocol
//! gold — capture it.
//!
//! Run: `cargo run --example probe_bootloader -- [count] [size]`

use std::time::Duration;

use idong::{usb::Transport, VID};

fn main() {
    env_logger::init();
    let count: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let size: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(64);

    println!("idong read-only probe — VID {VID:#06x}, EP 0x82 only, NO WRITES");
    let t = match Transport::open_bootloader() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("bootloader device not found/openable: {e}");
            eprintln!("udev rule for unprivileged access (then replug):");
            eprintln!("  SUBSYSTEM==\"usb\", ATTRS{{idVendor}}==\"0471\", ATTRS{{idProduct}}==\"2600\", MODE=\"0660\", TAG+=\"uaccess\"");
            std::process::exit(1);
        }
    };
    println!("claimed vendor interface (state: {:?})", t.state());

    let mut got = 0;
    for i in 0..count {
        match t.read_raw(size, Duration::from_millis(2000)) {
            Ok(data) => {
                got += 1;
                println!("[{i:3}] {} bytes: {}", data.len(), hex(&data));
            }
            Err(e) => println!("[{i:3}] {e}"),
        }
    }
    if got == 0 {
        println!("\nevery read timed out — the bootloader waits for the upload");
        println!("sequence on EP 0x02; that sequence is what the (safety-gated)");
        println!("upload_firmware example implements.");
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x} ")).collect()
}
