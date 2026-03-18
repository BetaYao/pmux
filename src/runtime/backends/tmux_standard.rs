//! Tmux Standard Mode backend - session-per-worktree, window-per-terminal.
//!
//! Each worktree maps to a tmux session named `pmux-<repo>-<branch>`.
//! Each terminal maps to a tmux window (single pane per window = independent PTY).
//! We own the PTY master via `portable-pty` for direct byte-level I/O.
//! tmux only provides session persistence (survives GUI restarts).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};

/// Sanitize string for tmux session/window names (replace non-alphanumeric except - and _ with -)
fn sanitize_tmux_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Derive a tmux session name from repo path and branch.
/// Format: pmux-<repo_dir>-<branch>
pub fn session_name(repo_path: &Path, branch: &str) -> String {
    let repo_dir = repo_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());
    let sanitized_repo = sanitize_tmux_name(&repo_dir);
    let sanitized_branch = sanitize_tmux_name(branch);
    format!("pmux-{}-{}", sanitized_repo, sanitized_branch)
}

/// Format a tmux window target string: "session:index"
pub fn window_target(session: &str, window_index: u32) -> String {
    format!("{}:{}", session, window_index)
}

/// Per-window state: owns the PTY and I/O threads
#[allow(dead_code)]
struct WindowState {
    window_index: u32,
    pane_id: PaneId,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    input_tx: flume::Sender<Vec<u8>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
}

/// Tmux Standard Mode backend.
///
/// Uses tmux for session persistence only. PTY I/O goes through portable-pty directly.
pub struct TmuxStandardBackend {
    session_name: String,
    worktree_path: PathBuf,
    windows: Mutex<HashMap<PaneId, Arc<WindowState>>>,
    window_counter: AtomicUsize,
    default_cols: u16,
    default_rows: u16,
}

impl TmuxStandardBackend {
    /// Create or attach to a tmux session, then create the primary window.
    pub fn new(
        session_name: &str,
        worktree_path: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Self, RuntimeError> {
        // Ensure tmux session exists (create if needed)
        ensure_tmux_session(session_name, worktree_path)?;

        let backend = Self {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows: Mutex::new(HashMap::new()),
            window_counter: AtomicUsize::new(0),
            default_cols: cols,
            default_rows: rows,
        };

        // Create primary window
        backend.create_window("main")?;

        Ok(backend)
    }

    /// Create a new window with its own PTY.
    fn create_window(&self, name: &str) -> Result<PaneId, RuntimeError> {
        let window_idx = self.window_counter.fetch_add(1, Ordering::SeqCst) as u32;
        let pane_id = format!("tmux-std:{}:{}:{}", self.session_name, window_idx, name);

        // Open PTY pair
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: self.default_rows,
                cols: self.default_cols,
                pixel_width: (self.default_cols).saturating_mul(8),
                pixel_height: (self.default_rows).saturating_mul(17),
            })
            .map_err(|e| RuntimeError::Backend(format!("openpty: {}", e)))?;

        // Spawn shell in worktree directory
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&self.worktree_path);

        let child: Box<dyn portable_pty::Child + Send + Sync> = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::Backend(format!("spawn: {}", e)))?;

        let master = pair.master;

        // Set up output reader thread
        let reader = master
            .try_clone_reader()
            .map_err(|e| RuntimeError::Backend(format!("clone_reader: {}", e)))?;
        let (output_tx, output_rx) = flume::unbounded();

        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut reader = reader;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Set up input writer thread (batched writes)
        let writer = master
            .take_writer()
            .map_err(|e| RuntimeError::Backend(format!("take_writer: {}", e)))?;
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();

        thread::spawn(move || {
            let mut writer = writer;
            let mut buffer = Vec::new();
            loop {
                match input_rx.recv() {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        while let Ok(bytes) = input_rx.try_recv() {
                            buffer.extend_from_slice(&bytes);
                        }
                        if writer.write_all(&buffer).is_err() || writer.flush().is_err() {
                            break;
                        }
                        buffer.clear();
                    }
                    Err(_) => break,
                }
            }
        });

        // Create corresponding tmux window for persistence
        create_tmux_window(&self.session_name, name);

        let state = Arc::new(WindowState {
            window_index: window_idx,
            pane_id: pane_id.clone(),
            master: Mutex::new(master),
            input_tx,
            output_rx: Mutex::new(Some(output_rx)),
            cols: AtomicU16::new(self.default_cols),
            rows: AtomicU16::new(self.default_rows),
            _child: Mutex::new(Some(child)),
        });

        if let Ok(mut windows) = self.windows.lock() {
            windows.insert(pane_id.clone(), state);
        }

        Ok(pane_id)
    }

    /// Get a reference to a window by pane ID.
    fn get_window(&self, pane_id: &PaneId) -> Option<Arc<WindowState>> {
        self.windows.lock().ok()?.get(pane_id).cloned()
    }

    /// List all pane IDs.
    fn list_all_panes(&self) -> Vec<PaneId> {
        self.windows
            .lock()
            .ok()
            .map(|w| w.keys().cloned().collect())
            .unwrap_or_default()
    }
}

