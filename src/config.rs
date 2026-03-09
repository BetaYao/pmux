// config.rs - Configuration management for pmux
use crate::runtime::backends::SessionBackend;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_check_interval() -> u64 {
    6
}

fn default_auto_update() -> UpdateConfig {
    UpdateConfig::default()
}

fn default_terminal_row_cache_size() -> usize {
    200
}

fn default_backend() -> String {
    "tmux".to_string()
}

fn default_tui_programs() -> Vec<String> {
    ["claude", "agent", "aider", "cursor"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_idle_str() -> String {
    "Idle".to_string()
}

fn default_agent_detect() -> AgentDetectConfig {
    AgentDetectConfig {
        agents: default_agent_detect_agents(),
    }
}

fn default_agent_detect_agents() -> Vec<AgentDef> {
    vec![
        AgentDef {
            name: "claude".to_string(),
            rules: vec![
                AgentRule {
                    status: "Running".to_string(),
                    patterns: vec!["to interrupt".to_string()],
                },
                AgentRule {
                    status: "Error".to_string(),
                    patterns: vec!["ERROR".to_string(), "error:".to_string()],
                },
                AgentRule {
                    status: "Waiting".to_string(),
                    patterns: vec![
                        "?".to_string(),
                        "(y/n)".to_string(),
                        "(yes/no)".to_string(),
                    ],
                },
            ],
            default_status: "Idle".to_string(),
        },
        AgentDef {
            name: "agent".to_string(),
            rules: vec![
                AgentRule {
                    status: "Running".to_string(),
                    patterns: vec!["to interrupt".to_string()],
                },
                AgentRule {
                    status: "Error".to_string(),
                    patterns: vec!["error".to_string()],
                },
                AgentRule {
                    status: "Waiting".to_string(),
                    patterns: vec!["?".to_string(), "> ".to_string()],
                },
            ],
            default_status: "Idle".to_string(),
        },
        AgentDef {
            name: "aider".to_string(),
            rules: vec![
                AgentRule {
                    status: "Running".to_string(),
                    patterns: vec!["thinking".to_string(), "sending".to_string()],
                },
                AgentRule {
                    status: "Waiting".to_string(),
                    patterns: vec!["> ".to_string()],
                },
            ],
            default_status: "Idle".to_string(),
        },
        AgentDef {
            name: "cursor".to_string(),
            rules: vec![],
            default_status: "Idle".to_string(),
        },
    ]
}

/// Per-CLI agent detection configuration.
/// Each agent has ordered text pattern rules to detect sub-states (Running, Waiting, Error, Idle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDetectConfig {
    #[serde(default = "default_agent_detect_agents")]
    pub agents: Vec<AgentDef>,
}

impl Default for AgentDetectConfig {
    fn default() -> Self {
        default_agent_detect()
    }
}

impl AgentDetectConfig {
    /// Find the AgentDef for a given command name (case-insensitive).
    pub fn find_agent(&self, cmd: &str) -> Option<&AgentDef> {
        let cmd_lower = cmd.to_lowercase();
        self.agents.iter().find(|a| a.name.to_lowercase() == cmd_lower)
    }
}

/// Definition of a single CLI agent and its status detection rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    /// CLI command name (e.g. "claude", "agent", "aider").
    /// Matched case-insensitively against tmux #{pane_current_command}.
    pub name: String,
    /// Ordered list of status rules. First matching rule wins.
    #[serde(default)]
    pub rules: Vec<AgentRule>,
    /// Status when no rule matches. Defaults to "Idle".
    #[serde(default = "default_idle_str")]
    pub default_status: String,
}

impl AgentDef {
    /// Detect status from terminal content by applying rules in order.
    /// First rule where any pattern substring-matches (case-insensitive) wins.
    /// Returns the default_status if no rule matches.
    pub fn detect_status(&self, content: &str) -> crate::agent_status::AgentStatus {
        let content_lower = content.to_lowercase();
        for rule in &self.rules {
            for pattern in &rule.patterns {
                if content_lower.contains(&pattern.to_lowercase()) {
                    return crate::agent_status::AgentStatus::from_status_str(&rule.status);
                }
            }
        }
        crate::agent_status::AgentStatus::from_status_str(&self.default_status)
    }
}

/// A single detection rule: a status to assign when any of the patterns match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRule {
    /// Target status name: "Running", "Waiting", "Error", "Idle"
    pub status: String,
    /// Text patterns. Case-insensitive substring match.
    /// If ANY pattern is found in the terminal content, this rule matches.
    pub patterns: Vec<String>,
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

