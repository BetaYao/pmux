# Terminal Engine Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace gpui-terminal (alacritty_terminal) with gpui-ghostty (Ghostty VT) and migrate from tmux -CC control mode to standard tmux with session-per-worktree, window-per-terminal.

**Architecture:** Each worktree maps to a tmux session. Each terminal pane maps to a tmux window (single pane per window = independent PTY). PTY bytes flow directly to gpui-ghostty TerminalSession/TerminalView. Split layout is managed entirely by pmux's UI layer.

**Tech Stack:** gpui-ghostty (Ghostty VT v1.2.3 + GPUI renderer), Zig 0.14.1 (build dependency), portable-pty, tmux (standard mode)

**Spec:** `docs/superpowers/specs/2026-03-18-terminal-engine-migration-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|----------------|
| `src/runtime/backends/tmux_standard.rs` | New tmux backend: session/window CRUD, PTY bridging, output streaming, resize |
| `scripts/bootstrap-zig.sh` | Download Zig 0.14.1 for ghostty_vt_sys build |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` | Replace `alacritty_terminal` with `gpui_ghostty_terminal` + `ghostty_vt` |
| `src/terminal/mod.rs` | Re-export gpui-ghostty types, remove alacritty re-exports |
| `src/ui/terminal_view.rs` | Wrap gpui-ghostty `TerminalView` instead of custom `TerminalElement` |
| `src/ui/terminal_manager.rs` | Create gpui-ghostty sessions, wire PTY I/O, simplify output pipeline |
| `src/ui/terminal_area_entity.rs` | Adapt to new TerminalView entity model |
| `src/runtime/backends/mod.rs` | Register new tmux_standard backend, update factory functions |
| `src/runtime/agent_runtime.rs` | Simplify trait (remove tmux-CC-specific methods) |
| `src/ui/app_root.rs` | Remove `coalesce_and_process_output`, adapt workspace switching |
| `src/config.rs` | Update backend enum (remove "tmux-cc", add "tmux-standard" or just "tmux") |

### Deleted Files

| File | Reason |
|------|--------|
| `src/terminal/terminal_element.rs` | Replaced by gpui-ghostty TerminalView renderer |
| `src/terminal/terminal_rendering.rs` | Replaced by gpui-ghostty renderer |
| `src/terminal/terminal_core.rs` | Replaced by gpui-ghostty TerminalSession |
| `src/terminal/input.rs` | Replaced by gpui-ghostty TerminalInput + key encoding |
| `src/terminal/terminal_input_handler.rs` | Replaced by gpui-ghostty built-in IME |
| `src/terminal/colors.rs` | Replaced by gpui-ghostty color handling |
| `src/terminal/box_drawing.rs` | Replaced by gpui-ghostty renderer |
| `src/runtime/backends/tmux_control_mode.rs` | Replaced by tmux_standard.rs |
| `src/runtime/backends/tmux.rs` | Legacy, already deprecated |

### Unchanged Files

| File | Why |
|------|-----|
| `src/terminal/content_extractor.rs` | Operates on raw bytes, independent of terminal engine |
| `src/shell_integration.rs` | OSC 133 byte-level parsing |
| `src/status_detector.rs` | Text pattern matching |
| `src/ui/split_pane_container.rs` | Pure UI layout |
| `src/ui/sidebar.rs` | Event consumer |
| `src/ui/notification_*.rs` | Event-driven |
| `src/runtime/backends/local_pty.rs` | Independent backend, keep as fallback |

---

## Task 1: Add gpui-ghostty dependency and Zig toolchain

**Files:**
- Modify: `Cargo.toml`
- Create: `scripts/bootstrap-zig.sh`

- [ ] **Step 1: Add bootstrap-zig.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

ZIG_VERSION="0.14.1"
INSTALL_DIR=".context/zig"

if [ -x "$INSTALL_DIR/zig" ]; then
    echo "Zig already installed at $INSTALL_DIR/zig"
    exit 0
fi

ARCH=$(uname -m)
case "$ARCH" in
    arm64|aarch64) ARCH="aarch64" ;;
    x86_64) ARCH="x86_64" ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

URL="https://ziglang.org/download/${ZIG_VERSION}/zig-macos-${ARCH}-${ZIG_VERSION}.tar.xz"
echo "Downloading Zig ${ZIG_VERSION} for ${ARCH}..."

