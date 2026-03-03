# Terminal Scroll & Selection Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks (Phase 1 tasks are independent).

**Goal:** Add mouse wheel scrolling, keyboard scroll shortcuts, click/double/triple-click text selection with auto-copy, and mouse event forwarding to terminal programs.

**Architecture:** All scroll/selection state lives in alacritty_terminal's `Term` (already behind `Arc<Mutex<Term>>`). Mouse event handlers registered in `TerminalElement::paint()` via `window.on_mouse_event()`. No backend changes — both tmux-cc and local PTY share the same VTE grid.

**Tech Stack:** Rust, GPUI, alacritty_terminal 0.25

**Regression command:** `cd tests/regression && ./run_all.sh --skip-build`

---

## Phase 1: Core APIs (Tasks 1–3 are independent, can be parallelized)

### Task 1: Terminal Core — Scroll & Selection API

**Files:**
- Modify: `src/terminal/terminal_core.rs`

**Step 1: Write failing tests**

Add these imports at the top of the `#[cfg(test)] mod tests` block:

```rust
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::SelectionType;
```

Add these test functions:

```rust
#[test]
fn test_scroll_display_up_down() {
    let term = Terminal::new("scroll-1".into(), TerminalSize { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 });
    let mut data = Vec::new();
    for i in 0..50 {
        data.extend_from_slice(format!("line {}\r\n", i).as_bytes());
    }
    term.process_output(&data);

    assert_eq!(term.display_offset(), 0);
    term.scroll_display(5);
    assert_eq!(term.display_offset(), 5);
    term.scroll_display(-3);
    assert_eq!(term.display_offset(), 2);
    term.scroll_to_bottom();
    assert_eq!(term.display_offset(), 0);
}

#[test]
fn test_scroll_display_clamps() {
    let term = Terminal::new("scroll-2".into(), TerminalSize::default());
    term.scroll_display(100);
    assert_eq!(term.display_offset(), 0);
    term.scroll_display(-100);
    assert_eq!(term.display_offset(), 0);
}

#[test]
fn test_selection_basic() {
    let term = Terminal::new("sel-1".into(), TerminalSize::default());
    term.process_output(b"Hello World\r\n");

    assert!(!term.has_selection());
    term.start_selection(Point::new(Line(0), Column(0)), Side::Left, SelectionType::Simple);
    assert!(term.has_selection());
    term.update_selection(Point::new(Line(0), Column(4)), Side::Right);
    let text = term.selection_text();
    assert!(text.is_some());
    assert_eq!(text.unwrap(), "Hello");

    term.clear_selection();
    assert!(!term.has_selection());
}
```

**Step 2: Run tests to verify failure**

```bash
cargo test terminal_core::tests::test_scroll_display -- --nocapture
cargo test terminal_core::tests::test_selection_basic -- --nocapture
```

Expected: FAIL — methods don't exist yet.

**Step 3: Implement scroll and selection methods**

Add imports at top of `terminal_core.rs`:

```rust
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
```

Add new impl block (or extend existing) on `Terminal`:

```rust
impl Terminal {
    pub fn scroll_display(&self, delta: i32) {
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Delta(delta));
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Bottom);
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    pub fn start_selection(
        &self,
        point: alacritty_terminal::index::Point,
        side: Side,
        ty: SelectionType,
    ) {
        let mut term = self.term.lock();
        term.selection = Some(Selection::new(ty, point, side));
    }

    pub fn update_selection(&self, point: alacritty_terminal::index::Point, side: Side) {
        let mut term = self.term.lock();
        if let Some(ref mut sel) = term.selection {
            sel.update(point, side);
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    pub fn has_selection(&self) -> bool {
        self.term.lock().selection.is_some()
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    pub fn selection_range(&self) -> Option<SelectionRange> {
        let term = self.term.lock();
        term.selection.as_ref().and_then(|s| s.to_range(&term))
    }
}
```

**Step 4: Run tests to verify pass**

```bash
cargo test terminal_core::tests::test_scroll -- --nocapture
cargo test terminal_core::tests::test_selection -- --nocapture
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/terminal/terminal_core.rs
git commit -m "feat: add scroll and selection API to Terminal core"
```

---

### Task 2: SGR Mouse Escape Encoding

**Files:**
- Modify: `src/terminal/input.rs`

**Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `input.rs`:

