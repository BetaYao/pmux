//! Session backend selection: Auto/Dtach/Tmux/Screen/Local with availability checks.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionBackend {
    #[default]
    Auto,
    Dtach,
    Tmux,
    Screen,
    Local,
}

impl SessionBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dtach => "dtach",
            Self::Tmux => "tmux",
            Self::Screen => "screen",
            Self::Local => "local",
        }
    }

    /// Resolve Auto to a concrete backend by checking availability.
    /// Priority: dtach > tmux > screen > local
    pub fn resolve(&self) -> ResolvedBackend {
        match self {
            Self::Auto => {
                if is_dtach_available() {
                    ResolvedBackend::Dtach
                } else if is_tmux_available() {
                    ResolvedBackend::Tmux
                } else if is_screen_available() {
                    ResolvedBackend::Screen
                } else {
                    ResolvedBackend::Local
                }
            }
            Self::Dtach => {
                if is_dtach_available() {
                    ResolvedBackend::Dtach
                } else {
                    ResolvedBackend::Local
                }
            }
            Self::Tmux => {
                if is_tmux_available() {
                    ResolvedBackend::Tmux
                } else {
                    ResolvedBackend::Local
                }
            }
            Self::Screen => {
                if is_screen_available() {
                    ResolvedBackend::Screen
                } else {
                    ResolvedBackend::Local
                }
            }
            Self::Local => ResolvedBackend::Local,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBackend {
    Dtach,
    Tmux,
    Screen,
    Local,
}

impl ResolvedBackend {
    pub fn supports_persistence(&self) -> bool {
        !matches!(self, Self::Local)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dtach => "dtach",
            Self::Tmux => "tmux",
            Self::Screen => "screen",
            Self::Local => "local",
        }
    }
}

pub fn is_dtach_available() -> bool {
    std::process::Command::new("dtach")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

pub fn is_tmux_available() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn is_screen_available() -> bool {
    std::process::Command::new("screen")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Path for dtach socket files
pub fn dtach_socket_path(session_name: &str) -> PathBuf {
    let dir = dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("pmux");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}.sock", session_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_backend_default_is_auto() {
        assert_eq!(SessionBackend::default(), SessionBackend::Auto);
    }

    #[test]
    fn test_local_resolves_to_local() {
        assert_eq!(SessionBackend::Local.resolve(), ResolvedBackend::Local);
    }

    #[test]
    fn test_resolved_backend_supports_persistence() {
        assert!(!ResolvedBackend::Local.supports_persistence());
        assert!(ResolvedBackend::Tmux.supports_persistence());
        assert!(ResolvedBackend::Dtach.supports_persistence());
        assert!(ResolvedBackend::Screen.supports_persistence());
    }

    #[test]
    fn test_dtach_socket_path() {
        let path = dtach_socket_path("pmux-test");
        assert!(path.to_string_lossy().contains("pmux-test.sock"));
    }
}
