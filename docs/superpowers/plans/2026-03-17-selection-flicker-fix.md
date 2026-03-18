# Selection Flicker Fix Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate selection flicker in the embedded terminal during active agent output by pausing resync and suppressing repaints while the user is dragging to select text.

**Architecture:** Add a `selecting` flag (+ timestamp) to `Terminal`. Mouse handlers set/clear it. The resync thread and render/idle ticks check it and skip their work. A 5-second timeout auto-clears the flag as a safety net.

**Tech Stack:** Rust, std::sync::atomic (AtomicBool, AtomicU64)

**Spec:** `docs/superpowers/specs/2026-03-17-selection-flicker-fix-design.md`

---

### Task 1: Add selecting fields to Terminal struct

**Files:**
- Modify: `src/terminal/terminal_core.rs:158-224` (struct fields + `new()`)

- [ ] **Step 1: Write failing tests for the selecting flag**

In `src/terminal/terminal_core.rs`, add to the `#[cfg(test)]` module (after the existing resync tests, ~line 1165):

```rust
#[test]
fn test_selecting_flag_default_false() {
    let term = Terminal::new("sel-test".into(), TerminalSize::default());
    assert!(!term.selecting().load(Ordering::Relaxed));
    assert_eq!(term.selecting_since().load(Ordering::Relaxed), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_selecting_flag_default_false`
Expected: FAIL — `selecting()` method does not exist

- [ ] **Step 3: Add fields and accessors**

In `src/terminal/terminal_core.rs`:

Add fields to the `Terminal` struct (after `last_output_time_ms` at line 179):

```rust
    /// Set to true while the user is dragging a mouse selection.
    /// The resync thread and render/idle ticks skip their work when true.
    selecting: Arc<AtomicBool>,
    /// Timestamp (millis since epoch) when selecting started. 0 when not selecting.
    /// Used by the resync thread to auto-clear selecting after 5 seconds.
    selecting_since: Arc<AtomicU64>,
```

Initialize in `new()` (after `last_output_time_ms` init at line 223):

```rust
            selecting: Arc::new(AtomicBool::new(false)),
            selecting_since: Arc::new(AtomicU64::new(0)),
```

Add accessors (after the `is_tmux_backed()` method at line ~326):

