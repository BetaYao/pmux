//! path_mapper.rs — Convert worktree paths to Claude Code project directory paths.
//!
//! Claude Code stores session JSONL files under ~/.claude/projects/{sanitized_path}/.
//! The sanitized path replaces '/' with '-' and '.' with '-'.

use std::path::{Path, PathBuf};

/// Convert a worktree path to Claude Code's project directory path.
///
/// Rule: replace '/' with '-', '.' with '-'
/// Example: /Users/matt/workspace/pmux → ~/.claude/projects/-Users-matt-workspace-pmux/
pub fn worktree_to_claude_project_dir(worktree_path: &Path) -> PathBuf {
    let claude_base = dirs::home_dir()
        .expect("home dir")
        .join(".claude")
        .join("projects");

    let sanitized = worktree_path
        .to_string_lossy()
        .replace('/', "-")
        .replace('.', "-");

    claude_base.join(sanitized)
}

/// Check if the project directory exists (Claude Code may not have been used in this worktree).
pub fn claude_project_dir_exists(worktree_path: &Path) -> bool {
    worktree_to_claude_project_dir(worktree_path).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_to_claude_project_dir() {
        let path = Path::new("/Users/matt/workspace/pmux");
        let result = worktree_to_claude_project_dir(path);
        let expected = dirs::home_dir()
            .unwrap()
            .join(".claude/projects/-Users-matt-workspace-pmux");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_worktree_with_dots() {
        let path = Path::new("/Users/matt/workspace/ganwork/.worktrees/feature-117");
        let result = worktree_to_claude_project_dir(path);
        let expected = dirs::home_dir()
            .unwrap()
            .join(".claude/projects/-Users-matt-workspace-ganwork--worktrees-feature-117");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_claude_project_dir_exists_false() {
        let path = Path::new("/nonexistent/path/that/does/not/exist");
        assert!(!claude_project_dir_exists(path));
    }
}
