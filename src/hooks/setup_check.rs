//! hooks/setup_check.rs - Startup check: detect tools needing hook setup

use crate::hooks::detector::{detect_all, ToolHookStatus};
use crate::hooks::installer::Installer;

pub struct SetupCheckResult {
    pub needs_action: Vec<ToolHookStatus>,
}

impl SetupCheckResult {
    /// Run detection and return tools that need install or update.
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

    /// Summary line for UI banner, e.g. "Claude Code, Aider — hooks not configured"
    pub fn summary(&self) -> String {
        if self.needs_action.is_empty() {
            return "All hooks up to date".to_string();
        }
        let names: Vec<_> = self.needs_action
            .iter()
            .map(|s| s.kind.display_name())
            .collect();
        format!("{} — hooks not configured", names.join(", "))
    }

    /// Install all tools that need setup. Returns list of (tool_name, success).
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
    use crate::hooks::detector::{ToolHookStatus, ToolKind, HOOKS_VERSION};

    fn make_status(kind: ToolKind, installed: bool, configured: bool, version: Option<u32>) -> ToolHookStatus {
        ToolHookStatus { kind, installed, hooks_configured: configured, hooks_version: version }
    }

    #[test]
    fn test_all_good_when_empty() {
        let r = SetupCheckResult { needs_action: vec![] };
        assert!(r.is_all_good());
        assert!(r.summary().contains("up to date"));
    }

    #[test]
    fn test_summary_lists_tool_names() {
        let r = SetupCheckResult {
            needs_action: vec![
                make_status(ToolKind::ClaudeCode, true, false, None),
                make_status(ToolKind::Aider, true, false, None),
            ],
        };
        let s = r.summary();
        assert!(s.contains("Claude Code"));
        assert!(s.contains("Aider"));
        assert!(s.contains("hooks not configured"));
    }

    #[test]
    fn test_not_all_good_when_needs_action() {
        let r = SetupCheckResult {
            needs_action: vec![make_status(ToolKind::ClaudeCode, true, false, None)],
        };
        assert!(!r.is_all_good());
    }
}
