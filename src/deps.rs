// deps.rs - Startup dependency checks for git
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DependencyError {
    #[error("{0}")]
    Missing(String),
}

/// Result of dependency check with details for self-check UI
#[derive(Clone, Debug)]
pub struct DependencyCheckResult {
    /// Names of missing commands (e.g. ["git"])
    pub missing: Vec<String>,
    /// Human-readable summary message
    pub message: String,
}

impl DependencyCheckResult {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Installation instructions for a command, platform-specific
pub fn installation_instructions(cmd: &str) -> &'static str {
    #[cfg(target_os = "macos")]
    {
        match cmd {
            "tmux" => "macOS: brew install tmux",
            "git" => "macOS: Xcode Command Line Tools (xcode-select --install) or brew install git",
            _ => "",
        }
    }
    #[cfg(target_os = "linux")]
    {
        match cmd {
            "tmux" => "Linux: sudo apt install tmux (Debian/Ubuntu) or sudo dnf install tmux (Fedora)",
            "git" => "Linux: sudo apt install git (Debian/Ubuntu) or sudo dnf install git (Fedora)",
            _ => "",
        }
    }
    #[cfg(target_os = "windows")]
    {
        match cmd {
            "tmux" => "Windows: winget install tmux.tmux or choco install tmux",
            "git" => "Windows: winget install Git.Git or https://git-scm.com/download/win",
            _ => "",
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        match cmd {
            "tmux" => "Install tmux from your package manager",
            "git" => "Install git from your package manager",
            _ => "",
        }
    }
}

/// Check that a command exists in PATH and executes successfully
fn check_cmd(name: &str, args: &[&str]) -> Result<(), DependencyError> {
    let output = Command::new(name).args(args).output();
    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(DependencyError::Missing(format!(
                "{} failed: {}",
                name,
                stderr.trim().lines().next().unwrap_or("non-zero exit")
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(DependencyError::Missing(format!(
            "{} not found. pmux needs {} for {}. Please install {} and ensure it is in PATH.",
            name,
            name,
            cmd_purpose(name),
            name
        ))),
        Err(e) => Err(DependencyError::Missing(format!(
            "Cannot execute {}: {}",
            name, e
        ))),
    }
}

fn cmd_purpose(name: &str) -> &'static str {
    match name {
        "tmux" => "terminal session management",
        "git" => "worktree and branch management",
        _ => "running",
    }
}

/// Check that all required dependencies (git) are available.
/// Returns Ok(()) if all are present and executable, Err with a user-friendly
/// message listing all missing dependencies otherwise.
pub fn check_dependencies() -> Result<(), DependencyError> {
    let result = check_dependencies_detailed();
    if result.is_ok() {
        Ok(())
    } else {
        Err(DependencyError::Missing(result.message))
    }
}

/// Run dependency check and return detailed result for self-check UI.
pub fn check_dependencies_detailed() -> DependencyCheckResult {
    let mut missing = Vec::new();

    if check_cmd("git", &["--version"]).is_err() {
        missing.push("git".to_string());
    }

    let message = if missing.is_empty() {
        String::new()
    } else {
        let list = missing.join(", ");
        format!(
            "pmux dependencies missing: {}.\n\npmux needs git for worktrees and branches.\nPlease install the above and ensure they are in PATH.",
            list
        )
    };

    DependencyCheckResult { missing, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_cmd_purpose() {
        assert_eq!(cmd_purpose("tmux"), "terminal session management");
        assert_eq!(cmd_purpose("git"), "worktree and branch management");
        assert_eq!(cmd_purpose("other"), "running");
    }

    #[test]
    fn test_check_dependencies_error_format() {
        // When deps are missing, error message has expected format
        let result = check_dependencies();
        match &result {
            Ok(()) => {}
            Err(e) => {
                let s = e.to_string();
                assert!(s.contains("pmux dependencies missing"));
                assert!(s.contains("PATH"));
            }
        }
    }
}
