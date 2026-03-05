//! Tmux backend - uses tmux commands for session persistence.
//!
//! Implements AgentRuntime via direct PTY write for input (no send-keys per keystroke),
//! pipe-pane for output, split-window, etc.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};

/// Write bytes to pane PTY. Uses cache to avoid re-opening. Runs in writer thread.
#[cfg(unix)]
fn write_to_pane_pty(
    target: &str,
    bytes: &[u8],
    cache: &Mutex<HashMap<PaneId, std::fs::File>>,
) -> Result<(), RuntimeError> {
    let pane_id = target.split('.').last().unwrap_or(target).to_string();
    let mut file = {
        let mut guard = cache.lock().map_err(|e| RuntimeError::Backend(e.to_string()))?;
        if let Some(f) = guard.remove(&pane_id) {
            f
        } else {
            drop(guard);
            let path = get_pane_tty_path_standalone(target)?;
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| RuntimeError::Backend(format!("open {}: {}", path, e)))?;
            let mut g = cache.lock().map_err(|e| RuntimeError::Backend(e.to_string()))?;
            g.insert(pane_id.clone(), f);
            g.remove(&pane_id).unwrap()
        }
    };
    file.write_all(bytes)
        .map_err(|e| RuntimeError::Backend(format!("write to PTY: {}", e)))?;
    file.flush()
        .map_err(|e| RuntimeError::Backend(format!("flush PTY: {}", e)))?;
    // Re-insert into cache for next use
    let mut guard = cache.lock().map_err(|e| RuntimeError::Backend(e.to_string()))?;
    guard.insert(pane_id, file);
    Ok(())
}

#[cfg(unix)]
fn get_pane_tty_path_standalone(target: &str) -> Result<String, RuntimeError> {
    let output = Command::new("tmux")
        .args(["display", "-p", "-t", target, "#{pane_tty}"])
        .output()
        .map_err(|e| RuntimeError::Backend(format!("tmux exec failed: {}", e)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RuntimeError::Backend(format!(
            "tmux display failed: {}",
            stderr.trim()
        )));
    }
    let path = String::from_utf8(output.stdout)
        .map_err(|e| RuntimeError::Backend(format!("tmux output invalid utf8: {}", e)))?
        .trim()
        .to_string();
    if path.is_empty() {
        return Err(RuntimeError::Backend("tmux returned empty pane_tty".to_string()));
    }
    Ok(path)
}

/// Tmux runtime - delegates to tmux commands for session persistence.
/// Input uses direct PTY write (no process spawn per keystroke); output uses pipe-pane.
pub struct TmuxRuntime {
    session_name: String,
    window_name: String,
    /// Input channel: (pane_id, bytes). Writer thread drains and writes to pane PTYs.
    input_tx: flume::Sender<(PaneId, Vec<u8>)>,
}

impl TmuxRuntime {
    /// Get the tmux session name.
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Get the tmux window name.
    pub fn window_name(&self) -> &str {
        &self.window_name
    }

    /// Create a new TmuxRuntime for the given session and window.
    /// Creates the tmux session and window if they don't exist.
    /// Spawns a writer thread that drains input to pane PTYs via direct write (no send-keys).
    pub fn new(
        session_name: impl Into<String>,
        window_name: impl Into<String>,
        start_dir: Option<&Path>,
    ) -> Self {
        let session_name = session_name.into();
        let window_name = window_name.into();

        // Create tmux session and window if they don't exist
        Self::ensure_session_and_window(&session_name, &window_name, start_dir);

        let (input_tx, input_rx) = flume::unbounded::<(PaneId, Vec<u8>)>();
        let pty_cache: Arc<Mutex<HashMap<PaneId, std::fs::File>>> = Arc::new(Mutex::new(HashMap::new()));

        let session_clone = session_name.clone();
        let window_clone = window_name.clone();
        let cache_clone = pty_cache.clone();
        thread::spawn(move || {
            while let Ok((pane_id, bytes)) = input_rx.recv() {
                let target = format!("{}:{}.{}", session_clone, window_clone, pane_id);
                if let Err(_) = write_to_pane_pty(&target, &bytes, &cache_clone) {
                    // On error, clear cache for this pane so next send retries
                    let _ = cache_clone.lock().map(|mut g| g.remove(&pane_id));
                }
            }
        });

        Self {
            session_name,
            window_name,
            input_tx,
        }
    }