```rust
#[test]
fn test_sgr_mouse_press() {
    let bytes = sgr_mouse_press(0, 10, 5);
    assert_eq!(bytes, b"\x1b[<0;11;6M");
}

#[test]
fn test_sgr_mouse_release() {
    let bytes = sgr_mouse_release(0, 10, 5);
    assert_eq!(bytes, b"\x1b[<0;11;6m");
}

#[test]
fn test_sgr_mouse_scroll_up() {
    let bytes = sgr_mouse_scroll(true, 5, 3);
    assert_eq!(bytes, b"\x1b[<64;6;4M");
}

#[test]
fn test_sgr_mouse_scroll_down() {
    let bytes = sgr_mouse_scroll(false, 5, 3);
    assert_eq!(bytes, b"\x1b[<65;6;4M");
}

#[test]
fn test_sgr_mouse_motion() {
    let bytes = sgr_mouse_motion(0, 20, 10);
    assert_eq!(bytes, b"\x1b[<32;21;11M");
}
```

**Step 2: Run tests to verify failure**

```bash
cargo test input::tests::test_sgr_mouse -- --nocapture
```

Expected: FAIL — functions don't exist.

**Step 3: Implement SGR mouse encoding**

Add to `src/terminal/input.rs` (at module level, outside `key_to_bytes`):

```rust
pub fn sgr_mouse_press(button: u8, col: usize, row: usize) -> Vec<u8> {
    format!("\x1b[<{};{};{}M", button, col + 1, row + 1).into_bytes()
}

pub fn sgr_mouse_release(button: u8, col: usize, row: usize) -> Vec<u8> {
    format!("\x1b[<{};{};{}m", button, col + 1, row + 1).into_bytes()
}

pub fn sgr_mouse_motion(button: u8, col: usize, row: usize) -> Vec<u8> {
    format!("\x1b[<{};{};{}M", button + 32, col + 1, row + 1).into_bytes()
}

pub fn sgr_mouse_scroll(up: bool, col: usize, row: usize) -> Vec<u8> {
    let button: u8 = if up { 64 } else { 65 };
    format!("\x1b[<{};{};{}M", button, col + 1, row + 1).into_bytes()
}
```

**Step 4: Run tests to verify pass**

```bash
cargo test input::tests::test_sgr_mouse -- --nocapture
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/terminal/input.rs
git commit -m "feat: add SGR mouse escape sequence encoding"
```

---

### Task 3: Module Re-exports

**Files:**
- Modify: `src/terminal/mod.rs`

**Step 1: Add re-exports**

Add to `src/terminal/mod.rs`:

```rust
pub use alacritty_terminal::selection::SelectionType;
pub use alacritty_terminal::index::Side;
pub use input::{sgr_mouse_press, sgr_mouse_release, sgr_mouse_motion, sgr_mouse_scroll};
```

**Step 2: Verify compilation**

```bash
cargo check
```

Expected: PASS

**Step 3: Commit**

```bash
git add src/terminal/mod.rs
git commit -m "feat: re-export selection and mouse types from terminal module"
```

---

### Phase 1 Regression Checkpoint

```bash
cargo test -- --nocapture 2>&1 | tail -20
cd tests/regression && ./run_all.sh --skip-build
```

Expected: All existing tests still pass. No behavioral changes yet — only new APIs added.

---

## Phase 2: Mouse Event Handlers (Tasks 4–5)

### Task 4: TerminalElement — Mouse Scroll, Selection, Reporting

**Files:**
- Modify: `src/terminal/terminal_element.rs`

This is the largest task. We register mouse event handlers in `paint()` and paint selection overlays.

**Step 1: Add imports and helper function**

Add imports at top of `terminal_element.rs`:

```rust
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::SelectionRange;
```

Add module-level helper:

```rust
fn pixel_to_grid(
    mouse_pos: Point<Pixels>,
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    display_offset: usize,
    cols: usize,
    rows: usize,
) -> (AlacPoint, Side) {
    let rel_x: f32 = (mouse_pos.x - origin.x).into();
    let rel_y: f32 = (mouse_pos.y - origin.y).into();
    let cell_w: f32 = cell_width.into();
    let line_h: f32 = line_height.into();

    let col_f = (rel_x / cell_w).max(0.0);
    let col = (col_f as usize).min(cols.saturating_sub(1));
    let side = if col_f.fract() < 0.5 { Side::Left } else { Side::Right };

    let row_f = (rel_y / line_h).max(0.0);
    let row = (row_f as usize).min(rows.saturating_sub(1));

    let line = Line(row as i32 - display_offset as i32);
    (AlacPoint::new(line, Column(col)), side)
}
```

**Step 2: Add `selection_range` field**

Add to `TerminalElement` struct:

```rust
selection_range: Option<SelectionRange>,
```

Initialize in `new()`:

```rust
selection_range: None,
```

Add builder:

