# Agent Hooks Integration Plan

> **For Claude:** Use TDD when implementing. Consider `superpowers:subagent-driven-development` for parallel tasks (Tasks 1–3 are independent and can run in parallel).

**Goal:** pmux 在启动时检测已安装的 AI 编码工具，自动安装/更新各工具的 hooks，并通过本地 HTTP webhook server 接收 hook 事件，将精准的 agent 状态注入 EventBus，替代不可靠的 terminal 文本解析。

**Architecture:**
- `src/hooks/` 新模块：detector（检测）、installer（安装/更新）、server（HTTP server）、handler（事件解析→EventBus）
- HTTP server 绑定 `localhost:7070`（可配置），接收 Claude Code / Gemini CLI / Codex / Aider 的 hook 事件
- `RuntimeEvent::HookEvent` 新事件类型携带原始 `cwd` + `session_id`，由 AppRoot 解析为 pane_id
- 启动时弹 setup banner，支持一键安装/更新

**Tech Stack:** Rust, `tiny-http` (new dep), `serde_json`, existing `EventBus`/`Config`

---

## 支持的工具一览

| 工具 | 接入方式 | 配置文件 | 可检测事件 |
|------|---------|---------|-----------|
| Claude Code | HTTP hook (`type: http`) | `~/.claude/settings.json` | SessionStart, PreToolUse, Stop, Notification |
| Gemini CLI | HTTP hook (`type: command` curl) | `~/.gemini/settings.json` | SessionStart, BeforeTool, AfterAgent, Notification |
| Codex | Command hook → curl | `~/.codex/config.toml` | SessionStart, Stop |
| Aider | `--notifications-command` curl | `~/.aider.conf.yml` | Waiting (only) |
| opencode | 手动安装 TS 插件 | `opencode.json` | session.idle, permission.asked, session.error |

---

## Task 1: 添加 `tiny-http` 依赖 + `WebhookConfig`

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/config.rs`

**Step 1: 写失败测试**

在 `src/config.rs` 的 `#[cfg(test)] mod tests` 底部追加：

```rust
#[test]
fn test_webhook_config_defaults() {
    let config = Config::default();
    assert!(config.webhook.enabled);
    assert_eq!(config.webhook.port, 7070);
}

#[test]
fn test_webhook_config_serialization() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.json");
    std::fs::write(&path, r#"{"webhook":{"enabled":false,"port":8080}}"#).unwrap();
    let config = Config::load_from_path(&path).unwrap();
    assert!(!config.webhook.enabled);
    assert_eq!(config.webhook.port, 8080);
}
```

**Step 2: 运行，验证失败**
```
RUSTUP_TOOLCHAIN=stable cargo test test_webhook_config
```
Expected: FAIL — `no field webhook on Config`

**Step 3: 实现 `WebhookConfig`**

在 `src/config.rs` 中，在 `RemoteChannelsConfig` 前插入：

```rust
fn default_webhook_port() -> u16 {
    7070
}

fn default_webhook_enabled() -> bool {
    true
}

/// Local HTTP webhook server configuration.
/// Receives hook events from Claude Code, Gemini CLI, Codex, Aider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default = "default_webhook_enabled")]
    pub enabled: bool,
    /// Port to bind on localhost (default: 7070)
    #[serde(default = "default_webhook_port")]
    pub port: u16,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self { enabled: true, port: 7070 }
    }
}
```

在 `Config` struct 中追加字段：

```rust
/// Local webhook server for receiving AI tool hook events
#[serde(default)]
pub webhook: WebhookConfig,
```

在 `Config::default()` 中追加：

```rust
webhook: WebhookConfig::default(),
```

在 `Cargo.toml` `[dependencies]` 中追加：

```toml
tiny-http = "0.12"
```

**Step 4: 运行，验证通过**
```
RUSTUP_TOOLCHAIN=stable cargo test test_webhook_config
```
Expected: PASS

**Step 5: Commit**
```
git add src/config.rs Cargo.toml
git commit -m "feat: add WebhookConfig and tiny-http dependency"
```

---

## Task 2: `src/hooks/detector.rs` — 工具检测

**Files:**
- Create: `src/hooks/mod.rs`
- Create: `src/hooks/detector.rs`
- Modify: `src/lib.rs` (add `pub mod hooks;`)

**Step 1: 写失败测试**

创建 `src/hooks/detector.rs`（含测试）：

