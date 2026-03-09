//! Local PTY backend - spawns shell directly in a PTY, no tmux.
//!
//! Supports multiple panes per worktree. True PTY write for input, direct read for output.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};
/// Single pane PTY instance. (WIP: multi-pane support)
#[allow(dead_code)]
struct LocalPtyPane {
    pane_id: PaneId,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    input_tx: flume::Sender<Vec<u8>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
}

/// Local PTY runtime - one shell per worktree, direct PTY read/write.
/// Input is queued via flume channel; a dedicated writer thread drains the queue
/// and writes to the PTY, so send_input never blocks the UI thread.
pub struct LocalPtyRuntime {
    worktree_path: std::path::PathBuf,
    pane_id: PaneId,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    /// Sender for input queue - writer thread owns the receiver and PTY writer
    input_tx: flume::Sender<Vec<u8>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
}

/// Local PTY Agent - manages multiple panes for a single worktree.
/// Each pane has its own PTY and shell process.
pub struct LocalPtyAgent {
    worktree_path: std::path::PathBuf,
    panes: Mutex<HashMap<PaneId, Arc<LocalPtyPane>>>,
    pane_counter: AtomicUsize,
    cols: u16,
    rows: u16,
}

impl LocalPtyAgent {
    /// Create a new LocalPtyAgent for the given worktree.
    /// Initializes with a single primary pane.
    pub fn new(worktree_path: &Path, cols: u16, rows: u16) -> Result<Self, RuntimeError> {
        let agent = Self {
            worktree_path: worktree_path.to_path_buf(),
            panes: Mutex::new(HashMap::new()),
            pane_counter: AtomicUsize::new(0),
            cols,
            rows,
        };

        // Create primary pane
        agent.create_pane("main")?;

        Ok(agent)
    }

    /// Create a new pane with the given name suffix.
    fn create_pane(&self, name_suffix: &str) -> Result<PaneId, RuntimeError> {
        let pane_id = format!("local:{}:{}", self.worktree_path.display(), name_suffix);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: self.rows,
                cols: self.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(&self.worktree_path);

        let child: Box<dyn portable_pty::Child + Send + Sync> = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let (output_tx, output_rx) = flume::unbounded();
        let reader = master
            .try_clone_reader()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        // Reader thread
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

        // Writer thread (batched: recv first, try_recv rest, write+flush once)
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

        let pane = Arc::new(LocalPtyPane {
            pane_id: pane_id.clone(),
            master: Mutex::new(master),
            input_tx,
            output_rx: Mutex::new(Some(output_rx)),
            cols: AtomicU16::new(self.cols),
            rows: AtomicU16::new(self.rows),
            _child: Mutex::new(Some(child)),
        });

        if let Ok(mut panes) = self.panes.lock() {
            panes.insert(pane_id.clone(), pane);
        }

        Ok(pane_id)
    }

    /// Get a reference to a pane by ID.
    fn get_pane(&self, pane_id: &PaneId) -> Option<Arc<LocalPtyPane>> {
        self.panes.lock().ok()?.get(pane_id).cloned()
    }

    /// List all pane IDs.
    fn list_all_panes(&self) -> Vec<PaneId> {
        self.panes
            .lock()
            .ok()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default()
    }
}