```rust
pub fn with_selection(mut self, range: Option<SelectionRange>) -> Self {
    self.selection_range = range;
    self
}
```

**Step 3: Register mouse event handlers in paint()**

At the end of `paint()`, **before** the "Register InputHandler" block (before `if self.focused {`), add:

```rust
// --- Mouse scroll handler ---
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let b = bounds;

    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _window, _cx| {
        if !phase.bubble() || !b.contains(&event.position) {
            return;
        }
        let mode = terminal.mode();
        let mouse_mode = mode.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION,
        );

        let delta_lines = match event.delta {
            ScrollDelta::Lines(d) => -(d.y as i32),
            ScrollDelta::Pixels(d) => {
                let lh_f: f32 = lh.into();
                if lh_f > 0.0 { -(d.y / lh_f) as i32 } else { 0 }
            }
        };
        if delta_lines == 0 {
            return;
        }

        if mouse_mode {
            if let Some(ref send) = on_input {
                let display_offset = terminal.display_offset();
                let (pt, _) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);
                let col = pt.column.0;
                let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                let up = delta_lines > 0;
                for _ in 0..delta_lines.unsigned_abs() {
                    send(&crate::terminal::sgr_mouse_scroll(up, col, row));
                }
            }
        } else {
            terminal.scroll_display(delta_lines);
        }
    });
}

// --- Mouse down handler (selection start) ---
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let b = bounds;

    window.on_mouse_event(move |event: &MouseDownEvent, phase, _window, _cx| {
        if !phase.bubble() || event.button != MouseButton::Left || !b.contains(&event.position) {
            return;
        }
        let mode = terminal.mode();
        let mouse_mode = mode.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION,
        );
        let display_offset = terminal.display_offset();
        let (pt, side) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);

        if mouse_mode {
            if let Some(ref send) = on_input {
                let col = pt.column.0;
                let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                send(&crate::terminal::sgr_mouse_press(0, col, row));
            }
        } else {
            use alacritty_terminal::selection::SelectionType;
            let sel_type = match event.click_count {
                2 => SelectionType::Semantic,
                3 => SelectionType::Lines,
                _ => SelectionType::Simple,
            };
            terminal.clear_selection();
            terminal.start_selection(pt, side, sel_type);
        }
    });
}

// --- Mouse move handler (selection update / motion reporting) ---
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let b = bounds;

    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, _cx| {
        if !phase.bubble() || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let mode = terminal.mode();
        let mouse_mode = mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION);
        let display_offset = terminal.display_offset();
        let (pt, side) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);

        if mouse_mode {
            if let Some(ref send) = on_input {
                let col = pt.column.0;
                let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                send(&crate::terminal::sgr_mouse_motion(0, col, row));
            }
        } else if terminal.has_selection() {
            terminal.update_selection(pt, side);
        }
    });
}

// --- Mouse up handler (selection finalize + clipboard) ---
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let b = bounds;

    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
        if !phase.bubble() || event.button != MouseButton::Left {
            return;
        }
        let mode = terminal.mode();
        let mouse_mode = mode.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION,
        );

        if mouse_mode {
            if let Some(ref send) = on_input {
                let display_offset = terminal.display_offset();
                let (pt, _) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);
                let col = pt.column.0;
                let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                send(&crate::terminal::sgr_mouse_release(0, col, row));
            }
        } else if let Some(text) = terminal.selection_text() {
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    });
}
```

**Step 4: Paint selection overlay**

In `paint()`, after painting `layout_rects` backgrounds (the `for rect in layout_rects { ... }` loop) and **before** painting `text_runs`, add:

```rust
// Paint selection overlay
if let Some(ref sel_range) = self.selection_range {
    let display_offset = self.terminal.display_offset() as i32;
    let sel_color = Hsla { h: 0.58, s: 0.6, l: 0.5, a: 0.35 };

    for row in 0..state.rows {
        let grid_line = Line(row as i32 - display_offset);
        let row_start = AlacPoint::new(grid_line, Column(0));
        let row_end = AlacPoint::new(grid_line, Column(state.cols.saturating_sub(1)));

        if sel_range.start <= row_end && sel_range.end >= row_start {
            let start_col = if grid_line == sel_range.start.line {
                sel_range.start.column.0
            } else {
                0
            };
            let end_col = if grid_line == sel_range.end.line {
                sel_range.end.column.0
            } else {
                state.cols.saturating_sub(1)
            };
            if start_col <= end_col {
                let sel_x = origin.x + cell_width * (start_col as f32);
                let sel_y = origin.y + line_height * (row as f32);
                let sel_w = cell_width * ((end_col - start_col + 1) as f32);
                window.paint_quad(quad(
                    Bounds::new(
                        Point::new(sel_x, sel_y),
                        Size::new(sel_w, line_height),
                    ),
                    px(0.0),
                    sel_color,
                    Edges::default(),
                    transparent_black(),
                    Default::default(),
                ));
            }
        }
    }
}
```

