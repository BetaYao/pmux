# Terminal Scroll & Selection Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.
> Tasks 1-3 are independent and can be parallelized. Tasks 4-6 depend on 1-3. Task 7 depends on 4.

**Goal:** Add mouse wheel scrolling, keyboard scroll shortcuts, click/double/triple-click text selection with auto-copy, and mouse event forwarding to programs in both terminal backends.

**Architecture:** All scroll/selection state lives in alacritty_terminal's `Term` (already behind `Arc<Mutex<Term>>`). Mouse event handlers are registered in `TerminalElement::paint()` via GPUI's div wrapper. No backend changes needed — both tmux-cc and local PTY feed the same VTE grid.

**Tech Stack:** Rust, GPUI, alacritty_terminal 0.25

---

## Task 1: Terminal Core — Scroll & Selection API

**Files:**
- Modify: `src/terminal/terminal_core.rs`

**Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `terminal_core.rs`:

```rust
#[test]
fn test_scroll_display_up_down() {
    let term = Terminal::new("scroll-1".into(), TerminalSize { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 });
    // Generate scrollback by writing enough lines
    let mut data = Vec::new();
    for i in 0..50 {
        data.extend_from_slice(format!("line {}\r\n", i).as_bytes());
    }
    term.process_output(&data);

    assert_eq!(term.display_offset(), 0);
    term.scroll_display(5); // scroll up into history
    assert_eq!(term.display_offset(), 5);
    term.scroll_display(-3); // scroll back down
    assert_eq!(term.display_offset(), 2);
    term.scroll_to_bottom();
    assert_eq!(term.display_offset(), 0);
}

#[test]
fn test_scroll_display_clamps() {
    let term = Terminal::new("scroll-2".into(), TerminalSize::default());
    // No scrollback yet
    term.scroll_display(100);
    assert_eq!(term.display_offset(), 0);
    term.scroll_display(-100);
    assert_eq!(term.display_offset(), 0);
}

#[test]
fn test_selection_basic() {
    use alacritty_terminal::index::{Column, Line, Point, Side};
    use alacritty_terminal::selection::SelectionType;

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
cargo test terminal_core::tests::test_scroll_display_up_down
cargo test terminal_core::tests::test_scroll_display_clamps
cargo test terminal_core::tests::test_selection_basic
```

Expected: FAIL — methods `scroll_display`, `scroll_to_bottom`, `display_offset`, `start_selection`, `update_selection`, `clear_selection`, `selection_text`, `has_selection` don't exist.

**Step 3: Implement scroll and selection methods**

Add to `Terminal` impl in `src/terminal/terminal_core.rs`:

```rust
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{Selection, SelectionType};
```

```rust
/// Scroll the display viewport. Positive = into history (up), negative = toward bottom.
pub fn scroll_display(&self, delta: i32) {
    let mut term = self.term.lock();
    term.scroll_display(Scroll::Delta(delta));
    self.dirty.store(true, Ordering::Relaxed);
}

/// Scroll to the bottom (live output).
pub fn scroll_to_bottom(&self) {
    let mut term = self.term.lock();
    term.scroll_display(Scroll::Bottom);
    self.dirty.store(true, Ordering::Relaxed);
}

/// Current display offset (0 = at bottom, >0 = scrolled into history).
pub fn display_offset(&self) -> usize {
    self.term.lock().grid().display_offset()
}

/// Start a new selection at the given grid point.
pub fn start_selection(&self, point: alacritty_terminal::index::Point, side: Side, ty: SelectionType) {
    let mut term = self.term.lock();
    term.selection = Some(Selection::new(ty, point, side));
}

/// Update the selection endpoint.
pub fn update_selection(&self, point: alacritty_terminal::index::Point, side: Side) {
    let mut term = self.term.lock();
    if let Some(ref mut sel) = term.selection {
        sel.update(point, side);
    }
}

/// Clear the active selection.
pub fn clear_selection(&self) {
    self.term.lock().selection = None;
}

/// Whether there is an active selection.
pub fn has_selection(&self) -> bool {
    self.term.lock().selection.is_some()
}

/// Get the selected text as a string.
pub fn selection_text(&self) -> Option<String> {
    self.term.lock().selection_to_string()
}

/// Get the selection range for rendering. Returns (start, end, is_block) in grid coordinates.
pub fn selection_range(&self) -> Option<alacritty_terminal::selection::SelectionRange> {
    let term = self.term.lock();
    term.selection.as_ref().and_then(|s| s.to_range(&term))
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

## Task 2: SGR Mouse Escape Encoding

**Files:**
- Modify: `src/terminal/input.rs`

**Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `input.rs`:

```rust
#[test]
fn test_sgr_mouse_press() {
    // Left button press at col=10, row=5 (1-indexed in SGR)
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
    // Motion with left button held (button 0 + 32 motion flag)
    let bytes = sgr_mouse_motion(0, 20, 10);
    assert_eq!(bytes, b"\x1b[<32;21;11M");
}
```

**Step 2: Run tests to verify failure**

```bash
cargo test input::tests::test_sgr_mouse
```

Expected: FAIL — functions don't exist.

**Step 3: Implement SGR mouse encoding**

Add to `src/terminal/input.rs`:

```rust
/// SGR mouse press: \x1b[<button;col;rowM (col/row are 1-indexed)
pub fn sgr_mouse_press(button: u8, col: usize, row: usize) -> Vec<u8> {
    format!("\x1b[<{};{};{}M", button, col + 1, row + 1).into_bytes()
}