impl AgentRuntime for TmuxStandardBackend {
    fn backend_type(&self) -> &'static str {
        "tmux-standard"
    }

    fn primary_pane_id(&self) -> Option<PaneId> {
        self.list_all_panes().first().cloned()
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        let window = self
            .get_window(pane_id)
            .ok_or_else(|| RuntimeError::PaneNotFound(pane_id.clone()))?;
        window
            .input_tx
            .send(bytes.to_vec())
            .map_err(|e| RuntimeError::Backend(e.to_string()))
    }

    fn send_key(
        &self,
        pane_id: &PaneId,
        key: &str,
        _use_literal: bool,
    ) -> Result<(), RuntimeError> {
        self.send_input(pane_id, key.as_bytes())
    }

    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        let window = self
            .get_window(pane_id)
            .ok_or_else(|| RuntimeError::PaneNotFound(pane_id.clone()))?;
        window.cols.store(cols, Ordering::SeqCst);
        window.rows.store(rows, Ordering::SeqCst);
        let guard = window
            .master
            .lock()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        guard
            .resize(PtySize {
                rows,
                cols,
                pixel_width: cols.saturating_mul(8),
                pixel_height: rows.saturating_mul(17),
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()))
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        let window = self.get_window(pane_id)?;
        window.output_rx.lock().ok().and_then(|mut g| g.take())
    }

    fn capture_initial_content(&self, _pane_id: &PaneId) -> Option<Vec<u8>> {
        // Direct PTY - no initial content to capture (stream starts from creation)
        None
    }

    fn list_panes(&self, _agent_id: &AgentId) -> Vec<PaneId> {
        self.list_all_panes()
    }

    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        if self.get_window(pane_id).is_some() {
            Ok(())
        } else {
            Err(RuntimeError::PaneNotFound(pane_id.clone()))
        }
    }

    fn split_pane(&self, _pane_id: &PaneId, _vertical: bool) -> Result<PaneId, RuntimeError> {
        // "split" creates a new tmux window (UI layer handles visual splitting)
        let idx = self.window_counter.load(Ordering::SeqCst);
        self.create_window(&format!("pane{}", idx))
    }

    fn kill_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        let mut windows = self
            .windows
            .lock()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        if let Some(window) = windows.remove(pane_id) {
            // Also kill the corresponding tmux window
            let target = window_target(&self.session_name, window.window_index);
            let _ = std::process::Command::new("tmux")
                .args(["kill-window", "-t", &target])
                .output();
            Ok(())
        } else {
            Err(RuntimeError::PaneNotFound(pane_id.clone()))
        }
    }

    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16) {
        if let Some(window) = self.get_window(pane_id) {
            (
                window.cols.load(Ordering::SeqCst),
                window.rows.load(Ordering::SeqCst),
            )
        } else {
            (self.default_cols, self.default_rows)
        }
    }

    fn open_diff(
        &self,
        worktree: &Path,
        _pane_id: Option<&PaneId>,
    ) -> Result<String, RuntimeError> {
        let idx = self.window_counter.load(Ordering::SeqCst);
        let diff_pane_id = self.create_window(&format!("diff{}", idx))?;

        let worktree_str = worktree.to_string_lossy();
        let cmd = format!(
            "nvim -c 'DiffviewOpen main...HEAD' '{}' 2>/dev/null || git diff main...HEAD --color=always | less -R\n",
            worktree_str
        );
        self.send_input(&diff_pane_id, cmd.as_bytes())?;

        Ok(diff_pane_id)
    }

    fn open_review(&self, worktree: &Path) -> Result<String, RuntimeError> {
        self.open_diff(worktree, None)
    }

    fn kill_window(&self, window_target_str: &str) -> Result<(), RuntimeError> {
        let status = std::process::Command::new("tmux")
            .args(["kill-window", "-t", window_target_str])
            .status()
            .map_err(|e| RuntimeError::Backend(format!("tmux kill-window: {}", e)))?;
        if status.success() {
            Ok(())
        } else {
            Err(RuntimeError::Backend(format!(
                "tmux kill-window -t {} failed",
                window_target_str
            )))
        }
    }

    fn session_info(&self) -> Option<(String, String)> {
        Some((self.session_name.clone(), "main".to_string()))
    }
}

