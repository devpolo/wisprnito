use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};

pub struct DeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub sample_rates: Vec<u32>,
}

pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    for device in host.input_devices()? {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let sample_rates = get_supported_sample_rates(&device, true);
        let existing = devices.iter_mut().find(|d: &&mut DeviceInfo| d.name == name);
        if let Some(existing) = existing {
            existing.is_input = true;
        } else {
            devices.push(DeviceInfo {
                name,
                is_input: true,
                is_output: false,
                sample_rates,
            });
        }
    }

    for device in host.output_devices()? {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let sample_rates = get_supported_sample_rates(&device, false);
        let existing = devices.iter_mut().find(|d: &&mut DeviceInfo| d.name == name);
        if let Some(existing) = existing {
            existing.is_output = true;
            if existing.sample_rates.is_empty() {
                existing.sample_rates = sample_rates;
            }
        } else {
            devices.push(DeviceInfo {
                name,
                is_input: false,
                is_output: true,
                sample_rates,
            });
        }
    }

    Ok(devices)
}

fn get_supported_sample_rates(device: &cpal::Device, input: bool) -> Vec<u32> {
    let rates_iter: Box<dyn Iterator<Item = cpal::SupportedStreamConfigRange>> = if input {
        match device.supported_input_configs() {
            Ok(it) => Box::new(it),
            Err(_) => return Vec::new(),
        }
    } else {
        match device.supported_output_configs() {
            Ok(it) => Box::new(it),
            Err(_) => return Vec::new(),
        }
    };
    let mut rates: Vec<u32> = rates_iter
        .flat_map(|c| {
            let min = c.min_sample_rate().0;
            let max = c.max_sample_rate().0;
            let common = [8000, 16000, 22050, 44100, 48000, 96000];
            common
                .iter()
                .filter(|&&r| r >= min && r <= max)
                .copied()
                .collect::<Vec<_>>()
        })
        .collect();
    rates.sort();
    rates.dedup();
    rates
}

pub fn find_device(name_fragment: &str, input: bool) -> Result<cpal::Device> {
    let host = cpal::default_host();
    let devices: Box<dyn Iterator<Item = cpal::Device>> = if input {
        Box::new(host.input_devices()?)
    } else {
        Box::new(host.output_devices()?)
    };

    let fragment_lower = name_fragment.to_lowercase();
    for device in devices {
        if let Ok(name) = device.name() {
            if name.to_lowercase().contains(&fragment_lower) {
                return Ok(device);
            }
        }
    }

    Err(anyhow!(
        "No {} device matching '{}'",
        if input { "input" } else { "output" },
        name_fragment
    ))
}

pub fn find_blackhole() -> Result<cpal::Device> {
    find_device("BlackHole", false)
}

pub fn default_input() -> Result<cpal::Device> {
    let host = cpal::default_host();
    host.default_input_device()
        .ok_or_else(|| anyhow!("No default input device found"))
}