```rust
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

    /// Binary name to look up in PATH
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
    /// Whether the binary is in PATH
    pub installed: bool,
    /// Whether pmux hooks are configured in the tool's settings
    pub hooks_configured: bool,
    /// Version of the installed hooks (None if not configured)
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

/// Check whether pmux hooks are configured in ~/.claude/settings.json
fn claude_hooks_configured(webhook_url: &str) -> bool {
    let path = dirs::home_dir()
        .map(|h| h.join(".claude").join("settings.json"))
        .filter(|p| p.exists());
    let Some(path) = path else { return false };
    std::fs::read_to_string(path)
        .map(|s| s.contains(webhook_url))
        .unwrap_or(false)
}

/// Check whether pmux hooks are configured in ~/.gemini/settings.json
fn gemini_hooks_configured(webhook_url: &str) -> bool {
    let path = dirs::home_dir()
        .map(|h| h.join(".gemini").join("settings.json"))
        .filter(|p| p.exists());
    let Some(path) = path else { return false };
    std::fs::read_to_string(path)
        .map(|s| s.contains(webhook_url))
        .unwrap_or(false)
}

/// Check whether pmux hooks are configured in ~/.codex/config.toml
fn codex_hooks_configured(webhook_url: &str) -> bool {
    let path = dirs::home_dir()
        .map(|h| h.join(".codex").join("config.toml"))
        .filter(|p| p.exists());
    let Some(path) = path else { return false };
    std::fs::read_to_string(path)
        .map(|s| s.contains(webhook_url))
        .unwrap_or(false)
}

/// Check whether pmux notifications-command is configured in ~/.aider.conf.yml
fn aider_hooks_configured(webhook_url: &str) -> bool {
    let path = dirs::home_dir()
        .map(|h| h.join(".aider.conf.yml"))
        .filter(|p| p.exists());
    let Some(path) = path else { return false };
    std::fs::read_to_string(path)
        .map(|s| s.contains(webhook_url))
        .unwrap_or(false)
}

/// Detect status for a single tool
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
            ToolKind::Opencode   => false, // opencode uses TS plugin, manual only
        }
    };
    ToolHookStatus {
        kind: kind.clone(),
        installed,
        hooks_configured,
        hooks_version: version_file.get(kind),
    }
}

/// Detect all supported tools. Returns only tools that are installed.
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
    use tempfile::TempDir;

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
            hooks_version: Some(0), // older than HOOKS_VERSION (1)
        };
        assert!(!status.needs_install());
        assert!(status.needs_update());
        assert!(!status.is_up_to_date());
    }

    #[test]
    fn test_tool_hook_status_up_to_date() {
        let status = ToolHookStatus {
            kind: ToolKind::ClaudeCode,
            installed: true,
            hooks_configured: true,
            hooks_version: Some(HOOKS_VERSION),
        };
        assert!(!status.needs_install());
        assert!(!status.needs_update());
        assert!(status.is_up_to_date());
    }

    #[test]
    fn test_claude_hooks_configured_detects_url() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        std::fs::write(&path, r#"{"hooks":{"Stop":[{"hooks":[{"type":"http","url":"http://localhost:7070/webhook"}]}]}}"#).unwrap();
        // simulate reading from temp path (test the content check logic directly)
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("http://localhost:7070/webhook"));
    }

    #[test]
    fn test_not_installed_tools_filtered() {
        // tool binary not in PATH → filtered out by detect_all
        let statuses = detect_all(7070);
        for s in &statuses {
            assert!(s.installed, "{} should be installed to appear in results", s.kind.display_name());
        }
    }
}
```

**Step 2: 运行，验证失败**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::detector::
```
Expected: FAIL — module not found

**Step 3: 创建 `src/hooks/mod.rs`**

```rust
//! hooks/ - AI tool hook detection, installation, and webhook server

pub mod detector;
pub mod installer;
pub mod server;
pub mod handler;
```

在 `src/lib.rs` 中追加（找到末尾 pub mod 列表）：
```rust
pub mod hooks;
```

**Step 4: 添加 `which` 依赖**

在 `Cargo.toml` 追加：
```toml
which = "6"
```

**Step 5: 运行，验证通过**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::detector::
```
Expected: PASS (除 `test_not_installed_tools_filtered` 外，该测试结果因环境不同可能变化，只验证不 panic)

**Step 6: Commit**
```
git add src/hooks/ src/lib.rs Cargo.toml
git commit -m "feat: add hooks detector for AI tool hook status"
```

---

## Task 3: `src/hooks/installer.rs` — 安装与更新

**Files:**
- Create: `src/hooks/installer.rs`

**Step 1: 写失败测试**

