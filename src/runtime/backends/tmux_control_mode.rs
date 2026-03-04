//! tmux control mode (-CC) backend.
//!
//! Uses `tmux -CC attach` for structured I/O instead of pipe-pane + capture-pane.
//! This eliminates the dual-datasource problem: gpui-terminal receives clean bytes
//! from %output events, with no FIFO, no capture injection, no Enter workaround.
//!
//! Protocol events parsed:
//! - %output %pane_id data  — pane output bytes (main data path)
//! - %begin / %end          — command response brackets
//! - %exit                  — session detached/exited
//! - %session-changed       — session switch
//! - %window-add/close      — window lifecycle

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};

// ── Protocol parser ────────────────────────────────────────────────────────────

/// Unescape tmux control mode output data.
/// tmux escapes non-printable characters and backslash as octal `\xxx`.
fn unescape_tmux_output(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\\' && i + 1 < data.len() {
            if data[i + 1] == b'\\' {
                result.push(b'\\');
                i += 2;
            } else if i + 3 < data.len()
                && (b'0'..=b'7').contains(&data[i + 1])
                && (b'0'..=b'7').contains(&data[i + 2])
                && (b'0'..=b'7').contains(&data[i + 3])
            {
                let val = ((data[i + 1] - b'0') as u16) * 64
                    + ((data[i + 2] - b'0') as u16) * 8
                    + (data[i + 3] - b'0') as u16;
                result.push(val as u8);
                i += 4;
            } else {
                result.push(data[i]);
                i += 1;
            }
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    strip_tmux_title_sequences(&result)
}

