//! Backend adapters implementing AgentRuntime.
//!
//! Supports both Local PTY (direct shell spawn) and Tmux (session persistence).

pub mod session_backend;
pub use session_backend::{ResolvedBackend, SessionBackend};

mod dtach;
mod local_pty;
mod screen;
mod shpool;
#[cfg(unix)]
pub mod tmux_standard;

pub use dtach::DtachRuntime;
pub use local_pty::LocalPtyRuntime;
pub use screen::ScreenRuntime;
pub use shpool::ShpoolRuntime;

use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::runtime::agent_runtime::{AgentRuntime, RuntimeError};
use crate::runtime::WorktreeState;

/// Environment variable to select backend. Valid values: "local", "tmux", "dtach", "screen", "shpool".
pub const PMUX_BACKEND_ENV: &str = "PMUX_BACKEND";

/// Default backend when environment variable is not set.
/// tmux (control mode) provides session persistence — agents keep running after GUI closes.
/// Falls back to local PTY automatically when tmux is not installed.
pub const DEFAULT_BACKEND: &str = "tmux";

/// Effective SessionBackend: PMUX_BACKEND env > config.session_backend (with config.backend fallback when Auto).
fn effective_session_backend(config: Option<&Config>) -> SessionBackend {
    if let Ok(env_val) = std::env::var(PMUX_BACKEND_ENV) {
        return match env_val.as_str() {
            "dtach" => SessionBackend::Dtach,
            "tmux" | "tmux-cc" | "tmux-standard" => SessionBackend::Tmux,
            "screen" => SessionBackend::Screen,
            "shpool" => SessionBackend::Shpool,
            "local" => SessionBackend::Local,
            _ => config.map(|c| c.session_backend).unwrap_or(SessionBackend::Tmux),
        };
    }
    let sb = config.map(|c| c.session_backend).unwrap_or(SessionBackend::Tmux);
    if sb == SessionBackend::Auto {
        if let Some(c) = config {
            return match c.backend.as_str() {
                "dtach" => SessionBackend::Dtach,
                "tmux" | "tmux-cc" | "tmux-standard" => SessionBackend::Tmux,
                "screen" => SessionBackend::Screen,
                "shpool" => SessionBackend::Shpool,
                "local" => SessionBackend::Local,
                _ => SessionBackend::Tmux,
            };
        }
    }
    sb
}

/// Resolve backend to string: PMUX_BACKEND env > config.session_backend > config.backend > default.
/// Returns the resolved backend string (dtach, tmux, screen, or local) for display.
pub fn resolve_backend(config: Option<&Config>) -> String {
    let session_backend = effective_session_backend(config);
    session_backend.resolve().as_str().to_string()
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
/// Uses the worktree directory name (last path component) as the window name.
/// This is stable even when branches are switched inside the terminal.
pub fn window_name_for_worktree(worktree_path: &Path, _branch_name: &str) -> String {
    let name = worktree_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());
    sanitize_tmux_name(&name)
}

/// Legacy window name (branch-based) for migration from old naming scheme.
pub fn legacy_window_name_for_worktree(branch_name: &str) -> String {
    let name = if branch_name.is_empty() || branch_name == "main" {
        "main".to_string()
    } else {
        branch_name.to_string()
    };
    sanitize_tmux_name(&name)
}

/// Sanitize a string for use as a tmux window/session name.
fn sanitize_tmux_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
}

/// Migrate a tmux window from legacy branch-based name to path-based name.
/// If old-name window exists but new-name doesn't, rename it via `tmux rename-window`.
#[cfg(unix)]
pub fn migrate_tmux_window_name(session: &str, old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    // List current window names in the session
    let output = match std::process::Command::new("tmux")
        .args(["list-windows", "-t", session, "-F", "#{window_name}"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return,
    };
    let windows: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    // If new name already exists, nothing to do
    if windows.iter().any(|w| w == new_name) {
        return;
    }
    // If old name exists, rename it
    if windows.iter().any(|w| w == old_name) {
        let _ = std::process::Command::new("tmux")
            .args([
                "rename-window",
                "-t",
                &format!("{}:{}", session, old_name),
                new_name,
            ])
            .output();
    }
}

#[cfg(not(unix))]
pub fn migrate_tmux_window_name(_session: &str, _old_name: &str, _new_name: &str) {}

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

/// List tmux window names for all pmux sessions matching this workspace.
/// Checks both legacy naming (`pmux-<repo>`) and new naming (`pmux-<repo>-*`).
/// Returns empty vec if tmux is unavailable or no matching sessions exist.
#[cfg(unix)]
pub fn list_tmux_windows(workspace_path: &Path) -> Vec<String> {
    if !tmux_available() {
        return Vec::new();
    }
    // Find all tmux sessions matching this workspace
    let repo_prefix = session_name_for_workspace(workspace_path);
    let all_sessions = match std::process::Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let matching_sessions: Vec<String> = String::from_utf8_lossy(&all_sessions.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| s == &repo_prefix || s.starts_with(&format!("{}-", repo_prefix)))
        .collect();

    let mut result = Vec::new();
    for session in &matching_sessions {
        let output = match std::process::Command::new("tmux")
            .args(["list-windows", "-t", session, "-F", "#{window_name}"])
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => continue,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        for name in stdout.lines().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if !result.contains(&name.to_string()) {
                result.push(name.to_string());
            }
        }
    }
    result
}

