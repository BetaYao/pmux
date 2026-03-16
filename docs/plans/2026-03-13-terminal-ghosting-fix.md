# Terminal Ghosting (残影) Fix Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.

**Goal:** Eliminate duplicate-line ghosting during fast terminal output (e.g. Claude Code streaming) by coalescing chunks before VTE processing, throttling render notifications, and reducing the resync interval.

**Architecture:** The output loop in `app_root.rs` currently calls `process_output()` per chunk within a 4ms window, creating intermediate grid states visible to GPUI. The fix: (1) concatenate all chunks in a 16ms window into one buffer, process once; (2) gate `cx.notify()` with an `output_active` flag; (3) reduce idle resync from 2s to 300ms; (4) detect CSI 2026 for local PTY mode.

**Tech Stack:** Rust, GPUI, alacritty_terminal

---

## Task 1: Add `output_active` and `synchronized_output` Fields to Terminal

**Files:**
- Modify: `src/terminal/terminal_core.rs:117–144` (Terminal struct)
- Modify: `src/terminal/terminal_core.rs` (new() constructor, new methods)

**Step 1: Add new fields to Terminal struct**

In `src/terminal/terminal_core.rs`, add three fields to the `Terminal` struct after `ime_marked_text`:

```rust
    /// True while the output loop is feeding data to the VTE parser.
    /// Used to gate cx.notify() — renders only fire after processing completes.
    output_active: AtomicBool,
    /// True when a CSI ?2026h (synchronized output) block is active.
    /// Only relevant for local PTY mode (tmux consumes these sequences).
    synchronized_output: AtomicBool,
```

**Step 2: Initialize fields in Terminal::new()**

Find the `Terminal::new()` constructor and add to the struct initializer:

```rust
    output_active: AtomicBool::new(false),
    synchronized_output: AtomicBool::new(false),
```

**Step 3: Add accessor methods**

Add after the existing `mode()` method (line 280):

```rust
    /// Mark output processing as active/inactive. Used by the output loop
    /// to signal that cx.notify() should only fire after processing completes.
    pub fn set_output_active(&self, active: bool) {
        self.output_active.store(active, Ordering::Relaxed);
    }

    pub fn is_output_active(&self) -> bool {
        self.output_active.load(Ordering::Relaxed)
    }

    /// Check if terminal is in alternate screen mode (TUI programs).
    pub fn is_alt_screen(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Synchronized output state (CSI ?2026h/l).
    pub fn set_synchronized_output(&self, sync: bool) {
        self.synchronized_output.store(sync, Ordering::Relaxed);
    }

    pub fn is_synchronized_output(&self) -> bool {
        self.synchronized_output.load(Ordering::Relaxed)
    }
```

**Step 4: Build to verify**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```
Expected: compiles with no errors.

**Step 5: Commit**

```bash
git add src/terminal/terminal_core.rs
git commit -m "feat: add output_active and synchronized_output fields to Terminal"
```

---

## Task 2: Extract Shared Output Loop Helper

**Files:**
- Modify: `src/ui/app_root.rs`

The two output loops (local ~L1297, tmux ~L1638) are nearly identical. Extract the shared coalescing logic into a helper to avoid duplicating the fix.

**Step 1: Create the helper function**

Add this function in `app_root.rs` (above `setup_local_terminal` or in a suitable location):

```rust
/// Coalesce terminal output chunks into a single buffer and process once.
///
/// Waits up to `coalesce_ms` after the first chunk for more data, then drains
/// all immediately available chunks. Processes everything in a single
/// `process_output()` call to avoid intermediate grid states (ghosting).
///
/// Returns `true` if output was received, `false` on idle timeout.
async fn coalesce_and_process_output(
    rx: &flume::Receiver<Vec<u8>>,
    terminal: &Terminal,
    ext: &mut ContentExtractor,
    idle_timeout: Duration,
    cx: &AsyncAppContext,
) -> Result<bool, flume::RecvError> {
    let mut coalesce_buf: Vec<u8> = Vec::with_capacity(8192);

    // Step 1: Wait for first chunk or idle timeout
    {
        let timer = cx.background_executor().timer(idle_timeout);
        let recv = rx.recv_async();
        pin_mut!(timer);
        pin_mut!(recv);
        match select(recv, timer).await {
            Either::Left((Ok(chunk), _)) => {
                coalesce_buf.extend_from_slice(&chunk);
            }
            Either::Left((Err(e), _)) => return Err(e),
            Either::Right((_, _)) => return Ok(false), // idle
        }
    }

    // Step 2: Coalesce — adaptive window based on alt-screen mode
    let coalesce_ms = if terminal.is_alt_screen() { 16 } else { 4 };
    {
        let timer = cx.background_executor().timer(Duration::from_millis(coalesce_ms));
        let recv = rx.recv_async();
        pin_mut!(timer);
        pin_mut!(recv);
        loop {
            match select(recv, timer).await {
                Either::Left((Ok(next), _)) => {
                    coalesce_buf.extend_from_slice(&next);
                    // Re-pin recv for next iteration
                    let recv = rx.recv_async();
                    pin_mut!(recv);
                    continue;
                }
                _ => break,
            }
        }
    }

    // Step 3: Drain all immediately available chunks
    while let Ok(next) = rx.try_recv() {
        coalesce_buf.extend_from_slice(&next);
    }

    // Step 4: Single-shot processing with output_active gating
    terminal.set_output_active(true);
    terminal.process_output(&coalesce_buf);
    ext.feed(&coalesce_buf);
    terminal.set_output_active(false);

    Ok(true)
}
```

**Note on the select loop:** The inner `select` loop needs careful handling of `pin_mut!` across iterations. If the borrow checker complains about re-pinning `recv`, restructure as a single `select` (not a loop) followed by `try_recv` drain — this is simpler and achieves the same effect:

```rust
    // Step 2: Coalesce — one additional wait then drain
    let coalesce_ms = if terminal.is_alt_screen() { 16 } else { 4 };
    {
        let timer = cx.background_executor().timer(Duration::from_millis(coalesce_ms));
        let recv = rx.recv_async();
        pin_mut!(timer);
        pin_mut!(recv);
        match select(recv, timer).await {
            Either::Left((Ok(next), _)) => {
                coalesce_buf.extend_from_slice(&next);
            }
            _ => {} // timeout or error
        }
    }

    // Step 3: Drain all immediately available (covers what the loop would have caught)
    while let Ok(next) = rx.try_recv() {
        coalesce_buf.extend_from_slice(&next);
    }