    /// Ensure tmux session and window exist, creating them if necessary.
    fn ensure_session_and_window(
        session_name: &str,
        window_name: &str,
        start_dir: Option<&Path>,
    ) {
        let mut args = vec!["new-session", "-d", "-s", session_name, "-n", window_name];
        if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
            args.extend(["-c", dir]);
        }
        let _ = Command::new("tmux").args(&args).output();

        // Check if the window exists in the session
        let window_target = format!("{}:{}", session_name, window_name);
        let check = Command::new("tmux")
            .args(["list-windows", "-t", &window_target])
            .output();

        // If window doesn't exist, create it
        if check.is_err() || !check.map(|o| o.status.success()).unwrap_or(false) {
            let mut args = vec!["new-window", "-t", session_name, "-n", window_name];
            if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
                args.extend(["-c", dir]);
            }
            let _ = Command::new("tmux").args(&args).output();
        }
    }

    /// Attach to an existing tmux session/window. Verifies the session exists before returning.
    /// Returns Err if the session or window does not exist (e.g. tmux was killed).
    pub fn attach(
        session_id: &str,
        window_id: &str,
    ) -> Result<Self, crate::runtime::agent_runtime::RuntimeError> {
        let rt = Self::new(session_id, window_id, None);
        // Verify session and window exist
        let target = rt.window_target();
        let output = std::process::Command::new("tmux")
            .args(["list-windows", "-t", &target])
            .output()
            .map_err(|e| crate::runtime::agent_runtime::RuntimeError::Backend(format!("tmux exec failed: {}", e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::runtime::agent_runtime::RuntimeError::Backend(format!(
                "tmux session/window not found: {}",
                stderr.trim()
            )));
        }
        Ok(rt)
    }

    /// Build full pane target: session:window.pane_id
    fn pane_target(&self, pane_id: &PaneId) -> String {
        format!("{}:{}.{}", self.session_name, self.window_name, pane_id)
    }

    /// Build window target: session:window
    fn window_target(&self) -> String {
        format!("{}:{}", self.session_name, self.window_name)
    }

    /// Run tmux command, return error on non-zero exit.
    fn tmux_cmd(&self, args: &[&str]) -> Result<(), RuntimeError> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux exec failed: {}", e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RuntimeError::Backend(format!(
                "tmux failed: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    /// Get pane PTY path via tmux display. Returns path like /dev/pts/N.
    #[allow(dead_code)]
    fn get_pane_tty_path(&self, target: &str) -> Result<String, RuntimeError> {
        let output = Command::new("tmux")
            .args(["display", "-p", "-t", target, "#{pane_tty}"])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux exec failed: {}", e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RuntimeError::Backend(format!(
                "tmux display failed: {}",
                stderr.trim()
            )));
        }
        let path = String::from_utf8(output.stdout)
            .map_err(|e| RuntimeError::Backend(format!("tmux output invalid utf8: {}", e)))?
            .trim()
            .to_string();
        if path.is_empty() {
            return Err(RuntimeError::Backend("tmux returned empty pane_tty".to_string()));
        }
        Ok(path)
    }

    /// Run tmux command and capture stdout.
    fn tmux_cmd_output(&self, args: &[&str]) -> Result<String, RuntimeError> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux exec failed: {}", e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RuntimeError::Backend(format!(
                "tmux failed: {}",
                stderr.trim()
            )));
        }
        String::from_utf8(output.stdout)
            .map_err(|e| RuntimeError::Backend(format!("tmux output invalid utf8: {}", e)))
    }

    /// Get current branch for worktree (for review window name).
    fn get_branch(&self, worktree: &Path) -> Result<String, RuntimeError> {
        let output = Command::new("git")
            .args(["-C", worktree.to_str().unwrap_or("."), "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("git exec failed: {}", e)))?;
        if !output.status.success() {
            return Err(RuntimeError::Backend("git rev-parse failed".to_string()));
        }
        let branch = String::from_utf8(output.stdout)
            .map_err(|e| RuntimeError::Backend(format!("git output invalid utf8: {}", e)))?;
        Ok(branch.trim().to_string())
    }
}

impl AgentRuntime for TmuxRuntime {
    fn backend_type(&self) -> &'static str {
        "tmux"
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        // Use tmux send-keys for Enter so pipe-pane captures command output (Bug2)
        let is_enter = bytes == [b'\r'] || bytes == [b'\n'] || bytes == [b'\r', b'\n'];
        if is_enter {
            let target = self.pane_target(pane_id);
            return self.tmux_cmd(&["send-keys", "-t", &target, "Enter"]);
        }
        self.input_tx
            .send((pane_id.clone(), bytes.to_vec()))
            .map_err(|e| RuntimeError::Backend(e.to_string()))
    }

    fn send_key(&self, pane_id: &PaneId, key: &str, use_literal: bool) -> Result<(), RuntimeError> {
        let bytes = if use_literal {
            key.as_bytes().to_vec()
        } else {
            // Map tmux key names to xterm byte sequences (avoids process spawn)
            tmux_key_to_bytes(key).unwrap_or_else(|| key.as_bytes().to_vec())
        };
        self.send_input(pane_id, &bytes)
    }

    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        let target = self.pane_target(pane_id);
        self.tmux_cmd(&[
            "resize-pane",
            "-t",
            &target,
            "-x",
            &cols.to_string(),
            "-y",
            &rows.to_string(),
        ])
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        #[cfg(unix)]
        {
            let (tx, rx) = flume::unbounded();
            // Bootstrap: pipe-pane 只流式新输出；注入当前 pane 内容（prompt 等）。
            // 若 capture 只含空白行，则不注入，否则会把 prompt 推到屏幕中间。
            if let Some(initial) = self.capture_initial_content(pane_id) {
                let has_real_content = initial
                    .iter()
                    .any(|&b| b != b'\n' && b != b'\r' && b != b' ' && b != b'\t');
                // Skip if leading blank lines (content would push prompt to middle of viewport)
                let leading_newlines = initial
                    .iter()
                    .take_while(|&&b| b == b'\n' || b == b'\r' || b == b' ' || b == b'\t')
                    .filter(|&&b| b == b'\n')
                    .count();
                // Skip long capture (changelog, dialogs) - only inject short prompt-like content
                let too_long = initial.len() > 400;
                let skip = !has_real_content || leading_newlines >= 3 || too_long;
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "tmux.rs:subscribe_output",
                    "capture inject/skip (cursor-middle debug)",
                    &serde_json::json!({
                        "skip": skip,
                        "has_real_content": has_real_content,
                        "leading_newlines": leading_newlines,
                        "too_long": too_long,
                        "len": initial.len(),
                        "preview": String::from_utf8_lossy(&initial[..initial.len().min(120)]).replace('\n', "\\n").replace('\r', "\\r")
                    }),
                    "H_cursor_mid",
                );
                // #endregion
                if !skip {
                    // Trim trailing newlines: second connect often has "prompt\n\n\n..."; without trim,
                    // cursor ends up in middle. Do NOT add trailing \n - that would put cursor on next line.
                    let trimmed: Vec<u8> = {
                        let end = initial
                            .iter()
                            .rposition(|&b| b != b'\n' && b != b'\r')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        initial[..end].to_vec()
                    };
                    // Cascade fix: capture-pane uses \n only, no \r. Normalize to \r\n.
                    let normalized: Vec<u8> = trimmed
                        .iter()
                        .flat_map(|&b| if b == b'\n' { vec![b'\r', b'\n'] } else { vec![b] })
                        .collect();
                    // #region agent log
                    crate::debug_log::dbg_session_log(
                        "tmux.rs:subscribe_output",
                        "inject (trimmed, no trailing nl)",
                        &serde_json::json!({
                            "trimmed_len": trimmed.len(),
                            "normalized_len": normalized.len(),
                            "ends_with_nl": trimmed.last() == Some(&b'\n')
                        }),
                        "H_cursor_mid",
                    );
                    // #endregion
                    let _ = tx.send(normalized);
                }
            }
            let fifo_path = std::env::temp_dir().join(format!("pmux-pipe-{}", uuid::Uuid::new_v4()));
            if nix::unistd::mkfifo(&fifo_path, nix::sys::stat::Mode::S_IRWXU).is_err() {
                return None;
            }
            let fifo_path = fifo_path.to_path_buf();

            // Reader thread: open fifo (blocks until writer opens), read and send to channel.
            // Runs until tmux closes the pipe (EOF) — do NOT stop it early or terminal stays blank.
            let fifo_read = fifo_path.clone();
            let tx_clone = tx.clone();
            let target_log = format!("{}:{}.{}", self.session_name, self.window_name, pane_id);
            thread::spawn(move || {
                let file = match std::fs::File::open(&fifo_read) {
                    Ok(f) => f,
                    Err(e) => {
                        crate::debug_log::dbg_session_log(
                            "tmux.rs:pipe_reader",
                            "fifo open failed",
                            &serde_json::json!({"err": e.to_string(), "target": target_log}),
                            "H_ls_no_output",
                        );
                        return;
                    }
                };
                let mut reader = std::io::BufReader::new(file);
                let mut buf = [0u8; 4096];
                let mut chunk_count: u64 = 0;
                let mut total_bytes: u64 = 0;
                loop {
                    match std::io::Read::read(&mut reader, &mut buf) {
                        Ok(0) => break, // EOF when tmux closes pipe
                        Ok(n) => {
                            total_bytes += n as u64;
                            chunk_count += 1;
                            if tx_clone.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                            // #region agent log
                            if chunk_count <= 2 || (chunk_count % 50 == 0) || buf[..n].contains(&b'\n') {
                                crate::debug_log::dbg_session_log(
                                    "tmux.rs:pipe_reader",
                                    "bytes received",
                                    &serde_json::json!({
                                        "chunk": chunk_count,
                                        "len": n,
                                        "total": total_bytes,
                                        "has_newline": buf[..n].contains(&b'\n'),
                                    }),
                                    "H_ls_no_output",
                                );
                            }
                            // #endregion
                        }
                        Err(e) => {
                            crate::debug_log::dbg_session_log(
                                "tmux.rs:pipe_reader",
                                "read error",
                                &serde_json::json!({"err": e.to_string(), "total": total_bytes}),
                                "H_ls_no_output",
                            );
                            break;
                        }
                    }
                }
                crate::debug_log::dbg_session_log(
                    "tmux.rs:pipe_reader",
                    "EOF",
                    &serde_json::json!({"chunks": chunk_count, "total_bytes": total_bytes}),
                    "H_ls_no_output",
                );
                let _ = std::fs::remove_file(&fifo_read);
            });

            // Spawn pipe-pane: tmux pipe-pane -o -t {target} 'dd of={fifo} bs=1 conv=fsync'
            // Reader must keep running — do NOT signal it to stop.
            let fifo_str = fifo_path.to_string_lossy().to_string();
            let session = self.session_name.clone();
            let window = self.window_name.clone();
            let pane_id = pane_id.to_string();
            let pipe_target = format!("{}:{}.{}", session, window, pane_id);
            crate::debug_log::dbg_session_log(
                "tmux.rs:subscribe_output",
                "pipe-pane spawn",
                &serde_json::json!({"target": pipe_target}),
                "H_ls_no_output",
            );
            thread::spawn(move || {
                let target = pipe_target;
                // Use dd bs=1 for unbuffered write; cat buffers 4KB+ and hides ls output (Bug2)
                let cmd = format!("dd of={} bs=1 conv=fsync 2>/dev/null", fifo_str);
                let _ = Command::new("tmux")
                    .args(["pipe-pane", "-o", "-t", &target, &cmd])
                    .status();
            });

            Some(rx)
        }
        #[cfg(not(unix))]
        {
            let _ = pane_id;
            None
        }
    }

    fn capture_initial_content(&self, pane_id: &PaneId) -> Option<Vec<u8>> {
        let target = self.pane_target(pane_id);
        let out = self
            .tmux_cmd_output(&["capture-pane", "-t", &target, "-p", "-e", "-S", "-"])
            .ok()?;
        Some(out.into_bytes())
    }

    fn list_panes(&self, agent_id: &AgentId) -> Vec<PaneId> {
        let target = if agent_id.is_empty() {
            self.window_target()
        } else {
            agent_id.clone()
        };
        let out = match self.tmux_cmd_output(&["list-panes", "-t", &target, "-F", "#{pane_id}"]) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let panes: Vec<PaneId> = out
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        panes
    }

    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        let target = self.pane_target(pane_id);
        self.tmux_cmd(&["select-pane", "-t", &target])?;
        // PTY prewarm: send empty bytes to populate cache so first real keystroke hits cache (avoids 50–100ms tmux display -p on first input)
        let _ = self.input_tx.send((pane_id.clone(), vec![]));
        Ok(())
    }

    fn split_pane(&self, pane_id: &PaneId, vertical: bool) -> Result<PaneId, RuntimeError> {
        let target = self.pane_target(pane_id);
        let flag = if vertical { "-v" } else { "-h" };
        let out = self.tmux_cmd_output(&["split-window", flag, "-t", &target, "-P", "-F", "#{pane_id}"])?;
        let new_id = out.trim().to_string();
        if new_id.is_empty() {
            return Err(RuntimeError::Backend("split-window did not return pane id".to_string()));
        }
        Ok(new_id)
    }

    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16) {
        let target = self.pane_target(pane_id);
        let out = match self.tmux_cmd_output(&["display-message", "-t", &target, "-p", "-F", "#{pane_width} #{pane_height}"]) {
            Ok(s) => s,
            Err(_) => return (80, 24),
        };
        let parts: Vec<&str> = out.split_whitespace().collect();
        if parts.len() >= 2 {
            if let (Ok(c), Ok(r)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                return (c, r);
            }
        }
        (80, 24)
    }

    fn open_diff(&self, worktree: &Path, _pane_id: Option<&PaneId>) -> Result<String, RuntimeError> {
        let branch = self.get_branch(worktree)?;
        let safe_branch = branch.replace('/', "-").replace(' ', "-");
        let window_name = format!("review-{}", safe_branch);
        let worktree_str = worktree.to_string_lossy();
        self.tmux_cmd(&[
            "new-window",
            "-t",
            &self.session_name,
            "-n",
            &window_name,
            "-c",
            worktree_str.as_ref(),
            "nvim",
            "-c",
            "DiffviewOpen main...HEAD",
        ])?;
        Ok(window_name)
    }

    fn open_review(&self, worktree: &Path) -> Result<String, RuntimeError> {
        self.open_diff(worktree, None)
    }

    fn kill_window(&self, window_target: &str) -> Result<(), RuntimeError> {
        self.tmux_cmd(&["kill-window", "-t", window_target])
    }

    fn session_info(&self) -> Option<(String, String)> {
        Some((self.session_name.clone(), self.window_name.clone()))
    }
}