**Step 5: Verify compilation**

```bash
cargo check
```

Fix any compilation errors. Common issues:
- `TermMode::intersects` might not exist; use `mode.contains(TermMode::MOUSE_REPORT_CLICK) || mode.contains(TermMode::MOUSE_DRAG) || mode.contains(TermMode::MOUSE_MOTION)` instead.
- `ScrollDelta::Pixels(d)` — `d` is `Point<Pixels>`, access `.y` for vertical.
- `ScrollDelta::Lines(d)` — `d` is `Point<f32>`, access `.y`.

**Step 6: Commit**

```bash
git add src/terminal/terminal_element.rs
git commit -m "feat: add mouse scroll, selection, and mouse reporting to terminal element"
```

---

### Task 5: Wire selection_range into TerminalView

**Files:**
- Modify: `src/ui/terminal_view.rs`

**Step 1: Pass selection_range when building TerminalElement**

In `terminal_view.rs`, in the `TerminalBuffer::Terminal` match arm where `TerminalElement::new(...)` is called (around line 163), add `.with_selection(...)`:

Find this code:

```rust
let mut elem = TerminalElement::new(
    terminal.clone(),
    focus_handle.clone(),
    ColorPalette::default(),
)
.with_focused(self.is_focused)
.with_search(matches, search_current)
.with_links(links, None);
```

Change to:

```rust
let selection_range = terminal.selection_range();
let mut elem = TerminalElement::new(
    terminal.clone(),
    focus_handle.clone(),
    ColorPalette::default(),
)
.with_focused(self.is_focused)
.with_search(matches, search_current)
.with_links(links, None)
.with_selection(selection_range);
```

**Step 2: Verify compilation**

```bash
cargo check
```

Expected: PASS

**Step 3: Commit**

```bash
git add src/ui/terminal_view.rs
git commit -m "feat: pass selection range to terminal element for rendering"
```

---

### Phase 2 Regression Checkpoint

```bash
cargo test -- --nocapture 2>&1 | tail -20
cargo build
cd tests/regression && ./run_all.sh --skip-build
```

Expected: All existing regression tests pass. New behavior: mouse scroll and selection work in the terminal (manual verification).

**Manual smoke test (required):**

1. `cargo run` — start pmux
2. Run a command that produces output (e.g. `ls -la /`)
3. Scroll up with trackpad/mouse wheel — older output should appear
4. Click+drag to select text — blue highlight should appear
5. Release — text should be in clipboard (paste with Cmd+V elsewhere)
6. Double-click a word — word is selected
7. Triple-click — line is selected
8. Type anything — terminal snaps back to bottom (wait, this is Task 6)

---

## Phase 3: Keyboard Shortcuts & Auto-scroll (Task 6)

### Task 6: Keyboard Scroll Shortcuts + Auto-scroll on Input

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: Add Shift+PageUp/Down/Home/End scroll shortcuts**

In `handle_key_down()`, add a new block **after** the `if event.keystroke.modifiers.platform { ... }` block (after the `return;` at ~line 2227) and **before** the "Forward all other keys" comment (~line 2230):

```rust
// Shift+key scroll shortcuts (no Cmd)
if event.keystroke.modifiers.shift && !event.keystroke.modifiers.platform {
    let scroll_handled = match event.keystroke.key.as_str() {
        "pageup" | "pagedown" | "home" | "end" => {
            if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(target) = self.active_pane_target.as_ref() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        match event.keystroke.key.as_str() {
                            "pageup" => {
                                let rows = terminal.size().rows;
                                terminal.scroll_display((rows as i32).saturating_sub(2));
                            }
                            "pagedown" => {
                                let rows = terminal.size().rows;
                                terminal.scroll_display(-((rows as i32).saturating_sub(2)));
                            }
                            "home" => terminal.scroll_display(i32::MAX / 2),
                            "end" => terminal.scroll_to_bottom(),
                            _ => {}
                        }
                        true
                    } else { false }
                } else { false }
            } else { false }
        }
        _ => false,
    };
    if scroll_handled {
        cx.notify();
        return;
    }
}
```

**Step 2: Add scroll-to-bottom on user input**

In `handle_key_down()`, in the block where `runtime.send_input(target, &bytes)` is called (~line 2262), add scroll-to-bottom **before** sending:

Find:

