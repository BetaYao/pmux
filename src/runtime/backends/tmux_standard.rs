//! Tmux Standard Mode backend - session-per-worktree, window-per-terminal.
//!
//! Each worktree maps to a tmux session named `pmux-<repo>-<branch>`.
//! Each terminal maps to a tmux window (single pane per window = independent PTY).
//! We own the PTY master via `portable-pty` for direct byte-level I/O.
//! tmux only provides session persistence (survives GUI restarts).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::PtySize;

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
    /// PTY master - None for recovered windows (tmux send-keys used instead)
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    input_tx: flume::Sender<Vec<u8>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
    /// Session name, needed for send-keys in recovered mode
    session_name: String,
    /// Whether this window was recovered (no PTY master)
    recovered: bool,
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
    /// Create or attach to a tmux session.
    /// If session already exists with windows, recovers them instead of creating new ones.
    pub fn new(
        session_name: &str,
        worktree_path: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Self, RuntimeError> {
        // Check if session already exists with windows
        let session_exists = Command::new("tmux")
            .args(["has-session", "-t", session_name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if session_exists {
            // Session exists — recover existing windows instead of creating new ones
            let existing = Self::discover_windows(session_name)?;
            if !existing.is_empty() {
                return Self::recover(session_name, worktree_path, cols, rows);
            }
        }

        // No existing session or no windows — create fresh
        ensure_tmux_session(session_name, worktree_path)?;

        let backend = Self {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows: Mutex::new(HashMap::new()),
            window_counter: AtomicUsize::new(0),
            default_cols: cols,
            default_rows: rows,
        };

        // Create primary window (portable-pty + tracking tmux window)
        backend.create_window("main")?;

        Ok(backend)
    }

    /// Create a new window backed by the tmux session's shell.
    ///
    /// Uses tmux's own PTY (via send-keys for input, pipe-pane for output).
    /// This ensures the shell survives backend drops (tab switches) and can
    /// be recovered later.
    fn create_window(&self, name: &str) -> Result<PaneId, RuntimeError> {
        let window_idx = self.window_counter.fetch_add(1, Ordering::SeqCst) as u32;
        let pane_id = format!("tmux-std:{}:{}:{}", self.session_name, window_idx, name);

        // For the first window (index 0), reuse the initial tmux window.
        // For subsequent windows, create a new tmux window.
        let tmux_window_index: u32;
        if window_idx == 0 {
            // Rename initial tmux window (created by new-session)
            let _ = Command::new("tmux")
                .args([
                    "rename-window",
                    "-t",
                    &format!("{}:0", self.session_name),
                    name,
                ])
                .output();
            tmux_window_index = 0;
        } else {
            // Create new tmux window
            let output = Command::new("tmux")
                .args([
                    "new-window",
                    "-t",
                    &self.session_name,
                    "-n",
                    name,
                    "-c",
                    &self.worktree_path.to_string_lossy(),
                    "-P",
                    "-F",
                    "#{window_index}",
                ])
                .output()
                .map_err(|e| RuntimeError::Backend(format!("tmux new-window: {}", e)))?;
            tmux_window_index = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse()
                .unwrap_or(window_idx);
        }

        let target = window_target(&self.session_name, tmux_window_index);

        // Set up output via pipe-pane polling (capture-pane)
        let (output_tx, output_rx) = flume::unbounded::<Vec<u8>>();
        let target_for_output = target.clone();
        thread::spawn(move || {
            let mut last_content = String::new();
            loop {
                thread::sleep(std::time::Duration::from_millis(50));
                let result = Command::new("tmux")
                    .args(["capture-pane", "-t", &target_for_output, "-p", "-e"])
                    .output();
                match result {
                    Ok(output) if output.status.success() => {
                        let content = String::from_utf8_lossy(&output.stdout).to_string();
                        if content != last_content {
                            if output_tx.send(content.as_bytes().to_vec()).is_err() {
                                break;
                            }
                            last_content = content;
                        }
                    }
                    _ => break,
                }
            }
        });

        // Set up input via tmux send-keys
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();
        let target_for_input = target.clone();
        thread::spawn(move || {
            loop {
                match input_rx.recv() {
                    Ok(bytes) => {
                        // Collect additional pending bytes
                        let mut all_bytes = bytes;
                        while let Ok(more) = input_rx.try_recv() {
                            all_bytes.extend_from_slice(&more);
                        }
                        // Send via tmux send-keys with literal flag
                        let text = String::from_utf8_lossy(&all_bytes).to_string();
                        let _ = Command::new("tmux")
                            .args(["send-keys", "-t", &target_for_input, "-l", &text])
                            .output();
                    }
                    Err(_) => break,
                }
            }
        });

        let state = Arc::new(WindowState {
            window_index: tmux_window_index,
            pane_id: pane_id.clone(),
            master: Mutex::new(None),
            input_tx,
            output_rx: Mutex::new(Some(output_rx)),
            cols: AtomicU16::new(self.default_cols),
            rows: AtomicU16::new(self.default_rows),
            _child: Mutex::new(None),
            session_name: self.session_name.clone(),
            recovered: false,
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

    /// Discover all pmux tmux sessions.
    pub fn discover_sessions() -> Vec<String> {
        let output = match Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("pmux-"))
            .map(|s| s.to_string())
            .collect()
    }

    /// Discover windows in an existing session.
    /// Returns a vec of (window_index, window_name, pane_current_path).
    pub fn discover_windows(session: &str) -> Result<Vec<(u32, String, String)>, RuntimeError> {
        let output = Command::new("tmux")
            .args([
                "list-windows",
                "-t",
                session,
                "-F",
                "#{window_index}:#{window_name}:#{pane_current_path}",
            ])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("list-windows failed: {e}")))?;

        let windows = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() == 3 {
                    Some((
                        parts[0].parse().unwrap_or(0),
                        parts[1].to_string(),
                        parts[2].to_string(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        Ok(windows)
    }

    /// Recover a backend by attaching to an existing tmux session.
    ///
    /// Discovered windows are tracked without PTY masters. Input uses `tmux send-keys`,
    /// output uses `tmux pipe-pane` to stream data back.
    pub fn recover(
        session_name: &str,
        worktree_path: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Self, RuntimeError> {
        // Verify the session exists
        let check = Command::new("tmux")
            .args(["has-session", "-t", session_name])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux has-session: {e}")))?;

        if !check.status.success() {
            return Err(RuntimeError::Backend(format!(
                "tmux session '{}' does not exist",
                session_name
            )));
        }

        let discovered = Self::discover_windows(session_name)?;

        let backend = Self {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows: Mutex::new(HashMap::new()),
            window_counter: AtomicUsize::new(0),
            default_cols: cols,
            default_rows: rows,
        };

        for (window_index, window_name, _path) in &discovered {
            let pane_id = format!(
                "tmux-std:{}:{}:{}",
                session_name, window_index, window_name
            );

            // Set up pipe-pane for output streaming
            let (output_tx, output_rx) = flume::unbounded::<Vec<u8>>();
            let target = window_target(session_name, *window_index);

            // Use pipe-pane to capture output from the existing tmux window.
            // We spawn a background thread that reads from a pipe-pane subprocess.
            let target_clone = target.clone();
            thread::spawn(move || {
                // Use tmux capture-pane in a loop as a simple polling mechanism
                // for recovered windows. pipe-pane requires a file target, so we
                // use periodic capture-pane -p instead.
                let mut last_content = String::new();
                loop {
                    thread::sleep(std::time::Duration::from_millis(100));
                    let result = Command::new("tmux")
                        .args(["capture-pane", "-t", &target_clone, "-p"])
                        .output();
                    match result {
                        Ok(output) if output.status.success() => {
                            let content = String::from_utf8_lossy(&output.stdout).to_string();
                            if content != last_content {
                                let diff_bytes = content.as_bytes().to_vec();
                                if output_tx.send(diff_bytes).is_err() {
                                    break;
                                }
                                last_content = content;
                            }
                        }
                        _ => break,
                    }
                }
            });

            // Set up input via tmux send-keys
            let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();
            let target_for_input = target.clone();
            thread::spawn(move || {
                loop {
                    match input_rx.recv() {
                        Ok(bytes) => {
                            // Collect any additional pending bytes
                            let mut all_bytes = bytes;
                            while let Ok(more) = input_rx.try_recv() {
                                all_bytes.extend_from_slice(&more);
                            }
                            // Send via tmux send-keys with literal flag
                            let text = String::from_utf8_lossy(&all_bytes).to_string();
                            let _ = Command::new("tmux")
                                .args(["send-keys", "-t", &target_for_input, "-l", &text])
                                .output();
                        }
                        Err(_) => break,
                    }
                }
            });

            let idx = backend.window_counter.fetch_add(1, Ordering::SeqCst) as u32;
            let _ = idx; // We use the discovered window_index instead

            let state = Arc::new(WindowState {
                window_index: *window_index,
                pane_id: pane_id.clone(),
                master: Mutex::new(None),
                input_tx,
                output_rx: Mutex::new(Some(output_rx)),
                cols: AtomicU16::new(cols),
                rows: AtomicU16::new(rows),
                _child: Mutex::new(None),
                session_name: session_name.to_string(),
                recovered: true,
            });

            if let Ok(mut windows) = backend.windows.lock() {
                windows.insert(pane_id, state);
            }
        }

        // Update window_counter to be past the highest discovered index
        let max_idx = discovered.iter().map(|(idx, _, _)| *idx).max().unwrap_or(0);
        backend
            .window_counter
            .store((max_idx + 1) as usize, Ordering::SeqCst);

        Ok(backend)
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
        if let Some(ref master) = *guard {
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: cols.saturating_mul(8),
                    pixel_height: rows.saturating_mul(17),
                })
                .map_err(|e| RuntimeError::Backend(e.to_string()))
        } else {
            // Recovered window: resize via tmux
            let target = window_target(&window.session_name, window.window_index);
            let _ = Command::new("tmux")
                .args([
                    "resize-window",
                    "-t",
                    &target,
                    "-x",
                    &cols.to_string(),
                    "-y",
                    &rows.to_string(),
                ])
                .output();
            Ok(())
        }
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
        // Return the current tmux window name (for worktree matching on recovery)
        let output = Command::new("tmux")
            .args([
                "display-message",
                "-t",
                &self.session_name,
                "-p",
                "#{window_name}",
            ])
            .output()
            .ok()?;
        let window_name = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();
        Some((
            self.session_name.clone(),
            if window_name.is_empty() {
                "main".to_string()
            } else {
                window_name
            },
        ))
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

    #[test]
    fn test_discover_sessions() {
        // Skip if tmux not available
        if !super::super::tmux_available() {
            eprintln!("skipping test_discover_sessions: tmux not available");
            return;
        }

        // Result is a Vec<String>, all starting with "pmux-"
        let sessions = TmuxStandardBackend::discover_sessions();
        for s in &sessions {
            assert!(s.starts_with("pmux-"), "session '{}' should start with pmux-", s);
        }
    }

    #[test]
    fn test_discover_existing_sessions() {
        // Skip if tmux not available
        if !super::super::tmux_available() {
            eprintln!("skipping test_discover_existing_sessions: tmux not available");
            return;
        }

        let session = "pmux-test-recovery";
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();

        Command::new("tmux")
            .args(["new-session", "-d", "-s", session, "-x", "120", "-y", "36"])
            .output()
            .unwrap();
        Command::new("tmux")
            .args(["new-window", "-t", session])
            .output()
            .unwrap();

        let windows = TmuxStandardBackend::discover_windows(session).unwrap();
        assert_eq!(windows.len(), 2);

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();
    }

    /// Test that new() recovers existing sessions instead of creating new windows
    #[test]
    fn test_new_recovers_existing_session() {
        if !super::super::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }

        let session = "pmux-test-new-recover";
        let dir = tempfile::tempdir().unwrap();

        // Clean up
        let _ = Command::new("tmux").args(["kill-session", "-t", session]).output();

        // First new() — creates session + window
        let backend1 = TmuxStandardBackend::new(session, dir.path(), 80, 24).unwrap();
        let pane1 = backend1.primary_pane_id().unwrap();
        drop(backend1);

        // Verify session still exists (tmux session persists after drop)
        let check = Command::new("tmux")
            .args(["has-session", "-t", session])
            .output()
            .unwrap();
        assert!(check.status.success(), "session should persist after drop");

        // Count windows before second new()
        let windows_before = TmuxStandardBackend::discover_windows(session).unwrap();
        let count_before = windows_before.len();

        // Second new() — should RECOVER, not create a new window
        let backend2 = TmuxStandardBackend::new(session, dir.path(), 80, 24).unwrap();
        let pane2 = backend2.primary_pane_id().unwrap();

        // Should NOT have created additional windows
        let windows_after = TmuxStandardBackend::discover_windows(session).unwrap();
        assert_eq!(
            windows_after.len(),
            count_before,
            "new() should recover existing windows, not create new ones. Before: {}, After: {}",
            count_before,
            windows_after.len()
        );

        drop(backend2);
        let _ = Command::new("tmux").args(["kill-session", "-t", session]).output();
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