/// Ensure a tmux session exists; create it if not.
fn ensure_tmux_session(session_name: &str, worktree_path: &Path) -> Result<(), RuntimeError> {
    // Check if session already exists
    let check = std::process::Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .map_err(|e| RuntimeError::Backend(format!("tmux has-session: {}", e)))?;

    if check.status.success() {
        return Ok(());
    }

    // Create new detached session
    let create = std::process::Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            session_name,
            "-c",
            &worktree_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| RuntimeError::Backend(format!("tmux new-session: {}", e)))?;

    if create.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&create.stderr);
        Err(RuntimeError::Backend(format!(
            "tmux new-session failed: {}",
            stderr.trim()
        )))
    }
}

/// Create a tmux window within the session (best-effort, for persistence tracking).
fn create_tmux_window(session_name: &str, window_name: &str) {
    let _ = std::process::Command::new("tmux")
        .args([
            "new-window",
            "-t",
            session_name,
            "-n",
            window_name,
        ])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_name() {
        let path = Path::new("/home/user/repos/my-project");
        let name = session_name(path, "feature-branch");
        assert_eq!(name, "pmux-my-project-feature-branch");
    }

    #[test]
    fn test_session_name_sanitizes() {
        let path = Path::new("/home/user/repos/my.project");
        let name = session_name(path, "feat/thing:v2");
        assert_eq!(name, "pmux-my-project-feat-thing-v2");
    }

    #[test]
    fn test_session_name_with_main_branch() {
        let path = Path::new("/home/user/repos/cool-repo");
        let name = session_name(path, "main");
        assert_eq!(name, "pmux-cool-repo-main");
    }

    #[test]
    fn test_window_target_format() {
        let target = window_target("pmux-repo-main", 0);
        assert_eq!(target, "pmux-repo-main:0");
    }

    #[test]
    fn test_window_target_with_index() {
        let target = window_target("pmux-repo-feat", 3);
        assert_eq!(target, "pmux-repo-feat:3");
    }

    #[test]
    fn test_sanitize_tmux_name_basic() {
        assert_eq!(sanitize_tmux_name("hello-world_123"), "hello-world_123");
    }

    #[test]
    fn test_sanitize_tmux_name_special_chars() {
        assert_eq!(sanitize_tmux_name("a.b:c/d"), "a-b-c-d");
    }

    #[test]
    fn test_sanitize_tmux_name_spaces() {
        assert_eq!(sanitize_tmux_name("my project"), "my-project");
    }

    /// Integration test: create a tmux session, verify it exists, clean up.
    /// Requires tmux to be installed.
    #[test]
    fn test_create_and_list_session() {
        // Skip if tmux not available
        if !super::super::tmux_available() {
            eprintln!("skipping test_create_and_list_session: tmux not available");
            return;
        }

        let test_session = "pmux-test-standard-backend";
        let dir = tempfile::tempdir().unwrap();

        // Clean up any leftover session from previous test runs
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", test_session])
            .output();

        // Create session
        let result = ensure_tmux_session(test_session, dir.path());
        assert!(result.is_ok(), "ensure_tmux_session failed: {:?}", result);

        // Verify session exists
        let check = std::process::Command::new("tmux")
            .args(["has-session", "-t", test_session])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "session {} should exist after creation",
            test_session
        );

        // Clean up
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", test_session])
            .output();
    }
}
