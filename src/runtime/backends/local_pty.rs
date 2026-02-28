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

        // Input queue + writer thread: input_rx -> PTY (non-blocking send_input)
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();
        thread::spawn(move || {
            let mut writer = writer;
            while let Ok(bytes) = input_rx.recv() {
                if writer.write_all(&bytes).is_err() || writer.flush().is_err() {
                    break;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::AgentRuntime;
    use std::time::Instant;

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
}
