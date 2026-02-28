//! Local PTY backend - spawns shell directly in a PTY, no tmux.
//!
//! One PTY per worktree. True PTY write for input, direct read for output.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use std::thread;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};

/// Local PTY runtime - one shell per worktree, direct PTY read/write.
pub struct LocalPtyRuntime {
    worktree_path: std::path::PathBuf,
    pane_id: PaneId,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
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

        let (tx, rx) = flume::unbounded();
        let reader = master
            .try_clone_reader()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let pane_id = format!("local:{}", worktree_path.display());
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut reader = reader;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            worktree_path: worktree_path.to_path_buf(),
            pane_id: pane_id.clone(),
            master: Mutex::new(master),
            writer: Mutex::new(Some(writer)),
            output_rx: Mutex::new(Some(rx)),
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
    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        if pane_id != &self.pane_id {
            return Err(RuntimeError::PaneNotFound(pane_id.clone()));
        }
        let mut guard = self
            .writer
            .lock()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        let w = guard
            .as_mut()
            .ok_or_else(|| RuntimeError::Backend("writer already taken".to_string()))?;
        w.write_all(bytes)
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        w.flush()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        Ok(())
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
        guard
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
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
            "split pane not implemented in local PTY backend".to_string(),
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
            "open_diff not implemented in local PTY backend".to_string(),
        ))
    }

    fn open_review(&self, _worktree: &Path) -> Result<String, RuntimeError> {
        Err(RuntimeError::Backend(
            "open_review not implemented in local PTY backend".to_string(),
        ))
    }

    fn kill_window(&self, _window_target: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
}