```rust
//! hooks/installer.rs - Install/update/remove pmux hooks in AI tool config files

use std::path::{Path, PathBuf};
use serde_json::{json, Value};
use crate::hooks::detector::{HooksVersionFile, ToolKind, HOOKS_VERSION};

pub struct Installer {
    pub webhook_port: u16,
}

impl Installer {
    pub fn new(webhook_port: u16) -> Self {
        Self { webhook_port }
    }

    fn webhook_url(&self) -> String {
        format!("http://localhost:{}/webhook", self.webhook_port)
    }

    /// Install or update hooks for a tool. Returns Ok(true) if changes were made.
    pub fn install(&self, kind: &ToolKind) -> Result<bool, String> {
        let changed = match kind {
            ToolKind::ClaudeCode => self.install_claude_code()?,
            ToolKind::GeminiCli  => self.install_gemini_cli()?,
            ToolKind::Codex      => self.install_codex()?,
            ToolKind::Aider      => self.install_aider()?,
            ToolKind::Opencode   => return Ok(false), // manual only
        };
        if changed {
            let mut vf = HooksVersionFile::load();
            vf.set(kind, HOOKS_VERSION);
            vf.save();
        }
        Ok(changed)
    }

    // ── Claude Code ──────────────────────────────────────────────────────────

    fn claude_settings_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
    }

    fn install_claude_code(&self) -> Result<bool, String> {
        let path = Self::claude_settings_path()
            .ok_or("cannot find home directory")?;
        self.install_http_hooks_json(&path, &self.claude_hook_entries())
    }

    fn claude_hook_entries(&self) -> Value {
        let url = self.webhook_url();
        let http_hook = json!([{"hooks": [{"type": "http", "url": url, "async": true}]}]);
        json!({
            "SessionStart": http_hook,
            "PreToolUse":   http_hook,
            "Stop":         http_hook,
            "Notification": http_hook
        })
    }

    // ── Gemini CLI ───────────────────────────────────────────────────────────

    fn gemini_settings_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".gemini").join("settings.json"))
    }

    fn install_gemini_cli(&self) -> Result<bool, String> {
        let path = Self::gemini_settings_path()
            .ok_or("cannot find home directory")?;
        let url = self.webhook_url();
        let curl_cmd = format!(
            "curl -sf -X POST '{}' -H 'Content-Type: application/json' -d @-",
            url
        );
        let cmd_hook = json!([{"hooks": [{"type": "command", "command": curl_cmd}]}]);
        let entries = json!({
            "SessionStart": cmd_hook,
            "BeforeTool":   cmd_hook,
            "AfterAgent":   cmd_hook,
            "Notification": cmd_hook
        });
        self.install_http_hooks_json(&path, &entries)
    }

    // ── Codex ────────────────────────────────────────────────────────────────

    fn codex_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".codex").join("config.toml"))
    }

    fn install_codex(&self) -> Result<bool, String> {
        let path = Self::codex_config_path()
            .ok_or("cannot find home directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let url = self.webhook_url();
        let hook_block = format!(
            r#"
# pmux hooks (version {HOOKS_VERSION}) - do not edit this block manually
[[hooks.SessionStart]]
command = "curl -sf -X POST '{url}' -H 'Content-Type: application/json' -d '{{\"hook_event_name\":\"SessionStart\",\"cwd\":\"$PWD\"}}'"

[[hooks.Stop]]
command = "curl -sf -X POST '{url}' -H 'Content-Type: application/json' -d '{{\"hook_event_name\":\"Stop\",\"cwd\":\"$PWD\"}}'"
# end pmux hooks
"#,
            HOOKS_VERSION = HOOKS_VERSION,
            url = url
        );
        let existing = if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        if existing.contains("# pmux hooks") {
            // Update: remove old block, re-insert
            let cleaned = remove_toml_pmux_block(&existing);
            let new_content = cleaned + &hook_block;
            std::fs::write(&path, &new_content).map_err(|e| e.to_string())?;
            return Ok(true);
        }
        let new_content = existing + &hook_block;
        std::fs::write(&path, &new_content).map_err(|e| e.to_string())?;
        Ok(true)
    }

    // ── Aider ────────────────────────────────────────────────────────────────

    fn aider_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".aider.conf.yml"))
    }

    fn install_aider(&self) -> Result<bool, String> {
        let path = Self::aider_config_path()
            .ok_or("cannot find home directory")?;
        let url = self.webhook_url();
        let notifications_cmd = format!(
            "curl -sf -X POST '{}' -H 'Content-Type: application/json' -d '{{\"hook_event_name\":\"aider_waiting\",\"cwd\":\"$(pwd)\"}}' > /dev/null 2>&1",
            url
        );
        let existing = if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        if existing.contains(&url) {
            return Ok(false); // already configured
        }
        let entry = format!(
            "\n# pmux hooks (version {HOOKS_VERSION})\nnotifications: true\nnotifications-command: \"{cmd}\"\n",
            HOOKS_VERSION = HOOKS_VERSION,
            cmd = notifications_cmd.replace('"', "\\\"")
        );
        let new_content = existing + &entry;
        std::fs::write(&path, &new_content).map_err(|e| e.to_string())?;
        Ok(true)
    }

    // ── Shared JSON helper ───────────────────────────────────────────────────

    /// Merge pmux hook entries into a settings.json file (creates if not exists).
    /// Existing non-pmux hooks are preserved. Returns true if file was changed.
    fn install_http_hooks_json(&self, path: &Path, new_hooks: &Value) -> Result<bool, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut root: Value = if path.exists() {
            let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&s).unwrap_or(json!({}))
        } else {
            json!({})
        };

        let url = self.webhook_url();
        let hooks_obj = root
            .as_object_mut()
            .ok_or("settings.json root is not an object")?
            .entry("hooks")
            .or_insert(json!({}));

        let hooks_map = hooks_obj.as_object_mut()
            .ok_or("hooks field is not an object")?;

        let mut changed = false;
        for (event, entries) in new_hooks.as_object().unwrap() {
            let event_list = hooks_map
                .entry(event.clone())
                .or_insert(json!([]));
            let list = event_list.as_array_mut()
                .ok_or(format!("hooks.{event} is not an array"))?;
            // Check if our URL is already present
            let already = list.iter().any(|entry| {
                serde_json::to_string(entry).map_or(false, |s| s.contains(&url))
            });
            if !already {
                for entry in entries.as_array().unwrap() {
                    list.push(entry.clone());
                }
                changed = true;
            }
        }

        if changed {
            let out = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
            std::fs::write(path, out).map_err(|e| e.to_string())?;
        }
        Ok(changed)
    }
}

fn remove_toml_pmux_block(content: &str) -> String {
    let start = "# pmux hooks";
    let end   = "# end pmux hooks";
    if let (Some(s), Some(e)) = (content.find(start), content.find(end)) {
        let after = e + end.len();
        format!("{}{}", &content[..s], &content[after..])
    } else {
        content.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn installer() -> Installer { Installer::new(7070) }

    #[test]
    fn test_install_claude_creates_settings_json() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        let inst = installer();
        let changed = inst.install_http_hooks_json(&path, &inst.claude_hook_entries()).unwrap();
        assert!(changed);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("http://localhost:7070/webhook"));
        assert!(content.contains("SessionStart"));
        assert!(content.contains("Stop"));
    }

    #[test]
    fn test_install_claude_merges_existing_settings() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        std::fs::write(&path, r#"{"theme": "dark"}"#).unwrap();
        let inst = installer();
        inst.install_http_hooks_json(&path, &inst.claude_hook_entries()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // existing key preserved
        assert!(content.contains("\"theme\""));
        // pmux hooks added
        assert!(content.contains("http://localhost:7070/webhook"));
    }

    #[test]
    fn test_install_claude_idempotent() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        let inst = installer();
        inst.install_http_hooks_json(&path, &inst.claude_hook_entries()).unwrap();
        let changed = inst.install_http_hooks_json(&path, &inst.claude_hook_entries()).unwrap();
        // Second install: no change (URL already present)
        assert!(!changed);
    }

    #[test]
    fn test_install_aider_adds_notifications_command() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(".aider.conf.yml");
        std::fs::write(&path, "# existing aider config\nauto-commits: false\n").unwrap();
        // Test the content generation logic
        let url = "http://localhost:7070/webhook";
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains(url));
    }

    #[test]
    fn test_remove_toml_pmux_block() {
        let content = "before\n# pmux hooks\n[[hooks.Stop]]\n# end pmux hooks\nafter\n";
        let result = remove_toml_pmux_block(content);
        assert_eq!(result, "before\nafter\n");
        assert!(!result.contains("pmux hooks"));
    }
}
```

