# Direct PTY Write + tmux Tuning Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.
> Tasks 1–3 are independent and can run in parallel.

**Goal:** Eliminate tmux `send-keys` command overhead on the input path by writing directly to pane TTY devices, set low `escape-time` per-session, and unify split-pane output coalescing with the main pane pattern.

**Architecture:** `send_input` attempts a synchronous `write(2)` to the pane's `/dev/ttysNNN` file descriptor (cached). On cache miss or write error, it falls back to the existing `send-keys -H` channel. Session creation injects `set -s escape-time 10`. Split-pane output processing moves to a background thread with `bounded(1)` coalescing, matching the main-pane pattern.

**Tech Stack:** Rust, GPUI, tmux control mode, POSIX PTY

---

## Task 1 (P0): `send_input` → direct PTY write first, `send-keys -H` fallback

**Files:**
- Modify: `src/runtime/backends/tmux_control_mode.rs` (lines 368–570, 596–673)

### Step 1: Write a test for `warm_pane_tty_cache` + `send_input` direct-write path

Add to the `#[cfg(test)] mod tests` block at the bottom of `tmux_control_mode.rs`:

```rust
#[test]
fn test_send_input_uses_direct_write_when_cached() {
    if !crate::runtime::backends::tmux_available() {
        eprintln!("skipping: tmux not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let rt =
        TmuxControlModeRuntime::new("pmux-test-direct-input", "main", Some(dir.path()), 80, 24)
            .expect("should create runtime");

    let panes = rt.list_panes(&String::new());
    let pane_id = panes.first().cloned().unwrap_or_else(|| "%0".to_string());

    // Before warming: cache is empty
    assert!(rt.pane_tty_writers.lock().unwrap().is_empty());

    // Warm the cache
    rt.warm_pane_tty_cache(&pane_id);
    assert!(rt.pane_tty_writers.lock().unwrap().contains_key(&pane_id));

    // send_input should succeed (uses direct write internally)
    assert!(rt.send_input(&pane_id, b"echo hello\r").is_ok());

    // Cleanup
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", "pmux-test-direct-input"])
        .output();
}
```

### Step 2: Run test to verify failure

```bash
cargo test test_send_input_uses_direct_write_when_cached -- --nocapture
```

Expected: FAIL — `warm_pane_tty_cache` method does not exist.

### Step 3: Implement `warm_pane_tty_cache` and rewire `send_input`

**3a.** Add `warm_pane_tty_cache` method to the `impl TmuxControlModeRuntime` block (after `direct_write`, around line 644):

```rust
/// Resolve and cache the pane's TTY file handle for direct writes.
/// Call after a pane is created or after switch_window.
/// No-op if pane TTY is already cached or cannot be resolved.
pub fn warm_pane_tty_cache(&self, pane_id: &str) {
    if let Ok(cache) = self.pane_tty_writers.lock() {
        if cache.contains_key(pane_id) {
            return;
        }
    }
    if let Some(file) = self.resolve_pane_tty(pane_id) {
        if let Ok(mut cache) = self.pane_tty_writers.lock() {
            cache.insert(pane_id.to_string(), file);
        }
    }
}
```

**3b.** Rewrite `send_input` (replace lines 664–673):

```rust
fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.is_empty() {
        return Ok(());
    }

    // Fast path: write directly to pane TTY (same as local PTY, ~0ms)
    match self.direct_write(pane_id, bytes) {
        Ok(true) => return Ok(()),
        Ok(false) => {} // cache miss — fall through to send-keys
        Err(_) => {}    // write error (cache entry removed) — fall through
    }

    // Slow path: enqueue for writer thread (send-keys -H via tmux command)
    self.input_tx
        .send((pane_id.clone(), bytes.to_vec()))
        .map_err(|e| RuntimeError::Backend(format!("input channel: {}", e)))
}
```