/// Auto-update preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Whether auto-update checking is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Hours between automatic checks (default: 6)
    #[serde(default = "default_check_interval")]
    pub check_interval_hours: u64,
    /// Version tag the user chose to skip (e.g. "v0.2.0")
    #[serde(default)]
    pub skipped_version: Option<String>,
    /// Unix timestamp of last successful check
    #[serde(default)]
    pub last_check_timestamp: Option<u64>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 6,
            skipped_version: None,
            last_check_timestamp: None,
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
            backend: default_backend(),
            session_backend: SessionBackend::default(),
            remote_channels: RemoteChannelsConfig::default(),
            last_terminal_cols: None,
            last_terminal_rows: None,
            tui_programs: default_tui_programs(),
            agent_detect: default_agent_detect(),
            auto_update: UpdateConfig::default(),
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
    /// TUI programs that don't use alternate screen (e.g. claude, agent).
    /// Deprecated: use agent_detect instead. Kept for backward compatibility.
    #[serde(default = "default_tui_programs")]
    pub tui_programs: Vec<String>,
    /// Per-CLI agent detection: ordered text pattern rules for each CLI.
    #[serde(default = "default_agent_detect")]
    pub agent_detect: AgentDetectConfig,
    /// Auto-update preferences
    #[serde(default = "default_auto_update")]
    pub auto_update: UpdateConfig,
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
        const VALID_BACKENDS: [&str; 6] = ["local", "tmux", "tmux-cc", "dtach", "screen", "shpool"];
        if !VALID_BACKENDS.contains(&config.backend.as_str()) {
            eprintln!(
                "pmux: invalid backend '{}' in config, using '{}'. Valid: local, tmux, tmux-cc, dtach, screen, shpool",
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
        } else if !self.workspace_paths.contains(&path.to_string()) {
            self.workspace_paths = vec![path.to_string()];
            self.active_workspace_index = 0;
        }
    }

    /// Save multi-repo workspace state (paths and active tab index only; worktree selection is by tmux window name, not persisted).
    pub fn save_workspaces(&mut self, paths: &[PathBuf], active_index: usize) {
        self.workspace_paths = paths
            .iter()
            .filter_map(|p| p.to_str().map(String::from))
            .collect();
        self.active_workspace_index = active_index.min(paths.len().saturating_sub(1));
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
        config.save_workspaces(&paths, 1);
        config.save_to_path(&config_path).unwrap();

        let loaded = Config::load_from_path(&config_path).unwrap();
        assert_eq!(loaded.workspace_paths.len(), 2);
        assert_eq!(loaded.active_workspace_index, 1);
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

    /// Test: agent_detect default has 4 agents
    #[test]
    fn test_agent_detect_default_config() {
        let config = Config::default();
        assert_eq!(config.agent_detect.agents.len(), 4);
        assert_eq!(config.agent_detect.agents[0].name, "claude");
        assert_eq!(config.agent_detect.agents[1].name, "agent");
        assert_eq!(config.agent_detect.agents[2].name, "aider");
        assert_eq!(config.agent_detect.agents[3].name, "cursor");
    }

    /// Test: agent_detect finds agent by name (case-insensitive)
    #[test]
    fn test_agent_detect_find_agent() {
        let config = Config::default();
        assert!(config.agent_detect.find_agent("claude").is_some());
        assert!(config.agent_detect.find_agent("Claude").is_some());
        assert!(config.agent_detect.find_agent("CLAUDE").is_some());
        assert!(config.agent_detect.find_agent("unknown-cli").is_none());
    }

    /// Test: detect_status first match wins
    #[test]
    fn test_agent_def_detect_status_first_match_wins() {
        use crate::agent_status::AgentStatus;
        let config = Config::default();
        let claude = config.agent_detect.find_agent("claude").unwrap();

        // "to interrupt" matches Running rule (first rule)
        assert_eq!(
            claude.detect_status("Press Escape twice to interrupt"),
            AgentStatus::Running
        );
    }

    /// Test: detect_status case insensitive
    #[test]
    fn test_agent_def_detect_status_case_insensitive() {
        use crate::agent_status::AgentStatus;
        let config = Config::default();
        let claude = config.agent_detect.find_agent("claude").unwrap();

        assert_eq!(
            claude.detect_status("press escape twice TO INTERRUPT this"),
            AgentStatus::Running
        );
    }

    /// Test: detect_status default when no match
    #[test]
    fn test_agent_def_detect_status_default() {
        use crate::agent_status::AgentStatus;
        let config = Config::default();
        let claude = config.agent_detect.find_agent("claude").unwrap();

        // No pattern matches → default_status = Idle
        assert_eq!(
            claude.detect_status("just some ordinary text"),
            AgentStatus::Idle
        );
    }

    /// Test: agent_detect serialization roundtrip
    #[test]
    fn test_agent_detect_serialization_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = Config::default();
        config.save_to_path(&config_path).unwrap();

        let loaded = Config::load_from_path(&config_path).unwrap();
        assert_eq!(loaded.agent_detect.agents.len(), 4);
        assert_eq!(loaded.agent_detect.agents[0].name, "claude");
        assert_eq!(loaded.agent_detect.agents[0].rules.len(), 3);
        assert_eq!(loaded.agent_detect.agents[0].rules[0].status, "Running");
    }

    /// Test: backward compat — old config without agent_detect loads defaults
    #[test]
    fn test_agent_detect_backward_compat() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.json");
        // Old config without agent_detect field
        std::fs::write(&path, r#"{"backend": "tmux"}"#).unwrap();
        let config = Config::load_from_path(&path).unwrap();
        // Should have default agent_detect
        assert_eq!(config.agent_detect.agents.len(), 4);
    }

    /// Test: migrate_from_legacy populates workspace_paths from recent_workspace
    #[test]
    fn test_config_migrate_from_legacy() {
        let mut config = Config {
            recent_workspace: Some("/home/user/project".to_string()),
            workspace_paths: vec![],
            active_workspace_index: 0,
            ..Default::default()
        };

        config.migrate_from_legacy();

        assert_eq!(config.workspace_paths, vec!["/home/user/project"]);
        assert_eq!(config.active_workspace_index, 0);
    }
}
