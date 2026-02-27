// deps.rs - Startup dependency checks for tmux and git
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
    /// Names of missing commands (e.g. ["tmux", "git"])
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
            "nvim" => "macOS: brew install neovim",
            "diffview" => "In nvim: :Lazy install sindrets/diffview.nvim (requires lazy.nvim)",
            _ => "",
        }
    }
    #[cfg(target_os = "linux")]
    {
        match cmd {
            "tmux" => "Linux: sudo apt install tmux (Debian/Ubuntu) or sudo dnf install tmux (Fedora)",
            "git" => "Linux: sudo apt install git (Debian/Ubuntu) or sudo dnf install git (Fedora)",
            "nvim" => "Linux: sudo apt install neovim (Debian/Ubuntu) or sudo dnf install neovim (Fedora)",
            "diffview" => "In nvim: :Lazy install sindrets/diffview.nvim (requires lazy.nvim)",
            _ => "",
        }
    }
    #[cfg(target_os = "windows")]
    {
        match cmd {
            "tmux" => "Windows: winget install tmux.tmux or choco install tmux",
            "git" => "Windows: winget install Git.Git or https://git-scm.com/download/win",
            "nvim" => "Windows: winget install Neovim.Neovim or choco install neovim",
            "diffview" => "In nvim: :Lazy install sindrets/diffview.nvim (requires lazy.nvim)",
            _ => "",
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        match cmd {
            "tmux" => "Install tmux from your package manager",
            "git" => "Install git from your package manager",
            "nvim" => "Install neovim from your package manager",
            "diffview" => "In nvim: :Lazy install sindrets/diffview.nvim (requires lazy.nvim)",
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
        "nvim" => "diff view",
        "diffview" => "diff view",
        _ => "running",
    }
}

/// Check that nvim can load the diffview plugin (run headless require)
/// Note: nvim exits 0 even when -c fails, so we must check stderr for errors
fn check_diffview() -> Result<(), DependencyError> {
    let output = Command::new("nvim")
        .args([
            "--headless",
            "-c",
            "lua require(\"diffview\")",
            "-c",
            "qa",
        ])
        .output()
        .map_err(|e| DependencyError::Missing(format!("Cannot execute nvim: {}", e)))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_lower = stderr.to_lowercase();

    // nvim exits 0 even when lua require fails; check stderr for module not found
    if output.status.success() && !stderr_lower.contains("module 'diffview' not found")
        && !stderr_lower.contains("module \"diffview\" not found")
    {
        Ok(())
    } else {
        Err(DependencyError::Missing(format!(
            "diffview not installed or failed to load: {}",
            stderr.trim().lines().next().unwrap_or("Install sindrets/diffview.nvim")
        )))
    }
}

/// Check that all required dependencies (tmux, git) are available.
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
/// Use this when you need to show missing deps and installation instructions.
pub fn check_dependencies_detailed() -> DependencyCheckResult {
    let mut missing = Vec::new();

    if check_cmd("tmux", &["-V"]).is_err() {
        missing.push("tmux".to_string());
    }
    if check_cmd("git", &["--version"]).is_err() {
        missing.push("git".to_string());
    }
    if check_cmd("nvim", &["--version"]).is_err() {
        missing.push("nvim".to_string());
    }
    if missing.contains(&"nvim".to_string()) {
        // Skip diffview check if nvim is missing
    } else if check_diffview().is_err() {
        missing.push("diffview".to_string());
    }

    let message = if missing.is_empty() {
        String::new()
    } else {
        let list = missing.join("、");
        format!(
            "pmux dependencies missing: {}.\n\npmux needs tmux for terminal sessions, git for worktrees and branches, nvim and diffview for diff view.\nPlease install the above and ensure they are in PATH.",
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
        assert_eq!(cmd_purpose("nvim"), "diff view");
        assert_eq!(cmd_purpose("diffview"), "diff view");
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
