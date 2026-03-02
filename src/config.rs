// config.rs - Configuration management for pmux
use crate::runtime::backends::SessionBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_terminal_row_cache_size() -> usize {
    200
}

fn default_backend() -> String {
    "tmux".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteChannelsConfig {
    #[serde(default)]
    pub discord: DiscordChannelConfig,
    #[serde(default)]
    pub kook: KookChannelConfig,
    #[serde(default)]
    pub feishu: FeishuChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Discord channel ID for sending and receiving (required when enabled)
    #[serde(default)]
    pub channel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KookChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub channel_id: Option<String>,
}

impl Default for DiscordChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channel_id: None,
        }
    }
}

impl Default for KookChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channel_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Feishu group chat ID (receive_id, chat_id)
    #[serde(default)]
    pub chat_id: Option<String>,
}

impl Default for FeishuChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_id: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            terminal_row_cache_size: 200,
            recent_workspace: None,
            workspace_paths: vec![],
            active_workspace_index: 0,
            per_repo_worktree_index: HashMap::new(),
            backend: default_backend(),
            session_backend: SessionBackend::default(),
            remote_channels: RemoteChannelsConfig::default(),
            last_terminal_cols: None,
            last_terminal_rows: None,
        }
    }
}
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Terminal row cache size (LRU). Default 200. Used for scrolling performance.
    #[serde(default = "default_terminal_row_cache_size")]
    pub terminal_row_cache_size: usize,
    /// Legacy single workspace path (for backward compatibility)
    #[serde(default)]
    pub recent_workspace: Option<String>,
    /// Multi-repo workspace paths
    #[serde(default)]
    pub workspace_paths: Vec<String>,
    /// Currently active workspace tab index
    #[serde(default)]
    pub active_workspace_index: usize,
    /// Per-repo worktree index: path string -> worktree index
    #[serde(default)]
    pub per_repo_worktree_index: HashMap<String, usize>,
    /// Runtime backend: "local" (PTY) or "tmux". Env PMUX_BACKEND overrides.
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Session backend: auto/dtach/tmux/screen/local. Auto resolves by availability.
    #[serde(default)]
    pub session_backend: SessionBackend,
    /// Remote notification channels (Discord, KOOK)
    #[serde(default)]
    pub remote_channels: RemoteChannelsConfig,
    /// Last known terminal dimensions (cols, rows) for faster TUI startup
    #[serde(default)]
    pub last_terminal_cols: Option<u16>,
    #[serde(default)]
    pub last_terminal_rows: Option<u16>,
}

impl Config {
    /// Migrate from legacy recent_workspace to workspace_paths if needed.
    /// Call after load for backward compatibility.
    pub fn migrate_from_legacy(&mut self) {
        if self.workspace_paths.is_empty() {
            if let Some(ref path) = self.recent_workspace {
                if !path.is_empty() {
                    self.workspace_paths = vec![path.clone()];
                    self.active_workspace_index = 0;
                }
            }
        }
    }

    /// Load configuration from a specific path
    /// Returns default config if file doesn't exist
    pub fn load_from_path(path: &PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;
        const VALID_BACKENDS: [&str; 5] = ["local", "tmux", "tmux-cc", "dtach", "screen"];
        if !VALID_BACKENDS.contains(&config.backend.as_str()) {
            eprintln!(
                "pmux: invalid backend '{}' in config, using '{}'. Valid: local, tmux, tmux-cc, dtach, screen",
                config.backend,
                default_backend()
            );
            config.backend = default_backend();
        }
        config.migrate_from_legacy();
        Ok(config)
    }

    /// Save configuration to a specific path
    pub fn save_to_path(&self, path: &PathBuf) -> Result<(), ConfigError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the recent workspace as a PathBuf (legacy, prefers first workspace_paths)
    pub fn get_recent_workspace(&self) -> Option<PathBuf> {
        self.workspace_paths
            .first()
            .map(PathBuf::from)
            .or_else(|| self.recent_workspace.as_ref().map(PathBuf::from))
    }