/// SGR mouse release: \x1b[<button;col;rowm
pub fn sgr_mouse_release(button: u8, col: usize, row: usize) -> Vec<u8> {
    format!("\x1b[<{};{};{}m", button, col + 1, row + 1).into_bytes()
}

/// SGR mouse motion: button + 32 (motion flag), \x1b[<btn;col;rowM
pub fn sgr_mouse_motion(button: u8, col: usize, row: usize) -> Vec<u8> {
    format!("\x1b[<{};{};{}M", button + 32, col + 1, row + 1).into_bytes()
}

/// SGR mouse scroll: up=64, down=65
pub fn sgr_mouse_scroll(up: bool, col: usize, row: usize) -> Vec<u8> {
    let button: u8 = if up { 64 } else { 65 };
    format!("\x1b[<{};{};{}M", button, col + 1, row + 1).into_bytes()
}
```

**Step 4: Run tests**

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

## Task 3: Re-export new types from terminal module

**Files:**
- Modify: `src/terminal/mod.rs`

**Step 1: Add re-exports**

Add the following re-exports so downstream code can use them:

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

## Task 4: TerminalElement — Mouse Scroll & Selection Handlers

**Files:**
- Modify: `src/terminal/terminal_element.rs`

This is the largest task. We add mouse event handlers for scrolling, selection, and mouse reporting.

**Step 1: Add helper — pixel to grid coordinate translation**

Add to `terminal_element.rs` (module-level function):

```rust
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{SelectionRange, SelectionType};

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

**Step 2: Add scroll callback and selection state fields**

Add new fields to `TerminalElement`:

```rust
pub struct TerminalElement {
    // ... existing fields ...
    selection_range: Option<SelectionRange>,  // cached for rendering
}
```

And a builder method:

```rust
pub fn with_selection(mut self, range: Option<SelectionRange>) -> Self {
    self.selection_range = range;
    self
}
```

**Step 3: Register mouse event handlers in paint()**

At the end of `paint()`, before the "Register InputHandler" block, add scroll and mouse handlers.

The approach: wrap the terminal content area in a div at the call site (in `terminal_view.rs`), using `.on_scroll_wheel()`, `.on_mouse_down()`, `.on_mouse_move()`, `.on_mouse_up()`.

However, since `TerminalElement` is a custom `Element` (not a `Render` component), we'll use GPUI's `window.on_mouse_event()` to register handlers during paint. These capture the terminal Arc and cell dimensions.

Add at end of `paint()`, before the InputHandler block:

```rust
// Mouse scroll handler
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let bounds_clone = bounds;

    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window| {
        if phase.bubble() && bounds_clone.contains(&event.position) {
            let mode = terminal.mode();
            let mouse_mode = mode.contains(TermMode::MOUSE_REPORT_CLICK)
                || mode.contains(TermMode::MOUSE_DRAG)
                || mode.contains(TermMode::MOUSE_MOTION);

            let delta_lines = match event.delta {
                ScrollDelta::Lines(delta) => -delta.y as i32,
                ScrollDelta::Pixels(delta) => {
                    let lh_f: f32 = lh.into();
                    (-delta.y / lh_f) as i32
                }
            };

            if delta_lines == 0 {
                return;
            }

            if mouse_mode {
                if let Some(ref send) = on_input {
                    let display_offset = terminal.display_offset();
                    let (point, _) = pixel_to_grid(
                        event.position, bounds_clone.origin,
                        cw, lh, display_offset, cols, rows,
                    );
                    let col = point.column.0;
                    let row = (point.line.0 + display_offset as i32) as usize;
                    let up = delta_lines > 0;
                    for _ in 0..delta_lines.unsigned_abs() {
                        let bytes = crate::terminal::sgr_mouse_scroll(up, col, row);
                        send(&bytes);
                    }
                }
            } else {
                terminal.scroll_display(delta_lines);
            }
        }
    });
}

// Mouse down handler (selection start)
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let bounds_clone = bounds;

    window.on_mouse_event(move |event: &MouseDownEvent, phase, _window| {
        if phase.bubble() && event.button == MouseButton::Left && bounds_clone.contains(&event.position) {
            let mode = terminal.mode();
            let mouse_mode = mode.contains(TermMode::MOUSE_REPORT_CLICK)
                || mode.contains(TermMode::MOUSE_DRAG)
                || mode.contains(TermMode::MOUSE_MOTION);

            let display_offset = terminal.display_offset();
            let (point, side) = pixel_to_grid(
                event.position, bounds_clone.origin,
                cw, lh, display_offset, cols, rows,
            );

            if mouse_mode {
                if let Some(ref send) = on_input {
                    let col = point.column.0;
                    let row = (point.line.0 + display_offset as i32) as usize;
                    let bytes = crate::terminal::sgr_mouse_press(0, col, row);
                    send(&bytes);
                }
            } else {
                let sel_type = match event.click_count {
                    2 => SelectionType::Semantic,
                    3 => SelectionType::Lines,
                    _ => SelectionType::Simple,
                };
                terminal.clear_selection();
                terminal.start_selection(point, side, sel_type);
            }
        }
    });
}

// Mouse move handler (selection update / mouse motion reporting)
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let bounds_clone = bounds;

    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window| {
        if phase.bubble() && event.pressed_button == Some(MouseButton::Left) && bounds_clone.contains(&event.position) {
            let mode = terminal.mode();
            let mouse_mode = mode.contains(TermMode::MOUSE_DRAG)
                || mode.contains(TermMode::MOUSE_MOTION);

            let display_offset = terminal.display_offset();
            let (point, side) = pixel_to_grid(
                event.position, bounds_clone.origin,
                cw, lh, display_offset, cols, rows,
            );

            if mouse_mode {
                if let Some(ref send) = on_input {
                    let col = point.column.0;
                    let row = (point.line.0 + display_offset as i32) as usize;
                    let bytes = crate::terminal::sgr_mouse_motion(0, col, row);
                    send(&bytes);
                }
            } else if terminal.has_selection() {
                terminal.update_selection(point, side);
            }
        }
    });
}

// Mouse up handler (selection finalize + clipboard copy)
{
    let terminal = self.terminal.clone();
    let on_input = self.on_input.clone();
    let cw = cell_width;
    let lh = line_height;
    let cols = state.cols;
    let rows = state.rows;
    let bounds_clone = bounds;

    window.on_mouse_event(move |event: &MouseUpEvent, phase, window| {
        if phase.bubble() && event.button == MouseButton::Left {
            let mode = terminal.mode();
            let mouse_mode = mode.contains(TermMode::MOUSE_REPORT_CLICK)
                || mode.contains(TermMode::MOUSE_DRAG)
                || mode.contains(TermMode::MOUSE_MOTION);

            if mouse_mode {
                if let Some(ref send) = on_input {
                    let display_offset = terminal.display_offset();
                    let (point, _) = pixel_to_grid(
                        event.position, bounds_clone.origin,
                        cw, lh, display_offset, cols, rows,
                    );
                    let col = point.column.0;
                    let row = (point.line.0 + display_offset as i32) as usize;
                    let bytes = crate::terminal::sgr_mouse_release(0, col, row);
                    send(&bytes);
                }
            } else if let Some(text) = terminal.selection_text() {
                if !text.is_empty() {
                    window.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
        }
    });
}
```

**Step 4: Paint selection highlighting**

In the `paint()` method, after painting layout_rects (background) but before painting text_runs, add selection overlay rendering:

```rust
// Paint selection overlay
if let Some(sel_range) = &self.selection_range {
    let display_offset = self.terminal.display_offset() as i32;
    let sel_color = Hsla { h: 0.58, s: 0.6, l: 0.5, a: 0.35 };

    for row in 0..state.rows {
        let grid_line = Line(row as i32 - display_offset);
        let grid_line_point_start = AlacPoint::new(grid_line, Column(0));
        let grid_line_point_end = AlacPoint::new(grid_line, Column(state.cols.saturating_sub(1)));

        if sel_range.start <= grid_line_point_end && sel_range.end >= grid_line_point_start {
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

Fix any compilation errors. The GPUI API names may need adjustment based on actual signatures.

**Step 6: Commit**

```bash
git add src/terminal/terminal_element.rs
git commit -m "feat: add mouse scroll, selection, and mouse reporting to terminal element"
```

---

## Task 5: Wire selection_range into TerminalView rendering

**Files:**
- Modify: `src/ui/terminal_view.rs` (where `TerminalElement` is constructed)

**Step 1: Pass selection_range to TerminalElement**

In `terminal_view.rs`, where `TerminalElement::new(...)` is called, add:

```rust
let selection_range = terminal.selection_range();
// ...
TerminalElement::new(terminal.clone(), focus_handle, palette)
    .with_selection(selection_range)
    // ... other builders