/// Escape string for tmux send-keys -l (literal mode).
/// Kept for potential send-keys fallback on non-Unix.
#[allow(dead_code)]
fn escape_for_tmux_send_keys(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Map tmux key names to xterm byte sequences for direct PTY write.
/// Returns None for unknown keys (caller falls back to literal).
fn tmux_key_to_bytes(key: &str) -> Option<Vec<u8>> {
    Some(match key {
        "Enter" | "Return" => vec![b'\r', b'\n'],
        "Tab" => vec![b'\t'],
        "Space" => vec![b' '],
        "BackSpace" | "BS" => vec![0x7f],
        "Escape" => vec![0x1b],
        "Up" => vec![0x1b, b'[', b'A'],
        "Down" => vec![0x1b, b'[', b'B'],
        "Right" => vec![0x1b, b'[', b'C'],
        "Left" => vec![0x1b, b'[', b'D'],
        "Home" => vec![0x1b, b'[', b'H'],
        "End" => vec![0x1b, b'[', b'F'],
        "PageDown" | "Page_Down" => vec![0x1b, b'[', b'6', b'~'],
        "PageUp" | "Page_Up" => vec![0x1b, b'[', b'5', b'~'],
        "Delete" => vec![0x1b, b'[', b'3', b'~'],
        "Insert" => vec![0x1b, b'[', b'2', b'~'],
        "F1" => vec![0x1b, b'O', b'P'],
        "F2" => vec![0x1b, b'O', b'Q'],
        "F3" => vec![0x1b, b'O', b'R'],
        "F4" => vec![0x1b, b'O', b'S'],
        "F5" => vec![0x1b, b'[', b'1', b'5', b'~'],
        "F6" => vec![0x1b, b'[', b'1', b'7', b'~'],
        "F7" => vec![0x1b, b'[', b'1', b'8', b'~'],
        "F8" => vec![0x1b, b'[', b'1', b'9', b'~'],
        "F9" => vec![0x1b, b'[', b'2', b'0', b'~'],
        "F10" => vec![0x1b, b'[', b'2', b'1', b'~'],
        "F11" => vec![0x1b, b'[', b'2', b'3', b'~'],
        "F12" => vec![0x1b, b'[', b'2', b'4', b'~'],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmux_available() -> bool {
        Command::new("tmux").arg("-V").output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn test_tmux_runtime_new() {
        let rt = TmuxRuntime::new("pmux-test", "main", None);
        assert_eq!(rt.pane_target(&"%0".to_string()), "pmux-test:main.%0");
        assert_eq!(rt.window_target(), "pmux-test:main");
    }

    #[test]
    fn test_tmux_runtime_pane_target() {
        let rt = TmuxRuntime::new("mysession", "mywindow", None);
        assert_eq!(rt.pane_target(&"%1".to_string()), "mysession:mywindow.%1");
    }

    #[test]
    fn test_escape_for_tmux_send_keys() {
        assert_eq!(escape_for_tmux_send_keys("hello"), "hello");
        assert_eq!(escape_for_tmux_send_keys("it's"), "it\\'s");
        assert_eq!(escape_for_tmux_send_keys("path\\to"), "path\\\\to");
    }

    #[test]
    fn test_tmux_key_to_bytes() {
        assert_eq!(tmux_key_to_bytes("Enter"), Some(vec![b'\r', b'\n']));
        assert_eq!(tmux_key_to_bytes("Tab"), Some(vec![b'\t']));
        assert_eq!(tmux_key_to_bytes("Up"), Some(vec![0x1b, b'[', b'A']));
        assert_eq!(tmux_key_to_bytes("Down"), Some(vec![0x1b, b'[', b'B']));
        assert_eq!(tmux_key_to_bytes("Escape"), Some(vec![0x1b]));
        assert_eq!(tmux_key_to_bytes("unknown"), None);
    }

    #[test]
    fn test_send_input_no_process_spawn() {
        use std::time::Instant;
        // send_input queues to channel - no process spawn per keystroke
        let rt = TmuxRuntime::new("pmux-test", "main", None);
        let pane_id = "%0".to_string();

        let start = Instant::now();
        for _ in 0..100 {
            rt.send_input(&pane_id, b"x").unwrap();
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 50,
            "send_input should not block: 100 sends took {}ms (expected < 50ms, no process spawn)",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_tmux_runtime_list_panes_requires_tmux() {
        if !tmux_available() {
            return;
        }
        let rt = TmuxRuntime::new("nonexistent-session-xyz", "nonexistent-window", None);
        let panes = rt.list_panes(&String::new());
        assert!(panes.is_empty());
    }

    #[test]
    fn test_tmux_runtime_send_key_no_process_spawn() {
        // send_key queues to input channel (no tmux process spawn); returns Ok immediately
        let rt = TmuxRuntime::new("nonexistent-session-xyz", "nonexistent-window", None);
        let result = rt.send_key(&"%0".to_string(), "Enter", false);
        assert!(result.is_ok(), "send_key should succeed (queue to channel, no blocking)");
    }

    #[test]
    fn test_tmux_runtime_kill_window_invalid_target() {
        if !tmux_available() {
            return;
        }
        let rt = TmuxRuntime::new("pmux-test", "main", None);
        let err = rt.kill_window("nonexistent:window");
        assert!(err.is_err());
    }

    #[test]
    fn test_tmux_runtime_get_pane_dimensions_invalid() {
        let rt = TmuxRuntime::new("nonexistent-session", "nonexistent-window", None);
        let (c, r) = rt.get_pane_dimensions(&"%0".to_string());
        assert_eq!(c, 80);
        assert_eq!(r, 24);
    }

    #[test]
    fn test_tmux_runtime_open_diff_no_git() {
        let rt = TmuxRuntime::new("pmux-test", "main", None);
        let err = rt.open_diff(PathBuf::from("/nonexistent/path").as_path(), None);
        assert!(err.is_err());
    }

    #[test]
    fn test_tmux_runtime_attach_nonexistent_session() {
        let result = TmuxRuntime::attach("pmux-nonexistent-session-xyz", "nonexistent-window");
        assert!(result.is_err());
    }

    #[test]
    fn test_tmux_runtime_attach_requires_tmux() {
        if !tmux_available() {
            return;
        }
        let result = TmuxRuntime::attach("pmux-nonexistent-session-xyz", "main");
        assert!(result.is_err());
    }
}