impl AgentRuntime for LocalPtyAgent {
    fn backend_type(&self) -> &'static str {
        "local"
    }

    fn primary_pane_id(&self) -> Option<PaneId> {
        // Return the first pane (usually "main")
        self.list_all_panes().first().cloned()
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        let pane = self
            .get_pane(pane_id)
            .ok_or_else(|| RuntimeError::PaneNotFound(pane_id.clone()))?;
        pane.input_tx
            .send(bytes.to_vec())
            .map_err(|e| RuntimeError::Backend(e.to_string()))
    }

    fn send_key(&self, pane_id: &PaneId, key: &str, _use_literal: bool) -> Result<(), RuntimeError> {
        self.send_input(pane_id, key.as_bytes())
    }

    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        let pane = self
            .get_pane(pane_id)
            .ok_or_else(|| RuntimeError::PaneNotFound(pane_id.clone()))?;
        pane.cols.store(cols, Ordering::SeqCst);
        pane.rows.store(rows, Ordering::SeqCst);
        let guard = pane
            .master
            .lock()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        // BUG-11: Provide approximate pixel dimensions instead of 0.
        // At 12pt Menlo: cell ≈ 7.2px wide, line ≈ 17px tall.
        // Exact values aren't critical (few programs use ws_xpixel/ws_ypixel),
        // but nonzero is better for sixel/image protocols.
        guard
            .resize(PtySize {
                rows,
                cols,
                pixel_width: (cols as u16).saturating_mul(8),
                pixel_height: (rows as u16).saturating_mul(17),
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()))
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        let pane = self.get_pane(pane_id)?;
        pane.output_rx.lock().ok().and_then(|mut g| g.take())
    }

    fn capture_initial_content(&self, _pane_id: &PaneId) -> Option<Vec<u8>> {
        None
    }

    fn list_panes(&self, _agent_id: &AgentId) -> Vec<PaneId> {
        self.list_all_panes()
    }

    fn focus_pane(&self, _pane_id: &PaneId) -> Result<(), RuntimeError> {
        // Local PTY doesn't need explicit focus, just selection tracking
        Ok(())
    }

    fn split_pane(&self, _pane_id: &PaneId, _vertical: bool) -> Result<PaneId, RuntimeError> {
        // Create a new pane with indexed name
        let idx = self.pane_counter.fetch_add(1, Ordering::SeqCst);
        self.create_pane(&format!("pane{}", idx))
    }

    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16) {
        if let Some(pane) = self.get_pane(pane_id) {
            (
                pane.cols.load(Ordering::SeqCst),
                pane.rows.load(Ordering::SeqCst),
            )
        } else {
            (self.cols, self.rows)
        }
    }

    fn open_diff(&self, worktree: &Path, _pane_id: Option<&PaneId>) -> Result<String, RuntimeError> {
        let idx = self.pane_counter.fetch_add(1, Ordering::SeqCst);
        let diff_pane_id = self.create_pane(&format!("diff{}", idx))?;

        let worktree_str = worktree.to_string_lossy();
        // Try nvim diffview first, fallback to git diff | less
        let cmd = format!(
            "nvim -c 'DiffviewOpen main...HEAD' '{}' 2>/dev/null || git diff main...HEAD --color=always | less -R\n",
            worktree_str
        );
        self.send_input(&diff_pane_id, cmd.as_bytes())?;

        Ok(diff_pane_id)
    }

    fn open_review(&self, worktree: &Path) -> Result<String, RuntimeError> {
        // Same as open_diff for local PTY
        self.open_diff(worktree, None)
    }

    fn kill_window(&self, _window_target: &str) -> Result<(), RuntimeError> {
        // Local PTY doesn't have windows to kill
        Ok(())
    }

    fn session_info(&self) -> Option<(String, String)> {
        None
    }
}

impl LocalPtyRuntime {
    /// Create a new LocalPtyRuntime by spawning a shell in the given worktree directory.
    pub fn new(worktree_path: &Path, cols: u16, rows: u16) -> Result<Self, RuntimeError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(worktree_path);

        let child: Box<dyn portable_pty::Child + Send + Sync> = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let (output_tx, output_rx) = flume::unbounded();
        let reader = master
            .try_clone_reader()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let pane_id = format!("local:{}", worktree_path.display());

        // Reader thread: PTY output -> output_tx
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

