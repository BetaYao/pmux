//! Backend adapters implementing AgentRuntime.
//!
//! Local PTY only - no tmux.

mod local_pty;

pub use local_pty::LocalPtyRuntime;

use std::path::Path;
use std::sync::Arc;

use crate::runtime::agent_runtime::{AgentRuntime, RuntimeError};
use crate::runtime::WorktreeState;

/// Create a LocalPtyRuntime for the given worktree path.
pub fn create_runtime(
    worktree_path: &Path,
    cols: u16,
    rows: u16,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    let rt = LocalPtyRuntime::new(worktree_path, cols, rows)?;
    Ok(Arc::new(rt))
}

/// Recover an AgentRuntime from persisted state.
/// Used when pmux restarts and needs to attach to existing sessions.
pub fn recover_runtime(
    backend: &str,
    _state: &WorktreeState,
    _event_bus: Option<Arc<crate::runtime::EventBus>>,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    match backend {
        "local" | "local_pty" => Err(RuntimeError::Backend(
            "local_pty does not support session recovery".into(),
        )),
        _ => Err(RuntimeError::Backend(format!(
            "unknown backend: {}",
            backend
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::WorktreeState;
    use std::path::PathBuf;

    #[test]
    fn test_recover_runtime_unknown_backend() {
        let state = WorktreeState {
            path: PathBuf::from("/tmp/test"),
            branch: "main".to_string(),
            agent_id: "test".to_string(),
            pane_ids: vec![],
            backend: "unknown".to_string(),
            backend_session_id: String::new(),
            backend_window_id: String::new(),
        };
        let result = recover_runtime("unknown_backend", &state, None);
        assert!(result.is_err());
    }
}