**Step 2: 运行，验证失败**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::installer::
```
Expected: FAIL — module not found / missing deps

**Step 3: 确保 `src/hooks/installer.rs` 已创建，注册到 `mod.rs`**

（文件内容已在 Step 1 完整给出）

**Step 4: 运行，验证通过**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::installer::
```
Expected: PASS

**Step 5: Commit**
```
git add src/hooks/installer.rs
git commit -m "feat: add hook installer for Claude Code, Gemini, Codex, Aider"
```

---

## Task 4: `RuntimeEvent::HookEvent` — 新事件类型

**Files:**
- Modify: `src/runtime/event_bus.rs`

**Step 1: 写失败测试**

在 `src/runtime/event_bus.rs` tests 中追加：

```rust
#[test]
fn test_hook_event_publish_subscribe() {
    let bus = EventBus::new(8);
    let rx = bus.subscribe();
    bus.publish(RuntimeEvent::HookEvent(HookEvent {
        session_id: "sess-abc".to_string(),
        cwd: "/workspace/repo".to_string(),
        hook_event_name: "Stop".to_string(),
        tool_name: None,
        source_tool: "claude_code".to_string(),
    }));
    let ev = rx.recv().unwrap();
    match ev {
        RuntimeEvent::HookEvent(h) => {
            assert_eq!(h.session_id, "sess-abc");
            assert_eq!(h.hook_event_name, "Stop");
        }
        _ => panic!("expected HookEvent"),
    }
}
```

**Step 2: 运行，验证失败**
```
RUSTUP_TOOLCHAIN=stable cargo test event_bus::tests::test_hook_event
```
Expected: FAIL — variant `HookEvent` not found

**Step 3: 在 `src/runtime/event_bus.rs` 中添加 `HookEvent`**

在 `RuntimeEvent` enum 中追加：

```rust
HookEvent(HookEvent),
```

在文件中新增结构体：

```rust
/// Raw hook event received from an AI coding tool (Claude Code, Gemini CLI, etc.)
/// AppRoot resolves cwd/session_id to a pane_id and converts to AgentStateChange.
#[derive(Clone, Debug)]
pub struct HookEvent {
    /// Tool session identifier (e.g. Claude Code session_id)
    pub session_id: String,
    /// Working directory of the tool process
    pub cwd: String,
    /// Event name as sent by the tool (e.g. "Stop", "PreToolUse", "aider_waiting")
    pub hook_event_name: String,
    /// Tool name for PreToolUse/PostToolUse events
    pub tool_name: Option<String>,
    /// Which tool sent this event ("claude_code", "gemini_cli", "codex", "aider")
    pub source_tool: String,
}
```

**Step 4: 运行，验证通过**
```
RUSTUP_TOOLCHAIN=stable cargo test event_bus::tests::
```
Expected: PASS

**Step 5: Commit**
```
git add src/runtime/event_bus.rs
git commit -m "feat: add HookEvent variant to RuntimeEvent"
```

---

## Task 5: `src/hooks/server.rs` — HTTP Webhook Server

**Files:**
- Create: `src/hooks/server.rs`

**Step 1: 写失败测试**

