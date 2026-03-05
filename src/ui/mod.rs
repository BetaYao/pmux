// ui/mod.rs - GUI components for pmux
pub mod app_root;
pub mod models;
pub mod delete_worktree_dialog_ui;
pub mod diff_overlay;
pub mod diff_view;
pub mod new_branch_dialog_ui;
pub mod new_branch_dialog_entity;
pub mod notification_panel;
pub mod sidebar;
pub mod split_pane_container;
pub mod status_bar;
pub mod tabbar;
pub mod terminal_controller;
pub mod terminal_area_entity;
pub mod terminal_view;
pub mod topbar;
pub mod topbar_entity;
pub mod notification_panel_entity;
pub mod workspace_tabbar;

use std::path::PathBuf;

/// Shared application state
#[derive(Clone, Debug)]
pub struct AppState {
    pub workspace_path: Option<PathBuf>,
    pub error_message: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workspace_path: None,
            error_message: None,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_workspace(path: PathBuf) -> Self {
        Self {
            workspace_path: Some(path),
            error_message: None,
        }
    }

    pub fn with_error(message: String) -> Self {
        Self {
            workspace_path: None,
            error_message: Some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(state.workspace_path.is_none());
        assert!(state.error_message.is_none());
    }

    #[test]
    fn test_app_state_with_workspace() {
        let path = PathBuf::from("/test/path");
        let state = AppState::with_workspace(path.clone());
        assert_eq!(state.workspace_path, Some(path));
        assert!(state.error_message.is_none());
    }

    #[test]
    fn test_app_state_with_error() {
        let state = AppState::with_error("Test error".to_string());
        assert!(state.workspace_path.is_none());
        assert_eq!(state.error_message, Some("Test error".to_string()));
    }
}