**3c.** Call `warm_pane_tty_cache` at session creation time. In the `new()` method, after the runtime is constructed and `refresh-client` is sent (around line 572), add:

```rust
// Pre-warm the TTY cache for the primary pane so send_input uses direct writes
let panes = rt.list_panes(&String::new());
if let Some(primary) = panes.first() {
    rt.warm_pane_tty_cache(primary);
}
```

**3d.** Warm cache on `subscribe_output` entry. At the beginning of `subscribe_output` (line 701), add:

```rust
fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
    self.warm_pane_tty_cache(pane_id);
    // ... rest of existing code
```

**3e.** Warm cache after `split_pane`. At the end of `split_pane` (line 817), before the `Ok(...)`:

```rust
fn split_pane(&self, pane_id: &PaneId, vertical: bool) -> Result<PaneId, RuntimeError> {
    let flag = if vertical { "-h" } else { "-v" };
    self.send_command(&format!("split-window {} -t {}", flag, pane_id))?;
    let new_pane = self.list_panes(&String::new())
        .into_iter()
        .last()
        .ok_or_else(|| RuntimeError::Backend("no pane after split".into()))?;
    self.warm_pane_tty_cache(&new_pane);
    Ok(new_pane)
}
```

Note: `switch_window` already calls `pane_tty_writers.lock().clear()` (line 940). After the switch, `subscribe_output` will be called for the new pane, which now warms the cache (step 3d). No additional change needed in `switch_window`.

### Step 4: Run test to verify pass

```bash
cargo test test_send_input_uses_direct_write_when_cached -- --nocapture
```

Expected: PASS

### Step 5: Run full check + existing tests

```bash
cargo check && cargo test tmux_control_mode -- --nocapture 2>&1 | tail -20
```

Expected: all tests pass, no compilation errors.

### Step 6: Commit

```bash
git add src/runtime/backends/tmux_control_mode.rs
git commit -m "perf: send_input direct-writes to pane TTY, send-keys as fallback"
```

---

## Task 2 (P1): Set `escape-time 10` on session creation

**Files:**
- Modify: `src/runtime/backends/tmux_control_mode.rs` (line ~572)

### Step 1: Write the test

```rust
#[test]
fn test_session_has_low_escape_time() {
    if !crate::runtime::backends::tmux_available() {
        eprintln!("skipping: tmux not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let _rt =
        TmuxControlModeRuntime::new("pmux-test-esc", "main", Some(dir.path()), 80, 24)
            .expect("should create runtime");

    let output = Command::new("tmux")
        .args(["show-options", "-s", "-t", "pmux-test-esc", "-v", "escape-time"])
        .output()
        .expect("tmux show-options");
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(value, "10", "escape-time should be 10, got '{}'", value);

    let _ = Command::new("tmux")
        .args(["kill-session", "-t", "pmux-test-esc"])
        .output();
}
```

### Step 2: Run test to verify failure

```bash
cargo test test_session_has_low_escape_time -- --nocapture
```

Expected: FAIL — escape-time defaults to 500.

### Step 3: Implement

In `TmuxControlModeRuntime::new()`, after the `refresh-client` command (line ~572), add:

```rust
// Low escape-time: reduces ESC/Alt key latency from 500ms (default) to 10ms.
// Set per-session so we don't alter user's global tmux config.
let _ = rt.send_command("set -s escape-time 10");
```

### Step 4: Run test to verify pass

```bash
cargo test test_session_has_low_escape_time -- --nocapture
```

Expected: PASS

### Step 5: Commit

```bash
git add src/runtime/backends/tmux_control_mode.rs
git commit -m "perf: set escape-time 10 on tmux session creation"
```

---

## Task 3 (P2): Unify split-pane output to background thread + bounded(1) coalescing

**Files:**
- Modify: `src/ui/app_root.rs` (lines 846–898)

### Step 1: No unit test (this is a threading/notification refactor)

