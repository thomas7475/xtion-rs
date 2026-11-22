//! Enumerate Xtion devices and print identity/calibration info.

use xtion::{list_devices, Xtion};

fn main() -> xtion::Result<()> {
    let devices = list_devices()?;
    if devices.is_empty() {
        println!("no Xtion devices found (look for USB 1d27:0601 / 1d27:0400)");
        return Ok(());
    }
    println!("found {} device(s):", devices.len());
    for d in &devices {
        println!(
            "  bus {:03} device {:03} (pid {:04x})",
            d.bus, d.address, d.pid
        );
    }

    // open the first one and print everything the handshake reads
    let mut cam = Xtion::open()?;
    let (depth_mps, image_mps) = cam.usb_max_packet_sizes();
    println!();
    println!("firmware:       {}", cam.version);
    println!(
        "usb speed:      {:?} (640x480@30 depth needs high speed)",
        cam.usb_speed()
    );
    println!(
        "bulk max packet: depth {} B, image {} B",
        depth_mps, image_mps
    );
    println!("serial number:  {}", cam.serial_number()?);
    println!("depth ConstShift: {}", cam.depth_const_shift());
    println!("calibration:");
    println!(
        "  zero plane distance (ZPD):   {} mm",
        cam.fixed_params.reference_distance
    );
    println!(
        "  zero plane pixel size (ZPPS): {} mm",
        cam.fixed_params.reference_pixel_size
    );
    println!(
        "  emitter-to-dCMOS (LDDIS):    {} cm",
        cam.fixed_params.d_cmos_emitter_distance
    );
    println!(
        "  dCMOS-to-rCMOS:              {} cm",
        cam.fixed_params.d_cmos_r_cmos_distance
    );
    let (w, h) = xtion::Resolution::Vga.dimensions();
    println!(
        "computed horizontal FOV: {:.2} deg @ {}x{}",
        2.0 * (cam.fixed_params.reference_pixel_size as f64 * 1280.0
            / 2.0
            / cam.fixed_params.reference_distance as f64)
            .atan()
            .to_degrees(),
        w,
        h
    );
    Ok(())
}