    /// Get all workspace paths as PathBuf
    pub fn get_workspace_paths(&self) -> Vec<PathBuf> {
        self.workspace_paths
            .iter()
            .map(PathBuf::from)
            .collect()
    }

    /// Save a new workspace path (legacy, for single-workspace)
    pub fn save_workspace(&mut self, path: &str) {
        self.recent_workspace = Some(path.to_string());
        if path.is_empty() {
            self.workspace_paths.clear();
            self.active_workspace_index = 0;
            self.per_repo_worktree_index.clear();
        } else if !self.workspace_paths.contains(&path.to_string()) {
            self.workspace_paths = vec![path.to_string()];
            self.active_workspace_index = 0;
        }
    }

    /// Save multi-repo workspace state
    pub fn save_workspaces(
        &mut self,
        paths: &[PathBuf],
        active_index: usize,
        per_repo_worktree_index: &HashMap<PathBuf, usize>,
    ) {
        self.workspace_paths = paths
            .iter()
            .filter_map(|p| p.to_str().map(String::from))
            .collect();
        self.active_workspace_index = active_index.min(paths.len().saturating_sub(1));
        self.per_repo_worktree_index = per_repo_worktree_index
            .iter()
            .filter_map(|(k, v)| k.to_str().map(|s| (s.to_string(), *v)))
            .collect();
        self.recent_workspace = self.workspace_paths.first().cloned();
    }

    /// Get terminal row cache size (default 200)
    pub fn terminal_row_cache_size(&self) -> usize {
        if self.terminal_row_cache_size == 0 {
            200
        } else {
            self.terminal_row_cache_size
        }
    }

    /// Get per-repo worktree index as PathBuf -> usize
    pub fn get_per_repo_worktree_index(&self) -> HashMap<PathBuf, usize> {
        self.per_repo_worktree_index
            .iter()
            .map(|(k, v)| (PathBuf::from(k), *v))
            .collect()
    }