        // Input queue + writer thread: input_rx -> PTY (batched writes)
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();
        thread::spawn(move || {
            let mut writer = writer;
            let mut buf = Vec::new();
            loop {
                // Recv first chunk (blocks until available)
                match input_rx.recv() {
                    Ok(bytes) => {
                        buf.extend_from_slice(&bytes);
                        // Drain all immediately available chunks
                        while let Ok(bytes) = input_rx.try_recv() {
                            buf.extend_from_slice(&bytes);
                        }
                        // Write all at once and flush once
                        if writer.write_all(&buf).is_err() || writer.flush().is_err() {
                            break;
                        }
                        buf.clear();
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            worktree_path: worktree_path.to_path_buf(),
            pane_id: pane_id.clone(),
            master: Mutex::new(master),
            input_tx,
            output_rx: Mutex::new(Some(output_rx)),
            cols: AtomicU16::new(cols),
            rows: AtomicU16::new(rows),
            _child: Mutex::new(Some(child)),
        })
    }

    pub fn worktree_path(&self) -> &Path {
        &self.worktree_path
    }

    pub fn pane_id(&self) -> &str {
        &self.pane_id
    }
}

impl AgentRuntime for LocalPtyRuntime {
    fn backend_type(&self) -> &'static str {
        "local"
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        // #region agent log
        crate::debug_log::dbg_log(
            "local_pty.rs:send_input",
            "entry",
            &serde_json::json!({"bytes_len": bytes.len(), "pane_match": pane_id == &self.pane_id}),
            "H4",
        );
        // #endregion
        if pane_id != &self.pane_id {
            return Err(RuntimeError::PaneNotFound(pane_id.clone()));
        }
        let result = self
            .input_tx
            .send(bytes.to_vec())
            .map_err(|e| RuntimeError::Backend(e.to_string()));
        // #region agent log
        crate::debug_log::dbg_log(
            "local_pty.rs:send_input",
            "exit",
            &serde_json::json!({"ok": result.is_ok()}),
            "H4",
        );
        // #endregion
        result
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
        if pane_id != &self.pane_id {
            return Err(RuntimeError::PaneNotFound(pane_id.clone()));
        }
        self.cols.store(cols, Ordering::SeqCst);
        self.rows.store(rows, Ordering::SeqCst);
        let guard = self
            .master
            .lock()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        let _ = guard
            .resize(PtySize {
                rows,
                cols,
                pixel_width: (cols as u16).saturating_mul(8),
                pixel_height: (rows as u16).saturating_mul(17),
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()));
        Ok(())
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        if pane_id != &self.pane_id {
            return None;
        }
        self.output_rx.lock().ok().and_then(|mut g| g.take())
    }

    fn capture_initial_content(&self, _pane_id: &PaneId) -> Option<Vec<u8>> {
        None
    }

    fn list_panes(&self, agent_id: &AgentId) -> Vec<PaneId> {
        let path_str = self.worktree_path.to_string_lossy();
        if agent_id.is_empty()
            || agent_id == &self.pane_id
            || agent_id.as_str() == path_str
            || agent_id == &format!("local:{}", path_str)
        {
            vec![self.pane_id.clone()]
        } else {
            vec![]
        }
    }

    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        if pane_id == &self.pane_id {
            Ok(())
        } else {
            Err(RuntimeError::PaneNotFound(pane_id.clone()))
        }
    }