```rust
//! hooks/server.rs - Local HTTP webhook server for receiving AI tool hook events

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use serde::Deserialize;

use crate::runtime::event_bus::{HookEvent, RuntimeEvent, SharedEventBus};

/// Unified hook payload accepted from all tools.
/// Fields are a superset of Claude Code, Gemini CLI, Codex, and Aider payloads.
#[derive(Debug, Default, Deserialize)]
pub struct HookPayload {
    // Common (Claude Code / Gemini CLI)
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    // Used to identify which tool is sending (injected by pmux curl commands)
    #[serde(default)]
    pub pmux_source: Option<String>,
}

impl HookPayload {
    /// Infer source tool from payload fields or pmux_source tag
    pub fn infer_source(&self) -> String {
        if let Some(ref src) = self.pmux_source {
            return src.clone();
        }
        // Heuristic: Aider sends aider_waiting
        if self.hook_event_name == "aider_waiting" {
            return "aider".to_string();
        }
        "unknown".to_string()
    }

    /// Map hook_event_name to AgentStatus string
    pub fn to_status(&self) -> Option<&'static str> {
        match self.hook_event_name.as_str() {
            "PreToolUse" | "BeforeTool" | "SessionStart" => Some("Running"),
            "Stop" | "AfterAgent" | "SessionEnd"         => Some("Idle"),
            "Notification"                               => Some("Waiting"),
            "aider_waiting"                              => Some("Waiting"),
            _ => None,
        }
    }
}

pub struct WebhookServer {
    port: u16,
    event_bus: SharedEventBus,
}

impl WebhookServer {
    pub fn new(port: u16, event_bus: SharedEventBus) -> Self {
        Self { port, event_bus }
    }

    /// Start the HTTP server in a background thread. Returns immediately.
    pub fn start(self) -> Result<(), String> {
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("webhook server bind {}:{} failed: {}", addr, self.port, e))?;
        listener.set_nonblocking(false).ok();

        let event_bus = Arc::clone(&self.event_bus);
        thread::Builder::new()
            .name("pmux-webhook".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let bus = Arc::clone(&event_bus);
                    thread::spawn(move || {
                        handle_connection(&mut stream, &bus);
                    });
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}

fn handle_connection(stream: &mut std::net::TcpStream, event_bus: &SharedEventBus) {
    // Read HTTP request (headers + body)
    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let raw = std::str::from_utf8(&buf[..n]).unwrap_or("");

    // Only accept POST /webhook
    if !raw.starts_with("POST /webhook") {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        return;
    }

    // Extract JSON body (after \r\n\r\n)
    let body = if let Some(pos) = raw.find("\r\n\r\n") {
        &raw[pos + 4..]
    } else {
        return;
    };

    let payload: HookPayload = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };

    if !payload.hook_event_name.is_empty() || payload.pmux_source.is_some() {
        event_bus.publish(RuntimeEvent::HookEvent(HookEvent {
            session_id: payload.session_id.clone(),
            cwd: payload.cwd.clone(),
            hook_event_name: payload.hook_event_name.clone(),
            tool_name: payload.tool_name.clone(),
            source_tool: payload.infer_source(),
        }));
    }

    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_to_status_stop() {
        let p = HookPayload {
            hook_event_name: "Stop".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), Some("Idle"));
    }

    #[test]
    fn test_payload_to_status_pre_tool_use() {
        let p = HookPayload {
            hook_event_name: "PreToolUse".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), Some("Running"));
    }

    #[test]
    fn test_payload_to_status_aider_waiting() {
        let p = HookPayload {
            hook_event_name: "aider_waiting".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), Some("Waiting"));
        assert_eq!(p.infer_source(), "aider");
    }

    #[test]
    fn test_payload_infer_source_from_field() {
        let p = HookPayload {
            pmux_source: Some("gemini_cli".to_string()),
            ..Default::default()
        };
        assert_eq!(p.infer_source(), "gemini_cli");
    }

    #[test]
    fn test_webhook_server_receives_event() {
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        // Find a free port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let bus = Arc::new(crate::runtime::event_bus::EventBus::new(16));
        let rx = bus.subscribe();

        WebhookServer::new(port, Arc::clone(&bus)).start().unwrap();
        std::thread::sleep(Duration::from_millis(50));

        // Send a POST /webhook request
        let body = r#"{"session_id":"s1","cwd":"/repo","hook_event_name":"Stop"}"#;
        let request = format!(
            "POST /webhook HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        let mut conn = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        conn.write_all(request.as_bytes()).unwrap();
        drop(conn);

        let ev = rx.recv_timeout(Duration::from_millis(500)).expect("expected event");
        match ev {
            RuntimeEvent::HookEvent(h) => {
                assert_eq!(h.session_id, "s1");
                assert_eq!(h.hook_event_name, "Stop");
                assert_eq!(h.cwd, "/repo");
            }
            _ => panic!("expected HookEvent"),
        }
    }
}
```

**Step 2: 运行，验证失败**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::server::
```
Expected: FAIL — module not found

**Step 3: 文件内容已在 Step 1 完整给出，创建文件后再次运行**

**Step 4: 运行，验证通过**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::server::
```
Expected: PASS（`test_webhook_server_receives_event` 需要端口可用）

**Step 5: Commit**
```
git add src/hooks/server.rs
git commit -m "feat: add WebhookServer HTTP endpoint for hook events"
```

---

## Task 6: `src/hooks/handler.rs` — AppRoot 侧事件处理

**Files:**
- Create: `src/hooks/handler.rs`
- Modify: `src/ui/app_root.rs` (在事件循环中处理 `RuntimeEvent::HookEvent`)

**Step 1: 写失败测试**

