// tmux/window.rs - Tmux window management
use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WindowError {
    #[error("tmux command failed: {0}")]
    CommandFailed(String),
    #[error("Window not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Information about a tmux window
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: String,
    pub name: String,
    pub session_name: String,
    pub active: bool,
}

impl WindowInfo {
    /// Create a new WindowInfo
    pub fn new(id: &str, name: &str, session: &str, active: bool) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            session_name: session.to_string(),
            active,
        }
    }

    /// Get the full target identifier
    pub fn target(&self) -> String {
        format!("{}:{}", self.session_name, self.id)
    }
}

/// List all windows in a session
pub fn list_windows(session_name: &str) -> Result<Vec<WindowInfo>, WindowError> {
    let output = Command::new("tmux")
        .args([
            "list-windows",
            "-t", session_name,
            "-F", "#{window_id}|#{window_name}|#{window_active}",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut windows = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 3 {
            let active = parts[2] == "1";
            windows.push(WindowInfo::new(
                parts[0],
                parts[1],
                session_name,
                active,
            ));
        }
    }

    Ok(windows)
}

/// Create a new window in a session with a specific working directory (runs default shell)
pub fn create_window_with_cwd(
    session_name: &str,
    name: &str,
    cwd: &Path,
) -> Result<String, WindowError> {
    let cwd_str = cwd.to_str().unwrap_or(".");
    let output = Command::new("tmux")
        .args([
            "new-window",
            "-t", session_name,
            "-n", name,
            "-c", cwd_str,
            "-P",
            "-F", "#{window_id}",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(window_id)
}

/// Create a new window in a session
pub fn create_window(session_name: &str, name: &str) -> Result<String, WindowError> {
    let output = Command::new("tmux")
        .args([
            "new-window",
            "-t", session_name,
            "-n", name,
            "-P", // print window id
            "-F", "#{window_id}",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(window_id)
}

/// Create a new window in a session and run a command in it
pub fn create_window_with_command(
    session_name: &str,
    name: &str,
    cwd: &Path,
    command: &str,
) -> Result<String, WindowError> {
    let cwd_str = cwd.to_str().unwrap_or(".");
    let output = Command::new("tmux")
        .args([
            "new-window",
            "-t", session_name,
            "-n", name,
            "-c", cwd_str,
            "-P",
            "-F", "#{window_id}",
            command,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(window_id)
}

/// Select (switch to) a window in a session
pub fn select_window(session_name: &str, window_name: &str) -> Result<(), WindowError> {
    let target = format!("{}:{}", session_name, window_name);
    let output = Command::new("tmux")
        .args(["select-window", "-t", &target])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    Ok(())
}

/// Kill a window
pub fn kill_window(target: &str) -> Result<(), WindowError> {
    let output = Command::new("tmux")
        .args(["kill-window", "-t", target])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    Ok(())
}

/// Rename a window
pub fn rename_window(target: &str, new_name: &str) -> Result<(), WindowError> {
    let output = Command::new("tmux")
        .args([
            "rename-window",
            "-t", target,
            new_name,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowError::CommandFailed(stderr.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: WindowInfo creation
    #[test]
    fn test_window_info_creation() {
        let window = WindowInfo::new("@0", "control-tower", "sdlc-test", true);
        assert_eq!(window.id, "@0");
        assert_eq!(window.name, "control-tower");
        assert!(window.active);
    }

    /// Test: WindowInfo target formatting
    #[test]
    fn test_window_target_formatting() {
        let window = WindowInfo::new("@1", "review", "my-session", false);
        assert_eq!(window.target(), "my-session:@1");
    }

    /// Test: API functions exist
    #[test]
    fn test_window_api_exists() {
        let _list_fn: fn(&str) -> Result<Vec<WindowInfo>, WindowError> = list_windows;
        let _create_fn: fn(&str, &str) -> Result<String, WindowError> = create_window;
        let _rename_fn: fn(&str, &str) -> Result<(), WindowError> = rename_window;
    }

    /// Test: WindowError construction
    #[test]
    fn test_window_error() {
        let err = WindowError::NotFound("@99".to_string());
        assert!(err.to_string().contains("@99"));
    }
}