/// Strip tmux/screen-specific ESC k ... ESC \ (set window title) sequences.
/// These are not standard VT100/xterm and alacritty_terminal's VTE parser
/// renders the enclosed text as literal characters instead of consuming it.
fn strip_tmux_title_sequences(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        // ESC k <title> ESC \ (or ESC k <title> BEL)
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'k' {
            // Skip past ESC k and the title content until ST (ESC \) or BEL
            i += 2;
            while i < data.len() {
                if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
                    i += 2; // skip ESC backslash (ST)
                    break;
                } else if data[i] == 0x07 {
                    i += 1; // skip BEL
                    break;
                } else {
                    i += 1;
                }
            }
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlModeEvent {
    Output { pane_id: String, data: Vec<u8> },
    BeginEnd { tag: String, response: Vec<u8> },
    Exit,
    SessionChanged { session_id: String, name: String },
    WindowAdd { window_id: String },
    WindowClose { window_id: String },
    LayoutChanged { window_id: String, layout: String },
    Unknown(String),
}

pub struct ControlModeParser {
    line_buf: Vec<u8>,
    in_begin: Option<String>,
    begin_response: Vec<u8>,
}

impl ControlModeParser {
    pub fn new() -> Self {
        Self {
            line_buf: Vec::new(),
            in_begin: None,
            begin_response: Vec::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ControlModeEvent> {
        let mut events = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                let line = std::mem::take(&mut self.line_buf);
                if let Some(ev) = self.parse_line(&line) {
                    events.push(ev);
                }
            } else {
                self.line_buf.push(b);
            }
        }
        events
    }

    fn parse_line(&mut self, line: &[u8]) -> Option<ControlModeEvent> {
        let s = String::from_utf8_lossy(line);
        let s = s.trim_end_matches('\r');

        // Inside a begin/end block
        if let Some(ref tag) = self.in_begin.clone() {
            if s.starts_with("%end ") && s.contains(tag.as_str()) {
                let response = std::mem::take(&mut self.begin_response);
                self.in_begin = None;
                return Some(ControlModeEvent::BeginEnd {
                    tag: tag.clone(),
                    response,
                });
            } else {
                self.begin_response.extend_from_slice(line);
                self.begin_response.push(b'\n');
                return None;
            }
        }

        if s.starts_with("%begin ") {
            let tag = s.strip_prefix("%begin ").unwrap_or("").to_string();
            self.in_begin = Some(tag);
            self.begin_response.clear();
            return None;
        }

        if s.starts_with("%output ") {
            let rest = &s["%output ".len()..];
            let space_idx = rest.find(' ')?;
            let pane_id = rest[..space_idx].to_string();
            let raw = rest[space_idx + 1..].as_bytes();
            let data = unescape_tmux_output(raw);
            return Some(ControlModeEvent::Output { pane_id, data });
        }

        if s == "%exit" {
            return Some(ControlModeEvent::Exit);
        }

        if s.starts_with("%session-changed ") {
            let rest = &s["%session-changed ".len()..];
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            return Some(ControlModeEvent::SessionChanged {
                session_id: parts.first().unwrap_or(&"").to_string(),
                name: parts.get(1).unwrap_or(&"").to_string(),
            });
        }

        if s.starts_with("%window-add ") {
            return Some(ControlModeEvent::WindowAdd {
                window_id: s["%window-add ".len()..].to_string(),
            });
        }

        if s.starts_with("%window-close ") {
            return Some(ControlModeEvent::WindowClose {
                window_id: s["%window-close ".len()..].to_string(),
            });
        }

        if s.starts_with("%layout-change ") {
            let rest = &s["%layout-change ".len()..];
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            return Some(ControlModeEvent::LayoutChanged {
                window_id: parts.first().unwrap_or(&"").to_string(),
                layout: parts.get(1).unwrap_or(&"").to_string(),
            });
        }

        Some(ControlModeEvent::Unknown(s.to_string()))
    }
}

// ── Runtime ───────────────────────────────────────────────────────────────────

pub struct TmuxControlModeRuntime {
    session_name: String,
    /// Interior mutability: updated by switch_window() when changing worktrees
    window_name: Mutex<String>,
    /// PTY master writer — send tmux commands here (used for non-input commands: resize, split, etc.)
    pty_writer: Arc<Mutex<std::fs::File>>,
    /// Async input channel — send_input enqueues here; a dedicated writer thread drains and writes
    input_tx: flume::Sender<(String, Vec<u8>)>,
    /// Per-pane output channels, fed by the parser thread from %output events
    pane_outputs: Arc<Mutex<HashMap<String, flume::Sender<Vec<u8>>>>>,
    /// Cached file handles to pane TTY devices for direct write (bypass send-keys)
    pane_tty_writers: Arc<Mutex<HashMap<String, std::fs::File>>>,
    /// Keeps the tmux -CC child alive
    _control_child: Arc<Mutex<Child>>,
    /// PTY master fd for updating winsize on resize (tmux reads this)
    pty_master_fd: i32,
    /// When true, subscribe_output skips capture_initial_content (caller sends C-l instead)
    skip_next_capture: std::sync::atomic::AtomicBool,
}

/// Build tmux send-keys commands for the given input bytes.
/// Pure function — no I/O, no locks.
/// Kept for backward reference; writer thread uses build_hex_send_keys.
#[allow(dead_code)]
fn build_send_keys_commands(pane_id: &str, bytes: &[u8]) -> Vec<String> {
    let mut commands = Vec::new();
    if bytes.is_empty() {
        return commands;
    }
    let mut i = 0;
    while i < bytes.len() {
        let run_start = i;
        while i < bytes.len() && bytes[i] >= 0x20 && bytes[i] < 0x7f {
            i += 1;
        }
        if run_start < i {
            let text = String::from_utf8_lossy(&bytes[run_start..i]);
            let escaped = text.replace('\'', "'\\''");
            commands.push(format!("send-keys -l -t {} '{}'", pane_id, escaped));
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] == 0x1b && i + 2 < bytes.len() && bytes[i + 1] == b'[' {
            let key = match bytes[i + 2] {
                b'A' => { i += 3; "Up" }
                b'B' => { i += 3; "Down" }
                b'C' => { i += 3; "Right" }
                b'D' => { i += 3; "Left" }
                b'H' => { i += 3; "Home" }
                b'F' => { i += 3; "End" }
                b'1'..=b'9' if i + 3 < bytes.len() && bytes[i + 3] == b'~' => {
                    let k = match bytes[i + 2] {
                        b'2' => "IC",
                        b'3' => "DC",
                        b'5' => "PPage",
                        b'6' => "NPage",
                        _ => { i += 1; "Escape" }
                    };
                    if k != "Escape" { i += 4; }
                    k
                }
                _ => { i += 1; "Escape" }
            };
            commands.push(format!("send-keys -t {} {}", pane_id, key));
        } else {
            match bytes[i] {
                0x0d | 0x0a => {
                    commands.push(format!("send-keys -t {} Enter", pane_id));
                    i += 1;
                }
                0x09 => {
                    commands.push(format!("send-keys -t {} Tab", pane_id));
                    i += 1;
                }
                0x08 | 0x7f => {
                    commands.push(format!("send-keys -t {} BSpace", pane_id));
                    i += 1;
                }
                0x1b => {
                    commands.push(format!("send-keys -t {} Escape", pane_id));
                    i += 1;
                }
                b @ 0x01..=0x1a => {
                    let letter = (b'a' + b - 1) as char;
                    commands.push(format!("send-keys -t {} C-{}", pane_id, letter));
                    i += 1;
                }
                _ => {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 {
                        i += 1;
                    }
                    let text = String::from_utf8_lossy(&bytes[start..i]);
                    let escaped = text.replace('\'', "'\\''");
                    commands.push(format!("send-keys -l -t {} '{}'", pane_id, escaped));
                }
            }
        }
    }
    commands
}

/// Max raw bytes per single `send-keys -H` command.
/// 512 bytes → ~1600 hex chars + prefix ≈ 1650 bytes, well within tmux's line buffer.
const HEX_SEND_KEYS_CHUNK: usize = 512;

/// Build tmux `send-keys -H` commands for the given input bytes.
/// Encodes all bytes as hex, producing ONE command per chunk.
/// This is faster than `build_send_keys_commands` because tmux processes
/// one command instead of N (one per control character boundary).
fn build_hex_send_keys(pane_id: &str, bytes: &[u8]) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes
        .chunks(HEX_SEND_KEYS_CHUNK)
        .map(|chunk| {
            let mut cmd = format!("send-keys -H -t {}", pane_id);
            for &b in chunk {
                use std::fmt::Write;
                write!(cmd, " {:02x}", b).unwrap();
            }
            cmd
        })
        .collect()
}