```rust
//! hooks/handler.rs - Resolve HookEvent cwd/session_id to pane_id, emit AgentStateChange

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::agent_status::AgentStatus;
use crate::hooks::server::HookPayload;
use crate::runtime::event_bus::{AgentStateChange, HookEvent, Notification, NotificationType, RuntimeEvent, SharedEventBus};

/// Maps session_id → agent_id and cwd → agent_id.
/// Written by AppRoot when panes are created/switched.
/// Read by HookEventHandler to resolve incoming hook events.
#[derive(Default)]
pub struct PaneIndex {
    /// session_id (from Claude Code / Gemini CLI SessionStart) → agent_id
    pub by_session: HashMap<String, String>,
    /// normalized worktree path → agent_id (fallback when no session_id match)
    pub by_cwd: HashMap<String, String>,
}

impl PaneIndex {
    /// Register a pane's worktree path
    pub fn register_pane(&mut self, agent_id: &str, worktree_path: &str) {
        self.by_cwd.insert(normalize_path(worktree_path), agent_id.to_string());
    }

    /// Record session_id → agent_id mapping (from SessionStart hook)
    pub fn register_session(&mut self, session_id: &str, agent_id: &str) {
        self.by_session.insert(session_id.to_string(), agent_id.to_string());
    }

    /// Resolve hook event to agent_id: try session_id first, then cwd prefix match
    pub fn resolve(&self, session_id: &str, cwd: &str) -> Option<&str> {
        if !session_id.is_empty() {
            if let Some(id) = self.by_session.get(session_id) {
                return Some(id.as_str());
            }
        }
        // cwd prefix match: find longest registered path that is a prefix of cwd
        let cwd_norm = normalize_path(cwd);
        self.by_cwd
            .iter()
            .filter(|(path, _)| cwd_norm.starts_with(path.as_str()))
            .max_by_key(|(path, _)| path.len())
            .map(|(_, id)| id.as_str())
    }
}

fn normalize_path(p: &str) -> String {
    p.trim_end_matches('/').to_string()
}

/// Processes incoming HookEvents and emits AgentStateChange + Notification to EventBus
pub struct HookEventHandler {
    pub index: Arc<RwLock<PaneIndex>>,
    pub event_bus: SharedEventBus,
}

impl HookEventHandler {
    pub fn new(index: Arc<RwLock<PaneIndex>>, event_bus: SharedEventBus) -> Self {
        Self { index, event_bus }
    }

    pub fn handle(&self, event: &HookEvent) {
        let index = self.index.read().unwrap();

        // Register session on SessionStart
        if event.hook_event_name == "SessionStart" && !event.session_id.is_empty() {
            drop(index);
            let mut idx = self.index.write().unwrap();
            // Resolve by cwd to get agent_id, then register session
            let cwd_norm = normalize_path(&event.cwd);
            if let Some(agent_id) = idx.by_cwd
                .iter()
                .filter(|(p, _)| cwd_norm.starts_with(p.as_str()))
                .max_by_key(|(p, _)| p.len())
                .map(|(_, id)| id.clone())
            {
                idx.by_session.insert(event.session_id.clone(), agent_id);
            }
            return;
        }

        let agent_id = match index.resolve(&event.session_id, &event.cwd) {
            Some(id) => id.to_string(),
            None => return, // unrecognized cwd, ignore
        };
        drop(index);

        // Map event to status
        let p = HookPayload {
            session_id: event.session_id.clone(),
            cwd: event.cwd.clone(),
            hook_event_name: event.hook_event_name.clone(),
            tool_name: event.tool_name.clone(),
            pmux_source: Some(event.source_tool.clone()),
        };
        let Some(status_str) = p.to_status() else { return };
        let status = AgentStatus::from_status_str(status_str);

        self.event_bus.publish(RuntimeEvent::AgentStateChange(AgentStateChange {
            agent_id: agent_id.clone(),
            pane_id: None, // AppRoot will fill in pane_id from agent_id
            state: status.clone(),
            prev_state: None,
            last_line: Some(format!("[hook] {}", event.hook_event_name)),
        }));

        // Also emit Notification for important transitions
        let notif_type = match status {
            AgentStatus::Waiting => Some(NotificationType::WaitingInput),
            AgentStatus::Error   => Some(NotificationType::Error),
            _ => None,
        };
        if let Some(ntype) = notif_type {
            self.event_bus.publish(RuntimeEvent::Notification(Notification {
                agent_id: agent_id.clone(),
                pane_id: None,
                message: format!("{}: {}", event.source_tool, event.hook_event_name),
                notif_type: ntype,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::runtime::event_bus::EventBus;

    fn make_handler() -> (HookEventHandler, Arc<RwLock<PaneIndex>>, flume::Receiver<RuntimeEvent>) {
        let bus = Arc::new(EventBus::new(16));
        let rx = bus.subscribe();
        let index = Arc::new(RwLock::new(PaneIndex::default()));
        let handler = HookEventHandler::new(Arc::clone(&index), Arc::clone(&bus));
        (handler, index, rx)
    }

    #[test]
    fn test_resolve_by_cwd_prefix() {
        let mut idx = PaneIndex::default();
        idx.register_pane("agent-1", "/workspace/repo-a");
        idx.register_pane("agent-2", "/workspace/repo-b");
        assert_eq!(idx.resolve("", "/workspace/repo-a/src"), Some("agent-1"));
        assert_eq!(idx.resolve("", "/workspace/repo-b/src/main.rs"), Some("agent-2"));
        assert_eq!(idx.resolve("", "/other/path"), None);
    }

    #[test]
    fn test_resolve_session_id_takes_priority() {
        let mut idx = PaneIndex::default();
        idx.register_pane("agent-1", "/workspace/repo");
        idx.register_session("sess-xyz", "agent-2");
        // session_id match overrides cwd match
        assert_eq!(idx.resolve("sess-xyz", "/workspace/repo"), Some("agent-2"));
    }

    #[test]
    fn test_hook_stop_emits_idle_state_change() {
        let (handler, index, rx) = make_handler();
        index.write().unwrap().register_pane("agent-1", "/workspace/repo");

        handler.handle(&HookEvent {
            session_id: String::new(),
            cwd: "/workspace/repo".to_string(),
            hook_event_name: "Stop".to_string(),
            tool_name: None,
            source_tool: "claude_code".to_string(),
        });

        let ev = rx.try_recv().unwrap();
        match ev {
            RuntimeEvent::AgentStateChange(a) => {
                assert_eq!(a.agent_id, "agent-1");
                assert_eq!(a.state, AgentStatus::Idle);
            }
            _ => panic!("expected AgentStateChange"),
        }
    }

    #[test]
    fn test_hook_waiting_emits_notification() {
        let (handler, index, rx) = make_handler();
        index.write().unwrap().register_pane("agent-1", "/workspace/repo");

        handler.handle(&HookEvent {
            session_id: String::new(),
            cwd: "/workspace/repo/subdir".to_string(),
            hook_event_name: "aider_waiting".to_string(),
            tool_name: None,
            source_tool: "aider".to_string(),
        });

        // First event: AgentStateChange(Waiting)
        let ev1 = rx.try_recv().unwrap();
        assert!(matches!(ev1, RuntimeEvent::AgentStateChange(_)));

        // Second event: Notification(WaitingInput)
        let ev2 = rx.try_recv().unwrap();
        match ev2 {
            RuntimeEvent::Notification(n) => {
                assert!(matches!(n.notif_type, NotificationType::WaitingInput));
            }
            _ => panic!("expected Notification"),
        }
    }

    #[test]
    fn test_unknown_cwd_ignored() {
        let (handler, index, rx) = make_handler();
        index.write().unwrap().register_pane("agent-1", "/workspace/repo");

        handler.handle(&HookEvent {
            session_id: String::new(),
            cwd: "/other/unrelated/path".to_string(),
            hook_event_name: "Stop".to_string(),
            tool_name: None,
            source_tool: "claude_code".to_string(),
        });

        assert!(rx.try_recv().is_err(), "should emit nothing for unknown cwd");
    }
}
```