mkdir -p "$INSTALL_DIR"
curl -fSL "$URL" | tar -xJ --strip-components=1 -C "$INSTALL_DIR"
echo "Zig installed to $INSTALL_DIR/zig"
```

- [ ] **Step 2: Run bootstrap**

Run: `chmod +x scripts/bootstrap-zig.sh && ./scripts/bootstrap-zig.sh`
Expected: Zig 0.14.1 installed at `.context/zig/zig`

- [ ] **Step 3: Add gpui-ghostty as git submodule or dependency**

Add to `Cargo.toml`:

```toml
# Remove:
# alacritty_terminal = "0.25"

# Add gpui-ghostty workspace members (as git dependency or path):
gpui_ghostty_terminal = { git = "https://github.com/Xuanwo/gpui-ghostty", rev = "<pin-latest-commit>" }
ghostty_vt = { git = "https://github.com/Xuanwo/gpui-ghostty", rev = "<pin-latest-commit>" }
```

Note: The exact integration method (git dep vs submodule vs vendored) depends on whether gpui-ghostty's GPUI pin is compatible with pmux's GPUI pin. Check both `rev` values match or are compatible.

- [ ] **Step 4: Verify compilation**

Run: `RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -50`
Expected: Compilation proceeds (may have unused import warnings, but no errors from gpui-ghostty itself)

- [ ] **Step 5: Add .context/ to .gitignore**

```
# Zig toolchain
.context/
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml scripts/bootstrap-zig.sh .gitignore
git commit -m "build: add gpui-ghostty dependency and Zig toolchain bootstrap"
```

---

## Task 2: Implement tmux standard backend

**Files:**
- Create: `src/runtime/backends/tmux_standard.rs`
- Modify: `src/runtime/backends/mod.rs`
- Modify: `src/runtime/agent_runtime.rs`

This is the core new backend. Each worktree = tmux session, each terminal = tmux window (single pane).

**PTY bridging strategy:** We own the PTY master via `portable-pty`, spawn the shell on the slave side, and create the tmux window to adopt the session. This gives us direct byte-level read/write on the PTY master while tmux provides session persistence. Input goes directly to PTY master (no `tmux send-keys` overhead). Output is read directly from PTY master (no `pipe-pane` or polling).

- [ ] **Step 1: Write test for session/window naming**

In `src/runtime/backends/tmux_standard.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_name() {
        assert_eq!(
            TmuxStandardBackend::session_name("/Users/me/work/my-project", "feature-x"),
            "pmux-my-project-feature-x"
        );
    }

    #[test]
    fn test_session_name_sanitizes() {
        // tmux session names cannot contain periods or colons
        assert_eq!(
            TmuxStandardBackend::session_name("/Users/me/work/my.project", "feat/x"),
            "pmux-my_project-feat_x"
        );
    }

    #[test]
    fn test_window_id_format() {
        assert_eq!(
            TmuxStandardBackend::window_target("pmux-proj-feat", 0),
            "pmux-proj-feat:0"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `RUSTUP_TOOLCHAIN=stable cargo test tmux_standard --lib 2>&1 | tail -5`
Expected: FAIL — module not found

- [ ] **Step 3: Implement TmuxStandardBackend struct and naming**

```rust
//! tmux standard mode backend.
//!
//! Mapping:
//!   worktree → tmux session "pmux-<repo>-<branch>"
//!   terminal → tmux window (single pane, independent PTY)
//!   split layout → pmux UI layer (SplitPaneContainer)

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runtime::agent_runtime::{AgentId, AgentRuntime, PaneId, RuntimeError};

/// Sanitize a string for use in tmux session/window names.
/// Replaces periods, colons, and slashes with underscores.
fn sanitize_tmux_name(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '.' | ':' | '/' => '_',
            _ => c,
        })
        .collect()
}

/// Per-window state: PTY master, reader/writer channels, dimensions.
struct WindowState {
    window_index: u32,
    pty_master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    output_tx: flume::Sender<Vec<u8>>,
    output_rx: flume::Receiver<Vec<u8>>,
    input_tx: flume::Sender<Vec<u8>>,
    cols: AtomicU16,
    rows: AtomicU16,
    _reader_handle: thread::JoinHandle<()>,
    _writer_handle: thread::JoinHandle<()>,
}

pub struct TmuxStandardBackend {
    session_name: String,
    worktree_path: PathBuf,
    windows: Mutex<HashMap<PaneId, Arc<WindowState>>>,
    default_cols: u16,
    default_rows: u16,
}

impl TmuxStandardBackend {
    /// Derive tmux session name from repo path and branch.
    pub fn session_name(repo_path: &str, branch: &str) -> String {
        let repo = Path::new(repo_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        format!("pmux-{}-{}", sanitize_tmux_name(&repo), sanitize_tmux_name(branch))
    }

    /// Format a tmux window target: "session:window_index"
    pub fn window_target(session: &str, window_index: u32) -> String {
        format!("{}:{}", session, window_index)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `RUSTUP_TOOLCHAIN=stable cargo test tmux_standard --lib 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Write test for session creation (integration)**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_create_and_list_session() {
        let session = "pmux-test-integration";
        // Ensure clean state
        let _ = Command::new("tmux").args(["kill-session", "-t", session]).output();

        let backend = TmuxStandardBackend::new(
            session,
            Path::new("/tmp"),
            120, 36,
        ).unwrap();

        // Session should exist
        let output = Command::new("tmux")
            .args(["has-session", "-t", session])
            .output()
            .unwrap();
        assert!(output.status.success());

        // Cleanup
        let _ = Command::new("tmux").args(["kill-session", "-t", session]).output();
    }
}
```

- [ ] **Step 6: Implement new(), create_window(), destroy_window()**

```rust
impl TmuxStandardBackend {
    /// Create a new backend, starting or attaching to the tmux session.
    pub fn new(
        session_name: &str,
        worktree_path: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Self, RuntimeError> {
        // Create session if it doesn't exist (detached, with initial size)
        let has = Command::new("tmux")
            .args(["has-session", "-t", session_name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !has {
            let status = Command::new("tmux")
                .args([
                    "new-session", "-d",
                    "-s", session_name,
                    "-x", &cols.to_string(),
                    "-y", &rows.to_string(),
                    "-c", &worktree_path.display().to_string(),
                ])
                .status()
                .map_err(|e| RuntimeError::Backend(format!("tmux new-session failed: {e}")))?;

            if !status.success() {
                return Err(RuntimeError::Backend("tmux new-session failed".into()));
            }
        }

        Ok(Self {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows: Mutex::new(HashMap::new()),
            next_window: Mutex::new(0),
            default_cols: cols,
            default_rows: rows,
        })
    }

    /// Create a new tmux window backed by a PTY we own.
    ///
    /// Strategy: We create a PTY pair via portable-pty, then use
    /// `tmux new-window` with the shell spawned on our PTY slave.
    /// This gives us the PTY master fd for direct byte-level I/O,
    /// while tmux manages the session persistence.
    pub fn create_window(&self, name: &str) -> Result<PaneId, RuntimeError> {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};

        // 1. Create PTY pair (we own the master, tmux gets the slave)
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: self.default_rows,
                cols: self.default_cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::Backend(format!("openpty failed: {e}")))?;

        // 2. Spawn shell on the slave side
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&self.worktree_path);
        let _child = pair.slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::Backend(format!("spawn shell failed: {e}")))?;

        // 3. Create tmux window that attaches to the same PTY
        //    Using respawn-window or new-window with the shell already running
        let output = Command::new("tmux")
            .args([
                "new-window", "-t", &self.session_name,
                "-n", name,
                "-c", &self.worktree_path.display().to_string(),
                "-P", "-F", "#{window_index}",
            ])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("tmux new-window failed: {e}")))?;

        let actual_idx: u32 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);

        let pane_id = Self::window_target(&self.session_name, actual_idx);

        // 4. Start PTY master reader thread → output channel
        let (output_tx, output_rx) = flume::unbounded();
        let mut reader = pair.master
            .try_clone_reader()
            .map_err(|e| RuntimeError::Backend(format!("clone PTY reader failed: {e}")))?;

        let pane_id_clone = pane_id.clone();
        let reader_handle = thread::Builder::new()
            .name(format!("pty-reader-{}", pane_id_clone))
            .spawn(move || {
                let mut buf = [0u8; 4096];
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
            })
            .map_err(|e| RuntimeError::Backend(format!("spawn reader failed: {e}")))?;

        // 5. Start PTY master writer thread ← input channel
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();
        let mut writer = pair.master
            .take_writer()
            .map_err(|e| RuntimeError::Backend(format!("take PTY writer failed: {e}")))?;

        let writer_handle = thread::Builder::new()
            .name(format!("pty-writer-{}", pane_id))
            .spawn(move || {
                use std::io::Write;
                while let Ok(bytes) = input_rx.recv() {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| RuntimeError::Backend(format!("spawn writer failed: {e}")))?;

        let state = Arc::new(WindowState {
            window_index: actual_idx,
            pty_master: Mutex::new(pair.master),
            output_tx: flume::Sender::clone(&flume::unbounded().0), // placeholder, real tx is in reader
            output_rx,
            input_tx,
            cols: AtomicU16::new(self.default_cols),
            rows: AtomicU16::new(self.default_rows),
            _reader_handle: reader_handle,
            _writer_handle: writer_handle,
        });

        self.windows.lock().unwrap().insert(pane_id.clone(), state);

        Ok(pane_id)
    }
}
```

- [ ] **Step 7: Run integration test**

Run: `RUSTUP_TOOLCHAIN=stable cargo test test_create_and_list_session --lib 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 8: Implement AgentRuntime trait for TmuxStandardBackend**