```

**Step 2: Verify compilation**

```bash
cargo check
```

**Step 3: Commit**

```bash
git add src/ui/terminal_view.rs
git commit -m "feat: pass selection range to terminal element for rendering"
```

---

## Task 6: Keyboard Scroll Shortcuts

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: Add keyboard scroll handling**

In `handle_key_down`, add a new block before the "Forward all other keys" section (after the Cmd+key shortcuts block, around line 2228). Handle Shift+PageUp/Down/Home/End:

```rust
// Shift+key scroll shortcuts (no Cmd modifier)
if event.keystroke.modifiers.shift && !event.keystroke.modifiers.platform {
    let handled = match event.keystroke.key.as_str() {
        "pageup" => {
            if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(target) = self.active_pane_target.as_ref() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        let rows = terminal.size().rows;
                        terminal.scroll_display((rows as i32).saturating_sub(2));
                        true
                    } else { false }
                } else { false }
            } else { false }
        }
        "pagedown" => {
            if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(target) = self.active_pane_target.as_ref() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        let rows = terminal.size().rows;
                        terminal.scroll_display(-((rows as i32).saturating_sub(2)));
                        true
                    } else { false }
                } else { false }
            } else { false }
        }
        "home" => {
            if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(target) = self.active_pane_target.as_ref() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        terminal.scroll_display(i32::MAX / 2);
                        true
                    } else { false }
                } else { false }
            } else { false }
        }
        "end" => {
            if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(target) = self.active_pane_target.as_ref() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        terminal.scroll_to_bottom();
                        true
                    } else { false }
                } else { false }
            } else { false }
        }
        _ => false,
    };
    if handled {
        cx.notify();
        return;
    }
}
```

**Step 2: Add scroll-to-bottom on user input**

In the same `handle_key_down`, in the block where `runtime.send_input(target, &bytes)` is called (around line 2262), add scroll-to-bottom before sending:

```rust
if let Some(bytes) = bytes_opt {
    // Auto-scroll to bottom when user types
    if let Ok(buffers) = self.terminal_buffers.lock() {
        if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
            if terminal.display_offset() > 0 {
                terminal.scroll_to_bottom();
            }
        }
    }
    let send_result = runtime.send_input(target, &bytes);
    // ...
}
```

**Step 3: Verify compilation**

```bash
cargo check
```

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: add keyboard scroll shortcuts and auto-scroll on input"
```

---

## Task 7: Integration Testing

**Files:**
- Modify: existing regression test framework or manual testing

**Step 1: Manual test checklist**

Run the app with `cargo run` and verify:

1. **Mouse scroll**: scroll up (into history) with trackpad/wheel, see older output. Scroll down to return.
2. **Selection**: click+drag to select text, verify blue highlight appears, verify text is in clipboard (Cmd+V in another app).
3. **Double-click**: double-click a word, verify word is selected.
4. **Triple-click**: triple-click a line, verify entire line is selected.
5. **Keyboard scroll**: Shift+PageUp/Down scroll by page, Shift+Home/End scroll to top/bottom.
6. **Auto-scroll**: scroll up, then type a character — should snap to bottom.
7. **Mouse reporting**: run `less` or `vim` (they enable mouse mode), verify scroll sends to the program, not pmux scrollback.
8. **Both backends**: test with tmux backend (default) and local PTY (if available).

**Step 2: Commit final adjustments**

```bash
git add -A
git commit -m "feat: terminal scroll and selection — complete"
```

---

## Summary of all files changed

| File | Changes |
|------|---------|
| `src/terminal/terminal_core.rs` | `scroll_display()`, `scroll_to_bottom()`, `display_offset()`, `start_selection()`, `update_selection()`, `clear_selection()`, `has_selection()`, `selection_text()`, `selection_range()` |
| `src/terminal/input.rs` | `sgr_mouse_press()`, `sgr_mouse_release()`, `sgr_mouse_motion()`, `sgr_mouse_scroll()` |
| `src/terminal/mod.rs` | Re-exports for `SelectionType`, `Side`, SGR functions |
| `src/terminal/terminal_element.rs` | `pixel_to_grid()` helper, `selection_range` field, mouse event handlers in `paint()`, selection overlay rendering |
| `src/ui/terminal_view.rs` | Pass `selection_range` to `TerminalElement` |
| `src/ui/app_root.rs` | Shift+PageUp/Down/Home/End scroll shortcuts, scroll-to-bottom on input |