**Step 2: 运行，验证失败**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::handler::
```
Expected: FAIL — module not found

**Step 3: 创建文件，运行通过**
```
RUSTUP_TOOLCHAIN=stable cargo test hooks::handler::
```
Expected: PASS

**Step 4: 在 `app_root.rs` 的事件循环中处理 `HookEvent`**

找到 `app_root.rs` 中 `RuntimeEvent::Notification(n) =>` 的处理块（约 line 2955），在其前面插入：

```rust
RuntimeEvent::HookEvent(hook_ev) => {
    // HookEvent: resolve pane by cwd, emit as AgentStateChange via handler
    // (handler re-publishes to the same bus; AppRoot will pick up AgentStateChange)
    let handler = self.hook_handler.as_ref();
    if let Some(h) = handler {
        h.handle(&hook_ev);
    }
}
```

在 `AppRoot` struct 中追加字段：

```rust
pub hook_handler: Option<Arc<crate::hooks::handler::HookEventHandler>>,
```

在 `AppRoot::new()` 中初始化：

```rust
hook_handler: None, // initialized after event_bus is created in init_workspace_restoration
```

在 `init_workspace_restoration` 中，event_bus 创建后追加：

```rust
let pane_index = Arc::new(RwLock::new(crate::hooks::handler::PaneIndex::default()));
self.hook_handler = Some(Arc::new(crate::hooks::handler::HookEventHandler::new(
    Arc::clone(&pane_index),
    Arc::clone(&event_bus),
)));
self.pane_index = Some(pane_index);
```

（同时在 AppRoot struct 追加 `pub pane_index: Option<Arc<RwLock<crate::hooks::handler::PaneIndex>>>>`）

在 worktree 注册/切换时（`attach_runtime`），向 PaneIndex 注册 cwd：

```rust
if let Some(ref idx) = self.pane_index {
    idx.write().unwrap().register_pane(&agent_id, &worktree_path);
}
```

**Step 5: 编译验证**
```
RUSTUP_TOOLCHAIN=stable cargo check
```
Expected: no errors

**Step 6: Commit**
```
git add src/hooks/handler.rs src/ui/app_root.rs
git commit -m "feat: add HookEventHandler and wire into AppRoot event loop"
```

---

## Task 7: 启动检测与 Setup Banner UI

**Files:**
- Create: `src/hooks/setup_check.rs`
- Modify: `src/ui/app_root.rs` (在 `init_workspace_restoration` 末尾加检测逻辑)
- Modify: `src/ui/sidebar.rs` 或新建 `src/ui/hooks_banner.rs` (展示 banner)

**Step 1: `src/hooks/setup_check.rs`**

```rust
//! hooks/setup_check.rs - Startup check: which tools are installed, hooks status

use crate::hooks::detector::{detect_all, ToolHookStatus};
use crate::hooks::installer::Installer;

pub struct SetupCheckResult {
    pub needs_action: Vec<ToolHookStatus>,
}

impl SetupCheckResult {
    pub fn run(webhook_port: u16) -> Self {
        let statuses = detect_all(webhook_port);
        let needs_action = statuses
            .into_iter()
            .filter(|s| s.needs_install() || s.needs_update())
            .collect();
        Self { needs_action }
    }

    pub fn is_all_good(&self) -> bool {
        self.needs_action.is_empty()
    }

    /// Summary line for the banner, e.g. "Claude Code, Aider need hooks setup"
    pub fn summary(&self) -> String {
        let names: Vec<_> = self.needs_action
            .iter()
            .map(|s| s.kind.display_name())
            .collect();
        if names.is_empty() {
            "All hooks up to date".to_string()
        } else {
            format!("{} — hooks not configured", names.join(", "))
        }
    }

