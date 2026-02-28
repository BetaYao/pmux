use std::path::PathBuf;
use crate::worktree::WorktreeInfo;

pub struct WorkspaceState {
    pub repo_path: PathBuf,
    pub repo_name: String,
    pub tmux_session: String,
    pub worktrees: Vec<WorktreeInfo>,
    pub selected_index: usize,
    pub pane_ids: Vec<String>,
    pub input_focused: bool,
}

impl WorkspaceState {
    pub fn new(repo_path: PathBuf) -> Self {
        let repo_name = repo_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let tmux_session = format!("pmux-{}", repo_name);
        Self {
            repo_path,
            repo_name,
            tmux_session,
            worktrees: Vec::new(),
            selected_index: 0,
            pane_ids: Vec::new(),
            input_focused: false,
        }
    }

    pub fn active_pane_id(&self) -> Option<&str> {
        self.pane_ids.get(self.selected_index).map(|s| s.as_str())
    }

    pub fn select_worktree(&mut self, index: usize) {
        if index < self.worktrees.len() {
            self.selected_index = index;
            self.input_focused = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_state_new() {
        let state = WorkspaceState::new(PathBuf::from("/home/user/myproject"));
        assert_eq!(state.repo_name, "myproject");
        assert_eq!(state.tmux_session, "pmux-myproject");
        assert_eq!(state.selected_index, 0);
        assert!(!state.input_focused);
    }

    #[test]
    fn test_active_pane_id_empty() {
        let state = WorkspaceState::new(PathBuf::from("/tmp/repo"));
        assert!(state.active_pane_id().is_none());
    }

    #[test]
    fn test_active_pane_id_with_panes() {
        let mut state = WorkspaceState::new(PathBuf::from("/tmp/repo"));
        state.pane_ids = vec!["%0".to_string(), "%1".to_string()];
        assert_eq!(state.active_pane_id(), Some("%0"));
        state.selected_index = 1;
        assert_eq!(state.active_pane_id(), Some("%1"));
    }

    #[test]
    fn test_select_worktree() {
        let mut state = WorkspaceState::new(PathBuf::from("/tmp/repo"));
        state.worktrees = vec![
            WorktreeInfo::new(PathBuf::from("/tmp/repo"), "main", "abc"),
            WorktreeInfo::new(PathBuf::from("/tmp/repo-feat"), "feat-x", "def"),
        ];
        state.input_focused = true;
        state.select_worktree(1);
        assert_eq!(state.selected_index, 1);
        assert!(!state.input_focused);
    }

    #[test]
    fn test_select_worktree_out_of_bounds() {
        let mut state = WorkspaceState::new(PathBuf::from("/tmp/repo"));
        state.select_worktree(99);
        assert_eq!(state.selected_index, 0);
    }
}
