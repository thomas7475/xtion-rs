//! Enumerate every idong sensor on the bus and report its lifecycle state.
//! Read-only — never opens a claim that writes.

fn main() {
    env_logger::init();
    let devices = idong::usb::list_devices();
    if devices.is_empty() {
        println!("no idong sensors found (expected: 0471:2600 bootloader or 0471:2500 runtime)");
        return;
    }
    for d in &devices {
        let state = match d.state {
            idong::usb::DeviceState::Bootloader => {
                "BOOTLOADER (waiting for firmware upload — see upload_firmware)"
            }
            idong::usb::DeviceState::Runtime => {
                "RUNTIME (firmware running; vendor iface MI_04 available)"
            }
            idong::usb::DeviceState::Legacy(pid) => {
                &format!("LEGACY generation (PID {pid:04x}) — identification only")
            }
        };
        println!(
            "bus {:03} addr {:03}  {}  serial={:?}",
            d.bus,
            d.address,
            d.vid_pid(),
            d.serial.as_deref().unwrap_or("(none — bootloader has no strings)")
        );
        println!("    state: {state}");
    }
}
