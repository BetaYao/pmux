// worktree_manager.rs - Git worktree creation and management
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use crate::worktree::WorktreeError;

/// Result of a git worktree operation
#[derive(Debug, Clone)]
pub struct WorktreeCreateResult {
    pub success: bool,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub error: Option<String>,
}

impl WorktreeCreateResult {
    pub fn success(worktree_path: PathBuf, branch_name: String) -> Self {
        Self {
            success: true,
            worktree_path: Some(worktree_path),
            branch_name: Some(branch_name),
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            worktree_path: None,
            branch_name: None,
            error: Some(error),
        }
    }
}

/// Manager for creating git worktrees
pub struct WorktreeManager {
    repo_path: PathBuf,
}

impl WorktreeManager {
    /// Creates a new WorktreeManager for the given repository
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }

    /// Returns the repository path
    pub fn repo_path(&self) -> &PathBuf {
        &self.repo_path
    }

    /// Gets a list of existing branches in the repository
    pub fn get_existing_branches(&self) -> Result<Vec<String>, String> {
        let output = Command::new("git")
            .arg("branch")
            .arg("--format=%(refname:short)")
            .current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to execute git command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to list branches: {}", stderr));
        }

        let branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();

        Ok(branches)
    }

    /// Checks if a branch already exists
    pub fn branch_exists(&self, branch_name: &str) -> bool {
        match self.get_existing_branches() {
            Ok(branches) => branches.contains(&branch_name.to_string()),
            Err(_) => false,
        }
    }

    /// Creates a new git worktree asynchronously
    ///
    /// This function executes `git worktree add <path> -b <branch_name>`
    /// and returns the result. Uses std::process::Command with blocking::unblock
    /// so it works without a tokio runtime (e.g. when run from GPUI's executor).
    pub async fn create_worktree_async(
        &self,
        branch_name: &str,
        worktree_path: &PathBuf,
    ) -> WorktreeCreateResult {
        let repo_path = self.repo_path.clone();
        let worktree_path_clone = worktree_path.clone();
        let branch_name_owned = branch_name.to_string();

        let output = blocking::unblock(move || {
            Command::new("git")
                .arg("worktree")
                .arg("add")
                .arg(&worktree_path_clone)
                .arg("-b")
                .arg(&branch_name_owned)
                .current_dir(&repo_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        })
        .await;

        match output {
            Ok(output) => {
                if output.status.success() {
                    WorktreeCreateResult::success(
                        worktree_path.clone(),
                        branch_name.to_string(),
                    )
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let error = if stderr.trim().is_empty() {
                        stdout
                    } else {
                        stderr
                    };
                    WorktreeCreateResult::error(error)
                }
            }
            Err(e) => WorktreeCreateResult::error(format!("Failed to execute git worktree: {}", e)),
        }
    }

    /// Creates a new git worktree synchronously
    pub fn create_worktree(
        &self,
        branch_name: &str,
        worktree_path: &PathBuf,
    ) -> WorktreeCreateResult {
        let output = Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg(worktree_path)
            .arg("-b")
            .arg(branch_name)
            .current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    WorktreeCreateResult::success(
                        worktree_path.clone(),
                        branch_name.to_string(),
                    )
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let error = if stderr.trim().is_empty() {
                        stdout
                    } else {
                        stderr
                    };
                    WorktreeCreateResult::error(error)
                }
            }
            Err(e) => WorktreeCreateResult::error(format!("Failed to execute git worktree: {}", e)),
        }
    }

    /// Gets a list of all worktrees for this repository
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, String> {
        let output = Command::new("git")
            .arg("worktree")
            .arg("list")
            .arg("--porcelain")
            .current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to execute git worktree list: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to list worktrees: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_worktree_list(&stdout)
    }

    /// Removes a git worktree synchronously using `git worktree remove <path>`
    pub fn remove_worktree(&self, worktree_path: &Path) -> Result<(), WorktreeError> {
        let output = Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg(worktree_path)
            .current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(WorktreeError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(WorktreeError::CommandFailed(stderr));
        }

        Ok(())
    }
}

/// Represents a git worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub commit: Option<String>,
    pub detached: bool,
}

impl WorktreeInfo {
    /// Creates a new WorktreeInfo
    pub fn new(path: PathBuf, branch: String) -> Self {
        Self {
            path,
            branch,
            commit: None,
            detached: false,
        }
    }

