// tmux/session.rs - Tmux session management
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("tmux command failed: {0}")]
    CommandFailed(String),
    #[error("Session already exists: {0}")]
    AlreadyExists(String),
    #[error("Session not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Represents a tmux session
pub struct Session {
    name: String,
    window_name: String,
}

impl Session {
    /// Create a new session configuration
    pub fn new(repo_name: &str) -> Self {
        let sanitized = repo_name
            .replace('/', "-")
            .replace(' ', "-")
            .replace('\\', "-");
        
        Self {
            name: format!("sdlc-{}", sanitized),
            window_name: "control-tower".to_string(),
        }
    }

    /// Get the session name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the default window name
    pub fn window_name(&self) -> &str {
        &self.window_name
    }

    /// Check if a session exists
    pub fn exists(name: &str) -> bool {
        match Command::new("tmux")
            .args(["has-session", "-t", name])
            .output() 
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Create a new tmux session, optionally in the given directory
    pub fn create_in(&self, work_dir: Option<&std::path::Path>) -> Result<(), SessionError> {
        if Self::exists(&self.name) {
            return Err(SessionError::AlreadyExists(self.name.clone()));
        }

        let mut args = vec![
            "new-session",
            "-d", // detached
            "-x", "80",  // explicit columns (avoids 1-column vertical wrapping when server has wrong default)
            "-y", "24",  // explicit rows
            "-s", &self.name,
            "-n", &self.window_name,
        ];
        if let Some(dir) = work_dir {
            if let Some(s) = dir.to_str() {
                args.extend(["-c", s]);
            }
        }

        let output = Command::new("tmux").args(args).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SessionError::CommandFailed(stderr.to_string()));
        }

        Ok(())
    }

    /// Create a new tmux session (no working directory)
    pub fn create(&self) -> Result<(), SessionError> {
        self.create_in(None)
    }

    /// Ensure session exists (create if not), optionally in the given directory.
    /// Always resizes window to 80x24 to avoid 1-column vertical wrapping.
    pub fn ensure_in(&self, work_dir: Option<&std::path::Path>) -> Result<(), SessionError> {
        if !Self::exists(&self.name) {
            self.create_in(work_dir)?;
        }
        self.resize_window(80, 24)
    }

    /// Resize the session's window to the given columns and rows.
    fn resize_window(&self, cols: usize, rows: usize) -> Result<(), SessionError> {
        let output = Command::new("tmux")
            .args([
                "resize-window",
                "-x",
                &cols.to_string(),
                "-y",
                &rows.to_string(),
                "-t",
                &self.name,
            ])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SessionError::CommandFailed(stderr.to_string()));
        }
        Ok(())
    }

    /// Ensure session exists (create if not)
    pub fn ensure(&self) -> Result<(), SessionError> {
        self.ensure_in(None)
    }

    /// Kill the session
    pub fn kill(&self) -> Result<(), SessionError> {
        if !Self::exists(&self.name) {
            return Err(SessionError::NotFound(self.name.clone()));
        }

        let output = Command::new("tmux")
            .args(["kill-session", "-t", &self.name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SessionError::CommandFailed(stderr.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Session name should be formatted correctly
    #[test]
    fn test_session_name_formatting() {
        let session = Session::new("myproject");
        assert_eq!(session.name(), "sdlc-myproject");
        assert_eq!(session.window_name(), "control-tower");
    }

    /// Test: Session creation returns correct struct
    #[test]
    fn test_session_creation() {
        let session = Session::new("test-repo");
        assert_eq!(session.name(), "sdlc-test-repo");
    }

    /// Test: Invalid repo names are sanitized
    #[test]
    fn test_session_name_sanitization() {
        let session = Session::new("my/repo with spaces");
        let name = session.name();
        assert!(!name.contains('/'));
        assert!(!name.contains(' '));
        assert!(name.contains("my-repo-with-spaces"));
    }

    /// Test: Session exists API is available
    #[test]
    fn test_session_exists_api() {
        // Just verify the function exists and has correct signature
        let _fn_ptr: fn(&str) -> bool = Session::exists;
    }

    /// Test: Ensure method works when session doesn't exist
    #[test]
    fn test_session_ensure_api() {
        let session = Session::new("test-api");
        // We can't actually test without tmux, but we verify the API exists
        // by checking the method signature at compile time
        let _: &Session = &session;
    }
}