```

**Step 2: Build to verify**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```
Expected: compiles (function is defined but not yet called).

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor: extract coalesce_and_process_output helper"
```

---

## Task 3: Replace Local Terminal Output Loop

**Files:**
- Modify: `src/ui/app_root.rs:1297–1368` (local terminal output loop)

**Step 1: Replace the output coalescing section**

Replace lines 1297–1368 (the `loop { ... }` body) with the new pattern. The key changes are:
- Replace the inline 4ms coalescing + per-chunk `process_output()` with `coalesce_and_process_output()`
- Add `cx.notify()` gating: only notify after `output_active` is cleared
- Keep the existing idle-timeout resync and status detection logic

```rust
                loop {
                    let idle_timeout = Duration::from_secs(2);

                    // Coalesce chunks and process in one shot
                    let got_output = match coalesce_and_process_output(
                        &rx,
                        &terminal_for_output,
                        &mut ext,
                        idle_timeout,
                        &cx,
                    ).await {
                        Ok(got) => got,
                        Err(_) => break, // channel closed
                    };

                    if !got_output {
                        // Idle timeout — resync from tmux and check agent status
                        if let Some(resync) = crate::runtime::backends::tmux_control_mode::capture_pane_resync(&pane_target_clone) {
                            terminal_for_output.process_output(&resync);
                        }
                        if let Some(ref agent_def) = agent_override {
                            let screen_text = terminal_for_output.screen_tail_text(
                                terminal_for_output.size().rows as usize,
                            );
                            let detected = agent_def.detect_status(&screen_text);
                            if let Some(ref pub_) = status_publisher {
                                let _ = pub_.force_status(
                                    &status_key_clone,
                                    detected,
                                    &screen_text,
                                    &agent_def.message_skip_patterns,
                                );
                            }
                        }
                        continue;
                    }

                    // Status detection (unchanged from existing code)
                    let now = Instant::now();
                    let phase = ext.shell_phase();
                    let alt_screen = terminal_for_output.mode()
                        .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
                    if phase != last_phase || alt_screen != last_alt_screen
                        || now.duration_since(last_status_check) >= status_interval
                    {
                        last_status_check = now;
                        // ... existing status detection code (keep as-is) ...
                    }

                    // Notify GPUI for render — output_active is already false
                    if let Some(ref tae) = term_area_entity {
                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
                    }
                }
```

**Step 2: Build and test**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 3: Manual verification**

```bash
RUSTUP_TOOLCHAIN=stable cargo run
```
- Open a workspace with Claude Code running
- Trigger fast streaming output
- Verify: no duplicate lines during output
- Verify: resize still works
- Verify: typing echo is responsive (should be ~4ms latency in normal mode)

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: replace local terminal output loop with coalesced single-shot processing"
```

---

## Task 4: Replace Tmux Terminal Output Loop

**Files:**
- Modify: `src/ui/app_root.rs:1638–1729` (tmux terminal output loop)

**Step 1: Apply the same pattern as Task 3**

Replace the tmux output loop with the identical `coalesce_and_process_output()` call. The structure is the same — only the surrounding context (tmux-specific agent detection, status publishing) may differ slightly.

Follow the same pattern as Task 3: replace the inline coalescing with the helper call, keep idle resync and status detection, add `cx.notify()` after processing.