```rust
impl AgentRuntime for TmuxStandardBackend {
    fn backend_type(&self) -> &'static str { "tmux-standard" }

    fn primary_pane_id(&self) -> Option<PaneId> {
        self.windows.lock().unwrap().keys().next().cloned()
    }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        // Write directly to PTY master — no tmux send-keys overhead
        if let Some(state) = self.windows.lock().unwrap().get(pane_id) {
            state.input_tx.send(bytes.to_vec())
                .map_err(|e| RuntimeError::Backend(format!("input channel closed: {e}")))?;
        }
        Ok(())
    }

    fn send_key(&self, pane_id: &PaneId, key: &str, use_literal: bool) -> Result<(), RuntimeError> {
        let mut args = vec!["send-keys", "-t", pane_id];
        if use_literal { args.push("-l"); }
        args.push(key);
        Command::new("tmux").args(&args).output()
            .map_err(|e| RuntimeError::Backend(format!("send-key failed: {e}")))?;
        Ok(())
    }

    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        if let Some(state) = self.windows.lock().unwrap().get(pane_id) {
            // Resize PTY master directly (sends SIGWINCH to child)
            state.pty_master.lock().unwrap()
                .resize(portable_pty::PtySize {
                    rows, cols, pixel_width: 0, pixel_height: 0,
                })
                .map_err(|e| RuntimeError::Backend(format!("PTY resize failed: {e}")))?;
            state.cols.store(cols, Ordering::Relaxed);
            state.rows.store(rows, Ordering::Relaxed);
        }
        Ok(())
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        self.windows.lock().unwrap()
            .get(pane_id)
            .map(|s| s.output_rx.clone())
    }

    fn capture_initial_content(&self, pane_id: &PaneId) -> Option<Vec<u8>> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", pane_id, "-p", "-e"])
            .output()
            .ok()?;
        if output.status.success() {
            Some(output.stdout)
        } else {
            None
        }
    }

    fn list_panes(&self, _agent_id: &AgentId) -> Vec<PaneId> {
        self.windows.lock().unwrap().keys().cloned().collect()
    }

    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        Command::new("tmux")
            .args(["select-window", "-t", pane_id])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("focus failed: {e}")))?;
        Ok(())
    }

    fn split_pane(&self, _pane_id: &PaneId, _vertical: bool) -> Result<PaneId, RuntimeError> {
        // In new architecture, "split" creates a new tmux window (not tmux split)
        // UI layer handles visual splitting via SplitPaneContainer
        self.create_window("split")
    }

    fn kill_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError> {
        self.windows.lock().unwrap().remove(pane_id);
        Command::new("tmux")
            .args(["kill-window", "-t", pane_id])
            .output()
            .map_err(|e| RuntimeError::Backend(format!("kill-window failed: {e}")))?;
        Ok(())
    }

    fn get_pane_dimensions(&self, pane_id: &PaneId) -> (u16, u16) {
        self.windows.lock().unwrap()
            .get(pane_id)
            .map(|s| (s.cols.load(Ordering::Relaxed), s.rows.load(Ordering::Relaxed)))
            .unwrap_or((self.default_cols, self.default_rows))
    }

    fn open_diff(&self, worktree: &Path, _pane_id: Option<&PaneId>) -> Result<String, RuntimeError> {
        let pane_id = self.create_window("diff")?;
        self.send_key(&pane_id, &format!("cd {} && git diff", worktree.display()), true)?;
        self.send_key(&pane_id, "Enter", false)?;
        Ok(pane_id)
    }

    fn open_review(&self, worktree: &Path) -> Result<String, RuntimeError> {
        self.open_diff(worktree, None)
    }

    fn kill_window(&self, window_target: &str) -> Result<(), RuntimeError> {
        self.kill_pane(&window_target.to_string())
    }

    fn session_info(&self) -> Option<(String, String)> {
        Some((self.session_name.clone(), "0".to_string()))
    }
}
```

