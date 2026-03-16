# Terminal Ghosting (残影) Fix Design

## Problem

When Claude Code (or other TUI programs) streams output rapidly through pmux's tmux control mode pipeline, duplicate lines appear — the same text renders on two adjacent rows. Resizing the window fixes it temporarily because resize triggers a full tmux `refresh-client` resync.

### Root Cause

The rendering pipeline has two fundamental issues:

1. **Chunked processing creates intermediate grid states.** tmux control mode delivers output as multiple `%output` events per logical "frame". The current code calls `process_output()` per chunk within a 4ms coalescing window, so the VTE parser advances the terminal grid incrementally. If GPUI renders between chunks, it paints an incomplete frame — producing ghosting.

2. **CSI 2026 synchronized output is lost.** Claude Code wraps each TUI frame in `CSI ?2026h` / `CSI ?2026l` (the DEC synchronized output protocol). Ghostty, Alacritty, and Wezterm all use this signal to gate rendering. However, tmux's internal terminal emulator consumes these sequences — they never reach pmux's `%output` stream. Verified experimentally: `capture-pane -e` shows no trace of 2026 sequences.

### Why Resize Fixes It

Resize triggers `resize-pane` + `refresh-client`, causing tmux to regenerate the full screen content. This is equivalent to a complete grid reset + authoritative resync.

## Solution

Four complementary mechanisms, ordered by impact:

### Part 1: Output Coalescing + Single-Shot Processing

**Location:** `src/ui/app_root.rs` — both output loops must be updated:
- Local terminal output loop (~line 1297)
- Tmux terminal output loop (~line 1638)

Both loops share the same 4ms coalescing pattern and must be updated consistently. Consider extracting the shared loop into a helper function to avoid duplication.

**Current behavior:**
```
chunk1 arrives → process_output(chunk1)   // grid at intermediate state
chunk2 arrives → process_output(chunk2)   // grid at another intermediate state
drain remaining → process_output(chunkN)  // ...
cx.notify()                               // render sees last intermediate state
```

**New behavior:**
```
chunk1 arrives → append to coalesce_buf
16ms window: collect all chunks → append to coalesce_buf
drain remaining → append to coalesce_buf
process_output(coalesce_buf)              // single VTE pass, grid jumps to final state
cx.notify()                               // render sees complete frame
```

Key changes:
- Coalescing window increased from 4ms to **16ms** (one 60fps frame)
- All chunks within the window are concatenated into a single `Vec<u8>`
- `process_output()` is called **once** per coalescing cycle
- `ContentExtractor.feed()` also receives the merged buffer once

**Note on ContentExtractor:** `ContentExtractor::feed()` uses an `Osc133Parser` that is byte-stream based, not chunk-boundary dependent. Feeding a larger merged buffer is safe — the parser maintains internal state across bytes regardless of chunk size.

```rust
let mut coalesce_buf: Vec<u8> = Vec::with_capacity(8192);

// Step 1: Wait for first chunk or idle timeout
match select(recv, idle_timer).await {
    Either::Left((Ok(chunk), _)) => {
        coalesce_buf.extend_from_slice(&chunk);
    }
    Either::Right(_) => { /* idle */ continue; }
}

// Step 2: Create timer AFTER first chunk arrives (so full 16ms window is available)
let frame_timer = cx.background_executor().timer(Duration::from_millis(16));
loop {
    match select(recv, frame_timer).await {
        Either::Left((Ok(next), _)) => coalesce_buf.extend_from_slice(&next),
        _ => break,
    }
}

// Step 3: Drain immediately available
while let Ok(next) = rx.try_recv() {
    coalesce_buf.extend_from_slice(&next);
}

// Step 4: Single-shot processing
terminal_for_output.process_output(&coalesce_buf);
ext.feed(&coalesce_buf);
coalesce_buf.clear();
cx.notify();
```

**Coalescing window strategy:** Use 16ms when alt-screen mode is detected (TUI programs like Claude Code), keep 4ms for normal mode (interactive shell typing). Alt-screen is where ghosting occurs; normal mode benefits from lower latency for keystroke echo.

```rust
let coalesce_ms = if terminal_for_output.is_alt_screen() { 16 } else { 4 };
let frame_timer = cx.background_executor().timer(Duration::from_millis(coalesce_ms));
```

### Part 2: Render Gating via cx.notify() Suppression

**Location:** `src/ui/app_root.rs` (output loop), `src/terminal/terminal_core.rs`

Rather than skipping `paint()` (which would produce blank frames in GPUI), suppress `cx.notify()` during output processing. Since `process_output()` runs on a background executor and `cx.notify()` merely schedules a render on the main thread, we gate at the notification level:

Add output-active tracking to `Terminal` (the struct is named `Terminal` in `terminal_core.rs`, line 117):

