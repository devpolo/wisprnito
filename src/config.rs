use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub default_input: Option<String>,
    pub default_output: Option<String>,
    pub pitch_semitones: Option<f32>,
    pub formant_ratio: Option<f32>,
    pub phase_jitter: Option<f32>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&data)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, data)?;
        Ok(())
    }

    pub fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".config")
            .join("wisprnito")
            .join("config.json")
    }
}
