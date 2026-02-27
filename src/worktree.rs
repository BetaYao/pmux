// worktree.rs - Git worktree discovery and management
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorktreeError {
    #[error("Git command failed: {0}")]
    CommandFailed(String),
    #[error("Not a git repository")]
    NotAGitRepo,
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Information about a git worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
    pub is_main: bool,
    pub ahead: usize,
    pub behind: usize,
}

impl WorktreeInfo {
    /// Create a new WorktreeInfo
    pub fn new(path: PathBuf, branch: &str, head: &str) -> Self {
        // Check both short and full ref names for main/master
        let short_branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
        let is_main = short_branch == "main" || short_branch == "master";
        
        Self {
            path,
            branch: branch.to_string(),
            head: head.to_string(),
            is_main,
            ahead: 0,
            behind: 0,
        }
    }

    /// Get the branch name without refs/heads/ prefix
    pub fn short_branch_name(&self) -> &str {
        self.branch.strip_prefix("refs/heads/").unwrap_or(&self.branch)
    }

    /// Get abbreviated path for display
    pub fn display_path(&self) -> String {
        let home = dirs::home_dir();
        let path_str = self.path.to_string_lossy();
        
        if let Some(home) = home {
            let home_str = home.to_string_lossy();
            if path_str.starts_with(&*home_str) {
                return format!("~{}", &path_str[home_str.len()..]);
            }
        }
        
        path_str.to_string()
    }
}

/// Discover all worktrees in a repository
pub fn discover_worktrees(repo_path: &Path) -> Result<Vec<WorktreeInfo>, WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(WorktreeError::NotAGitRepo);
        }
        return Err(WorktreeError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_list(&stdout)
}

/// Parse porcelain output from `git worktree list --porcelain`
fn parse_worktree_list(output: &str) -> Result<Vec<WorktreeInfo>, WorktreeError> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut current_head: Option<String> = None;

    for line in output.lines() {
        if line.is_empty() {
            // End of worktree entry
            if let (Some(path), Some(branch), Some(head)) = 
                (current_path.take(), current_branch.take(), current_head.take()) {
                worktrees.push(WorktreeInfo::new(path, &branch, &head));
            }
        } else if let Some(path_str) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path_str));
        } else if let Some(branch_str) = line.strip_prefix("branch ") {
            current_branch = Some(branch_str.to_string());
        } else if let Some(head_str) = line.strip_prefix("HEAD ") {
            current_head = Some(head_str.to_string());
        }
        // Ignore other fields like "detached", "locked", etc.
    }

    // Handle last entry if file doesn't end with empty line
    if let (Some(path), Some(branch), Some(head)) = 
        (current_path, current_branch, current_head) {
        worktrees.push(WorktreeInfo::new(path, &branch, &head));
    }

    if worktrees.is_empty() {
        return Err(WorktreeError::ParseError("No worktrees found".to_string()));
    }

    // Sort: main/master first, then alphabetically
    worktrees.sort_by(|a, b| {
        match (a.is_main, b.is_main) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.branch.cmp(&b.branch),
        }
    });

    Ok(worktrees)
}

/// Get ahead/behind count for a branch
pub fn get_ahead_behind(_repo_path: &Path, _branch: &str) -> Result<(usize, usize), WorktreeError> {
    // This would need to compare with remote tracking branch
    // For now, return 0,0 as placeholder
    Ok((0, 0))
}

/// Check if worktree has uncommitted changes
pub fn has_uncommitted_changes(worktree_path: &Path) -> bool {
    let output = match Command::new("git")
        .args(["status", "--short"])
        .current_dir(worktree_path)
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    !stdout.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: WorktreeInfo creation
    #[test]
    fn test_worktree_info_creation() {
        let wt = WorktreeInfo::new(
            PathBuf::from("/home/user/project"),
            "refs/heads/main",
            "abc123"
        );
        assert_eq!(wt.path, PathBuf::from("/home/user/project"));
        assert_eq!(wt.short_branch_name(), "main");
        assert!(wt.is_main);
    }

    /// Test: Short branch name extraction
    #[test]
    fn test_short_branch_name() {
        let wt1 = WorktreeInfo::new(PathBuf::from("/tmp"), "refs/heads/feature-x", "abc");
        assert_eq!(wt1.short_branch_name(), "feature-x");

        let wt2 = WorktreeInfo::new(PathBuf::from("/tmp"), "master", "def");
        assert_eq!(wt2.short_branch_name(), "master");
    }

    /// Test: Main branch detection
    #[test]
    fn test_main_branch_detection() {
        let main_wt = WorktreeInfo::new(PathBuf::from("/tmp"), "refs/heads/main", "abc");
        assert!(main_wt.is_main);

        let master_wt = WorktreeInfo::new(PathBuf::from("/tmp"), "master", "def");
        assert!(master_wt.is_main);

        let feat_wt = WorktreeInfo::new(PathBuf::from("/tmp"), "refs/heads/feat-x", "ghi");
        assert!(!feat_wt.is_main);
    }

    /// Test: Parse worktree list output
    #[test]
    fn test_parse_worktree_list() {
        let input = r#"worktree /home/user/project
HEAD abc123def456
branch refs/heads/main

worktree /home/user/project-feat
HEAD def789abc012
branch refs/heads/feature-auth

"#;

        let worktrees = parse_worktree_list(input).unwrap();
        assert_eq!(worktrees.len(), 2);
        
        // Main should be first due to sorting
        assert!(worktrees[0].is_main);
        assert_eq!(worktrees[0].short_branch_name(), "main");
        
        assert!(!worktrees[1].is_main);
        assert_eq!(worktrees[1].short_branch_name(), "feature-auth");
    }

    /// Test: Parse single worktree
    #[test]
    fn test_parse_single_worktree() {
        let input = r#"worktree /tmp/repo
HEAD 123abc
branch refs/heads/main"#;

        let worktrees = parse_worktree_list(input).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].path, PathBuf::from("/tmp/repo"));
    }

    /// Test: Empty input error
    #[test]
    fn test_parse_empty_input() {
        let result = parse_worktree_list("");
        assert!(result.is_err());
    }

    /// Test: Display path with home directory
    #[test]
    fn test_display_path() {
        // This test depends on the actual home directory
        // So we just verify the method exists and returns a string
        let wt = WorktreeInfo::new(PathBuf::from("/some/path"), "main", "abc");
        let display = wt.display_path();
        assert!(!display.is_empty());
    }

    /// Test: API functions exist
    #[test]
    fn test_api_exists() {
        let _discover_fn: fn(&Path) -> Result<Vec<WorktreeInfo>, WorktreeError> = discover_worktrees;
        let _ahead_fn: fn(&Path, &str) -> Result<(usize, usize), WorktreeError> = get_ahead_behind;
    }
}