- [ ] **Step 9: Register backend in mod.rs**

In `src/runtime/backends/mod.rs`, add:

```rust
pub mod tmux_standard;
```

Update `create_runtime_from_env()` to support the new backend:

```rust
"tmux" | "tmux-standard" => {
    let backend = tmux_standard::TmuxStandardBackend::new(
        &session_name_for_workspace(path),
        path,
        cols,
        rows,
    )?;
    Ok(RuntimeCreationResult {
        runtime: Arc::new(backend),
        fallback_message: None,
    })
}
```

- [ ] **Step 10: Run full test suite**

Run: `RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -20`
Expected: All existing tests pass, new tests pass

- [ ] **Step 11: Commit**

```bash
git add src/runtime/backends/tmux_standard.rs src/runtime/backends/mod.rs
git commit -m "feat: add tmux standard mode backend (session-per-worktree, window-per-terminal)"
```

---

## Task 2b: Simplify AgentRuntime trait

**Files:**
- Modify: `src/runtime/agent_runtime.rs`

Remove tmux-CC-specific methods that are no longer needed.

- [ ] **Step 1: Remove control-mode-specific methods**

In `src/runtime/agent_runtime.rs`, remove these methods from the `AgentRuntime` trait:

```rust
// Remove these:
fn set_skip_initial_capture(&self) {}
fn switch_window(&self, _window_name: &str, _start_dir: Option<&Path>) -> Result<(), RuntimeError> { ... }
```

