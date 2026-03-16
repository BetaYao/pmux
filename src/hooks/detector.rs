//! hooks/detector.rs - Detect installed AI coding tools and hook status

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Version of the pmux hooks schema. Bump when hook URLs/events change.
pub const HOOKS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    ClaudeCode,
    GeminiCli,
    Codex,
    Aider,
    Opencode,
}

impl ToolKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::GeminiCli  => "Gemini CLI",
            Self::Codex      => "Codex",
            Self::Aider      => "Aider",
            Self::Opencode   => "opencode",
        }
    }

    pub fn binary_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::GeminiCli  => "gemini",
            Self::Codex      => "codex",
            Self::Aider      => "aider",
            Self::Opencode   => "opencode",
        }
    }

    pub fn all() -> &'static [ToolKind] {
        &[
            ToolKind::ClaudeCode,
            ToolKind::GeminiCli,
            ToolKind::Codex,
            ToolKind::Aider,
            ToolKind::Opencode,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct ToolHookStatus {
    pub kind: ToolKind,
    pub installed: bool,
    pub hooks_configured: bool,
    pub hooks_version: Option<u32>,
}

impl ToolHookStatus {
    pub fn needs_install(&self) -> bool {
        self.installed && !self.hooks_configured
    }

    pub fn needs_update(&self) -> bool {
        self.installed
            && self.hooks_configured
            && self.hooks_version.map_or(true, |v| v < HOOKS_VERSION)
    }

    pub fn is_up_to_date(&self) -> bool {
        self.installed
            && self.hooks_configured
            && self.hooks_version == Some(HOOKS_VERSION)
    }
}

/// Persistent record of which tool hooks are installed and at what version.
/// Stored at ~/.config/pmux/hooks-version.json
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HooksVersionFile {
    #[serde(default)]
    pub claude_code: Option<u32>,
    #[serde(default)]
    pub gemini_cli: Option<u32>,
    #[serde(default)]
    pub codex: Option<u32>,
    #[serde(default)]
    pub aider: Option<u32>,
    #[serde(default)]
    pub opencode: Option<u32>,
}

impl HooksVersionFile {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pmux")
            .join("hooks-version.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() { return Self::default(); }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    pub fn get(&self, kind: &ToolKind) -> Option<u32> {
        match kind {
            ToolKind::ClaudeCode => self.claude_code,
            ToolKind::GeminiCli  => self.gemini_cli,
            ToolKind::Codex      => self.codex,
            ToolKind::Aider      => self.aider,
            ToolKind::Opencode   => self.opencode,
        }
    }

    pub fn set(&mut self, kind: &ToolKind, version: u32) {
        match kind {
            ToolKind::ClaudeCode => self.claude_code = Some(version),
            ToolKind::GeminiCli  => self.gemini_cli  = Some(version),
            ToolKind::Codex      => self.codex        = Some(version),
            ToolKind::Aider      => self.aider        = Some(version),
            ToolKind::Opencode   => self.opencode      = Some(version),
        }
    }
}

/// Check whether a binary exists in the system PATH
pub fn is_in_path(binary: &str) -> bool {
    which::which(binary).is_ok()
}

fn settings_contains_url(path: &PathBuf, webhook_url: &str) -> bool {
    if !path.exists() { return false; }
    std::fs::read_to_string(path)
        .map(|s| s.contains(webhook_url))
        .unwrap_or(false)
}

fn claude_hooks_configured(webhook_url: &str) -> bool {
    let Some(path) = dirs::home_dir().map(|h| h.join(".claude").join("settings.json")) else {
        return false;
    };
    settings_contains_url(&path, webhook_url)
}

fn gemini_hooks_configured(webhook_url: &str) -> bool {
    let Some(path) = dirs::home_dir().map(|h| h.join(".gemini").join("settings.json")) else {
        return false;
    };
    settings_contains_url(&path, webhook_url)
}

fn codex_hooks_configured(webhook_url: &str) -> bool {
    let Some(path) = dirs::home_dir().map(|h| h.join(".codex").join("config.toml")) else {
        return false;
    };
    settings_contains_url(&path, webhook_url)
}

fn aider_hooks_configured(webhook_url: &str) -> bool {
    let Some(path) = dirs::home_dir().map(|h| h.join(".aider.conf.yml")) else {
        return false;
    };
    settings_contains_url(&path, webhook_url)
}

pub fn detect_tool(kind: &ToolKind, webhook_port: u16, version_file: &HooksVersionFile) -> ToolHookStatus {
    let webhook_url = format!("http://localhost:{}/webhook", webhook_port);
    let installed = is_in_path(kind.binary_name());
    let hooks_configured = if !installed {
        false
    } else {
        match kind {
            ToolKind::ClaudeCode => claude_hooks_configured(&webhook_url),
            ToolKind::GeminiCli  => gemini_hooks_configured(&webhook_url),
            ToolKind::Codex      => codex_hooks_configured(&webhook_url),
            ToolKind::Aider      => aider_hooks_configured(&webhook_url),
            ToolKind::Opencode   => false,
        }
    };
    ToolHookStatus {
        kind: kind.clone(),
        installed,
        hooks_configured,
        hooks_version: version_file.get(kind),
    }
}

/// Detect all supported tools. Returns only those that are installed.
pub fn detect_all(webhook_port: u16) -> Vec<ToolHookStatus> {
    let version_file = HooksVersionFile::load();
    ToolKind::all()
        .iter()
        .map(|kind| detect_tool(kind, webhook_port, &version_file))
        .filter(|s| s.installed)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hooks_version_file_roundtrip() {
        let mut vf = HooksVersionFile::default();
        vf.set(&ToolKind::ClaudeCode, 1);
        vf.set(&ToolKind::GeminiCli, 2);
        let json = serde_json::to_string(&vf).unwrap();
        let loaded: HooksVersionFile = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.get(&ToolKind::ClaudeCode), Some(1));
        assert_eq!(loaded.get(&ToolKind::GeminiCli), Some(2));
        assert_eq!(loaded.get(&ToolKind::Aider), None);
    }

    #[test]
    fn test_tool_hook_status_needs_install() {
        let status = ToolHookStatus {
            kind: ToolKind::ClaudeCode,
            installed: true,
            hooks_configured: false,
            hooks_version: None,
        };
        assert!(status.needs_install());
        assert!(!status.needs_update());
        assert!(!status.is_up_to_date());
    }

    #[test]
    fn test_tool_hook_status_needs_update() {
        let status = ToolHookStatus {
            kind: ToolKind::ClaudeCode,
            installed: true,
            hooks_configured: true,
            hooks_version: Some(0),
        };
        assert!(!status.needs_install());
        assert!(status.needs_update());
    }

    #[test]
    fn test_tool_hook_status_up_to_date() {
        let status = ToolHookStatus {
            kind: ToolKind::ClaudeCode,
            installed: true,
            hooks_configured: true,
            hooks_version: Some(HOOKS_VERSION),
        };
        assert!(status.is_up_to_date());
    }

    #[test]
    fn test_detect_all_only_returns_installed() {
        // Only installed tools appear in results
        let statuses = detect_all(7070);
        for s in &statuses {
            assert!(s.installed);
        }
    }

    #[test]
    fn test_tool_kind_all_has_five_entries() {
        assert_eq!(ToolKind::all().len(), 5);
    }
}
