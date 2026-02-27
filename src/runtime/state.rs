//! runtime/state.rs - Runtime state for session recovery
//!
//! Persists workspace/worktree mapping to backend sessions so pmux can
//! recover (attach to existing tmux sessions) on restart.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeStateError {
    #[error("Config directory not found")]
    ConfigDirNotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Single worktree entry with backend mapping
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorktreeState {
    pub branch: String,
    pub path: PathBuf,
    pub agent_id: String,
    pub pane_ids: Vec<String>,
    pub backend: String,
    pub backend_session_id: String,
    pub backend_window_id: String,
}

/// Workspace with multiple worktrees
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub path: PathBuf,
    pub worktrees: Vec<WorktreeState>,
}

/// Runtime state for recover
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RuntimeState {
    pub workspaces: Vec<WorkspaceState>,
}

impl RuntimeState {
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("pmux").join("runtime_state.json"))
    }

    pub fn load() -> Result<Self, RuntimeStateError> {
        let path = Self::default_path()
            .ok_or(RuntimeStateError::ConfigDirNotFound)?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(&path)?;
        let state: Self = serde_json::from_str(&json)?;
        Ok(state)
    }

    pub fn save(&self) -> Result<(), RuntimeStateError> {
        let path = Self::default_path()
            .ok_or(RuntimeStateError::ConfigDirNotFound)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Find workspace state by repo path
    pub fn find_workspace(&self, path: &PathBuf) -> Option<&WorkspaceState> {
        self.workspaces.iter().find(|w| &w.path == path)
    }

    /// Find workspace state by repo path (mutable)
    pub fn find_workspace_mut(&mut self, path: &PathBuf) -> Option<&mut WorkspaceState> {
        self.workspaces.iter_mut().find(|w| &w.path == path)
    }

    /// Upsert worktree for a workspace
    pub fn upsert_worktree(
        &mut self,
        workspace_path: PathBuf,
        worktree: WorktreeState,
    ) {
        if let Some(ws) = self.find_workspace_mut(&workspace_path) {
            if let Some(idx) = ws.worktrees.iter().position(|w| w.path == worktree.path) {
                ws.worktrees[idx] = worktree;
            } else {
                ws.worktrees.push(worktree);
            }
        } else {
            self.workspaces.push(WorkspaceState {
                path: workspace_path,
                worktrees: vec![worktree],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_state_default() {
        let state = RuntimeState::default();
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn test_upsert_worktree_new_workspace() {
        let mut state = RuntimeState::default();
        state.upsert_worktree(
            PathBuf::from("/repo"),
            WorktreeState {
                branch: "main".to_string(),
                path: PathBuf::from("/repo"),
                agent_id: "sdlc-repo:main".to_string(),
                pane_ids: vec!["%0".to_string()],
                backend: "tmux".to_string(),
                backend_session_id: "sdlc-repo".to_string(),
                backend_window_id: "@0".to_string(),
            },
        );
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].worktrees.len(), 1);
        assert_eq!(state.workspaces[0].worktrees[0].branch, "main");
    }

    #[test]
    fn test_upsert_worktree_update_existing() {
        let mut state = RuntimeState::default();
        state.upsert_worktree(
            PathBuf::from("/repo"),
            WorktreeState {
                branch: "main".to_string(),
                path: PathBuf::from("/repo"),
                agent_id: "sdlc-repo:main".to_string(),
                pane_ids: vec!["%0".to_string()],
                backend: "tmux".to_string(),
                backend_session_id: "sdlc-repo".to_string(),
                backend_window_id: "@0".to_string(),
            },
        );
        state.upsert_worktree(
            PathBuf::from("/repo"),
            WorktreeState {
                branch: "main".to_string(),
                path: PathBuf::from("/repo"),
                agent_id: "sdlc-repo:main".to_string(),
                pane_ids: vec!["%0".to_string(), "%1".to_string()],
                backend: "tmux".to_string(),
                backend_session_id: "sdlc-repo".to_string(),
                backend_window_id: "@0".to_string(),
            },
        );
        assert_eq!(state.workspaces[0].worktrees[0].pane_ids.len(), 2);
    }

    #[test]
    fn test_find_workspace() {
        let mut state = RuntimeState::default();
        state.upsert_worktree(
            PathBuf::from("/repo"),
            WorktreeState {
                branch: "main".to_string(),
                path: PathBuf::from("/repo"),
                agent_id: "a".to_string(),
                pane_ids: vec![],
                backend: "tmux".to_string(),
                backend_session_id: "s".to_string(),
                backend_window_id: "@0".to_string(),
            },
        );
        let ws = state.find_workspace(&PathBuf::from("/repo"));
        assert!(ws.is_some());
        assert!(state.find_workspace(&PathBuf::from("/other")).is_none());
    }
}
