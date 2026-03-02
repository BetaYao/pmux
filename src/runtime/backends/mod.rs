//! Backend adapters implementing AgentRuntime.
//!
//! Supports both Local PTY (direct shell spawn) and Tmux (session persistence).

pub mod session_backend;
pub use session_backend::{SessionBackend, ResolvedBackend};

mod dtach;
mod local_pty;
mod screen;
#[cfg(unix)]
mod tmux;
pub mod tmux_control_mode;

pub use dtach::DtachRuntime;
pub use local_pty::LocalPtyRuntime;
pub use screen::ScreenRuntime;
#[cfg(unix)]
pub use tmux::TmuxRuntime;

use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::runtime::agent_runtime::{AgentRuntime, RuntimeError};
use crate::runtime::WorktreeState;

/// Environment variable to select backend. Valid values: "local", "tmux".
pub const PMUX_BACKEND_ENV: &str = "PMUX_BACKEND";

/// Default backend when environment variable is not set.
/// tmux (control mode) provides session persistence — agents keep running after GUI closes.
/// Falls back to local PTY automatically when tmux is not installed.
pub const DEFAULT_BACKEND: &str = "tmux";

/// Resolve backend: PMUX_BACKEND env > config.backend > DEFAULT_BACKEND ("tmux").
/// Invalid values fall back to DEFAULT_BACKEND.
pub fn resolve_backend(config: Option<&Config>) -> String {
    const VALID: [&str; 3] = ["local", "tmux", "tmux-cc"];
    let from_env = std::env::var(PMUX_BACKEND_ENV).ok();
    let from_config = config.map(|c| c.backend.as_str());
    let raw = from_env.as_deref().or(from_config).unwrap_or(DEFAULT_BACKEND);
    if VALID.contains(&raw) {
        raw.to_string()
    } else {
        DEFAULT_BACKEND.to_string()
    }
}

/// Session naming for tmux backend. One workspace (repo) = one session.
/// Example: /foo/repo -> "pmux-repo"
pub fn session_name_for_workspace(workspace_path: &Path) -> String {
    format!(
        "pmux-{}",
        workspace_path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_else(|| "default".into())
    )
}

/// Window naming for tmux backend. One worktree/agent = one window.
/// Main worktree -> "main"; linked worktrees -> sanitized branch name.
pub fn window_name_for_worktree(_worktree_path: &Path, branch_name: &str) -> String {
    let name = if branch_name.is_empty() || branch_name == "main" {
        "main".to_string()
    } else {
        branch_name.to_string()
    };
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
}

/// Target for killing a worktree's window: session:window
pub fn window_target(workspace_path: &Path, window_name: &str) -> String {
    format!("{}:{}", session_name_for_workspace(workspace_path), window_name)
}

/// Check if tmux is available (installed and runnable).
#[cfg(unix)]
pub fn tmux_available() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn tmux_available() -> bool {
    false
}

/// Result of runtime creation; may include fallback message when tmux was requested but unavailable.
pub struct RuntimeCreationResult {
    pub runtime: Arc<dyn AgentRuntime>,
    /// Set when backend was tmux but tmux unavailable, so we fell back to local.
    pub fallback_message: Option<String>,
}

/// Create a runtime for the given worktree.
/// Backend resolution: PMUX_BACKEND env > config.backend > "tmux".
///
/// Tmux: one workspace = one session, one worktree = one window.
///
/// # Examples
/// ```bash
/// PMUX_BACKEND=tmux pmux
/// ```
pub fn create_runtime_from_env(
    workspace_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
    cols: u16,
    rows: u16,
    config: Option<&Config>,
) -> Result<RuntimeCreationResult, RuntimeError> {
    let backend = resolve_backend(config);

    // #region agent log
    crate::debug_log::dbg_session_log(
        "mod.rs:create_runtime_from_env",
        "backend resolved",
        &serde_json::json!({
            "backend": &backend,
            "tmux_available": tmux_available(),
            "workspace_path": workspace_path.to_string_lossy(),
            "worktree_path": worktree_path.to_string_lossy(),
            "branch_name": branch_name,
            "config_backend": config.map(|c| c.backend.as_str()).unwrap_or("none"),
        }),
        "H_backend",
    );
    // #endregion

    match backend.as_str() {
        "tmux" | "tmux-cc" => {
            #[cfg(unix)]
            {
                if !tmux_available() {
                    let rt = create_runtime(worktree_path, cols, rows)?;
                    return Ok(RuntimeCreationResult {
                        runtime: rt,
                        fallback_message: Some(
                            "tmux not installed — using local PTY (no session persistence). \
                             Install tmux for persistent agent sessions."
                                .to_string(),
                        ),
                    });
                }
                let session_name = session_name_for_workspace(workspace_path);
                let window_name = window_name_for_worktree(worktree_path, branch_name);
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "mod.rs:create_runtime_from_env",
                    "creating TmuxControlModeRuntime",
                    &serde_json::json!({"session_name": &session_name, "window_name": &window_name}),
                    "H_backend",
                );
                // #endregion
                let tmux_result = tmux_control_mode::TmuxControlModeRuntime::new(
                    &session_name,
                    &window_name,
                    Some(worktree_path),
                    cols,
                    rows,
                );
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "mod.rs:create_runtime_from_env",
                    "TmuxControlModeRuntime::new result",
                    &serde_json::json!({"ok": tmux_result.is_ok(), "err": tmux_result.as_ref().err().map(|e| e.to_string())}),
                    "H_backend",
                );
                // #endregion
                let runtime = Arc::new(
                    tmux_result.map_err(|e| RuntimeError::Backend(format!("tmux: {}", e)))?,
                );
                Ok(RuntimeCreationResult {
                    runtime,
                    fallback_message: None,
                })
            }
            #[cfg(not(unix))]
            {
                let rt = create_runtime(worktree_path, cols, rows)?;
                Ok(RuntimeCreationResult {
                    runtime: rt,
                    fallback_message: Some(
                        "tmux not supported on this platform — using local PTY.".to_string(),
                    ),
                })
            }
        }
        "local" | _ => {
            let rt = create_runtime(worktree_path, cols, rows)?;
            Ok(RuntimeCreationResult {
                runtime: rt,
                fallback_message: None,
            })
        }
    }
}

