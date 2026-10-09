use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpscaleConfig {
    #[serde(default = "default_width")]
    pub target_width: u32,

    #[serde(default = "default_height")]
    pub target_height: u32,

    #[serde(default = "default_mode")]
    pub mode: String,

    #[serde(default = "default_filter")]
    pub filter: String,

    #[serde(default = "default_sharpen")]
    pub sharpen: f32,

    #[serde(default = "default_quality")]
    pub quality: u8,
}

fn default_width() -> u32 {
    720
}
fn default_height() -> u32 {
    480
}
fn default_mode() -> String {
    "stretch".to_string()
}
fn default_filter() -> String {
    "lanczos3".to_string()
}
fn default_sharpen() -> f32 {
    0.35
}
fn default_quality() -> u8 {
    95
}

impl Default for UpscaleConfig {
    fn default() -> Self {
        Self {
            target_width: default_width(),
            target_height: default_height(),
            mode: default_mode(),
            filter: default_filter(),
            sharpen: default_sharpen(),
            quality: default_quality(),
        }
    }
}

impl UpscaleConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: UpscaleConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