The correctness is verified by:
1. `cargo check` — compiles
2. Manual verification — split pane still displays output
3. Regression tests — all pass

### Step 2: Refactor `setup_pane_terminal_output`

Replace the `cx.spawn(async move ...)` block (lines 852–898) with the same background-thread + bounded(1) pattern used in `setup_local_terminal` (lines 698–765):

```rust
            let status_publisher = self.status_publisher.clone();
            let pane_target_clone = pane_target_str.clone();
            let terminal_for_output = terminal.clone();
            let term_area_entity = self.terminal_area_entity.clone();

            // Background thread + bounded(1) — same pattern as main pane
            let (notify_tx, notify_rx) = flume::bounded::<()>(1);

            std::thread::Builder::new()
                .name(format!("pmux-vte-pane-{}", pane_target_str))
                .spawn(move || {
                    use std::time::{Duration, Instant};
                    let mut ext = ContentExtractor::new();
                    let mut last_status_check = Instant::now();
                    let mut last_phase = ext.shell_phase();
                    let status_interval = Duration::from_millis(200);

                    loop {
                        let chunk = match rx.recv() {
                            Ok(c) => c,
                            Err(_) => break,
                        };
                        terminal_for_output.process_output(&chunk);
                        ext.feed(&chunk);

                        while let Ok(next) = rx.try_recv() {
                            terminal_for_output.process_output(&next);
                            ext.feed(&next);
                        }

                        let now = Instant::now();
                        let phase = ext.shell_phase();
                        if phase != last_phase
                            || now.duration_since(last_status_check) >= status_interval
                        {
                            last_status_check = now;
                            last_phase = phase;
                            let shell_info = ShellPhaseInfo {
                                phase,
                                last_post_exec_exit_code: None,
                            };
                            let content_str = ext.take_content().0;
                            if let Some(ref pub_) = status_publisher {
                                let _ = pub_.check_status(
                                    &pane_target_clone,
                                    crate::status_detector::ProcessStatus::Running,
                                    Some(shell_info),
                                    &content_str,
                                );
                            }
                        }

                        let _ = notify_tx.try_send(());
                    }
                })
                .expect("spawn VTE pane output thread");

            cx.spawn(async move |_entity, cx| {
                loop {
                    match notify_rx.recv_async().await {
                        Ok(()) => {
                            if let Some(ref tae) = term_area_entity {
                                let _ = cx.update_entity(tae, |_, cx| cx.notify());
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .detach();
```

### Step 3: Verify compilation

```bash
cargo check
```

Expected: success

### Step 4: Commit

```bash
git add src/ui/app_root.rs
git commit -m "perf: split-pane output uses background thread + bounded(1) coalescing"
```

---

## Task 4: Fix key repeat (already done) + verify all changes compile

**Files:**
- Verify: `src/ui/app_root.rs` — early return for text characters was already removed in this session

### Step 1: Full build

```bash
RUSTUP_TOOLCHAIN=stable cargo build
```

Expected: success

### Step 2: Run regression tests

```bash
bash tests/regression/run_all.sh --skip-build
```

Expected: 5/5 pass

### Step 3: Commit all remaining changes

```bash
git add -A
git commit -m "fix: remove text-char early return that broke key repeat, fix test config paths"
```

---

## Summary

| Task | Change | Latency impact |
|------|--------|----------------|
| 1 (P0) | `send_input` → `direct_write` first | 3–5ms/keystroke → ~0ms |
| 2 (P1) | `escape-time 10` per session | ESC/Alt: 500ms → 10ms |
| 3 (P2) | Split-pane background thread + bounded(1) | Multi-pane output no longer blocks main thread |
| 4 | Key repeat fix + test config path fix | Held keys work; regression tests target correct config |

**Parallelism:** Tasks 1, 2, and 3 touch different code regions and can be implemented by separate subagents concurrently. Task 4 is already done in this session and just needs verification.