/// Create a LocalPtyAgent for the given worktree path.
/// Returns an AgentRuntime that supports multiple panes.
pub fn create_runtime(
    worktree_path: &Path,
    cols: u16,
    rows: u16,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    Ok(Arc::new(local_pty::LocalPtyAgent::new(
        worktree_path, cols, rows,
    )?))
}

/// Create a TmuxRuntime for the given session and window.
/// Session persistence allows agents to continue running after pmux closes.
#[cfg(unix)]
pub fn create_tmux_runtime(
    session_name: impl Into<String>,
    window_name: impl Into<String>,
    worktree_path: &Path,
) -> Arc<dyn AgentRuntime> {
    let rt = TmuxRuntime::new(session_name, window_name, Some(worktree_path));
    Arc::new(rt)
}

/// Non-Unix fallback: create_local_runtime
#[cfg(not(unix))]
pub fn create_tmux_runtime(
    _session_name: impl Into<String>,
    _window_name: impl Into<String>,
    _worktree_path: &Path,
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
    cols: u16,
    rows: u16,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    match backend {
        "local" | "local_pty" => Err(RuntimeError::Backend(
            "local_pty does not support session recovery".into(),
        )),
        "tmux" | "tmux-cc" => {
            let runtime = tmux_control_mode::TmuxControlModeRuntime::new(
                &state.backend_session_id,
                &state.backend_window_id,
                None,
                cols,
                rows,
            )
            .map_err(|e| RuntimeError::Backend(format!("tmux recover: {}", e)))?;
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
    _cols: u16,
    _rows: u16,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    match backend {
        "local" | "local_pty" => Err(RuntimeError::Backend(
            "local_pty does not support session recovery".into(),
        )),
        "tmux" | "tmux-cc" => Err(RuntimeError::Backend(
            "tmux not supported on non-Unix platforms".into(),
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
    use crate::config::Config;
    use crate::runtime::WorktreeState;
    use std::path::PathBuf;

    #[test]
    fn test_default_backend_is_tmux() {
        assert_eq!(DEFAULT_BACKEND, "tmux");
    }

    #[test]
    fn test_resolve_backend_defaults_to_tmux() {
        std::env::remove_var("PMUX_BACKEND");
        let backend = resolve_backend(None);
        assert_eq!(backend, "tmux");
    }

    #[test]
    fn test_resolve_backend_env_overrides_config() {
        std::env::set_var(PMUX_BACKEND_ENV, "tmux");
        let config = Config {
            backend: "local".into(),
            ..Config::default()
        };
        assert_eq!(resolve_backend(Some(&config)), "tmux");
        std::env::remove_var(PMUX_BACKEND_ENV);
    }

    #[test]
    fn test_resolve_backend_config_overrides_default() {
        std::env::remove_var(PMUX_BACKEND_ENV);
        let config = Config {
            backend: "tmux".into(),
            ..Config::default()
        };
        assert_eq!(resolve_backend(Some(&config)), "tmux");
    }

    #[test]
    fn test_resolve_backend_respects_config() {
        std::env::remove_var("PMUX_BACKEND");
        let mut config = crate::config::Config::default();
        config.backend = "tmux".to_string();
        let backend = resolve_backend(Some(&config));
        assert_eq!(backend, "tmux");
    }

    #[test]
    fn test_resolve_backend_env_overrides_config_local() {
        std::env::set_var("PMUX_BACKEND", "local");
        let mut config = crate::config::Config::default();
        config.backend = "tmux".to_string();
        let backend = resolve_backend(Some(&config));
        assert_eq!(backend, "local");
        std::env::remove_var("PMUX_BACKEND");
    }

    #[test]
    fn test_resolve_backend_accepts_tmux_cc() {
        std::env::set_var("PMUX_BACKEND", "tmux-cc");
        let backend = resolve_backend(None);
        assert_eq!(backend, "tmux-cc");
        std::env::remove_var("PMUX_BACKEND");
    }

    #[test]
    fn test_resolve_backend_invalid_fallback() {
        std::env::remove_var(PMUX_BACKEND_ENV);
        let config = Config {
            backend: "docker".into(),
            ..Config::default()
        };
        assert_eq!(resolve_backend(Some(&config)), DEFAULT_BACKEND);
    }

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
            split_tree_json: None,
        };
        let result = recover_runtime("unknown_backend", &state, None, 80, 24);
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
            split_tree_json: None,
        };
        let result = recover_runtime("local", &state, None, 80, 24);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not support"));
    }

    #[test]
    fn test_create_tmux_runtime_unix() {
        let rt = create_tmux_runtime("pmux-test-session", "test-window", Path::new("."));
        // Just verify it creates without panicking
        // The actual tmux operations require tmux binary
        let _ = rt.primary_pane_id();
    }
}