#[cfg(not(unix))]
pub fn list_tmux_windows(_workspace_path: &Path) -> Vec<String> {
    Vec::new()
}

/// Kill a tmux window by workspace path and window name (e.g. for orphan cleanup).
/// Searches across all pmux sessions matching the workspace (legacy + new naming).
#[cfg(unix)]
pub fn kill_tmux_window(workspace_path: &Path, window_name: &str) -> Result<(), RuntimeError> {
    // Try the legacy target first
    let target = window_target(workspace_path, window_name);
    let status = std::process::Command::new("tmux")
        .args(["kill-window", "-t", &target])
        .status();
    if let Ok(s) = &status {
        if s.success() {
            return Ok(());
        }
    }
    // Fallback: search across all pmux sessions matching this workspace
    let repo_prefix = session_name_for_workspace(workspace_path);
    if let Ok(output) = std::process::Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
    {
        for session in String::from_utf8_lossy(&output.stdout).lines() {
            let session = session.trim();
            if session == &repo_prefix || session.starts_with(&format!("{}-", repo_prefix)) {
                let target = format!("{}:{}", session, window_name);
                let result = std::process::Command::new("tmux")
                    .args(["kill-window", "-t", &target])
                    .status();
                if let Ok(s) = result {
                    if s.success() {
                        return Ok(());
                    }
                }
            }
        }
    }
    Err(RuntimeError::Backend(format!(
        "tmux kill-window for '{}' in workspace '{}' failed",
        window_name,
        workspace_path.display()
    )))
}

#[cfg(not(unix))]
pub fn kill_tmux_window(_workspace_path: &Path, _window_name: &str) -> Result<(), RuntimeError> {
    Err(RuntimeError::Backend("tmux not supported on this platform".into()))
}

/// Kill an entire tmux session by workspace path.
#[cfg(unix)]
pub fn kill_tmux_session(workspace_path: &Path) -> Result<(), RuntimeError> {
    let session_name = session_name_for_workspace(workspace_path);
    let status = std::process::Command::new("tmux")
        .args(["kill-session", "-t", &session_name])
        .status()
        .map_err(|e| RuntimeError::Backend(format!("tmux kill-session: {}", e)))?;
    // Session may not exist (already dead), treat as ok either way
    let _ = status;
    Ok(())
}

#[cfg(not(unix))]
pub fn kill_tmux_session(_workspace_path: &Path) -> Result<(), RuntimeError> {
    Ok(())
}

/// Result of runtime creation; may include fallback message when tmux was requested but unavailable.
pub struct RuntimeCreationResult {
    pub runtime: Arc<dyn AgentRuntime>,
    /// Set when backend was tmux but tmux unavailable, so we fell back to local.
    pub fallback_message: Option<String>,
}

