use anyhow::{anyhow, Result};
use std::process::Command;

pub fn check_pulse_available() -> bool {
    Command::new("pactl")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn setup_pulse_null_sink() -> Result<()> {
    if !check_pulse_available() {
        return Err(anyhow!("PulseAudio is not available"));
    }

    // Load null sink
    let status = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            "sink_name=wisprnito",
            "sink_properties=device.description=Wisprnito",
        ])
        .status()?;

    if !status.success() {
        return Err(anyhow!("Failed to create null sink"));
    }

    // Load loopback from null sink monitor to default output
    let status = Command::new("pactl")
        .args([
            "load-module",
            "module-loopback",
            "source=wisprnito.monitor",
        ])
        .status()?;

    if !status.success() {
        return Err(anyhow!("Failed to create loopback"));
    }

    println!("PulseAudio null sink 'wisprnito' created with loopback");
    Ok(())
}
