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

    pub fn claude_hook_entries(&self) -> Value {
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
            "\n# pmux hooks (version {HOOKS_VERSION}) - do not edit this block manually\n\
            [[hooks.SessionStart]]\n\
            command = \"curl -sf -X POST '{url}' -H 'Content-Type: application/json' -d '{{\\\"hook_event_name\\\":\\\"SessionStart\\\",\\\"cwd\\\":\\\"$PWD\\\"}}'\" \n\n\
            [[hooks.Stop]]\n\
            command = \"curl -sf -X POST '{url}' -H 'Content-Type: application/json' -d '{{\\\"hook_event_name\\\":\\\"Stop\\\",\\\"cwd\\\":\\\"$PWD\\\"}}'\" \n\
            # end pmux hooks\n",
            HOOKS_VERSION = HOOKS_VERSION,
            url = url
        );
        let existing = if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        if existing.contains("# pmux hooks") {
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
        let existing = if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        if existing.contains(&url) {
            return Ok(false);
        }
        let notifications_cmd = format!(
            "curl -sf -X POST '{}' -H 'Content-Type: application/json' \
            -d '{{\"hook_event_name\":\"aider_waiting\",\"cwd\":\"$(pwd)\"}}' > /dev/null 2>&1",
            url
        );
        let entry = format!(
            "\n# pmux hooks (version {})\nnotifications: true\nnotifications-command: \"{}\"\n",
            HOOKS_VERSION,
            notifications_cmd.replace('"', "\\\"")
        );
        let new_content = existing + &entry;
        std::fs::write(&path, &new_content).map_err(|e| e.to_string())?;
        Ok(true)
    }

    // ── Shared JSON helper ───────────────────────────────────────────────────

    /// Merge pmux hook entries into a settings.json file.
    /// Creates the file if it doesn't exist.
    /// Existing non-pmux hooks are preserved.
    /// Returns true if file was changed.
    pub fn install_http_hooks_json(&self, path: &Path, new_hooks: &Value) -> Result<bool, String> {
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
        assert!(content.contains("\"theme\""));
        assert!(content.contains("http://localhost:7070/webhook"));
    }

    #[test]
    fn test_install_claude_idempotent() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        let inst = installer();
        inst.install_http_hooks_json(&path, &inst.claude_hook_entries()).unwrap();
        let changed = inst.install_http_hooks_json(&path, &inst.claude_hook_entries()).unwrap();
        assert!(!changed, "second install should report no changes");
    }

    #[test]
    fn test_remove_toml_pmux_block() {
        let content = "before\n# pmux hooks\n[[hooks.Stop]]\n# end pmux hooks\nafter\n";
        let result = remove_toml_pmux_block(content);
        assert_eq!(result, "before\nafter\n");
        assert!(!result.contains("pmux hooks"));
    }

    #[test]
    fn test_install_returns_false_for_opencode() {
        let inst = installer();
        let result = inst.install(&ToolKind::Opencode).unwrap();
        assert!(!result, "opencode is manual only");
    }
}
