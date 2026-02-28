//! Backend adapters implementing AgentRuntime.
//!
//! Supports both Local PTY (direct shell spawn) and Tmux (session persistence).

mod local_pty;
#[cfg(unix)]
mod tmux;

pub use local_pty::LocalPtyRuntime;
#[cfg(unix)]
pub use tmux::TmuxRuntime;

use std::path::Path;
use std::sync::Arc;

use crate::runtime::agent_runtime::{AgentRuntime, RuntimeError};
use crate::runtime::WorktreeState;

/// Environment variable to select backend. Valid values: "local", "tmux".
pub const PMUX_BACKEND_ENV: &str = "PMUX_BACKEND";

/// Default backend when environment variable is not set.
pub const DEFAULT_BACKEND: &str = "local";

/// Create a runtime for the given worktree path, using the backend specified
/// by the `PMUX_BACKEND` environment variable (defaults to local PTY).
///
/// # Examples
/// ```bash
/// # Use local PTY (default)
/// pmux
///
/// # Use tmux for session persistence
/// PMUX_BACKEND=tmux pmux
/// ```
pub fn create_runtime_from_env(
    worktree_path: &Path,
    cols: u16,
    rows: u16,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    let backend = std::env::var(PMUX_BACKEND_ENV).unwrap_or_else(|_| DEFAULT_BACKEND.to_string());

    match backend.as_str() {
        "tmux" => {
            #[cfg(unix)]
            {
                let session_name = format!("pmux-{}", worktree_path.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_else(|| "default".into()));
                Ok(create_tmux_runtime(session_name, "main"))
            }
            #[cfg(not(unix))]
            Err(RuntimeError::Backend(
                "tmux backend not supported on non-Unix platforms".into(),
            ))
        }
        "local" | _ => create_runtime(worktree_path, cols, rows),
    }
}

/// Create a LocalPtyRuntime for the given worktree path.
pub fn create_runtime(
    worktree_path: &Path,
    cols: u16,
    rows: u16,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    let rt = LocalPtyRuntime::new(worktree_path, cols, rows)?;
    Ok(Arc::new(rt))
}

/// Create a TmuxRuntime for the given session and window.
/// Session persistence allows agents to continue running after pmux closes.
#[cfg(unix)]
pub fn create_tmux_runtime(
    session_name: impl Into<String>,
    window_name: impl Into<String>,
) -> Arc<dyn AgentRuntime> {
    let rt = TmuxRuntime::new(session_name, window_name);
    Arc::new(rt)
}

/// Non-Unix fallback: create_local_runtime
#[cfg(not(unix))]
pub fn create_tmux_runtime(
    _session_name: impl Into<String>,
    _window_name: impl Into<String>,
) -> Arc<dyn AgentRuntime> {
    panic!("tmux backend not supported on non-Unix platforms")
}

/// Recover an AgentRuntime from persisted state.
/// Used when pmux restarts and needs to attach to existing sessions.
#[cfg(unix)]
pub fn recover_runtime(
    backend: &str,
    state: &WorktreeState,
    _event_bus: Option<Arc<crate::runtime::EventBus>>,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    match backend {
        "local" | "local_pty" => Err(RuntimeError::Backend(
            "local_pty does not support session recovery".into(),
        )),
        "tmux" => {
            // Attach to existing tmux session/window
            let runtime = TmuxRuntime::attach(
                &state.backend_session_id,
                &state.backend_window_id,
            )?;
            Ok(Arc::new(runtime))
        }
        _ => Err(RuntimeError::Backend(format!(
            "unknown backend: {}",
            backend
        ))),
    }
}

/// Non-Unix fallback: tmux not supported
#[cfg(not(unix))]
pub fn recover_runtime(
    backend: &str,
    _state: &WorktreeState,
    _event_bus: Option<Arc<crate::runtime::EventBus>>,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    match backend {
        "local" | "local_pty" => Err(RuntimeError::Backend(
            "local_pty does not support session recovery".into(),
        )),
        "tmux" => Err(RuntimeError::Backend(
            "tmux backend not supported on non-Unix platforms".into(),
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

    #[test]
    fn test_recover_runtime_local_pty_not_supported() {
        let state = WorktreeState {
            path: PathBuf::from("/tmp/test"),
            branch: "main".to_string(),
            agent_id: "test".to_string(),
            pane_ids: vec![],
            backend: "local".to_string(),
            backend_session_id: String::new(),
            backend_window_id: String::new(),
        };
        let result = recover_runtime("local", &state, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not support"));
    }

    #[test]
    fn test_create_tmux_runtime_unix() {
        let rt = create_tmux_runtime("pmux-test-session", "test-window");
        // Just verify it creates without panicking
        // The actual tmux operations require tmux binary
        let _ = rt.primary_pane_id();
    }
}
