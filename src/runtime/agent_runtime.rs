//! agent_runtime.rs - AgentRuntime trait and core types
//!
//! UI 只依赖此 API，不直接调用 tmux。

use std::path::Path;

use crate::agent_status::AgentStatus;
use downcast_rs::DowncastSync;
use thiserror::Error;

pub type AgentId = String;
pub type PaneId = String;

#[derive(Clone, Debug)]
pub struct TerminalEvent {
    pub bytes: Vec<u8>,
    pub pane_id: PaneId,
    pub timestamp: std::time::Instant,
}

#[derive(Clone, Debug)]
pub struct AgentStateChange {
    pub agent_id: AgentId,
    pub state: AgentStatus,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("pane not found: {0}")]
    PaneNotFound(String),
    #[error("session error: {0}")]
    Session(String),
}

/// AgentRuntime trait - UI 通过此 API 操作终端。
pub trait AgentRuntime: Send + Sync + DowncastSync {
    /// Return the backend type identifier (e.g., "local", "tmux").
    fn backend_type(&self) -> &'static str;

    /// Primary pane ID for this runtime (e.g. single pane for local PTY).
    fn primary_pane_id(&self) -> Option<PaneId> {
        self.list_panes(&String::new()).first().cloned()
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError>;
    fn send_key(&self, pane_id: &PaneId, key: &str, use_literal: bool) -> Result<(), RuntimeError>;
    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError>;
    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>>;
    /// Capture existing pane content (with escape sequences) for initial display.
    /// pipe-pane -o only streams new output; this bootstraps the terminal view.
    fn capture_initial_content(&self, pane_id: &PaneId) -> Option<Vec<u8>>;
    fn list_panes(&self, agent_id: &AgentId) -> Vec<PaneId>;
    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError>;
    fn split_pane(&self, pane_id: &PaneId, vertical: bool) -> Result<PaneId, RuntimeError>;
    fn kill_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError>;
    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16);
    fn open_diff(&self, worktree: &Path, pane_id: Option<&PaneId>) -> Result<String, RuntimeError>;
    fn open_review(&self, worktree: &Path) -> Result<String, RuntimeError>;
    fn kill_window(&self, window_target: &str) -> Result<(), RuntimeError>;

    /// Returns (session_id, window_id) for backends that support session persistence.
    /// - tmux: Some((session_name, window_name)) e.g. Some(("pmux-feature-x", "main"))
    /// - local_pty: None (no session to recover)
    fn session_info(&self) -> Option<(String, String)>;

    /// Tell the runtime to skip capture_initial_content on the next subscribe_output call.
    /// Used when pane dimensions don't match — the caller will send C-l instead.
    fn set_skip_initial_capture(&self) {}


    /// Switch to a different window within the same session.
    /// Used when switching worktrees within the same repo — avoids destroying
    /// and recreating the control-mode connection.
    /// Creates the window if it doesn't exist yet.
    fn switch_window(&self, _window_name: &str, _start_dir: Option<&Path>) -> Result<(), RuntimeError> {
        Err(RuntimeError::Backend("switch_window not supported by this backend".into()))
    }
}

downcast_rs::impl_downcast!(sync AgentRuntime);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_error_display() {
        let err = RuntimeError::Backend("test".to_string());
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_terminal_event_creation() {
        let ev = TerminalEvent {
            bytes: vec![1, 2, 3],
            pane_id: "%0".to_string(),
            timestamp: std::time::Instant::now(),
        };
        assert_eq!(ev.pane_id, "%0");
        assert_eq!(ev.bytes.len(), 3);
    }

    #[test]
    fn test_agent_state_change_creation() {
        let ev = AgentStateChange {
            agent_id: "agent-1".to_string(),
            state: AgentStatus::Running,
        };
        assert_eq!(ev.agent_id, "agent-1");
    }
}