These were only used by the control mode backend for window switching within a session. In the new model, each window is independent.

- [ ] **Step 2: Fix any compilation errors from removed methods**

Run: `RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -30`

Remove any call sites that reference `set_skip_initial_capture` or `switch_window`. These should only be in `app_root.rs` workspace switching logic.

- [ ] **Step 3: Run tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/runtime/agent_runtime.rs
git commit -m "refactor: remove tmux-CC-specific methods from AgentRuntime trait"
```

---

## Task 3: Replace terminal view with gpui-ghostty TerminalView

**Files:**
- Modify: `src/ui/terminal_view.rs`
- Modify: `src/terminal/mod.rs`

This task replaces the custom TerminalElement rendering with gpui-ghostty's TerminalView.

- [ ] **Step 1: Update terminal/mod.rs re-exports**

Replace the contents of `src/terminal/mod.rs`:

```rust
//! Terminal module - re-exports gpui-ghostty types and content extraction.

pub mod content_extractor;

pub use content_extractor::{ContentExtractor, extract_last_line, extract_last_line_filtered};
pub use gpui_ghostty_terminal::{TerminalConfig, TerminalSession, view::TerminalInput};
pub use gpui_ghostty_terminal::view::TerminalView as GhosttyTerminalView;
```

- [ ] **Step 2: Rewrite terminal_view.rs to wrap gpui-ghostty TerminalView**

Replace `src/ui/terminal_view.rs` with a wrapper that holds a gpui-ghostty TerminalView entity:

```rust
use gpui::*;
use gpui_ghostty_terminal::view::TerminalView as GhosttyView;

/// Terminal buffer state — either a live terminal, an error, or empty placeholder.
pub enum TerminalBuffer {
    Empty,
    Error(String),
    Terminal {
        view: Entity<GhosttyView>,
    },
}

pub struct TerminalViewWrapper {
    pane_id: String,
    title: String,
    buffer: TerminalBuffer,
}

impl TerminalViewWrapper {
    pub fn new(pane_id: String, title: String) -> Self {
        Self { pane_id, title, buffer: TerminalBuffer::Empty }
    }

    pub fn with_terminal(pane_id: String, title: String, view: Entity<GhosttyView>) -> Self {
        Self { pane_id, title, buffer: TerminalBuffer::Terminal { view } }
    }

    pub fn with_error(pane_id: String, title: String, error: String) -> Self {
        Self { pane_id, title, buffer: TerminalBuffer::Error(error) }
    }

    pub fn pane_id(&self) -> &str { &self.pane_id }
    pub fn title(&self) -> &str { &self.title }

    pub fn terminal_view(&self) -> Option<&Entity<GhosttyView>> {
        match &self.buffer {
            TerminalBuffer::Terminal { view } => Some(view),
            _ => None,
        }
    }
}

