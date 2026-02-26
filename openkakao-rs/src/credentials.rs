use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::model::KakaoCredentials;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn credentials_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    Ok(home
        .join(".config")
        .join("openkakao")
        .join("credentials.json"))
}

pub fn load_credentials() -> Result<Option<KakaoCredentials>> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let creds: KakaoCredentials = serde_json::from_str(&data)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    Ok(Some(creds))
}

pub fn save_credentials(creds: &KakaoCredentials) -> Result<PathBuf> {
    let path = credentials_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let data = serde_json::to_string_pretty(creds).context("Failed to serialize credentials")?;
    let mut file = fs::File::create(&path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    file.write_all(data.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;

    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set permissions on {}", path.display()))?;

    Ok(path)
}