    /// Get the default config path (~/.config/pmux/config.json)
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pmux")
            .join("config.json")
    }

    /// Load from default path
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_path(&Self::default_path())
    }

    /// Save to default path
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to_path(&Self::default_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Test: Config should be readable when file exists
    #[test]
    fn test_config_read_existing_file() {
        // Arrange: Create a temp directory with config file
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write test config
        let test_config = r#"{"recent_workspace": "/path/to/repo"}"#;
        std::fs::write(&config_path, test_config).unwrap();

        // Act: Try to read config
        let config = Config::load_from_path(&config_path);

        // Assert: Should successfully load with correct path
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.recent_workspace, Some("/path/to/repo".to_string()));
    }

    /// Test: Config should return default when file doesn't exist
    #[test]
    fn test_config_load_nonexistent_file() {
        // Arrange: Use a path that doesn't exist
        let nonexistent_path = PathBuf::from("/tmp/nonexistent/config.json");

        // Act: Try to load from non-existent path
        let config = Config::load_from_path(&nonexistent_path);

        // Assert: Should return default config (None for recent_workspace)
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.recent_workspace, None);
    }

    /// Test: Config should save correctly
    #[test]
    fn test_config_save() {
        // Arrange: Create temp directory
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = Config {
            recent_workspace: Some("/home/user/project".to_string()),
            ..Default::default()
        };

        // Act: Save config
        let result = config.save_to_path(&config_path);

        // Assert: Should save successfully
        assert!(result.is_ok());

        // Verify: Read back and check
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("/home/user/project"));
    }

    /// Test: Config should handle invalid JSON gracefully
    #[test]
    fn test_config_load_invalid_json() {
        // Arrange: Create temp file with invalid JSON
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        std::fs::write(&config_path, "not valid json").unwrap();

        // Act: Try to load invalid config
        let result = Config::load_from_path(&config_path);

        // Assert: Should return error
        assert!(result.is_err());
    }

    /// Test: get_recent_workspace should return the saved path
    #[test]
    fn test_get_recent_workspace_returns_saved_path() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = Config {
            recent_workspace: Some("/workspace/myrepo".to_string()),
            ..Default::default()
        };
        config.save_to_path(&config_path).unwrap();

        // Act
        let loaded = Config::load_from_path(&config_path).unwrap();
        let workspace = loaded.get_recent_workspace();

        // Assert
        assert_eq!(workspace, Some(PathBuf::from("/workspace/myrepo")));
    }

    /// Test: save_workspace should update and persist the path
    #[test]
    fn test_save_workspace_updates_config() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = Config {
            recent_workspace: None,
            ..Default::default()
        };

        // Act
        config.save_workspace("/new/workspace/path");
        config.save_to_path(&config_path).unwrap();

        // Assert: Load and verify
        let loaded = Config::load_from_path(&config_path).unwrap();
        assert_eq!(
            loaded.recent_workspace,
            Some("/new/workspace/path".to_string())
        );
    }

    /// Test: Config multi-workspace save and load
    #[test]
    fn test_config_multi_workspace_save_load() {
        use std::collections::HashMap;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = Config::default();
        let paths = vec![
            PathBuf::from("/path/repo1"),
            PathBuf::from("/path/repo2"),
        ];
        let mut per_repo = HashMap::new();
        per_repo.insert(PathBuf::from("/path/repo1"), 1);
        per_repo.insert(PathBuf::from("/path/repo2"), 0);

        config.save_workspaces(&paths, 1, &per_repo);
        config.save_to_path(&config_path).unwrap();

        let loaded = Config::load_from_path(&config_path).unwrap();
        assert_eq!(loaded.workspace_paths.len(), 2);
        assert_eq!(loaded.active_workspace_index, 1);
        let loaded_per_repo = loaded.get_per_repo_worktree_index();
        assert_eq!(loaded_per_repo.get(&PathBuf::from("/path/repo1")), Some(&1));
        assert_eq!(loaded_per_repo.get(&PathBuf::from("/path/repo2")), Some(&0));
    }

    /// Test: Config invalid backend falls back to default (tmux)
    #[test]
    fn test_config_load_invalid_backend_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        std::fs::write(&path, r#"{"backend": "docker"}"#).unwrap();
        let config = Config::load_from_path(&path).unwrap();
        assert_eq!(config.backend, "tmux");
    }

    /// Test: Config backend field is loaded from JSON
    #[test]
    fn test_config_backend_field() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        std::fs::write(&path, r#"{"backend": "tmux"}"#).unwrap();
        let config = Config::load_from_path(&path).unwrap();
        assert_eq!(config.backend, "tmux");
    }

    /// Test: Config accepts tmux-cc as valid backend (alias for tmux control mode)
    #[test]
    fn test_config_backend_tmux_cc_accepted() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        std::fs::write(&path, r#"{"backend": "tmux-cc"}"#).unwrap();
        let config = Config::load_from_path(&path).unwrap();
        assert_eq!(config.backend, "tmux-cc");
    }

    /// Test: remote_channels loads discord.enabled, kook.channel_id, feishu.chat_id from JSON
    #[test]
    fn test_config_remote_channels() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"remote_channels":{"discord":{"enabled":true},"kook":{"enabled":true,"channel_id":"123"},"feishu":{"enabled":true,"chat_id":"oc_abc"}}}"#,
        )
        .unwrap();
        let config = Config::load_from_path(&path).unwrap();
        assert!(config.remote_channels.discord.enabled);
        assert_eq!(config.remote_channels.kook.channel_id.as_deref(), Some("123"));
        assert!(config.remote_channels.feishu.enabled);
        assert_eq!(config.remote_channels.feishu.chat_id.as_deref(), Some("oc_abc"));
    }

    /// Test: migrate_from_legacy populates workspace_paths from recent_workspace
    #[test]
    fn test_config_migrate_from_legacy() {
        let mut config = Config {
            recent_workspace: Some("/home/user/project".to_string()),
            workspace_paths: vec![],
            active_workspace_index: 0,
            per_repo_worktree_index: HashMap::new(),
            ..Default::default()
        };

        config.migrate_from_legacy();

        assert_eq!(config.workspace_paths, vec!["/home/user/project"]);
        assert_eq!(config.active_workspace_index, 0);
    }
}