    /// Returns the short branch name (removes "heads/" prefix)
    pub fn short_branch_name(&self) -> String {
        self.branch
            .strip_prefix("refs/heads/")
            .unwrap_or(&self.branch)
            .to_string()
    }

    /// Number of commits ahead of the base branch (placeholder for future implementation)
    pub fn ahead(&self) -> u32 {
        0
    }
}

/// Parses the output of `git worktree list --porcelain`
fn parse_worktree_list(output: &str) -> Result<Vec<WorktreeInfo>, String> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut current_commit: Option<String> = None;
    let mut current_detached = false;

    for line in output.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path_str));
        } else if let Some(branch_str) = line.strip_prefix("branch ") {
            current_branch = Some(branch_str.to_string());
        } else if let Some(commit_str) = line.strip_prefix("commit ") {
            current_commit = Some(commit_str.to_string());
        } else if line == "detached" {
            current_detached = true;
        } else if line.is_empty() {
            // End of worktree entry
            if let Some(path) = current_path.take() {
                let branch = current_branch.take().unwrap_or_else(|| {
                    if current_detached { "(detached)".to_string() } else { "(unknown)".to_string() }
                });
                worktrees.push(WorktreeInfo {
                    path,
                    branch,
                    commit: current_commit.take(),
                    detached: current_detached,
                });
            }
            current_detached = false;
        }
    }

    // Handle last worktree if output doesn't end with empty line
    if let Some(path) = current_path {
        let branch = current_branch.unwrap_or_else(|| {
            if current_detached { "(detached)".to_string() } else { "(unknown)".to_string() }
        });
        worktrees.push(WorktreeInfo {
            path,
            branch,
            commit: current_commit,
            detached: current_detached,
        });
    }

    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Test: Parse worktree list output
    #[test]
    fn test_parse_worktree_list() {
        let output = r#"worktree /home/user/project
branch refs/heads/main
commit abc123

worktree /home/user/project/feature-test
branch refs/heads/feature-test
commit def456
"#;

        let worktrees = parse_worktree_list(output).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].short_branch_name(), "main");
        assert_eq!(worktrees[1].short_branch_name(), "feature-test");
    }

    /// Test: Parse worktree list with detached HEAD
    #[test]
    fn test_parse_worktree_list_detached() {
        let output = r#"worktree /home/user/project
branch refs/heads/main
commit abc123

worktree /home/user/project/temp
detached
commit def456
"#;

        let worktrees = parse_worktree_list(output).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert!(!worktrees[0].detached);
        assert!(worktrees[1].detached);
    }

    /// Test: WorktreeCreateResult creation
    #[test]
    fn test_worktree_create_result() {
        let success = WorktreeCreateResult::success(
            PathBuf::from("/path/to/worktree"),
            "feature".to_string(),
        );
        assert!(success.success);
        assert_eq!(success.worktree_path, Some(PathBuf::from("/path/to/worktree")));
        assert_eq!(success.branch_name, Some("feature".to_string()));

        let failure = WorktreeCreateResult::error("Failed".to_string());
        assert!(!failure.success);
        assert_eq!(failure.error, Some("Failed".to_string()));
    }

    /// Test: WorktreeInfo short branch name
    #[test]
    fn test_worktree_info_short_branch_name() {
        let info = WorktreeInfo::new(
            PathBuf::from("/path"),
            "refs/heads/feature/test".to_string(),
        );
        assert_eq!(info.short_branch_name(), "feature/test");
    }

    /// Test: WorktreeInfo short branch name without prefix
    #[test]
    fn test_worktree_info_short_branch_name_no_prefix() {
        let info = WorktreeInfo::new(
            PathBuf::from("/path"),
            "main".to_string(),
        );
        assert_eq!(info.short_branch_name(), "main");
    }

    /// Test: remove_worktree API exists
    #[test]
    fn test_remove_worktree_api_exists() {
        let manager = WorktreeManager::new(PathBuf::from("/tmp/repo"));
        let _fn: fn(&WorktreeManager, &Path) -> Result<(), WorktreeError> = WorktreeManager::remove_worktree;
        let _ = _fn(&manager, Path::new("/tmp/repo-wt"));
    }
}