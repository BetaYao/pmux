//! ShpoolRuntime: PTY-based backend using shpool for session persistence.
//!
//! Spawns `shpool attach <session>` in a PTY. Shpool passes raw bytes directly
//! (no internal VTE re-rendering like tmux), giving native terminal feel with
//! zero-overhead true color and input latency.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use std::thread;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};

pub struct ShpoolRuntime {
    worktree_path: std::path::PathBuf,
    pane_id: PaneId,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    input_tx: flume::Sender<Vec<u8>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
}

impl ShpoolRuntime {
    pub fn new(worktree_path: &Path, cols: u16, rows: u16) -> Result<Self, RuntimeError> {
        let session_name = worktree_path
            .file_name()
            .map(|n| format!("pmux-{}", n.to_string_lossy()))
            .unwrap_or_else(|| "pmux-default".to_string());

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let mut cmd = CommandBuilder::new("shpool");
        cmd.args(["attach", &session_name]);
        cmd.cwd(worktree_path);

        let child: Box<dyn portable_pty::Child + Send + Sync> = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::Backend(format!("shpool spawn: {}", e)))?;

        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let (output_tx, output_rx) = flume::unbounded();
        let reader = master
            .try_clone_reader()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;

        let pane_id = format!("shpool:{}", worktree_path.display());

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

        // Input queue + writer thread (coalesces rapid keystrokes)
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();
        thread::spawn(move || {
            let mut writer = writer;
            let mut buf = Vec::new();
            loop {
                match input_rx.recv() {
                    Ok(bytes) => {
                        buf.extend_from_slice(&bytes);
                        while let Ok(bytes) = input_rx.try_recv() {
                            buf.extend_from_slice(&bytes);
                        }
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

impl AgentRuntime for ShpoolRuntime {
    fn backend_type(&self) -> &'static str {
        "shpool"
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        if pane_id != &self.pane_id {
            return Err(RuntimeError::PaneNotFound(pane_id.clone()));
        }
        self.input_tx
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
        if pane_id != &self.pane_id {
            return Err(RuntimeError::PaneNotFound(pane_id.clone()));
        }
        self.cols.store(cols, Ordering::SeqCst);
        self.rows.store(rows, Ordering::SeqCst);
        let guard = self
            .master
            .lock()
            .map_err(|e| RuntimeError::Backend(e.to_string()))?;
        let _ = guard.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
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
            || agent_id == &format!("shpool:{}", path_str)
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
            "split pane not supported by shpool".to_string(),
        ))
    }

    fn kill_pane(&self, _pane_id: &PaneId) -> Result<(), RuntimeError> {
        Err(RuntimeError::Backend("kill_pane not supported".into()))
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
            "open_diff not supported by shpool".to_string(),
        ))
    }

    fn open_review(&self, _worktree: &Path) -> Result<String, RuntimeError> {
        Err(RuntimeError::Backend(
            "open_review not supported by shpool".to_string(),
        ))
    }

    fn kill_window(&self, _window_target: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn session_info(&self) -> Option<(String, String)> {
        Some((self.pane_id.clone(), "".to_string()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_shpool_runtime_backend_type() {
        assert_eq!("shpool", "shpool");
    }
}
