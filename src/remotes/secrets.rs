//! Secrets loading from ~/.config/pmux/secrets.json

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub remote_channels: RemoteChannelSecrets,
}

#[derive(Debug, Default, Deserialize)]
pub struct RemoteChannelSecrets {
    #[serde(default)]
    pub discord: DiscordSecrets,
    #[serde(default)]
    pub kook: KookSecrets,
}

#[derive(Debug, Default, Deserialize)]
pub struct DiscordSecrets {
    pub webhook_url: Option<String>,
    pub bot_token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct KookSecrets {
    pub bot_token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SecretsLoadError {
    #[error("Config directory not found")]
    ConfigDirNotFound,
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl Secrets {
    /// Load secrets from the default path (~/.config/pmux/secrets.json).
    /// Returns default if the file does not exist.
    pub fn load() -> Result<Self, SecretsLoadError> {
        let path = Self::path().ok_or(SecretsLoadError::ConfigDirNotFound)?;
        Self::load_from_path(&path)
    }

    /// Load secrets from a specific path.
    /// Returns default if the file does not exist.
    pub fn load_from_path(path: &PathBuf) -> Result<Self, SecretsLoadError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let secrets: Self = serde_json::from_str(&content)?;
        Ok(secrets)
    }

    /// Returns the default secrets file path, or None if config dir is not available.
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("pmux").join("secrets.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_secrets_load_missing_returns_default() {
        let temp = TempDir::new().unwrap();
        let pmux_dir = temp.path().join("pmux");
        std::fs::create_dir_all(&pmux_dir).unwrap();
        // No secrets.json - load_from_path on non-existent returns default
        let path = pmux_dir.join("secrets.json");
        let s = Secrets::load_from_path(&path).unwrap();
        assert!(s.remote_channels.discord.webhook_url.is_none());
    }

    #[test]
    fn test_secrets_load_from_file() {
        let temp = TempDir::new().unwrap();
        let pmux_dir = temp.path().join("pmux");
        std::fs::create_dir_all(&pmux_dir).unwrap();
        let path = pmux_dir.join("secrets.json");
        std::fs::write(
            &path,
            r#"{"remote_channels":{"discord":{"webhook_url":"https://discord.com/api/webhooks/xxx"}}}"#,
        )
        .unwrap();
        let s = Secrets::load_from_path(&path).unwrap();
        assert_eq!(
            s.remote_channels.discord.webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/xxx")
        );
    }
}