```rust
if let Some(bytes) = bytes_opt {
    let send_result = runtime.send_input(target, &bytes);
```

Change to:

```rust
if let Some(bytes) = bytes_opt {
    if let Ok(buffers) = self.terminal_buffers.lock() {
        if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
            if terminal.display_offset() > 0 {
                terminal.scroll_to_bottom();
            }
        }
    }
    let send_result = runtime.send_input(target, &bytes);
```

**Step 3: Verify compilation**

```bash
cargo check
```

Expected: PASS

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: add keyboard scroll shortcuts and auto-scroll on input"
```

---

### Phase 3 Regression Checkpoint

```bash
cargo test -- --nocapture 2>&1 | tail -20
cargo build
cd tests/regression && ./run_all.sh --skip-build
```

Expected: All regression tests pass.

**Manual smoke test (required):**

1. `cargo run` — start pmux
2. Generate scrollback: `for i in $(seq 1 100); do echo "line $i"; done`
3. Shift+PageUp — scrolls up one page
4. Shift+PageDown — scrolls down one page
5. Shift+Home — scrolls to top of history
6. Shift+End — scrolls to bottom
7. Scroll up with mouse, then type any character — snaps to bottom

---

## Phase 4: Integration Testing (Task 7)

### Task 7: Full Manual Test + Final Regression

**Manual test checklist:**

| # | Test | Backend | Steps | Expected |
|---|------|---------|-------|----------|
| 1 | Mouse scroll up/down | tmux | Scroll trackpad up, then down | Older lines appear, then return |
| 2 | Mouse scroll up/down | local | Same | Same |
| 3 | Click+drag select | tmux | Click-hold-drag across text | Blue highlight on selected cells |
| 4 | Auto-copy | tmux | Release after selecting | Cmd+V in TextEdit shows selected text |
| 5 | Double-click word | tmux | Double-click on a word | Word is highlighted and copied |
| 6 | Triple-click line | tmux | Triple-click on a line | Entire line highlighted and copied |
| 7 | Shift+PageUp/Down | tmux | Press Shift+PageUp, then Shift+PageDown | Scrolls by ~1 page |
| 8 | Shift+Home/End | tmux | Press Shift+Home, then Shift+End | Scrolls to top, then bottom |
| 9 | Auto-scroll on type | tmux | Scroll up, then type `a` | Snaps to bottom, `a` sent to shell |
| 10 | Mouse mode (vim) | tmux | Open vim, scroll in vim | Scroll goes to vim, not pmux |
| 11 | Mouse mode (less) | tmux | `man ls`, scroll | Scroll goes to less |
| 12 | Exit mouse mode | tmux | Quit vim, scroll | Scroll is pmux scrollback again |
| 13 | Split panes | tmux | Cmd+D split, scroll in each pane | Each pane scrolls independently |

**Final regression run:**

```bash
cargo test -- --nocapture 2>&1 | tail -20
cargo build
cd tests/regression && ./run_all.sh --skip-build
```

Expected: All regression tests pass. All manual tests pass.

**Commit:**

```bash
git add -A
git commit -m "feat: terminal scroll and selection — complete"
```

---

## Summary of all files changed

| File | Changes |
|------|---------|
| `src/terminal/terminal_core.rs` | `scroll_display()`, `scroll_to_bottom()`, `display_offset()`, `start_selection()`, `update_selection()`, `clear_selection()`, `has_selection()`, `selection_text()`, `selection_range()` + 3 unit tests |
| `src/terminal/input.rs` | `sgr_mouse_press()`, `sgr_mouse_release()`, `sgr_mouse_motion()`, `sgr_mouse_scroll()` + 5 unit tests |
| `src/terminal/mod.rs` | Re-exports: `SelectionType`, `Side`, SGR functions |
| `src/terminal/terminal_element.rs` | `pixel_to_grid()` helper, `selection_range` field + builder, 4 mouse event handlers in `paint()`, selection overlay painting |
| `src/ui/terminal_view.rs` | Pass `terminal.selection_range()` to `TerminalElement` |
| `src/ui/app_root.rs` | Shift+PageUp/Down/Home/End scroll, auto-scroll-to-bottom on input |

## Dependency graph

```
Phase 1 (parallel):  Task 1 ─┐
                     Task 2 ─┤─→ Phase 1 Regression Checkpoint
                     Task 3 ─┘
Phase 2 (sequential): Task 4 → Task 5 → Phase 2 Regression Checkpoint
Phase 3 (sequential): Task 6 → Phase 3 Regression Checkpoint
Phase 4 (manual):     Task 7 → Final Regression Checkpoint
```