```rust
// terminal_core.rs - new field on Terminal struct
pub struct Terminal {
    // ... existing fields
    output_active: AtomicBool,
}
```

Output side (in the coalescing loop):
```rust
terminal_for_output.set_output_active(true);
terminal_for_output.process_output(&coalesce_buf);
ext.feed(&coalesce_buf);
terminal_for_output.set_output_active(false);
// Only notify AFTER output_active is cleared
cx.notify();
```

This ensures that any GPUI render triggered by the `cx.notify()` call sees the final grid state, not an intermediate one. The `output_active` flag can also be checked by any observer that might trigger auxiliary renders.

### Part 3: capture-pane Resync (Reduce Existing Timeout)

**Location:** `src/ui/app_root.rs` output loop

**Note:** The codebase already has a `capture_pane_resync()` mechanism (in `tmux_control_mode.rs:114`) that runs on the existing 2-second idle timeout. This part proposes reducing that timeout to 300ms for faster recovery after output bursts, not adding a separate resync path.

The existing `capture_pane_resync()` function already does per-line cursor positioning with `CSI row;1H` + content + `CSI K`. We reuse it as-is.

Change: reduce the idle timeout that triggers resync from 2 seconds to 300ms:

```rust
// Existing idle_timeout used for resync — change from 2s to 300ms
let resync_timeout = Duration::from_millis(300);
```

Rate limiting (unchanged from existing):
- Minimum 1 second between resync operations
- No resync during active output (only triggers after idle threshold)

### Part 4: CSI 2026 Gate for Local PTY Mode

**Location:** `src/terminal/terminal_core.rs`

For the `local_pty.rs` backend (where tmux doesn't intercept sequences), detect CSI 2026 and suppress `cx.notify()` until the sync block ends.

```rust
// terminal_core.rs - new fields on Terminal struct
pub struct Terminal {
    // ... existing
    synchronized_output: AtomicBool,
    sync_timeout_handle: Mutex<Option<Task<()>>>,
}
```

Detection in `process_output()` via VTE parser callback rather than naive byte scan (to handle sequences split across chunks correctly):

```rust
// In the VTE event handler (alacritty_terminal's Handler trait),
// intercept set_private_mode / unset_private_mode for mode 2026.
// alacritty_terminal calls these when it encounters CSI ?2026h/l.
//
// If alacritty_terminal does not expose mode 2026 events (it treats them as no-op),
// add a custom hook in the Processor/Handler to detect the mode number
// and set the synchronized_output flag on Terminal.
```

Notification gating (in the output loop, alongside Part 2):
```rust
// After process_output completes:
if !terminal_for_output.is_synchronized_output() {
    cx.notify(); // only render when not in sync block
}
```

Safety timeout prevents hangs if the program crashes without sending `CSI ?2026l`:
```rust
fn reset_sync_timeout(&self, cx: &AsyncAppContext) {
    let mut handle = self.sync_timeout_handle.lock();
    let sync_flag = self.synchronized_output.clone();
    *handle = Some(cx.spawn(async move {
        Timer::after(Duration::from_secs(1)).await;
        sync_flag.store(false, Ordering::Relaxed);
        // The next output cycle's cx.notify() will trigger render
    }));
}
```

**Scope:**
- tmux mode: CSI 2026 consumed by tmux, this logic never activates (zero overhead)
- local PTY mode: directly effective, eliminates ghosting at the source
- Future-proof: if tmux adds 2026 passthrough, pmux benefits automatically

## Files to Modify

| File | Changes |
|------|---------|
| `src/ui/app_root.rs` | Rewrite **both** output coalescing loops — local (~L1297) and tmux (~L1638) — with single-shot processing (Part 1); add output_active gating around process_output (Part 2); reduce idle resync timeout to 300ms (Part 3); add sync output notification gating (Part 4). Consider extracting shared loop into a helper. |
| `src/terminal/terminal_core.rs` | Add `output_active: AtomicBool`, `synchronized_output: AtomicBool`, `sync_timeout_handle` fields and methods to `Terminal` struct (Parts 2, 4). Add `is_alt_screen()` accessor if not already present. |

## Verification

- **Manual test:** Run Claude Code in pmux, trigger rapid streaming output, observe no duplicate lines
- **Resize test:** Confirm resize still works (should be unchanged)
- **Latency test:** Interactive typing in normal (non-alt-screen) mode should not feel sluggish (4ms coalescing preserved)
- **Alt-screen test:** TUI programs (htop, vim, Claude Code) should have no ghosting with 16ms coalescing
- **Resync test:** After rapid output stops, grid should match `tmux capture-pane` within 500ms
- **Local PTY test:** If local PTY backend is available, verify CSI 2026 gating works (output appears atomically)