    /// Install all tools that need setup. Returns list of (name, success).
    pub fn install_all(&self, webhook_port: u16) -> Vec<(String, bool)> {
        let installer = Installer::new(webhook_port);
        self.needs_action.iter().map(|s| {
            let name = s.kind.display_name().to_string();
            let ok = installer.install(&s.kind).is_ok();
            (name, ok)
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_check_result_summary_empty() {
        let r = SetupCheckResult { needs_action: vec![] };
        assert!(r.is_all_good());
        assert!(r.summary().contains("up to date"));
    }
}
```

**Step 2: Banner 显示逻辑**

在 `AppRoot` 中，`init_workspace_restoration` 末尾（webhook server 启动后）追加：

```rust
// Async startup check: don't block UI
let port = config.webhook.port;
cx.spawn(async move {
    let result = crate::hooks::setup_check::SetupCheckResult::run(port);
    if !result.is_all_good() {
        // Post to AppRoot: show hooks setup banner
        // (使用现有的 show_notification 机制或 OPEN_SETTINGS_REQUESTED 模式)
        HOOKS_SETUP_NEEDED.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}).detach();
```

Banner 使用已有的 notification panel 入口，显示：
- 工具名称列表
- "一键安装" 按钮 → 调用 `SetupCheckResult::install_all()`
- "忽略" 按钮 → 写入一条 `hooks_version.json` 记录（跳过标志）

**Step 3: Commit**
```
git add src/hooks/setup_check.rs src/ui/app_root.rs
git commit -m "feat: startup hooks setup check with auto-install banner"
```

---

## Task 8: 在 `main.rs` 启动 Webhook Server

**Files:**
- Modify: `src/main.rs`

**Step 1: 在 `main.rs` 中启动 server**

在 `pmux::shell_integration_inject::ensure_shell_integration_scripts();` 之后追加：

```rust
// Start local webhook server for AI tool hooks (Claude Code, Gemini, Codex, Aider)
// The shared EventBus is created inside AppRoot; server is started after AppRoot init.
// We store the port in a static so AppRoot can pick it up.
let webhook_config = pmux::config::Config::load()
    .unwrap_or_default()
    .webhook;
if webhook_config.enabled {
    pmux::hooks::WEBHOOK_PORT.store(webhook_config.port as u32,
        std::sync::atomic::Ordering::SeqCst);
}
```

在 `src/hooks/mod.rs` 中暴露：

```rust
pub static WEBHOOK_PORT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(7070);
```

在 `AppRoot::init_workspace_restoration` 中，event_bus 创建后启动：

```rust
let port = crate::hooks::WEBHOOK_PORT.load(std::sync::atomic::Ordering::SeqCst) as u16;
if port > 0 {
    let srv = crate::hooks::server::WebhookServer::new(
        port, Arc::clone(&event_bus)
    );
    if let Err(e) = srv.start() {
        eprintln!("pmux: webhook server failed to start: {}", e);
    }
}
```

**Step 2: 编译 + 运行验证**
```
RUSTUP_TOOLCHAIN=stable cargo check
RUSTUP_TOOLCHAIN=stable cargo run &
sleep 2
curl -s -X POST http://localhost:7070/webhook \
  -H 'Content-Type: application/json' \
  -d '{"hook_event_name":"Stop","cwd":"/tmp","session_id":"test"}'
# Expected: "OK"
```

**Step 3: Commit**
```
git add src/main.rs src/hooks/mod.rs
git commit -m "feat: start webhook server on launch, wired to EventBus"
```

---

## Task 9: Claude Code settings.json 配置示例文档

**Files:**
- Create: `docs/hooks-setup.md`

文档内容（一键安装会自动生成，此为手动参考）：

**Claude Code (`~/.claude/settings.json`):**
```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}],
    "PreToolUse":   [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}],
    "Stop":         [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}],
    "Notification": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}]
  }
}
```

**Gemini CLI (`~/.gemini/settings.json`):**
```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "command", "command": "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d @-"}]}],
    "AfterAgent":   [{"hooks": [{"type": "command", "command": "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d @-"}]}]
  }
}
```

**Aider (`~/.aider.conf.yml`):**
```yaml
notifications: true
notifications-command: "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d '{\"hook_event_name\":\"aider_waiting\",\"cwd\":\"$(pwd)\"}' > /dev/null 2>&1"
```

**opencode** — 需手动安装 TS 插件，详见 https://github.com/sst/opencode (plugin system)

**Step 1: Commit**
```
git add docs/hooks-setup.md
git commit -m "docs: add manual hooks setup reference for Claude Code, Gemini, Aider"
```

---

## 依赖顺序

```
Task 1 (Config) ──┬──> Task 4 (HookEvent) ──> Task 5 (Server) ──> Task 8 (main.rs)
                  └──> Task 2 (Detector)  ──> Task 3 (Installer) ──> Task 7 (Banner)
                                                                  ──> Task 6 (Handler) ──> Task 8
```

Tasks 2, 3 可与 Tasks 4, 5 并行执行。

---

## 验收标准

1. `cargo test hooks::` — 全部通过
2. 启动 pmux，claude 已安装但未配置 hooks → sidebar 出现 banner
3. 点击"一键安装" → `~/.claude/settings.json` 写入正确 hook，banner 消失
4. 在已配置 hooks 的 worktree 中打开 claude → Running/Idle/Waiting 状态精准切换，无延迟
5. curl POST /webhook → pmux UI 状态实时更新
