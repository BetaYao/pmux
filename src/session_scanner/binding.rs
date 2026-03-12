//! binding.rs — Manage pane_id ↔ session_id bindings.
//!
//! Each pane in pmux may be bound to a Claude Code session (identified by a JSONL file).
//! A pane always binds to the most recently active session in its project directory.

use std::collections::HashMap;
use std::path::PathBuf;

/// Manages the mapping from pane IDs to their bound Claude Code sessions.
pub struct SessionBinding {
    /// pane_id → bound session info
    bindings: HashMap<String, BoundSession>,
}

/// Information about a bound Claude Code session.
pub struct BoundSession {
    pub session_id: String,
    pub jsonl_path: PathBuf,
    pub project_dir: PathBuf,
}

impl SessionBinding {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Bind or update a pane's session.
    pub fn bind(
        &mut self,
        pane_id: &str,
        session_id: String,
        jsonl_path: PathBuf,
        project_dir: PathBuf,
    ) {
        self.bindings.insert(
            pane_id.to_string(),
            BoundSession {
                session_id,
                jsonl_path,
                project_dir,
            },
        );
    }

    /// Get the bound session for a pane.
    pub fn get(&self, pane_id: &str) -> Option<&BoundSession> {
        self.bindings.get(pane_id)
    }

    /// Remove binding when pane is closed.
    pub fn unbind(&mut self, pane_id: &str) {
        self.bindings.remove(pane_id);
    }

    /// Check if session changed (different JSONL file is now active).
    pub fn session_changed(&self, pane_id: &str, new_session_id: &str) -> bool {
        self.bindings
            .get(pane_id)
            .map(|b| b.session_id != new_session_id)
            .unwrap_or(true)
    }
}

impl Default for SessionBinding {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_and_get() {
        let mut binding = SessionBinding::new();
        binding.bind(
            "pane-1",
            "session-abc".to_string(),
            PathBuf::from("/tmp/abc.jsonl"),
            PathBuf::from("/tmp/project"),
        );

        let bound = binding.get("pane-1").unwrap();
        assert_eq!(bound.session_id, "session-abc");
        assert_eq!(bound.jsonl_path, PathBuf::from("/tmp/abc.jsonl"));
    }

    #[test]
    fn test_get_unbound() {
        let binding = SessionBinding::new();
        assert!(binding.get("pane-1").is_none());
    }

    #[test]
    fn test_unbind() {
        let mut binding = SessionBinding::new();
        binding.bind(
            "pane-1",
            "session-abc".to_string(),
            PathBuf::from("/tmp/abc.jsonl"),
            PathBuf::from("/tmp/project"),
        );
        binding.unbind("pane-1");
        assert!(binding.get("pane-1").is_none());
    }

    #[test]
    fn test_session_changed() {
        let mut binding = SessionBinding::new();
        binding.bind(
            "pane-1",
            "session-abc".to_string(),
            PathBuf::from("/tmp/abc.jsonl"),
            PathBuf::from("/tmp/project"),
        );

        assert!(!binding.session_changed("pane-1", "session-abc"));
        assert!(binding.session_changed("pane-1", "session-def"));
        assert!(binding.session_changed("pane-2", "session-abc")); // unbound pane
    }

    #[test]
    fn test_rebind_updates_session() {
        let mut binding = SessionBinding::new();
        binding.bind(
            "pane-1",
            "session-abc".to_string(),
            PathBuf::from("/tmp/abc.jsonl"),
            PathBuf::from("/tmp/project"),
        );
        binding.bind(
            "pane-1",
            "session-def".to_string(),
            PathBuf::from("/tmp/def.jsonl"),
            PathBuf::from("/tmp/project"),
        );

        let bound = binding.get("pane-1").unwrap();
        assert_eq!(bound.session_id, "session-def");
    }
}
