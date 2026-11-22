//! Full boot: firmware upload + re-enumeration.
//!
//! **SAFETY-GATED.** The upload state machine refuses to run on hardware
//! until `protocol::boot::BOOT_CONFIRMED` is flipped after a USBPcap
//! transcript of the vendor stack byte-confirms the sequence (DRAFT §2.5,
//! milestone M3). Until then this example performs a dry run of the
//! framing against the supplied firmware files only.
//!
//! Run: cargo run --example upload_firmware -- \
//!          --fw-ldr ens_code.ldr --fw-bit A_a_v4.4_BG0313.bit \
//!          --fw-hex app_m8051_2500.hex --i-know-the-upload-is-unconfirmed

use std::time::Duration;

use idong::device::FirmwareSet;
use idong::error::IdongError;
use idong::protocol::boot::{blob_crc, BootUploader, BOOT_CONFIRMED};

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let (mut ldr, mut bit, mut hex, mut confirm) = (None, None, None, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--fw-ldr" => ldr = args.next(),
            "--fw-bit" => bit = args.next(),
            "--fw-hex" => hex = args.next(),
            "--i-know-the-upload-is-unconfirmed" => confirm = true,
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let (ldr, bit, hex) = match (ldr, bit, hex) {
        (Some(l), Some(b), Some(h)) => (l, b, h),
        _ => {
            eprintln!("usage: upload_firmware --fw-ldr F --fw-bit F --fw-hex F [--i-know-the-upload-is-unconfirmed]");
            std::process::exit(2);
        }
    };

    let fw = FirmwareSet::load(&ldr, &bit, &hex).expect("firmware set rejected");
    let blob = std::fs::read(&fw.bf527_ldr).expect("read .ldr");
    println!("BF527 image: {} bytes, {} packets of 64, crc32 {:#010x}",
             blob.len(), blob.len().div_ceil(64), blob_crc(&blob));

    if !BOOT_CONFIRMED {
        println!("\nSAFETY GATE: upload sequence not yet confirmed (BOOT_CONFIRMED=false).");
        if !confirm {
            println!("Dry run only — no device was touched. Re-run with");
            println!("--i-know-the-upload-is-unconfirmed to attempt the real upload");
            println!("anyway (the state machine itself also refuses).");
            return;
        }
    }

    match Transport::open_bootloader() {
        Ok(t) => {
            let mut up = BootUploader::new(&t, &blob);
            match up.run() {
                Ok(info) => {
                    println!("boot OK: {:?}", info);
                    match idong::usb::wait_runtime_reenum(Duration::from_secs(10)) {
                        Ok(_rt) => println!("device re-enumerated as 0471:2500 (runtime)"),
                        Err(e) => eprintln!("no re-enumeration: {e}"),
                    }
                }
                Err(e @ IdongError::Boot { .. }) => {
                    eprintln!("boot refused/failed: {e}");
                }
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Err(e) => eprintln!("no bootloader device: {e}"),
    }
}

use idong::usb::Transport;
