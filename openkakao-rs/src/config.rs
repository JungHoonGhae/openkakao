use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenKakaoConfig {
    #[serde(default)]
    pub mode: ModeConfig,
    #[serde(default)]
    pub send: SendConfig,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModeConfig {
    #[serde(default)]
    pub unattended: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SendConfig {
    #[serde(default)]
    pub allow_non_interactive: bool,
    pub default_prefix: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WatchConfig {
    #[serde(default)]
    pub allow_side_effects: bool,
    pub default_max_reconnect: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    pub prefer_relogin: Option<bool>,
    pub auto_renew: Option<bool>,
}

pub fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    Ok(home.join(".config").join("openkakao").join("config.toml"))
}

pub fn load_config() -> Result<OpenKakaoConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(OpenKakaoConfig::default());
    }

    let data =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let config: OpenKakaoConfig = toml::from_str(&data)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_safe() {
        let config = OpenKakaoConfig::default();
        assert!(!config.mode.unattended);
        assert!(!config.send.allow_non_interactive);
        assert!(!config.watch.allow_side_effects);
    }
}