**Step 2: Build and test**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: replace tmux terminal output loop with coalesced single-shot processing"
```

---

## Task 5: Reduce Idle Resync Timeout to 300ms

**Files:**
- Modify: `src/ui/app_root.rs` (both output loops, or the helper)

**Step 1: Add a short resync after output bursts**

The existing 2s idle timeout already triggers `capture_pane_resync()`. Add a second, shorter timeout (300ms) specifically after output processing to catch post-burst ghosting faster.

In `coalesce_and_process_output()` or in the calling loop, after processing output and before the next iteration:

```rust
    if got_output {
        // Quick resync check: wait 300ms for more output, resync if idle
        let resync_timeout = Duration::from_millis(300);
        let timer = cx.background_executor().timer(resync_timeout);
        let recv = rx.recv_async();
        pin_mut!(timer);
        pin_mut!(recv);
        match select(recv, timer).await {
            Either::Left((Ok(chunk), _)) => {
                // More output coming — feed it and continue the loop
                terminal_for_output.process_output(&chunk);
                ext.feed(&chunk);
                // Will be coalesced in next iteration
            }
            Either::Right((_, _)) => {
                // 300ms idle after output burst — resync
                if let Some(resync) = crate::runtime::backends::tmux_control_mode::capture_pane_resync(&pane_target_clone) {
                    terminal_for_output.process_output(&resync);
                    // Notify to render the corrected frame
                    if let Some(ref tae) = term_area_entity {
                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
                    }
                }
            }
            _ => {} // channel closed, will break on next iteration
        }
    }
```

**Step 2: Add rate limiting**

Track last resync time to avoid excessive capture-pane calls:

```rust
    let mut last_resync = Instant::now() - Duration::from_secs(2); // allow immediate first resync

    // In the resync block:
    if now.duration_since(last_resync) >= Duration::from_secs(1) {
        if let Some(resync) = capture_pane_resync(&pane_target_clone) {
            terminal_for_output.process_output(&resync);
            last_resync = Instant::now();
        }
    }
```

**Step 3: Build and test**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

Manual test: fast output → stop → verify grid matches within ~500ms.

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: add 300ms post-burst resync for faster ghosting recovery"
```

---

## Task 6: CSI 2026 Detection for Local PTY Mode

**Files:**
- Modify: `src/terminal/terminal_core.rs:188–228` (process_output method)

**Step 1: Add CSI 2026 scanning in process_output()**

Add detection before the VTE `processor.advance()` call. Use a simple byte window scan since at this point the data is already a complete coalesced buffer (not split across chunks):

```rust
    pub fn process_output(&self, data: &[u8]) {
        // Detect CSI ?2026h/l (synchronized output) in the coalesced buffer.
        // Safe to scan raw bytes here because the coalescing step ensures
        // we see complete escape sequences (not split across calls).
        let sync_start = b"\x1b[?2026h";
        let sync_end = b"\x1b[?2026l";

        let mut found_start = false;
        let mut found_end = false;
        for window in data.windows(sync_start.len()) {
            if window == sync_start {
                found_start = true;
            }
            if window == sync_end {
                found_end = true;
            }
        }

        if found_start && !found_end {
            self.synchronized_output.store(true, Ordering::Relaxed);
        }
        if found_end {
            self.synchronized_output.store(false, Ordering::Relaxed);
        }

        // ... existing UTF-8 handling and VTE processing ...
```

**Step 2: Gate cx.notify() on synchronized_output**

In the output loop (both local and tmux), after `coalesce_and_process_output()`:

```rust
    // Only notify GPUI if not in a synchronized output block
    if !terminal_for_output.is_synchronized_output() {
        if let Some(ref tae) = term_area_entity {
            let _ = cx.update_entity(tae, |_, cx| cx.notify());
        }
    }
```

**Step 3: Build and test**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

Note: This is effectively a no-op in tmux mode (sequences don't arrive), but prepares for local PTY mode.

**Step 4: Commit**

```bash
git add src/terminal/terminal_core.rs src/ui/app_root.rs
git commit -m "feat: detect CSI 2026 synchronized output for local PTY mode"
```

---

## Task 7: Integration Testing

**Files:**
- No new files — manual testing

**Step 1: Run full test suite**

```bash
RUSTUP_TOOLCHAIN=stable cargo test
```
Expected: all existing tests pass.

**Step 2: Manual testing matrix**

| Scenario | Expected | Check |
|----------|----------|-------|
| Claude Code streaming output | No duplicate lines | |
| Fast `cat large_file.txt` | No ghosting | |
| Resize during output | Display corrects immediately | |
| Interactive typing (normal mode) | <4ms echo latency | |
| TUI program (htop, vim) | No ghosting, 16ms coalescing | |
| Output stops → 300ms → grid correct | Grid matches capture-pane | |
| Workspace switch during output | Clean terminal on return | |

**Step 3: Final commit**

```bash
git add -A
git commit -m "docs: add terminal ghosting fix plan and design spec"
```
