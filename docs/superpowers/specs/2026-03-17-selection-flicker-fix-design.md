# Selection Flicker Fix Design

## Problem

When Claude Code (or any agent) is actively producing output in pmux's embedded terminal, users cannot smoothly select text with the mouse. The selection highlight flickers and jumps during drag because two refresh mechanisms continuously repaint the terminal:

1. **Resync thread** (`terminal_core.rs`): Every 33ms during active output cooldown, runs `capture_pane_resync()` to overwrite the VTE grid, then sets `render_dirty = true`.
2. **Render tick** (`terminal_manager.rs`): Every 16ms, checks the dirty flag and fires `cx.notify()` to trigger a full repaint.

These repaints cause the selection overlay to visually flicker as the underlying grid content gets rewritten mid-drag.

## Solution

Add a `selecting: Arc<AtomicBool>` flag to `Terminal`. Mouse event handlers in `TerminalElement` set/clear it. The resync thread and render tick check it and skip their work when `true`.

## Design

### Data Flow

```
Terminal (Arc<AtomicBool> selecting)
  ↑ set by
TerminalElement (mouse_down → true, mouse_up → false)
  ↑ read by
terminal_core.rs resync thread (skip capture-pane when true)
terminal_manager.rs render_tick (skip cx.notify() when true)
```

### Changes

#### 1. `src/terminal/terminal_core.rs` — Terminal struct

- Add field: `selecting: Arc<AtomicBool>`, initialized to `false` in `new()`
- Add field: `selecting_since: Arc<AtomicU64>`, stores the timestamp (millis since epoch) when selecting started, `0` when not selecting
- Add accessor: `pub fn selecting(&self) -> &Arc<AtomicBool>`
- Add accessor: `pub fn selecting_since(&self) -> &Arc<AtomicU64>`
- Resync thread (line ~275, the `!was_dirty` branch): before entering `capture_pane_resync()`, check `selecting.load(Ordering::Relaxed)`. If `true`, check `selecting_since` — if elapsed >= 5 seconds, auto-clear `selecting` to `false` and proceed normally; otherwise decrement cooldown and `continue` (skip this cycle).

#### 2. `src/terminal/terminal_element.rs` — Mouse event handlers

- `MouseDownEvent` handler (line ~778, non-mouse-mode branch): after `terminal.start_selection(...)`, store current timestamp to `terminal.selecting_since()` and call `terminal.selecting().store(true, Ordering::Relaxed)`
- `MouseUpEvent` handler (line ~834): add `terminal.selecting().store(false, Ordering::Relaxed)` and reset `terminal.selecting_since().store(0, ...)` unconditionally, **outside** the `if is_mouse_mode(mode)` block (before the early return or at the top of the handler after the button check). Note: this handler currently has no bounds check, so the flag is cleared regardless of mouse position. Mouse-mode selection (e.g., vim, tmux copy-mode) is intentionally not covered — those applications manage their own selection.

#### 3. `src/ui/terminal_manager.rs` — Render tick and idle tick branches

- Shell path render_tick (line ~748 and ~1263): before calling `cx.notify()`, read `terminal_for_output.selecting().load(Ordering::Relaxed)`. If `true`, skip the notify.
- Shell path idle_tick (line ~768 and ~1283): same check — skip `cx.notify()` when selecting is `true`.
- The terminal still accumulates output via `process_output()`, so no data is lost — the repaint simply defers until selection ends.

### Edge Cases

**Selection state leak**: If `MouseUpEvent` somehow doesn't fire (e.g., system-level event stealing), the resync thread stays paused. This is acceptable because:
- `process_output()` still runs — terminal data is not lost
- The next `MouseDownEvent` or `MouseUpEvent` resets the flag
- GPUI's `MouseUpEvent` is registered without bounds checking, so it fires regardless of mouse position

**Resync recovery after selection**: When `selecting` goes `false`, the resync thread's next 33ms cycle naturally picks up any accumulated VTE drift via `capture_pane_resync()`. No special "catch-up" logic needed — the existing cooldown mechanism handles this.

**Multi-pane isolation**: Each `Terminal` instance has its own `selecting` flag. Selecting in one pane does not affect resync or rendering in other panes.

**Long selection hold (5s timeout)**: If the user holds the mouse for over 5 seconds, the resync thread auto-clears the `selecting` flag and resumes normal operation. This prevents indefinite resync pausing from edge cases (e.g., user forgets they're holding the mouse, or system event swallows the mouse-up). The render_tick/idle_tick also check `selecting_since` for the same 5s timeout.

## Scope

~20 lines of code across 3 files. No new dependencies. No API changes. No config changes.

## Success Criteria

- Dragging to select text in the embedded terminal produces a stable, non-flickering highlight while an agent is actively producing output
- Releasing the mouse restores normal resync behavior within one cycle (~33ms)
- No regression in terminal ghosting/artifact fixes