```rust
    /// Returns the selecting flag for mouse selection state.
    pub fn selecting(&self) -> &Arc<AtomicBool> {
        &self.selecting
    }

    /// Returns the timestamp of when selecting started (millis since epoch).
    pub fn selecting_since(&self) -> &Arc<AtomicU64> {
        &self.selecting_since
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_selecting_flag_default_false`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/terminal/terminal_core.rs
git commit -m "feat: add selecting flag and timestamp to Terminal struct"
```

---

### Task 2: Gate resync thread on selecting flag with 5s timeout

**Files:**
- Modify: `src/terminal/terminal_core.rs:250-317` (resync thread in `new_tmux()`)

- [ ] **Step 1: Write failing test for selecting gate**

In `src/terminal/terminal_core.rs` test module:

```rust
#[test]
fn test_selecting_flag_skips_resync_in_spirit() {
    // Verify the selecting flag is cloned into new_tmux terminals
    // (The actual resync thread gating is an integration behavior —
    // we verify the flag is accessible and wired correctly)
    let term = Terminal::new_tmux("sel-resync".into(), TerminalSize::default());
    assert!(!term.selecting().load(Ordering::Relaxed));

    // Simulate selection start
    term.selecting().store(true, Ordering::Relaxed);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    term.selecting_since().store(now_ms, Ordering::Relaxed);

    assert!(term.selecting().load(Ordering::Relaxed));
    assert!(term.selecting_since().load(Ordering::Relaxed) > 0);

    // Simulate selection end
    term.selecting().store(false, Ordering::Relaxed);
    term.selecting_since().store(0, Ordering::Relaxed);
    assert!(!term.selecting().load(Ordering::Relaxed));
}
```

- [ ] **Step 2: Run test to verify it passes** (fields already added in Task 1)

Run: `cargo test test_selecting_flag_skips_resync_in_spirit`
Expected: PASS (the struct fields are already there from Task 1)

- [ ] **Step 3: Gate the resync thread**

In `src/terminal/terminal_core.rs`, inside the resync thread's `!was_dirty` branch (line ~275), add a selecting check **before** the elapsed-time check. The current code at line 275 is:

```rust
                        if !was_dirty {
                            let now_ms = std::time::SystemTime::now()
```

Replace with:

```rust
                        if !was_dirty {
                            // Skip resync while user is selecting text (prevents flicker).
                            // Auto-clear after 5 seconds as a safety net.
                            if selecting.load(Ordering::Relaxed) {
                                let sel_start = selecting_since.load(Ordering::Relaxed);
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                    selecting.store(false, Ordering::Relaxed);
                                    selecting_since.store(0, Ordering::Relaxed);
                                } else {
                                    cooldown = cooldown.saturating_sub(1);
                                    std::thread::sleep(std::time::Duration::from_millis(33));
                                    continue;
                                }
                            }
                            let now_ms = std::time::SystemTime::now()
```

Also clone the selecting fields into the thread closure. At line ~242-248, where the existing Arc clones are:

```rust
        let term = t.term.clone();
        let processor = t.processor.clone();
        let render_dirty = t.dirty.clone();
        let stop = t.resync_stop.clone();
        let dirty = t.output_dirty.clone();
        let gen = t.process_generation.clone();
        let last_output = t.last_output_time_ms.clone();
```

Add after `let last_output = ...`:

```rust
        let selecting = t.selecting.clone();
        let selecting_since = t.selecting_since.clone();
```

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/terminal/terminal_core.rs
git commit -m "feat: gate resync thread on selecting flag with 5s timeout"
```

---

### Task 3: Wire mouse handlers to set/clear selecting flag

**Files:**
- Modify: `src/terminal/terminal_element.rs:764-849` (mouse event handlers)

- [ ] **Step 1: Set selecting on MouseDownEvent**

In `src/terminal/terminal_element.rs`, in the `MouseDownEvent` handler's non-mouse-mode branch (line ~785), after `terminal.start_selection(pt, side, sel_type);`, add:

```rust
                    terminal.selecting().store(true, Ordering::Relaxed);
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    terminal.selecting_since().store(now_ms, Ordering::Relaxed);
```

Add the import at file level (after `use std::sync::Arc;` on line 13):

```rust
use std::sync::atomic::Ordering;
```

- [ ] **Step 2: Clear selecting on MouseUpEvent**

In the `MouseUpEvent` handler (line ~834), add the clearing **after** the button check but **before** the `is_mouse_mode` branch (line ~838, before `let mode = terminal.mode();`):

```rust
                // Clear selecting flag unconditionally on mouse-up
                // (regardless of mouse mode or position)
                terminal.selecting().store(false, Ordering::Relaxed);
                terminal.selecting_since().store(0, Ordering::Relaxed);
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo check`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src/terminal/terminal_element.rs
git commit -m "feat: wire mouse handlers to set/clear selecting flag"
```

---

### Task 4: Gate render_tick and idle_tick on selecting flag

**Files:**
- Modify: `src/ui/terminal_manager.rs:746-774,1261-1289` (shell path render/idle tick branches)

- [ ] **Step 1: Gate render_tick in first output loop (setup_terminal_output)**

In `src/ui/terminal_manager.rs`, the render_tick branch at line ~746:

```rust
                            Either::Right((Either::Left((_, _)), _)) => {
                                // ── render_tick fired ──
                                if dirty {
                                    terminal_for_output.take_dirty();
                                    if !modal_open.load(Ordering::Relaxed) {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
```

Wrap the `cx.notify()` calls with a selecting check + 5s timeout auto-clear. Replace the render_tick branch with:

```rust
                            Either::Right((Either::Left((_, _)), _)) => {
                                // ── render_tick fired ──
                                let mut is_selecting = terminal_for_output.selecting().load(Ordering::Relaxed);
                                // Auto-clear selecting after 5 seconds (safety net for lost mouse-up)
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if dirty {
                                    terminal_for_output.take_dirty();
                                    if !modal_open.load(Ordering::Relaxed) && !is_selecting {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                    dirty = false;
                                } else {
                                    if terminal_for_output.take_dirty()
                                        && !modal_open.load(Ordering::Relaxed)
                                        && !is_selecting
                                    {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                }
                            }
```

- [ ] **Step 2: Gate idle_tick in first output loop**

Same file, idle_tick branch at line ~766:

```rust
                            Either::Right((Either::Right((_, _)), _)) => {
                                // ── idle_tick fired ──
                                if terminal_for_output.take_dirty()
                                    && !modal_open.load(Ordering::Relaxed)
                                {
```

Replace with:

```rust
                            Either::Right((Either::Right((_, _)), _)) => {
                                // ── idle_tick fired ──
                                let mut is_selecting = terminal_for_output.selecting().load(Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if terminal_for_output.take_dirty()
                                    && !modal_open.load(Ordering::Relaxed)
                                    && !is_selecting
                                {
```

- [ ] **Step 3: Gate render_tick in second output loop (setup_pane_terminal_output)**

Same changes at line ~1261 (the duplicate shell path). Replace render_tick branch with the same pattern (including 5s timeout auto-clear):

```rust
                            Either::Right((Either::Left((_, _)), _)) => {
                                // ── render_tick fired ──
                                let mut is_selecting = terminal_for_output.selecting().load(Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if dirty {
                                    terminal_for_output.take_dirty();
                                    if !modal_open.load(Ordering::Relaxed) && !is_selecting {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                    dirty = false;
                                } else {
                                    if terminal_for_output.take_dirty()
                                        && !modal_open.load(Ordering::Relaxed)
                                        && !is_selecting
                                    {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                }
                            }
```

- [ ] **Step 4: Gate idle_tick in second output loop**

Same file, idle_tick at line ~1281 (same pattern with 5s timeout):

```rust
                            Either::Right((Either::Right((_, _)), _)) => {
                                // ── idle_tick fired ──
                                let mut is_selecting = terminal_for_output.selecting().load(Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if terminal_for_output.take_dirty()
                                    && !modal_open.load(Ordering::Relaxed)
                                    && !is_selecting
                                {
```

- [ ] **Step 5: Build and run all tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src/ui/terminal_manager.rs
git commit -m "feat: gate render_tick and idle_tick on selecting flag"
```

---

### Task 5: Manual verification

- [ ] **Step 1: Run pmux**

Run: `RUSTUP_TOOLCHAIN=stable cargo run`

- [ ] **Step 2: Test selection during active output**

1. Start an agent (e.g., Claude Code) in a pane — let it produce continuous output
2. While output is streaming, click and drag to select text
3. Verify: selection highlight is stable, no flickering or jumping
4. Release mouse — verify terminal resumes normal rendering

- [ ] **Step 3: Test 5-second timeout**

1. While output is streaming, click and hold (start a selection) without releasing for >5 seconds
2. Verify: after ~5 seconds, the terminal starts updating again (resync resumes)

- [ ] **Step 4: Test no regression**

1. Switch between worktree tabs — no ghosting or artifacts
2. Type in the terminal — no input lag
3. Selection in idle terminal (no active output) — works normally