impl Render for TerminalViewWrapper {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.buffer {
            TerminalBuffer::Terminal { view } => {
                div()
                    .size_full()
                    .child(view.clone())
                    .into_any_element()
            }
            TerminalBuffer::Error(msg) => {
                div()
                    .size_full()
                    .bg(rgb(0x1e1e1e))
                    .text_color(rgb(0xff6b6b))
                    .p_4()
                    .child(msg.clone())
                    .into_any_element()
            }
            TerminalBuffer::Empty => {
                div()
                    .size_full()
                    .bg(rgb(0x1e1e1e))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x666666))
                    .child("—")
                    .into_any_element()
            }
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -30`
Expected: May have errors from consumers of old TerminalView API — these are expected and fixed in Task 4

- [ ] **Step 4: Commit**

```bash
git add src/terminal/mod.rs src/ui/terminal_view.rs
git commit -m "feat: replace custom TerminalElement with gpui-ghostty TerminalView wrapper"
```

---

## Task 4: Rewire terminal_manager.rs to use gpui-ghostty

**Files:**
- Modify: `src/ui/terminal_manager.rs`

This is the largest single task. Replace `setup_local_terminal()` to create gpui-ghostty sessions and wire PTY I/O.

- [ ] **Step 1: Replace Terminal creation with TerminalSession creation**

In `terminal_manager.rs`, replace the `setup_local_terminal` method. The new pattern:

```rust
use gpui_ghostty_terminal::{TerminalConfig, TerminalSession};
use gpui_ghostty_terminal::view::{TerminalView as GhosttyView, TerminalInput};

