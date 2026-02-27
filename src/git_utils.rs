// git_utils.rs - Git repository validation utilities
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not a git repository")]
    NotARepository,
}

/// Check if a path is a valid git repository
/// Supports:
/// - Normal repositories (contains .git/ directory)
/// - Bare repositories (.git file pointing to gitdir)
/// - Worktrees (.git file with gitdir reference)
pub fn is_git_repository(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    // Check for .git directory (normal repo)
    let git_dir = path.join(".git");
    if git_dir.is_dir() {
        return true;
    }

    // Check for .git file (bare repo or worktree)
    if git_dir.is_file() {
        // Read the .git file to verify it's a valid git reference
        if let Ok(content) = std::fs::read_to_string(&git_dir) {
            // Bare repos and worktrees have "gitdir: " prefix
            return content.trim().starts_with("gitdir:");
        }
    }

    false
}

/// Validate that a path is a git repository
/// Returns Ok(()) if valid, Err otherwise
pub fn validate_git_repository(path: &Path) -> Result<(), GitError> {
    if !path.exists() {
        return Err(GitError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Path does not exist",
        )));
    }

    if !is_git_repository(path) {
        return Err(GitError::NotARepository);
    }

    Ok(())
}

/// Get a user-friendly error message for git validation failures
pub fn get_git_error_message(path: &Path, error: &GitError) -> String {
    match error {
        GitError::NotARepository => {
            format!(
                "The selected directory '{}' is not a Git repository.\n\nPlease select a folder containing a .git directory.",
                path.display()
            )
        }
        GitError::Io(e) => {
            format!("Cannot access path '{}': {}", path.display(), e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Test: Normal git repository should be detected
    #[test]
    fn test_detect_normal_git_repo() {
        // Arrange: Create a temp directory with .git subdirectory
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        // Act
        let result = is_git_repository(temp_dir.path());

        // Assert
        assert!(
            result,
            "Should detect normal git repository with .git directory"
        );
    }

    /// Test: Non-git directory should return false
    #[test]
    fn test_reject_non_git_directory() {
        // Arrange: Create a temp directory without .git
        let temp_dir = TempDir::new().unwrap();

        // Act
        let result = is_git_repository(temp_dir.path());

        // Assert
        assert!(!result, "Should reject directory without .git");
    }

    /// Test: Non-existent path should return false
    #[test]
    fn test_reject_nonexistent_path() {
        // Arrange: Use a path that doesn't exist
        let nonexistent = Path::new("/tmp/definitely/not/a/repo");

        // Act
        let result = is_git_repository(nonexistent);

        // Assert
        assert!(!result, "Should reject non-existent path");
    }

    /// Test: Bare git repository should be detected
    #[test]
    fn test_detect_bare_git_repo() {
        // Arrange: Create a temp directory with .git file containing gitdir reference
        let temp_dir = TempDir::new().unwrap();
        let git_file = temp_dir.path().join(".git");
        fs::write(&git_file, "gitdir: /path/to/bare/repo.git\n").unwrap();

        // Act
        let result = is_git_repository(temp_dir.path());

        // Assert
        assert!(result, "Should detect bare git repository with .git file");
    }

    /// Test: Regular .git file (not a repo reference) should return false
    #[test]
    fn test_reject_regular_git_file() {
        // Arrange: Create a temp directory with .git file that's not a repo reference
        let temp_dir = TempDir::new().unwrap();
        let git_file = temp_dir.path().join(".git");
        fs::write(&git_file, "This is just a regular file\n").unwrap();

        // Act
        let result = is_git_repository(temp_dir.path());

        // Assert
        assert!(!result, "Should reject .git file without gitdir reference");
    }

    /// Test: validate_git_repository returns Ok for valid repo
    #[test]
    fn test_validate_git_repository_success() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        // Act
        let result = validate_git_repository(temp_dir.path());

        // Assert
        assert!(result.is_ok());
    }

    /// Test: validate_git_repository returns Err for invalid repo
    #[test]
    fn test_validate_git_repository_failure() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        // Act
        let result = validate_git_repository(temp_dir.path());

        // Assert
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GitError::NotARepository));
    }

    /// Test: Error message is user-friendly
    #[test]
    fn test_error_message_is_user_friendly() {
        // Arrange
        let path = Path::new("/some/path");
        let error = GitError::NotARepository;

        // Act
        let message = get_git_error_message(path, &error);

        // Assert
        assert!(message.contains("not a Git repository"));
        assert!(message.contains(".git"));
    }
}