/// Open a raw PTY pair with a specific window size. Returns (master_fd, slave_fd).
/// The PTY is set to raw mode (no echo, no output processing).
fn open_raw_pty(cols: u16, rows: u16) -> Result<(i32, i32), RuntimeError> {
    unsafe {
        let mut master: libc::c_int = 0;
        let mut slave: libc::c_int = 0;
        let ws = libc::winsize {
            ws_col: cols,
            ws_row: rows,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &ws as *const libc::winsize as *mut libc::winsize,
        ) != 0
        {
            return Err(RuntimeError::Backend("openpty failed".into()));
        }
        // Set raw mode: no echo, no signal processing, no output processing
        let mut term: libc::termios = std::mem::zeroed();
        libc::tcgetattr(master, &mut term);
        libc::cfmakeraw(&mut term);
        libc::tcsetattr(master, libc::TCSANOW, &term);
        Ok((master, slave))
    }
}

impl TmuxControlModeRuntime {
    /// Returns true if tmux's default-shell is zsh. Used to decide whether to pass
    /// `zsh -o nopromptsp` as the new-session/new-window command (avoids send-keys echo flash).
    fn is_default_shell_zsh() -> bool {
        let out = Command::new("tmux")
            .args(["show", "-g", "default-shell"])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout);
                let path = s.trim().to_lowercase();
                path.ends_with("zsh") || path.contains("/zsh")
            }
            _ => false,
        }
    }

    /// Query the current (active) window name of a session. Used when attaching without specifying a window.
    /// If the tmux command fails with "server exited unexpectedly", removes the stale socket so the next
    /// create path can start a fresh server.
    fn current_window_name(session_name: &str) -> String {
        let out = Command::new("tmux")
            .args([
                "list-windows",
                "-t",
                session_name,
                "-F",
                "#{?window_active,#{window_name},}",
            ])
            .output();
        if let Ok(ref o) = out {
            if !o.status.success() {
                let stderr = String::from_utf8_lossy(&o.stderr);
                if stderr.contains("server exited unexpectedly") {
                    Self::remove_stale_tmux_socket();
                }
            }
        }
        let stdout = match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };
        stdout
            .lines()
            .find_map(|l: &str| {
                let s = l.trim();
                if s.is_empty() { None } else { Some(s.to_string()) }
            })
            .unwrap_or_else(|| "main".to_string())
    }

    /// Create (or attach to) a tmux session and connect via control mode.
    /// When `window_name` is `None`, only attaches to the session and uses tmux's current window (match by name, no persist).
    /// When `Some(name)`, ensures session/window exist (create path).
    pub fn new(
        session_name: &str,
        window_name: Option<&str>,
        start_dir: Option<&Path>,
        initial_cols: u16,
        initial_rows: u16,
    ) -> Result<Self, RuntimeError> {
        let (window_name, skip_create_and_send_keys) = match window_name {
            Some(wn) => (wn.to_string(), false),
            None => {
                let wn = Self::current_window_name(session_name);
                // Attach-only path: session must exist (e.g. after tmux kill-server it doesn't → caller will create).
                let has = Command::new("tmux")
                    .args(["has-session", "-t", session_name])
                    .output();
                let session_exists = has
                    .as_ref()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if !session_exists {
                    return Err(RuntimeError::Backend(
                        "tmux session does not exist (e.g. after kill-server); use create path".into(),
                    ));
                }
                (wn, true)
            }
        };

        if !skip_create_and_send_keys {
            // Ensure session+window exists
            let mut create_args = vec!["new-session", "-d", "-s", session_name, "-n", &window_name];
            let dir_owned;
            if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
                dir_owned = dir.to_string();
                create_args.extend(["-c", &dir_owned]);
            }
            if Self::is_default_shell_zsh() {
                create_args.extend(["zsh", "-o", "nopromptsp"]);
            }
            let mut new_sess = Command::new("tmux").args(&create_args).output();
            if let Ok(ref o) = new_sess {
                if !o.status.success() {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if stderr.contains("server exited unexpectedly") {
                        Self::remove_stale_tmux_socket();
                        new_sess = Command::new("tmux").args(&create_args).output();
                    }
                }
            }
            if let Ok(ref o) = new_sess {
                if !o.status.success() {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if stderr.contains("duplicate session") {
                        let win_check = Command::new("tmux")
                            .args(["list-windows", "-t", session_name, "-F", "#{window_name}"])
                            .output();
                        let window_exists = win_check
                            .as_ref()
                            .map(|o| {
                                String::from_utf8_lossy(&o.stdout)
                                    .lines()
                                    .any(|l| l.trim() == window_name)
                            })
                            .unwrap_or(false);
                        if !window_exists {
                            let mut win_args =
                                vec!["new-window", "-d", "-t", session_name, "-n", &window_name];
                            if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
                                win_args.extend(["-c", dir]);
                            }
                            if Self::is_default_shell_zsh() {
                                win_args.extend(["zsh", "-o", "nopromptsp"]);
                            }
                            let _ = Command::new("tmux").args(&win_args).output();
                        }
                    }
                }
            }
        }

        // Configure tmux for true color, low escape-time (affects all sessions on this server).
        let _ = Command::new("tmux").args(["set", "-g", "default-terminal", "xterm-256color"]).output();
        let _ = Command::new("tmux").args(["set", "-as", "terminal-features", ",xterm-256color:RGB"]).output();
        let _ = Command::new("tmux").args(["set", "-s", "escape-time", "10"]).output();

        // Open raw PTY at the target size — tmux reads the PTY winsize to set client dims
        let (master_fd, slave_fd) = open_raw_pty(initial_cols, initial_rows)?;

        // Prevent master from leaking to child process
        unsafe {
            let flags = libc::fcntl(master_fd, libc::F_GETFD);
            libc::fcntl(master_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }

        // Dup slave for stdout; Stdio::from_raw_fd takes ownership and closes on drop
        let slave_dup = unsafe { libc::dup(slave_fd) };
        let child = unsafe {
            Command::new("tmux")
                .args(["-CC", "attach", "-t", session_name])
                .env("TERM", "xterm-256color")
                .stdin(Stdio::from_raw_fd(slave_fd))
                .stdout(Stdio::from_raw_fd(slave_dup))
                .stderr(Stdio::null())
                .spawn()
        }
        .map_err(|e| RuntimeError::Backend(format!("tmux -CC spawn failed: {}", e)))?;
        // slave_fd and slave_dup are now owned by Stdio; no manual close needed

        // Create reader/writer from master fd
        let master_reader = unsafe { std::fs::File::from_raw_fd(libc::dup(master_fd)) };
        let master_writer = unsafe { std::fs::File::from_raw_fd(master_fd) };

        let pane_outputs: Arc<Mutex<HashMap<String, flume::Sender<Vec<u8>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Parser thread: read control mode output from PTY master
        let outputs_for_thread = pane_outputs.clone();
        thread::spawn(move || {
            let mut parser = ControlModeParser::new();
            let mut buf_reader = BufReader::new(master_reader);
            let mut line_buf = Vec::new();
            loop {
                line_buf.clear();
                match buf_reader.read_until(b'\n', &mut line_buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        for event in parser.feed(&line_buf) {
                            match event {
                                ControlModeEvent::Output { pane_id, data } => {
                                    if let Ok(map) = outputs_for_thread.lock() {
                                        if let Some(tx) = map.get(&pane_id) {
                                            let _ = tx.send(data);
                                        } else {
                                            // #region agent log
                                            let keys: Vec<_> = map.keys().cloned().collect();
                                            crate::debug_log::dbg_session_log(
                                                "tmux_cc.rs:parser_thread",
                                                "output pane_id NOT in map",
                                                &serde_json::json!({
                                                    "output_pane_id": &pane_id,
                                                    "map_keys": keys,
                                                    "data_len": data.len(),
                                                }),
                                                "H_route",
                                            );
                                            // #endregion
                                        }
                                    }
                                }
                                ControlModeEvent::Exit => return,
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let pty_master_fd_dup = unsafe { libc::dup(master_fd) };
        let pty_writer = Arc::new(Mutex::new(master_writer));

        // Async input channel + writer thread with merged send-keys.
        // Merges consecutive keystrokes to the same pane into one send-keys command
        // for ~10x better throughput (3ms per command regardless of char count).
        let (input_tx, input_rx) = flume::unbounded::<(String, Vec<u8>)>();
        let pty_writer_for_input = pty_writer.clone();
        thread::spawn(move || {
            use std::io::Write;
            let mut cmd_buf = Vec::<u8>::with_capacity(256);
            loop {
                let (first_pane, first_bytes) = match input_rx.recv() {
                    Ok(v) => v,
                    Err(_) => break,
                };

                // Drain all immediately available inputs, merging by pane_id.
                // During paste or fast typing, multiple keystrokes queue up while
                // the previous flush is in progress — merge them into one command.
                let mut merged: Vec<(String, Vec<u8>)> = vec![(first_pane, first_bytes)];
                while let Ok((pane, bytes)) = input_rx.try_recv() {
                    if let Some(last) = merged.last_mut() {
                        if last.0 == pane {
                            last.1.extend_from_slice(&bytes);
                            continue;
                        }
                    }
                    merged.push((pane, bytes));
                }

                let mut writer = match pty_writer_for_input.lock() {
                    Ok(w) => w,
                    Err(_) => break,
                };

                // Write all commands into a single buffer, then write_all + flush once
                cmd_buf.clear();
                for (pane_id, bytes) in &merged {
                    for cmd in build_hex_send_keys(pane_id, bytes) {
                        cmd_buf.extend_from_slice(cmd.as_bytes());
                        cmd_buf.push(b'\n');
                    }
                }
                if !cmd_buf.is_empty() {
                    if writer.write_all(&cmd_buf).is_err() {
                        return;
                    }
                    let _ = writer.flush();
                }
            }
        });

        let rt = Self {
            session_name: session_name.to_string(),
            window_name: Mutex::new(window_name),
            pty_writer,
            input_tx,
            pane_outputs,
            pane_tty_writers: Arc::new(Mutex::new(HashMap::new())),
            _control_child: Arc::new(Mutex::new(child)),
            pty_master_fd: pty_master_fd_dup,
            skip_next_capture: std::sync::atomic::AtomicBool::new(false),
        };

        // Set client size so tmux generates output for panes at the correct dimensions
        let _ = rt.send_command(&format!("refresh-client -C {},{}", initial_cols, initial_rows));
        // #region agent log
        crate::debug_log::dbg_session_log(
            "tmux_cc.rs:new",
            "constructor refresh-client sent",
            &serde_json::json!({"session": session_name, "cols": initial_cols, "rows": initial_rows}),
            "H_dims",
        );
        // #endregion

        Ok(rt)
    }

    /// Send a raw tmux command via the PTY master.
    fn send_command(&self, cmd: &str) -> Result<(), RuntimeError> {
        let mut writer = self
            .pty_writer
            .lock()
            .map_err(|e| RuntimeError::Backend(format!("lock: {}", e)))?;
        writeln!(writer, "{}", cmd)
            .map_err(|e| RuntimeError::Backend(format!("write: {}", e)))?;
        writer
            .flush()
            .map_err(|e| RuntimeError::Backend(format!("flush: {}", e)))
    }

    /// Resolve pane TTY path via tmux display-message and open for writing.
    /// Returns None if tmux fails or the path cannot be opened.
    pub fn resolve_pane_tty(&self, pane_id: &str) -> Option<std::fs::File> {
        let output = Command::new("tmux")
            .args([
                "display-message",
                "-t",
                pane_id,
                "-p",
                "#{pane_tty}",
            ])
            .output()
            .ok()?;
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return None;
        }
        OpenOptions::new()
            .write(true)
            .open(&path)
            .ok()
    }

    /// Write bytes directly to the pane's TTY device. Returns Ok(true) on success,
    /// Ok(false) on cache miss. On write error, removes the pane from cache and returns Err.
    pub fn direct_write(&self, pane_id: &str, bytes: &[u8]) -> Result<bool, RuntimeError> {
        let mut cache = self
            .pane_tty_writers
            .lock()
            .map_err(|e| RuntimeError::Backend(format!("lock: {}", e)))?;
        let file = match cache.get_mut(pane_id) {
            Some(f) => f,
            None => return Ok(false),
        };
        if let Err(e) = file.write_all(bytes) {
            cache.remove(pane_id);
            return Err(RuntimeError::Backend(format!("direct TTY write: {}", e)));
        }
        if let Err(e) = file.flush() {
            cache.remove(pane_id);
            return Err(RuntimeError::Backend(format!("direct TTY flush: {}", e)));
        }
        Ok(true)
    }

    fn remove_stale_tmux_socket() {
        let uid = unsafe { libc::getuid() };
        let socket_path = format!("/tmp/tmux-{}/default", uid);
        if std::path::Path::new(&socket_path).exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
    }
}

impl Drop for TmuxControlModeRuntime {
    fn drop(&mut self) {
        // Detach cleanly, then kill child to prevent orphaned tmux -CC processes
        let _ = self.send_command("detach");
        if let Ok(mut child) = self._control_child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl AgentRuntime for TmuxControlModeRuntime {
    fn backend_type(&self) -> &'static str {
        "tmux-cc"
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        if bytes.is_empty() {
            return Ok(());
        }

        // Non-blocking: enqueue for the writer thread (never blocks UI thread)
        self.input_tx
            .send((pane_id.clone(), bytes.to_vec()))
            .map_err(|e| RuntimeError::Backend(format!("input channel: {}", e)))
    }

    fn send_key(&self, pane_id: &PaneId, key: &str, use_literal: bool) -> Result<(), RuntimeError> {
        if use_literal {
            self.send_command(&format!("send-keys -l -t {} '{}'", pane_id, key))
        } else {
            self.send_command(&format!("send-keys -t {} {}", pane_id, key))
        }
    }

    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        // Update the PTY winsize so tmux's client size stays consistent
        unsafe {
            let ws = libc::winsize {
                ws_col: cols,
                ws_row: rows,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            libc::ioctl(self.pty_master_fd, libc::TIOCSWINSZ, &ws);
        }
        self.send_command(&format!(
            "resize-pane -t {} -x {} -y {}",
            pane_id, cols, rows
        ))?;
        self.send_command(&format!("refresh-client -C {},{}", cols, rows))
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        let (tx, rx) = flume::unbounded();

        // Resolve "session:window.0" to actual pane id (%0, %1, ...) so parser's %output events find this channel.
        let map_key = if pane_id.contains(':') {
            let target = pane_id
                .rsplitn(2, '.')
                .nth(1)
                .unwrap_or(pane_id)
                .to_string();
            let out = Command::new("tmux")
                .args(["list-panes", "-t", &target, "-F", "#{pane_id}"])
                .output()
                .ok()?;
            let first = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)?;
            first
        } else {
            pane_id.to_string()
        };

        let skip = self.skip_next_capture.swap(false, std::sync::atomic::Ordering::SeqCst);

        // #region agent log
        crate::debug_log::dbg_session_log(
            "tmux_cc.rs:subscribe_output",
            "subscribe_output entry",
            &serde_json::json!({"pane_id": pane_id, "map_key": &map_key, "skip_capture": skip}),
            "H_flash",
        );
        // #endregion

        if !skip {
            if let Some(initial) = self.capture_initial_content(pane_id) {
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "tmux_cc.rs:subscribe_output",
                    "initial content captured",
                    &serde_json::json!({"pane_id": pane_id, "len": initial.len()}),
                    "H_flash",
                );
                // #endregion
                let _ = tx.send(initial);
            }
        }

        if let Ok(mut map) = self.pane_outputs.lock() {
            map.insert(map_key, tx);
        }

        Some(rx)
    }

    fn capture_initial_content(&self, pane_id: &PaneId) -> Option<Vec<u8>> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", pane_id, "-p", "-e"])
            .output()
            .ok()?;
        // #region agent log
        crate::debug_log::dbg_session_log(
            "tmux_cc.rs:capture_initial_content",
            "capture result",
            &serde_json::json!({
                "pane_id": pane_id,
                "success": output.status.success(),
                "stdout_len": output.stdout.len(),
                "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            "H_capture",
        );
        // #endregion
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        // Trim trailing blank lines that would scroll the viewport and corrupt
        // VTE parser state. Prepend cursor-home + clear-screen so the injected
        // snapshot starts from a known-clean terminal state.
        let text = String::from_utf8_lossy(&output.stdout);
        let trimmed = text.trim_end_matches('\n');
        if trimmed.is_empty() {
            return None;
        }
        let mut result = b"\x1b[H\x1b[2J".to_vec(); // CSI H = home, CSI 2J = clear
        // capture-pane uses \n (LF) between lines, but VTE interprets LF as
        // "move cursor down" without returning to column 0. Use \r\n so each
        // line starts at the left margin.
        let mut with_crlf = trimmed.replace('\n', "\r\n");
        // First line after attach often has no trailing space (clear/redraw timing); ensure one so cursor isn't flush with prompt.
        if !with_crlf.is_empty() && !with_crlf.ends_with(' ') {
            with_crlf.push(' ');
        }
        result.extend_from_slice(with_crlf.as_bytes());
        Some(result)
    }

    fn list_panes(&self, _agent_id: &AgentId) -> Vec<PaneId> {
        let wn = self.window_name.lock().map(|w| w.clone()).unwrap_or_default();
        let target = format!("{}:{}", self.session_name, wn);
        let output_result = Command::new("tmux")
            .args(["list-panes", "-t", &target, "-F", "#{pane_id}"])
            .output();
        let result = output_result
            .as_ref()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        // #region agent log
        {
            let stderr_str = output_result.as_ref().ok()
                .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                .unwrap_or_default();
            let status = output_result.as_ref().ok().map(|o| o.status.success());
            crate::debug_log::dbg_session_log(
                "tmux_cc.rs:list_panes",
                "list_panes result",
                &serde_json::json!({
                    "target": &target,
                    "panes": &result,
                    "success": status,
                    "stderr": stderr_str,
                }),
                "H_pane_empty",
            );
        }
        // #endregion
        result
    }

    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        self.send_command(&format!("select-pane -t {}", pane_id))
    }

    fn split_pane(&self, pane_id: &PaneId, vertical: bool) -> Result<PaneId, RuntimeError> {
        let flag = if vertical { "-h" } else { "-v" };
        self.send_command(&format!("split-window {} -t {}", flag, pane_id))?;
        // Return last pane in list (the newly created one)
        self.list_panes(&String::new())
            .into_iter()
            .last()
            .ok_or_else(|| RuntimeError::Backend("no pane after split".into()))
    }

    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16) {
        let output = Command::new("tmux")
            .args(["display-message", "-t", pane_id, "-p", "-F", "#{pane_width} #{pane_height}"])
            .output()
            .ok();
        if let Some(o) = output {
            let text = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = text.trim().split_whitespace().collect();
            if parts.len() >= 2 {
                if let (Ok(c), Ok(r)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                    return (c, r);
                }
            }
        }
        (80, 24)
    }

    fn open_diff(
        &self,
        worktree: &Path,
        _pane_id: Option<&PaneId>,
    ) -> Result<String, RuntimeError> {
        let branch = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(worktree)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "branch".into());
        let safe_branch = branch.replace('/', "-").replace(' ', "-");
        let window_name = format!("review-{}", safe_branch);
        let worktree_str = worktree.to_string_lossy();
        let target = format!("{}:{}", self.session_name, window_name);
        let args_new = [
            "new-window",
            "-d",
            "-t",
            &self.session_name,
            "-n",
            &window_name,
            "-c",
            worktree_str.as_ref(),
        ];
        Command::new("tmux")
            .args(&args_new)
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux new-window: {}", e)))?;
        let cmd = format!(
            "nvim -c 'DiffviewOpen main...HEAD' '{}' 2>/dev/null || git diff main...HEAD --color=always | less -R\n",
            worktree.to_string_lossy()
        );
        Command::new("tmux")
            .args(["send-keys", "-t", &target, &cmd, "Enter"])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux send-keys: {}", e)))?;
        Ok(window_name)
    }

    fn open_review(&self, worktree: &Path) -> Result<String, RuntimeError> {
        self.open_diff(worktree, None)
    }

    fn kill_window(&self, window_target: &str) -> Result<(), RuntimeError> {
        self.send_command(&format!("kill-window -t {}", window_target))
    }

    fn set_skip_initial_capture(&self) {
        self.skip_next_capture.store(true, std::sync::atomic::Ordering::SeqCst);
    }


    fn session_info(&self) -> Option<(String, String)> {
        let wn = self.window_name.lock().map(|w| w.clone()).unwrap_or_default();
        Some((self.session_name.clone(), wn))
    }

    fn switch_window(&self, window_name: &str, start_dir: Option<&Path>) -> Result<(), RuntimeError> {
        // Check if window already exists — reuse it to preserve content
        let win_check = Command::new("tmux")
            .args(["list-windows", "-t", &self.session_name, "-F", "#{window_name}"])
            .output();
        let window_exists = win_check
            .as_ref()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|l| l.trim() == window_name)
            })
            .unwrap_or(false);

        // #region agent log
        crate::debug_log::dbg_session_log(
            "tmux_cc.rs:switch_window",
            "switch_window called",
            &serde_json::json!({
                "window_name": window_name,
                "window_exists": window_exists,
                "session_name": &self.session_name,
                "all_windows": win_check.as_ref().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
            }),
            "H5",
        );
        // #endregion

        if !window_exists {
            // Create via synchronous CLI so the window is ready immediately.
            let mut win_args = vec![
                "new-window".to_string(),
                "-d".to_string(),
                "-t".to_string(),
                self.session_name.clone(),
                "-n".to_string(),
                window_name.to_string(),
            ];
            if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
                win_args.extend(["-c".to_string(), dir.to_string()]);
            }
            let args_ref: Vec<&str> = win_args.iter().map(|s| s.as_str()).collect();
            let _ = Command::new("tmux").args(&args_ref).output();

            // Disable PROMPT_SP in the new window's shell (same reason as in new())
            let pane_target = format!("{}:{}", self.session_name, window_name);
            let _ = Command::new("tmux")
                .args(["send-keys", "-t", &pane_target, " unsetopt PROMPT_SP 2>/dev/null; clear", "Enter"])
                .output();
        }

        // Update window_name BEFORE any pane queries so list_panes targets the
        // correct window.
        if let Ok(mut wn) = self.window_name.lock() {
            *wn = window_name.to_string();
        }

        if let Ok(mut map) = self.pane_outputs.lock() {
            map.clear();
        }

        if let Ok(mut cache) = self.pane_tty_writers.lock() {
            cache.clear();
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    // ── Parser tests (no tmux required) ──────────────────────

    #[test]
    fn test_parse_output_event() {
        let mut parser = ControlModeParser::new();
        let events = parser.feed(b"%output %0 hello world\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ControlModeEvent::Output { pane_id, data } => {
                assert_eq!(pane_id, "%0");
                assert_eq!(data, b"hello world");
            }
            _ => panic!("expected Output event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_parse_output_unescapes_octal() {
        let mut parser = ControlModeParser::new();
        // \033 = ESC (0x1b), \015 = CR (0x0d), \012 = LF (0x0a)
        let events = parser.feed(b"%output %0 \\033[32mhello\\033[0m\\015\\012\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ControlModeEvent::Output { pane_id, data } => {
                assert_eq!(pane_id, "%0");
                assert_eq!(data, b"\x1b[32mhello\x1b[0m\r\n");
            }
            _ => panic!("expected Output event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_parse_output_unescapes_backslash() {
        let mut parser = ControlModeParser::new();
        let events = parser.feed(b"%output %0 path\\\\to\\\\file\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ControlModeEvent::Output { pane_id, data } => {
                assert_eq!(pane_id, "%0");
                assert_eq!(data, b"path\\to\\file");
            }
            _ => panic!("expected Output event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_unescape_tmux_output() {
        assert_eq!(unescape_tmux_output(b"hello"), b"hello");
        assert_eq!(unescape_tmux_output(b"\\033"), b"\x1b");
        assert_eq!(unescape_tmux_output(b"\\\\"), b"\\");
        assert_eq!(unescape_tmux_output(b"a\\015\\012b"), b"a\r\nb");
        assert_eq!(unescape_tmux_output(b"\\177"), b"\x7f");
        // Incomplete octal at end of input
        assert_eq!(unescape_tmux_output(b"\\01"), b"\\01");
    }

    #[test]
    fn test_parse_exit_event() {
        let mut parser = ControlModeParser::new();
        let events = parser.feed(b"%exit\n");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ControlModeEvent::Exit));
    }

    #[test]
    fn test_parse_partial_line() {
        let mut parser = ControlModeParser::new();
        let events1 = parser.feed(b"%output %0 hel");
        assert!(events1.is_empty(), "partial line should not emit events");
        let events2 = parser.feed(b"lo\n");
        assert_eq!(events2.len(), 1);
        match &events2[0] {
            ControlModeEvent::Output { pane_id, data } => {
                assert_eq!(pane_id, "%0");
                assert_eq!(data, b"hello");
            }
            _ => panic!("expected Output event"),
        }
    }

    #[test]
    fn test_parse_begin_end() {
        let mut parser = ControlModeParser::new();
        let events =
            parser.feed(b"%begin 1234567890 1 0\nsome response\n%end 1234567890 1 0\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ControlModeEvent::BeginEnd { .. })),
            "should have BeginEnd event, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_session_changed() {
        let mut parser = ControlModeParser::new();
        let events = parser.feed(b"%session-changed $1 mysession\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ControlModeEvent::SessionChanged { session_id, name } => {
                assert_eq!(session_id, "$1");
                assert_eq!(name, "mysession");
            }
            _ => panic!("expected SessionChanged"),
        }
    }

    #[test]
    fn test_parse_multiple_outputs() {
        let mut parser = ControlModeParser::new();
        let events = parser.feed(b"%output %0 line1\n%output %1 line2\n%exit\n");
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], ControlModeEvent::Output { pane_id, .. } if pane_id == "%0"));
        assert!(matches!(&events[1], ControlModeEvent::Output { pane_id, .. } if pane_id == "%1"));
        assert!(matches!(&events[2], ControlModeEvent::Exit));
    }

    // ── Runtime tests (require tmux) ──────────────────────────

    #[test]
    fn test_control_mode_runtime_backend_type() {
        if !crate::runtime::backends::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rt =
            TmuxControlModeRuntime::new("pmux-test-cc", Some("main"), Some(dir.path()), 80, 24)
                .expect("should create control mode runtime");
        assert_eq!(rt.backend_type(), "tmux-cc");

        // Cleanup
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", "pmux-test-cc"])
            .output();
    }

    #[test]
    fn test_control_mode_subscribe_output_returns_receiver() {
        if !crate::runtime::backends::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rt =
            TmuxControlModeRuntime::new("pmux-test-sub", Some("main"), Some(dir.path()), 80, 24)
                .expect("should create runtime");

        let panes = rt.list_panes(&String::new());
        let pane_id = panes.first().cloned().unwrap_or_else(|| "%0".to_string());
        let rx = rt.subscribe_output(&pane_id);
        assert!(rx.is_some(), "subscribe_output should return a receiver");

        // Cleanup
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", "pmux-test-sub"])
            .output();
    }

    #[test]
    fn test_control_mode_session_info() {
        if !crate::runtime::backends::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rt =
            TmuxControlModeRuntime::new("pmux-test-info", Some("main-window"), Some(dir.path()), 80, 24)
                .expect("should create runtime");
        let info = rt.session_info();
        assert_eq!(info, Some(("pmux-test-info".to_string(), "main-window".to_string())));

        // Cleanup
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", "pmux-test-info"])
            .output();
    }

    #[test]
    fn test_resolve_pane_tty_returns_file() {
        if !crate::runtime::backends::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rt = TmuxControlModeRuntime::new("pmux-test-tty", Some("main"), Some(dir.path()), 80, 24)
            .expect("should create runtime");
        let panes = rt.list_panes(&String::new());
        let pane_id = panes.first().cloned().unwrap_or_else(|| "%0".to_string());
        let file = rt.resolve_pane_tty(&pane_id);
        assert!(file.is_some(), "should resolve pane TTY for {}", pane_id);
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", "pmux-test-tty"])
            .output();
    }

    #[test]
    fn test_direct_write_cache_miss_then_hit() {
        if !crate::runtime::backends::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rt = TmuxControlModeRuntime::new("pmux-test-dw", Some("main"), Some(dir.path()), 80, 24)
            .expect("should create runtime");
        let panes = rt.list_panes(&String::new());
        let pane_id = panes.first().cloned().unwrap_or_else(|| "%0".to_string());

        // Cache miss
        assert!(matches!(rt.direct_write(&pane_id, b"x"), Ok(false)));

        // Populate cache
        if let Some(file) = rt.resolve_pane_tty(&pane_id) {
            rt.pane_tty_writers
                .lock()
                .unwrap()
                .insert(pane_id.clone(), file);
        }

        // Cache hit
        assert!(matches!(rt.direct_write(&pane_id, b"x"), Ok(true)));

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", "pmux-test-dw"])
            .output();
    }

    // ── Hex send-keys tests (no tmux required) ──────────────

    #[test]
    fn test_build_hex_send_keys_empty() {
        let cmds = build_hex_send_keys("%0", &[]);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_build_hex_send_keys_printable() {
        let cmds = build_hex_send_keys("%0", b"hello");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "send-keys -H -t %0 68 65 6c 6c 6f");
    }

    #[test]
    fn test_build_hex_send_keys_with_enter() {
        let cmds = build_hex_send_keys("%0", b"ls\r");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "send-keys -H -t %0 6c 73 0d");
    }

    #[test]
    fn test_build_hex_send_keys_escape_sequence() {
        let cmds = build_hex_send_keys("%0", b"\x1b[A");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "send-keys -H -t %0 1b 5b 41");
    }

    #[test]
    fn test_build_hex_send_keys_ctrl_c() {
        let cmds = build_hex_send_keys("%0", b"\x03");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "send-keys -H -t %0 03");
    }

    #[test]
    fn test_build_hex_send_keys_utf8() {
        let cmds = build_hex_send_keys("%0", "你".as_bytes());
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "send-keys -H -t %0 e4 bd a0");
    }

    #[test]
    fn test_build_hex_send_keys_chunking() {
        let data = vec![0x61u8; 600];
        let cmds = build_hex_send_keys("%0", &data);
        assert_eq!(cmds.len(), 2);
        let hex_count_1 = cmds[0].trim_start_matches("send-keys -H -t %0 ")
            .split_whitespace().count();
        assert_eq!(hex_count_1, 512);
        let hex_count_2 = cmds[1].trim_start_matches("send-keys -H -t %0 ")
            .split_whitespace().count();
        assert_eq!(hex_count_2, 88);
    }

    #[test]
    fn test_hex_send_keys_roundtrip() {
        if !crate::runtime::backends::tmux_available() {
            eprintln!("skipping: tmux not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rt = TmuxControlModeRuntime::new("pmux-test-hex", Some("main"), Some(dir.path()), 80, 24)
            .expect("should create runtime");

        let panes = rt.list_panes(&String::new());
        let pane_id = panes.first().cloned().unwrap_or_else(|| "%0".to_string());

        let rx = rt.subscribe_output(&pane_id).expect("should get receiver");

        std::thread::sleep(std::time::Duration::from_millis(500));
        while rx.try_recv().is_ok() {}

        let input = b"echo HEX_OK\r";
        rt.send_input(&pane_id, input).expect("send_input should work");

        std::thread::sleep(std::time::Duration::from_millis(1000));
        let mut output = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            output.extend_from_slice(&chunk);
        }

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("HEX_OK"),
            "expected 'HEX_OK' in output, got: {}",
            output_str
        );

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", "pmux-test-hex"])
            .output();
    }
}