/// Set up a terminal pane backed by the runtime's output stream.
pub fn setup_terminal_pane(
    &mut self,
    runtime: &Arc<dyn AgentRuntime>,
    pane_id: &str,
    cols: u16,
    rows: u16,
    cx: &mut Context<Self>,
) -> Result<(), RuntimeError> {
    // 1. Subscribe to output from runtime
    let output_rx = runtime.subscribe_output(&pane_id.to_string())
        .ok_or_else(|| RuntimeError::PaneNotFound(pane_id.to_string()))?;

    // 2. Create gpui-ghostty session
    let config = TerminalConfig {
        cols,
        rows,
        default_fg: ghostty_vt::Rgb { r: 0xcc, g: 0xcc, b: 0xcc },
        default_bg: ghostty_vt::Rgb { r: 0x1e, g: 0x1e, b: 0x1e },
        update_window_title: true,
    };
    let session = TerminalSession::new(config)
        .map_err(|e| RuntimeError::Backend(format!("TerminalSession::new failed: {:?}", e)))?;

    // 3. Create input callback → runtime.send_input
    let runtime_clone = runtime.clone();
    let pane_id_clone = pane_id.to_string();
    let input = TerminalInput::new(move |bytes| {
        let _ = runtime_clone.send_input(&pane_id_clone, bytes);
    });

    // 4. Create TerminalView entity
    let focus = cx.focus_handle();
    let view = cx.new(|_| GhosttyView::new_with_input(session, focus.clone(), input));

    // 5. Start output pump: runtime output_rx → view.queue_output_bytes()
    //    Also tee to ContentExtractor for agent status detection
    let view_clone = view.clone();
    let pane_id_owned = pane_id.to_string();
    cx.spawn(async move |this, mut cx| {
        let mut content_extractor = ContentExtractor::new();
        loop {
            match output_rx.recv_async().await {
                Ok(bytes) => {
                    // Feed to gpui-ghostty view
                    let _ = cx.update(|_, cx| {
                        view_clone.update(cx, |v, cx| {
                            v.queue_output_bytes(&bytes, cx);
                        });
                    });

                    // Feed to content extractor for status detection
                    content_extractor.feed(&bytes);

                    // TODO: Publish status via StatusPublisher
                }
                Err(_) => break, // Channel closed
            }
        }
    }).detach();

    // 6. Store buffer reference
    let buffer = TerminalBuffer::Terminal { view };
    self.buffers.lock().unwrap().insert(pane_id.to_string(), buffer);

    Ok(())
}
```

- [ ] **Step 2: Add resize handling**

```rust
/// Resize a terminal pane.
pub fn resize_pane(
    &self,
    pane_id: &str,
    runtime: &Arc<dyn AgentRuntime>,
    cols: u16,
    rows: u16,
    cx: &mut Context<Self>,
) {
    // Resize in runtime (tmux resize-window)
    let _ = runtime.resize(&pane_id.to_string(), cols, rows);

    // Resize in gpui-ghostty view
    if let Some(TerminalBuffer::Terminal { view }) = self.buffers.lock().unwrap().get(pane_id) {
        view.update(cx, |v, cx| {
            v.resize_terminal(cols, rows, cx);
        });
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -50`
Expected: Compilation errors from other files that still reference old types — addressed in Task 5

- [ ] **Step 4: Commit**

```bash
git add src/ui/terminal_manager.rs
git commit -m "feat: rewire terminal_manager to create gpui-ghostty sessions with PTY I/O"
```

---

## Task 5: Adapt terminal_area_entity.rs and app_root.rs

**Files:**
- Modify: `src/ui/terminal_area_entity.rs`
- Modify: `src/ui/app_root.rs`

- [ ] **Step 1: Update TerminalAreaEntity to use new TerminalBuffer**

Update `src/ui/terminal_area_entity.rs` to work with the new `TerminalBuffer` type from `terminal_view.rs`. The key change: instead of passing `Arc<Terminal>` + callbacks, pass the gpui-ghostty `Entity<GhosttyView>`.

Adapt the `Render` impl to render `TerminalViewWrapper` entities inside `SplitPaneContainer`.

- [ ] **Step 2: Remove coalesce_and_process_output from app_root.rs**

In `src/ui/app_root.rs`:
- Remove the `coalesce_and_process_output()` function (output pipeline now lives in terminal_manager)
- Remove `detect_agent_in_pane()` and `is_pane_shell()` if they're only used by the old pipeline (or move them to a shared location)
- Update workspace switching to use the new terminal setup flow

- [ ] **Step 3: Verify compilation**

Run: `RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -50`
Expected: Fewer errors; may still have some from other UI files referencing old types

- [ ] **Step 4: Fix remaining compilation errors**

Address all remaining references to old `Terminal`, `TerminalElement`, `key_to_bytes`, etc. across the codebase. Use compiler errors as the guide.

- [ ] **Step 5: Run tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -20`
Expected: Tests compile and pass (some old terminal tests may need updating/removal)

- [ ] **Step 6: Commit**

```bash
git add src/ui/terminal_area_entity.rs src/ui/app_root.rs
git commit -m "feat: adapt UI layer to gpui-ghostty TerminalView entities"
```

---

## Task 6: Session recovery

**Files:**
- Modify: `src/runtime/backends/tmux_standard.rs`
- Modify: `src/runtime/backends/mod.rs`

- [ ] **Step 1: Write test for session discovery**

```rust
#[test]
fn test_discover_existing_sessions() {
    let session = "pmux-test-recovery";
    let _ = Command::new("tmux").args(["kill-session", "-t", session]).output();

    // Create a session with 2 windows
    Command::new("tmux")
        .args(["new-session", "-d", "-s", session, "-x", "120", "-y", "36"])
        .output().unwrap();
    Command::new("tmux")
        .args(["new-window", "-t", session])
        .output().unwrap();

    let windows = TmuxStandardBackend::discover_windows(session).unwrap();
    assert_eq!(windows.len(), 2);

    let _ = Command::new("tmux").args(["kill-session", "-t", session]).output();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `RUSTUP_TOOLCHAIN=stable cargo test test_discover_existing_sessions --lib 2>&1 | tail -5`
Expected: FAIL

- [ ] **Step 3: Implement discover_sessions() and discover_windows()**

```rust
impl TmuxStandardBackend {
    /// Discover all pmux tmux sessions.
    pub fn discover_sessions() -> Vec<String> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()
            .unwrap_or_default();

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("pmux-"))
            .map(|s| s.to_string())
            .collect()
    }

    /// Discover windows in an existing session.
    pub fn discover_windows(session: &str) -> Result<Vec<(u32, String, String)>, RuntimeError> {
        let output = Command::new("tmux")
            .args([
                "list-windows", "-t", session,
                "-F", "#{window_index}:#{window_name}:#{pane_current_path}",
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

    /// Recover a backend from an existing tmux session.
    /// Reattaches output readers to all existing windows.
    pub fn recover(
        session_name: &str,
        worktree_path: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Self, RuntimeError> {
        let backend = Self {
            session_name: session_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            windows: Mutex::new(HashMap::new()),
            next_window: Mutex::new(0),
            default_cols: cols,
            default_rows: rows,
        };

        // Discover and reattach to existing windows
        let windows = Self::discover_windows(session_name)?;
        for (idx, name, _path) in &windows {
            let pane_id = Self::window_target(session_name, *idx);
            let (output_tx, output_rx) = flume::unbounded();
            let reader_handle = backend.start_output_reader(&pane_id, output_tx.clone())?;

            let state = Arc::new(WindowState {
                window_index: *idx,
                output_tx,
                output_rx,
                cols: AtomicU16::new(cols),
                rows: AtomicU16::new(rows),
                _reader_handle: reader_handle,
            });

            backend.windows.lock().unwrap().insert(pane_id, state);
        }

        let max_idx = windows.iter().map(|(i, _, _)| *i).max().unwrap_or(0);
        *backend.next_window.lock().unwrap() = max_idx + 1;

        Ok(backend)
    }
}
```

- [ ] **Step 4: Run test**

Run: `RUSTUP_TOOLCHAIN=stable cargo test test_discover_existing_sessions --lib 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Wire recovery into mod.rs recover_runtime()**

Update `recover_runtime()` in `src/runtime/backends/mod.rs` to use `TmuxStandardBackend::recover()`.

- [ ] **Step 6: Commit**

```bash
git add src/runtime/backends/tmux_standard.rs src/runtime/backends/mod.rs
git commit -m "feat: implement tmux session discovery and recovery for standard backend"
```

---

## Task 7: Cleanup — delete old terminal engine files

**Files:**
- Delete: `src/terminal/terminal_element.rs`
- Delete: `src/terminal/terminal_rendering.rs`
- Delete: `src/terminal/terminal_core.rs`
- Delete: `src/terminal/input.rs`
- Delete: `src/terminal/terminal_input_handler.rs`
- Delete: `src/terminal/colors.rs`
- Delete: `src/terminal/box_drawing.rs`
- Delete: `src/runtime/backends/tmux_control_mode.rs`
- Delete: `src/runtime/backends/tmux.rs`
- Modify: `Cargo.toml` (remove `alacritty_terminal`)

- [ ] **Step 1: Delete old terminal files**

```bash
rm src/terminal/terminal_element.rs
rm src/terminal/terminal_rendering.rs
rm src/terminal/terminal_core.rs
rm src/terminal/input.rs
rm src/terminal/terminal_input_handler.rs
rm src/terminal/colors.rs
rm src/terminal/box_drawing.rs
```

- [ ] **Step 2: Delete old backend files**

```bash
rm src/runtime/backends/tmux_control_mode.rs
rm src/runtime/backends/tmux.rs
```

- [ ] **Step 3: Remove alacritty_terminal from Cargo.toml**

Remove the line:
```toml
alacritty_terminal = "0.25"
```

- [ ] **Step 4: Remove old module declarations**

In `src/terminal/mod.rs`, ensure only the new modules are declared (content_extractor + re-exports).

In `src/runtime/backends/mod.rs`, remove `pub mod tmux_control_mode;` and `pub mod tmux;`.

- [ ] **Step 5: Fix any remaining compilation errors**

Run: `RUSTUP_TOOLCHAIN=stable cargo check 2>&1`
Expected: Clean compilation with no references to deleted files

- [ ] **Step 6: Run full test suite**

Run: `RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "chore: remove alacritty_terminal, custom renderer, and tmux control mode backend"
```

---

## Task 8: End-to-end validation

**Files:** None (testing only)

- [ ] **Step 1: Build and run**

Run: `RUSTUP_TOOLCHAIN=stable cargo run`
Expected: App launches, terminal renders

- [ ] **Step 2: Verify basic shell interaction**

- Type commands in terminal, verify echo and output rendering
- Verify prompt renders correctly
- Verify colors (ls --color) work

- [ ] **Step 3: Verify TUI apps**

- Run `vim` or `nano` — verify alt-screen works
- Run `htop` — verify mouse mode works
- Run `less` — verify scrollback works

- [ ] **Step 4: Verify agent status detection**

- Start a Claude Code session
- Verify sidebar status updates (Running/Waiting/Idle)
- Verify notifications trigger

- [ ] **Step 5: Verify split panes**

- Create split pane
- Verify independent resize
- Verify independent input routing

- [ ] **Step 6: Verify session recovery**

- Kill pmux (Cmd+Q)
- Relaunch
- Verify terminals reconnect to existing tmux sessions

- [ ] **Step 7: Verify tmux session naming**

```bash
tmux list-sessions
```
Expected: Sessions named `pmux-<repo>-<branch>`

```bash
tmux list-windows -t "pmux-<session>"
```
Expected: One window per terminal pane

- [ ] **Step 8: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: end-to-end validation fixes for gpui-ghostty migration"
```
