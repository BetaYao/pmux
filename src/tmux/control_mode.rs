// tmux/control_mode.rs - Tmux control mode (-CC) protocol handling
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ControlModeError {
    #[error("tmux command failed: {0}")]
    SpawnFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

/// Handle to an active tmux control mode connection.
/// Call shutdown() to detach and clean up.
pub struct ControlModeHandle {
    child: Option<Child>,
    output_rx: Receiver<(String, Vec<u8>)>,
    shutdown_tx: Sender<()>,
}

impl ControlModeHandle {
    /// Receive the next pane output. Returns (pane_target, raw_bytes).
    /// Pane target format: "session:window.pane" (e.g. "sdlc-repo:@0.%0")
    pub fn try_recv(&self) -> Option<(String, Vec<u8>)> {
        self.output_rx.try_recv().ok()
    }

    /// Block until the next pane output or shutdown.
    pub fn recv(&self) -> Result<(String, Vec<u8>), std::sync::mpsc::RecvError> {
        self.output_rx.recv()
    }

    /// Detach from tmux and wait for the reader thread to exit.
    pub fn shutdown(mut self) -> Result<(), ControlModeError> {
        let _ = self.shutdown_tx.send(());
        if let Some(mut child) = self.child.take() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = writeln!(stdin);
            }
            let _ = child.wait();
        }
        Ok(())
    }
}

/// Unescape tmux control mode octal sequences (\XXX) to raw bytes.
fn unescape_octal(s: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let (Some(d1), Some(d2), Some(d3)) =
                (chars.next(), chars.next(), chars.next())
            {
                if d1.is_ascii_digit() && d2.is_ascii_digit() && d3.is_ascii_digit() {
                    let octal: String = [d1, d2, d3].iter().collect();
                    if let Ok(byte) = u8::from_str_radix(&octal, 8) {
                        result.push(byte);
                        continue;
                    }
                }
            }
            result.push(b'\\');
        } else {
            result.extend_from_slice(c.to_string().as_bytes());
        }
    }
    result
}

/// Build pane_id -> target mapping. Returns e.g. {"%0" -> "sdlc-repo:@0.%0", "%1" -> "sdlc-repo:@0.%1"}
pub fn build_pane_map(session_name: &str) -> Result<HashMap<String, String>, ControlModeError> {
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            session_name,
            "-a",
            "-F",
            "#{pane_id}|#{session_name}:#{window_id}.#{pane_id}",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ControlModeError::SessionNotFound(stderr.to_string()));
    }

    let mut map = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((pane_id, target)) = line.split_once('|') {
            map.insert(pane_id.to_string(), target.to_string());
        }
    }
    Ok(map)
}

/// Attach to a tmux session in control mode.
/// Spawns a background thread that reads protocol lines and sends %output to the channel.
pub fn attach(session_name: &str) -> Result<ControlModeHandle, ControlModeError> {
    let pane_map = std::sync::Arc::new(std::sync::Mutex::new(build_pane_map(session_name)?));

    let mut child = Command::new("tmux")
        .args(["-CC", "attach-session", "-t", session_name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ControlModeError::SpawnFailed(e.to_string()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ControlModeError::SpawnFailed("no stdout".to_string()))?;

    let (output_tx, output_rx) = std::sync::mpsc::channel();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
    let session_name_for_refresh = session_name.to_string();

    let _reader_handle = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let line = line.trim_end_matches('\n').trim_end_matches('\r');
                    if line.starts_with("%output ") {
                        let rest = &line[8..];
                        if let Some(space) = rest.find(' ') {
                            let pane_id = rest[..space].trim();
                            let value = rest[space + 1..].trim();
                            let bytes = unescape_octal(value);
                            if let Ok(map) = pane_map.lock() {
                                if let Some(target) = map.get(pane_id) {
                                    let _ = output_tx.send((target.clone(), bytes));
                                }
                            }
                        }
                    } else if line.starts_with("%window-add")
                        || line.starts_with("%window-close")
                        || line.starts_with("%sessions-changed")
                    {
                        if let Ok(new_map) = build_pane_map(&session_name_for_refresh) {
                            if let Ok(mut map) = pane_map.lock() {
                                *map = new_map;
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(ControlModeHandle {
        child: Some(child),
        output_rx,
        shutdown_tx,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_octal() {
        assert_eq!(unescape_octal("hello"), b"hello");
        assert_eq!(unescape_octal("a\\012b"), b"a\nb");
        assert_eq!(unescape_octal("\\134"), b"\\");
    }
}