/// Create a runtime for the given worktree.
/// Backend resolution: PMUX_BACKEND env > config.session_backend > config.backend.
/// Auto resolves: dtach > tmux > screen > local by availability.
///
/// # Examples
/// ```bash
/// PMUX_BACKEND=tmux pmux
/// PMUX_BACKEND=dtach pmux
/// ```
pub fn create_runtime_from_env(
    workspace_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
    cols: u16,
    rows: u16,
    config: Option<&Config>,
) -> Result<RuntimeCreationResult, RuntimeError> {
    let session_backend = effective_session_backend(config);
    let resolved = session_backend.resolve();

    log::info!(
        "Session backend: {:?} (resolved from {:?})",
        resolved,
        session_backend
    );

    // #region agent log
    crate::debug_log::dbg_session_log(
        "mod.rs:create_runtime_from_env",
        "backend resolved",
        &serde_json::json!({
            "resolved": resolved.as_str(),
            "session_backend": session_backend.as_str(),
            "workspace_path": workspace_path.to_string_lossy(),
            "worktree_path": worktree_path.to_string_lossy(),
            "branch_name": branch_name,
        }),
        "H_backend",
    );
    // #endregion

    match resolved {
        ResolvedBackend::Dtach => {
            let rt = DtachRuntime::new(worktree_path, cols, rows)
                .map_err(|e| RuntimeError::Backend(format!("dtach: {}", e)))?;
            Ok(RuntimeCreationResult {
                runtime: Arc::new(rt),
                fallback_message: None,
            })
        }
        ResolvedBackend::Shpool => {
            let rt = ShpoolRuntime::new(worktree_path, cols, rows)
                .map_err(|e| RuntimeError::Backend(format!("shpool: {}", e)))?;
            Ok(RuntimeCreationResult {
                runtime: Arc::new(rt),
                fallback_message: None,
            })
        }
        ResolvedBackend::Tmux => {
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
                let sess_name =
                    tmux_standard::session_name(workspace_path, branch_name);
                let rt = tmux_standard::TmuxStandardBackend::new(
                    &sess_name,
                    worktree_path,
                    cols,
                    rows,
                )
                .map_err(|e| RuntimeError::Backend(format!("tmux: {}", e)))?;
                Ok(RuntimeCreationResult {
                    runtime: Arc::new(rt),
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
        ResolvedBackend::Screen => {
            let rt = ScreenRuntime::new(worktree_path, cols, rows)
                .map_err(|e| RuntimeError::Backend(format!("screen: {}", e)))?;
            Ok(RuntimeCreationResult {
                runtime: Arc::new(rt),
                fallback_message: None,
            })
        }
        ResolvedBackend::Local => {
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
        "dtach" => Err(RuntimeError::Backend(
            "dtach does not support session recovery".into(),
        )),
        "shpool" => Err(RuntimeError::Backend(
            "shpool does not support session recovery".into(),
        )),
        "screen" => Err(RuntimeError::Backend(
            "screen does not support session recovery".into(),
        )),
        "tmux" | "tmux-cc" | "tmux-standard" => {
            let runtime = tmux_standard::TmuxStandardBackend::recover(
                &state.backend_session_id,
                &state.path,
                cols,
                rows,
            )
            .map_err(|e| RuntimeError::Backend(format!("tmux-standard recover: {}", e)))?;
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
        "dtach" => Err(RuntimeError::Backend(
            "dtach does not support session recovery".into(),
        )),
        "shpool" => Err(RuntimeError::Backend(
            "shpool does not support session recovery".into(),
        )),
        "screen" => Err(RuntimeError::Backend(
            "screen does not support session recovery".into(),
        )),
        "tmux" | "tmux-cc" => Err(RuntimeError::Backend(
            "tmux not supported on non-Unix platforms".into(),
        )),
        "tmux-standard" => Err(RuntimeError::Backend(
            "tmux-standard not supported on non-Unix platforms".into(),
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
    fn test_resolve_backend_defaults_to_tmux_or_local() {
        std::env::remove_var("PMUX_BACKEND");
        let backend = resolve_backend(None);
        // With no config, we default to Tmux; resolve returns tmux if available else local
        assert!(
            backend == "tmux" || backend == "local",
            "expected tmux or local, got {}",
            backend
        );
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
        // tmux-cc maps to Tmux; resolve returns tmux if available else local
        assert!(
            backend == "tmux" || backend == "local",
            "expected tmux or local, got {}",
            backend
        );
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
    fn test_session_name_for_workspace_is_prefix_of_tmux_standard_name() {
        // Ensures list_tmux_windows can find new-format sessions by prefix
        let workspace_path = Path::new("/Users/me/work/my-project");
        let legacy_name = session_name_for_workspace(workspace_path);
        assert_eq!(legacy_name, "pmux-my-project");

        let new_name = tmux_standard::session_name(workspace_path, "main");
        assert_eq!(new_name, "pmux-my-project-main");

        // New name starts with legacy prefix + "-"
        assert!(
            new_name.starts_with(&format!("{}-", legacy_name)),
            "new session name '{}' should start with legacy prefix '{}-'",
            new_name,
            legacy_name
        );
    }

    #[test]
    fn test_list_tmux_windows_finds_new_format_sessions() {
        // Integration test: create a session with new naming, verify list_tmux_windows finds it
        if !tmux_available() {
            return;
        }
        let session = "pmux-list-test-main";
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();

        // Create session with new-format name
        let _ = std::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", session, "-x", "80", "-y", "24"])
            .output();

        // list_tmux_windows should find windows via prefix matching
        let workspace = Path::new("/tmp/list-test");
        let windows = list_tmux_windows(workspace);
        // The session name "pmux-list-test-main" starts with "pmux-list-test-"
        // which is the prefix from session_name_for_workspace("/tmp/list-test") = "pmux-list-test"
        assert!(
            !windows.is_empty(),
            "list_tmux_windows should find windows in new-format session"
        );

        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();
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
        match result {
            Err(e) => assert!(e.to_string().contains("does not support")),
            Ok(_) => panic!("expected error for local pty recovery"),
        }
    }

}