    fn split_pane(&self, _pane_id: &PaneId, _vertical: bool) -> Result<PaneId, RuntimeError> {
        Err(RuntimeError::Backend(
            "split pane not implemented in single LocalPtyRuntime - use LocalPtyAgent".to_string(),
        ))
    }

    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16) {
        if pane_id == &self.pane_id {
            (
                self.cols.load(Ordering::SeqCst),
                self.rows.load(Ordering::SeqCst),
            )
        } else {
            (80, 24)
        }
    }

    fn open_diff(
        &self,
        _worktree: &Path,
        _pane_id: Option<&PaneId>,
    ) -> Result<String, RuntimeError> {
        Err(RuntimeError::Backend(
            "open_diff not implemented in LocalPtyRuntime - use LocalPtyAgent".to_string(),
        ))
    }

    fn open_review(&self, _worktree: &Path) -> Result<String, RuntimeError> {
        Err(RuntimeError::Backend(
            "open_review not implemented in LocalPtyRuntime - use LocalPtyAgent".to_string(),
        ))
    }

    fn kill_window(&self, _window_target: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn session_info(&self) -> Option<(String, String)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::AgentRuntime;
    use std::time::Instant;

    #[allow(deprecated)]
    #[test]
    fn test_send_input_does_not_block() {
        let dir = tempfile::tempdir().unwrap();
        let rt = LocalPtyRuntime::new(dir.path(), 80, 24).unwrap();
        let pane_id = rt.pane_id().to_string();

        let start = Instant::now();
        for _ in 0..100 {
            rt.send_input(&pane_id, b"x").unwrap();
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 50,
            "send_input should not block: 100 sends took {}ms (expected < 50ms)",
            elapsed.as_millis()
        );
    }

    #[allow(deprecated)]
    #[test]
    fn test_rapid_keystrokes_no_contention() {
        let dir = tempfile::tempdir().unwrap();
        let rt = LocalPtyRuntime::new(dir.path(), 80, 24).unwrap();
        let pane_id = rt.pane_id().to_string();

        let mut ok_count = 0u32;
        for i in 0..500 {
            let byte = (i % 26) as u8 + b'a';
            match rt.send_input(&pane_id, &[byte]) {
                Ok(()) => ok_count += 1,
                Err(e) => panic!("send_input failed at {}: {}", i, e),
            }
        }

        assert_eq!(ok_count, 500, "all 500 rapid sends should succeed");
    }

    #[test]
    fn test_local_pty_agent_creates_primary_pane() {
        let dir = tempfile::tempdir().unwrap();
        let agent = LocalPtyAgent::new(dir.path(), 80, 24).unwrap();

        // Should have primary pane
        let primary = agent.primary_pane_id();
        assert!(primary.is_some());
        assert!(primary.unwrap().contains("main"));
    }

    #[test]
    fn test_local_pty_agent_split_pane() {
        let dir = tempfile::tempdir().unwrap();
        let agent = LocalPtyAgent::new(dir.path(), 80, 24).unwrap();

        let primary = agent.primary_pane_id().unwrap();
        let new_pane = agent.split_pane(&primary, true).unwrap();

        // Should have 2 panes
        let panes = agent.list_panes(&String::new());
        assert_eq!(panes.len(), 2);
        assert!(panes.contains(&new_pane));
    }

    #[test]
    fn test_create_runtime_returns_local_pty_agent() {
        let dir = tempfile::tempdir().unwrap();
        let rt = super::super::create_runtime(dir.path(), 80, 24).unwrap();
        // LocalPtyAgent supports split_pane; LocalPtyRuntime does not
        let primary = rt.primary_pane_id().unwrap();
        let result = rt.split_pane(&primary, true);
        assert!(result.is_ok(), "production runtime should support split_pane, got: {:?}", result);
    }

    #[test]
    fn test_local_pty_agent_open_diff() {
        let dir = tempfile::tempdir().unwrap();
        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .ok();

        let agent = LocalPtyAgent::new(dir.path(), 80, 24).unwrap();
        let result = agent.open_diff(dir.path(), None);

        // Should succeed (command sent to pane)
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_diff_creates_new_pane() {
        let dir = tempfile::tempdir().unwrap();
        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .ok();
        let agent = LocalPtyAgent::new(dir.path(), 80, 24).unwrap();

        let panes_before = agent.list_panes(&String::new()).len();
        let result = agent.open_diff(dir.path(), None);
        assert!(result.is_ok(), "open_diff should succeed, got: {:?}", result);
        let panes_after = agent.list_panes(&String::new()).len();
        assert_eq!(
            panes_after,
            panes_before + 1,
            "open_diff should create a new pane (before={}, after={})",
            panes_before,
            panes_after
        );
    }
}
