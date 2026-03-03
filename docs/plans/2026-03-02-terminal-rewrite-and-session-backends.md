# Terminal Rewrite & Session Backends Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks within each Phase.

**Goal:** Remove vendored gpui-terminal, build a self-owned terminal layer (Okena-style), upgrade GPUI to Zed main, add dtach/screen session backends, and add terminal enhancements (search, URL detection, focus fog).

**Architecture:** Direct `alacritty_terminal` integration with custom GPUI `Element` for rendering, event-driven I/O pipeline, pluggable session backends with auto-detection.

**Tech Stack:** Rust, GPUI (Zed main), alacritty_terminal 0.25, portable-pty, parking_lot, flume

**Reference Projects:**
- [Okena](https://github.com/contember/okena) — self-built terminal on GPUI + alacritty_terminal
- [Zed](https://github.com/zed-industries/zed) — `crates/terminal/` + `crates/terminal_view/`
- Previous brainstorm: `docs/plans/2026-02-28-terminal-element-brainstorm.md`

---

## Overview

| Phase | Scope | Est. | Gate |
|-------|-------|------|------|
| **1** | GPUI upgrade to Zed main | 1–2 days | 回归测试 12 项 + 性能基线 |
| **2** | Self-built terminal core (replace gpui-terminal) | 5–7 days | 渲染 10 项 + TUI 6 项 + 输入 10 项 + Agent 状态 3 项 |
| **3** | Session backends (dtach/screen + auto-detect) | 2–3 days | 4 backend × 创建/持久化/恢复 + Phase 2 回归 |
| **4** | Terminal enhancements (search, URL, fog) | 2–3 days | 搜索 6 项 + URL 7 项 + 雾化 4 项 + 全量回归 |

> **规则：每个 Phase 结束后必须执行 Phase Gate 中的所有回归和功能测试用例，全部通过后方可进入下一 Phase。**

---

## Phase 1: GPUI Upgrade to Zed Main

**Goal:** Upgrade GPUI from pinned crates.io 0.2.2 (vendor) / git (pmux) to tracking Zed main, remove version conflicts.

### Task 1.1: Update Cargo.toml GPUI dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Update main Cargo.toml**

Remove the `rev` pin (if any) and ensure both `gpui` and `gpui_platform` track Zed main:

```toml
# Before
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }

# After (same, but verify no rev pin exists)
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
```

Ensure `[patch.crates-io]` still maps gpui/gpui_platform to git:

```toml
[patch.crates-io]
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
```

**Step 2: Run `cargo update -p gpui` to fetch latest**

```bash
RUSTUP_TOOLCHAIN=stable cargo update -p gpui
```

**Step 3: Build and fix breaking changes**

```bash
RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -100
```

Common GPUI API changes to watch for:
- `Element` trait signature changes (`request_layout`, `prepaint`, `paint`)
- `Window` method renames
- `Render` trait changes
- `IntoElement` / `RenderOnce` changes
- `Component` → `RenderOnce` migration

Fix each compiler error. This is the hardest part of Phase 1 — expect 10–30 errors across UI files.

**Step 4: Verify build**

```bash
RUSTUP_TOOLCHAIN=stable cargo build
```

**Step 5: Run tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test
```

**Step 6: Manual smoke test**

```bash
RUSTUP_TOOLCHAIN=stable cargo run
```
- Open a workspace
- Verify terminal renders correctly
- Verify keyboard input works
- Verify sidebar, tabs, splits work

**Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "chore: upgrade GPUI to Zed main branch"
```

### Phase 1 Gate: Regression & Functional Tests

> **硬性要求**：以下所有检查项全部通过后方可进入 Phase 2。任何失败必须在本 Phase 修复。

**自动化测试**

```bash
RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -5
# Expected: test result: ok. X passed; 0 failed
```

**编译检查（零 warning 目标）**

```bash
RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | grep -c "warning"
# Expected: 0 (or same as before upgrade — no new warnings)
```

**功能回归清单**（手动，逐项确认）

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 启动 & 打开 workspace | `cargo run` → 选择 git 目录 | 窗口正常打开，无 panic |
| 2 | 终端渲染 | 打开 workspace 后查看终端 | 文字、颜色、光标正常显示 |
| 3 | 键盘输入 | 在终端输入 `ls`、`echo hello`、Ctrl+C | 输入回显正常，Ctrl+C 中断 |
| 4 | 方向键 & Tab | 终端内按上下左右、Tab 补全 | 行为与原生终端一致 |
| 5 | vim 兼容 | `vim /tmp/test.txt` → 编辑 → `:wq` | 全屏 TUI 正常，光标位置正确 |
| 6 | htop / top | 运行 `htop` 或 `top` | 全屏刷新正常，无乱码 |
| 7 | CJK 字符 | `echo "你好世界"` | 中文正常显示，无错位 |
| 8 | 分屏 (⌘D / ⌘⇧D) | 快捷键分屏 | 两个 pane 独立工作 |
| 9 | Sidebar 状态 | 启动 agent，观察 sidebar | Running/Waiting 状态图标正确 |
| 10 | 多 workspace Tab | 打开多个 workspace | Tab 切换正常 |
| 11 | tmux backend | `PMUX_BACKEND=tmux cargo run` | tmux control mode 正常工作 |
| 12 | local backend | `PMUX_BACKEND=local cargo run` | local PTY 正常工作 |

**性能基线**（记录用于后续 Phase 对比）

```bash
# 记录启动时间
time RUSTUP_TOOLCHAIN=stable cargo run -- --help 2>/dev/null

# 记录二进制大小
ls -lh target/debug/pmux
```

---

## Phase 2: Self-Built Terminal Core

**Goal:** Remove `gpui-terminal` vendored crate entirely. Replace with self-built terminal that directly uses `alacritty_terminal` for VTE parsing and custom GPUI `Element` for rendering.

### Design Overview

```
New terminal architecture:

src/terminal/
├── mod.rs                    # Module exports
├── terminal.rs               # Terminal struct (Arc<Mutex<Term>>)
├── terminal_element.rs       # Custom GPUI Element for rendering
├── terminal_rendering.rs     # BatchedTextRun, LayoutRect helpers
├── input.rs                  # keystroke_to_bytes, InputHandler
├── colors.rs                 # ColorPalette (copy from vendor)
├── box_drawing.rs            # Box drawing (copy from vendor)
├── event_proxy.rs            # EventListener for alacritty_terminal
├── stream_adapter.rs         # Simplified: just tee_output (remove RuntimeReader/Writer)
└── content_extractor.rs      # Existing (unchanged)

src/ui/
├── terminal_view.rs          # Updated: TerminalBuffer uses new Terminal
└── app_root.rs               # Updated: new terminal setup flow
```

**I/O pipeline (event-driven, no tee):**

```
Runtime.subscribe_output(pane_id)
    → rx (flume::Receiver<Vec<u8>>)

cx.spawn(async move {
    while let Ok(chunk) = rx.recv_async().await {
        terminal.process_output(&chunk);        // Feed VTE parser
        cx.notify();                            // Trigger repaint
    }
})

ContentExtractor subscribes to Terminal's dirty signal:
    terminal.with_content(|term| { ... })       // Read from Term state
```

### Task 2.1: Add alacritty_terminal direct dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add alacritty_terminal and parking_lot to Cargo.toml**

```toml
# Add to [dependencies]:
alacritty_terminal = "0.25"
parking_lot = "0.12"
```

Keep `gpui-terminal` for now (will remove in Task 2.8).

**Step 2: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add alacritty_terminal and parking_lot direct dependencies"
```

---

### Task 2.2: Create Terminal struct

**Files:**
- Create: `src/terminal/terminal_core.rs`

**Step 1: Write the Terminal struct**

Reference: Okena's `src/terminal/terminal.rs` and vendor's `src/terminal.rs`.

```rust
// src/terminal/terminal_core.rs

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use alacritty_terminal::grid::Dimensions;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Terminal size in cells
#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: f32,
    pub cell_height: f32,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 }
    }
}

/// Event listener that captures title changes, bell, and PTY write requests
pub struct TermEventProxy {
    title: Arc<Mutex<Option<String>>>,
    has_bell: Arc<Mutex<bool>>,
    pty_write_tx: flume::Sender<Vec<u8>>,
}

impl TermEventProxy {
    pub fn new(
        title: Arc<Mutex<Option<String>>>,
        has_bell: Arc<Mutex<bool>>,
        pty_write_tx: flume::Sender<Vec<u8>>,
    ) -> Self {
        Self { title, has_bell, pty_write_tx }
    }
}

impl EventListener for TermEventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Title(t) => { *self.title.lock() = Some(t); }
            TermEvent::ResetTitle => { *self.title.lock() = None; }
            TermEvent::Bell => { *self.has_bell.lock() = true; }
            TermEvent::PtyWrite(data) => {
                let _ = self.pty_write_tx.send(data.into_bytes());
            }
            _ => {}
        }
    }
}

/// Core terminal wrapping alacritty_terminal::Term
pub struct Terminal {
    term: Arc<Mutex<Term<TermEventProxy>>>,
    processor: Mutex<Processor>,
    pub terminal_id: String,
    size: Mutex<TerminalSize>,
    title: Arc<Mutex<Option<String>>>,
    has_bell: Arc<Mutex<bool>>,
    dirty: AtomicBool,
    pty_write_tx: flume::Sender<Vec<u8>>,
    pty_write_rx: flume::Receiver<Vec<u8>>,
}

impl Terminal {
    pub fn new(terminal_id: String, size: TerminalSize) -> Self {
        let config = TermConfig::default();
        let term_size = TermSize::new(size.cols as usize, size.rows as usize);
        let title = Arc::new(Mutex::new(None));
        let has_bell = Arc::new(Mutex::new(false));
        let (pty_write_tx, pty_write_rx) = flume::unbounded();

        let event_proxy = TermEventProxy::new(
            title.clone(),
            has_bell.clone(),
            pty_write_tx.clone(),
        );
        let term = Term::new(config, &term_size, event_proxy);

        Self {
            term: Arc::new(Mutex::new(term)),
            processor: Mutex::new(Processor::new()),
            terminal_id,
            size: Mutex::new(size),
            title,
            has_bell,
            dirty: AtomicBool::new(false),
            pty_write_tx,
            pty_write_rx,
        }
    }

    /// Feed PTY output bytes into the VTE parser
    pub fn process_output(&self, data: &[u8]) {
        let mut term = self.term.lock();
        let mut processor = self.processor.lock();
        processor.advance(&mut *term, data);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Check and clear dirty flag
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Read-only access to the terminal grid for rendering
    pub fn with_content<F, R>(&self, f: F) -> R
    where F: FnOnce(&Term<TermEventProxy>) -> R {
        let term = self.term.lock();
        f(&term)
    }

    /// Resize the terminal grid
    pub fn resize(&self, new_size: TerminalSize) {
        *self.size.lock() = new_size;
        let mut term = self.term.lock();
        let term_size = TermSize::new(new_size.cols as usize, new_size.rows as usize);
        term.resize(term_size);
    }

    /// Get current size
    pub fn size(&self) -> TerminalSize {
        *self.size.lock()
    }

    /// Get title (from OSC sequences)
    pub fn title(&self) -> Option<String> {
        self.title.lock().clone()
    }

    /// Check and clear bell flag
    pub fn take_bell(&self) -> bool {
        let mut bell = self.has_bell.lock();
        let had = *bell;
        *bell = false;
        had
    }

    /// Receiver for PTY write-back events (e.g., cursor position report)
    pub fn pty_write_rx(&self) -> &flume::Receiver<Vec<u8>> {
        &self.pty_write_rx
    }

    /// Get terminal mode flags (for cursor visibility, app cursor mode, etc.)
    pub fn mode(&self) -> TermMode {
        self.term.lock().mode()
    }
}
```

**Step 2: Add module to `src/terminal/mod.rs`**

```rust
pub mod terminal_core;
pub use terminal_core::{Terminal, TerminalSize};
```

**Step 3: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 4: Commit**

```bash
git add src/terminal/terminal_core.rs src/terminal/mod.rs
git commit -m "feat: add Terminal struct wrapping alacritty_terminal directly"
```

---

### Task 2.3: Copy and adapt colors + box_drawing from vendor

**Files:**
- Create: `src/terminal/colors.rs` (copy from `vendor/gpui-terminal/src/colors.rs`)
- Create: `src/terminal/box_drawing.rs` (copy from `vendor/gpui-terminal/src/box_drawing.rs`)

**Step 1: Copy colors.rs**

Copy `vendor/gpui-terminal/src/colors.rs` to `src/terminal/colors.rs`. Adjust imports to use `gpui` directly (no `crate::` prefix changes needed since both use `gpui::*`).

**Step 2: Copy box_drawing.rs**

Copy `vendor/gpui-terminal/src/box_drawing.rs` to `src/terminal/box_drawing.rs`. Same import adjustments.

**Step 3: Add modules**

In `src/terminal/mod.rs`:
```rust
pub mod colors;
pub mod box_drawing;
pub use colors::{ColorPalette, ColorPaletteBuilder};
```

**Step 4: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 5: Commit**

```bash
git add src/terminal/colors.rs src/terminal/box_drawing.rs src/terminal/mod.rs
git commit -m "feat: copy colors and box_drawing from vendor into terminal module"
```

---

### Task 2.4: Create terminal_rendering.rs (BatchedTextRun, LayoutRect)

**Files:**
- Create: `src/terminal/terminal_rendering.rs`

**Step 1: Write rendering helpers**

Reference: Okena's `src/elements/terminal_rendering.rs` and vendor's `render.rs`.

```rust
// src/terminal/terminal_rendering.rs

use crate::terminal::colors::ColorPalette;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::*;

/// Batched text run — adjacent cells with same style
pub struct BatchedTextRun {
    pub start_line: i32,
    pub start_col: i32,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
}

impl BatchedTextRun {
    pub fn new(start_line: i32, start_col: i32, c: char, style: TextRun) -> Self {
        let mut text = String::with_capacity(100);
        text.push(c);
        Self { start_line, start_col, text, cell_count: 1, style }
    }

    pub fn can_append(&self, other_style: &TextRun, line: i32, col: i32) -> bool {
        self.start_line == line
            && self.start_col + self.cell_count as i32 == col
            && self.style.font == other_style.font
            && self.style.color == other_style.color
            && self.style.background_color == other_style.background_color
            && self.style.underline == other_style.underline
            && self.style.strikethrough == other_style.strikethrough
    }

    pub fn append_char(&mut self, c: char) {
        self.text.push(c);
        self.cell_count += 1;
        self.style.len += c.len_utf8();
    }

    pub fn paint(
        &self,
        origin: Point<Pixels>,
        cell_width: Pixels,
        line_height: Pixels,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = Point::new(
            origin.x + self.start_col as f32 * cell_width,
            origin.y + self.start_line as f32 * line_height,
        );
        let run_style = TextRun {
            len: self.text.len(),
            font: self.style.font.clone(),
            color: self.style.color,
            background_color: self.style.background_color,
            underline: self.style.underline.clone(),
            strikethrough: self.style.strikethrough.clone(),
        };
        let _ = window
            .text_system()
            .shape_line(self.text.clone().into(), font_size, &[run_style], Some(cell_width))
            .paint(pos, line_height, TextAlign::Left, None, window, cx);
    }
}

/// Background rectangle for non-default bg cells
pub struct LayoutRect {
    pub line: i32,
    pub start_col: i32,
    pub num_cells: usize,
    pub color: Hsla,
}

impl LayoutRect {
    pub fn new(line: i32, col: i32, color: Hsla) -> Self {
        Self { line, start_col: col, num_cells: 1, color }
    }

    pub fn extend(&mut self) {
        self.num_cells += 1;
    }

    pub fn paint(&self, origin: Point<Pixels>, cell_width: Pixels, line_height: Pixels, window: &mut Window) {
        let position = point(
            px((f32::from(origin.x) + self.start_col as f32 * f32::from(cell_width)).floor()),
            origin.y + line_height * self.line as f32,
        );
        let size = size(
            px((f32::from(cell_width) * self.num_cells as f32).ceil()),
            line_height,
        );
        window.paint_quad(fill(Bounds::new(position, size), self.color));
    }
}

/// Check if a color is the default background
pub fn is_default_bg(color: &Color, palette: &ColorPalette) -> bool {
    matches!(color, Color::Named(NamedColor::Background))
}
```

**Step 2: Add module**

In `src/terminal/mod.rs`:
```rust
pub mod terminal_rendering;
```

**Step 3: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 4: Commit**

```bash
git add src/terminal/terminal_rendering.rs src/terminal/mod.rs
git commit -m "feat: add terminal rendering helpers (BatchedTextRun, LayoutRect)"
```

---

### Task 2.5: Create TerminalElement (custom GPUI Element)

**Files:**
- Create: `src/terminal/terminal_element.rs`

**Step 1: Write the TerminalElement**

This is the core rendering component. Reference: Okena's `elements/terminal_element.rs`.

```rust
// src/terminal/terminal_element.rs

use crate::terminal::colors::ColorPalette;
use crate::terminal::terminal_core::Terminal;
use crate::terminal::terminal_rendering::{BatchedTextRun, LayoutRect, is_default_bg};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::grid::Dimensions;
use gpui::*;
use std::sync::Arc;

pub struct TerminalElement {
    terminal: Arc<Terminal>,
    focus_handle: FocusHandle,
    palette: ColorPalette,
}

impl TerminalElement {
    pub fn new(terminal: Arc<Terminal>, focus_handle: FocusHandle, palette: ColorPalette) -> Self {
        Self { terminal, focus_handle, palette }
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element { self }
}

pub struct TerminalElementState {
    cell_width: Pixels,
    line_height: Pixels,
    font_size: Pixels,
    font: Font,
    font_bold: Font,
    font_italic: Font,
    font_bold_italic: Font,
}

impl Element for TerminalElement {
    type RequestLayoutState = TerminalElementState;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> { None }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> { None }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let font_size = px(14.0);  // TODO: from config

        let font = Font {
            family: "Menlo".into(),
            features: FontFeatures::disable_ligatures(),
            fallbacks: Some(FontFallbacks::from_fonts(vec![
                "JetBrains Mono".into(),
                "SF Mono".into(),
                "Monaco".into(),
            ])),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        };

        let font_bold = Font { weight: FontWeight::BOLD, ..font.clone() };
        let font_italic = Font { style: FontStyle::Italic, ..font.clone() };
        let font_bold_italic = Font {
            weight: FontWeight::BOLD,
            style: FontStyle::Italic,
            ..font.clone()
        };

        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system
            .advance(font_id, font_size, 'm')
            .map(|size| size.width)
            .unwrap_or(font_size * 0.6);
        let line_height = font_size * 1.2;

        let style = Style {
            size: Size {
                width: relative(1.0).into(),
                height: relative(1.0).into(),
            },
            ..Default::default()
        };

        let layout_id = window.request_layout(style, [], cx);

        (layout_id, TerminalElementState {
            cell_width, line_height, font_size, font,
            font_bold, font_italic, font_bold_italic,
        })
    }

    fn prepaint(
        &mut self, _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        _window: &mut Window, _cx: &mut App,
    ) -> Self::PrepaintState {}

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let palette = &self.palette;
        let cell_width = state.cell_width;
        let line_height = state.line_height;
        let font_size = state.font_size;
        let cell_width_f = f32::from(cell_width);
        let line_height_f = f32::from(line_height);

        // Calculate terminal size and resize if needed
        let new_cols = ((f32::from(bounds.size.width) - 0.5) / cell_width_f).floor().max(1.0) as u16;
        let new_rows = ((f32::from(bounds.size.height) - 0.5) / line_height_f).floor().max(1.0) as u16;
        let current_size = self.terminal.size();
        if new_cols != current_size.cols || new_rows != current_size.rows {
            self.terminal.resize(crate::terminal::terminal_core::TerminalSize {
                cols: new_cols, rows: new_rows,
                cell_width: cell_width_f, cell_height: line_height_f,
            });
            // TODO: notify runtime to resize PTY
        }

        // Paint background
        let bg = palette.background();
        window.paint_quad(fill(bounds, bg));

        // Render grid
        self.terminal.with_content(|term| {
            let grid = term.grid();
            let screen_lines = grid.screen_lines();
            let cols = grid.columns();
            let display_offset = grid.display_offset() as i32;
            let origin = bounds.origin;

            let mut batched_runs: Vec<BatchedTextRun> = Vec::new();
            let mut rects: Vec<LayoutRect> = Vec::new();
            let mut current_batch: Option<BatchedTextRun> = None;
            let mut current_rect: Option<LayoutRect> = None;

            for row in 0..screen_lines {
                let visual_line = row as i32;
                let buffer_line = visual_line - display_offset;

                if let Some(batch) = current_batch.take() { batched_runs.push(batch); }
                if let Some(rect) = current_rect.take() { rects.push(rect); }

                for col in 0..cols {
                    let cell_point = alacritty_terminal::index::Point {
                        line: Line(buffer_line),
                        column: Column(col),
                    };
                    let cell = &grid[cell_point];

                    let mut fg = cell.fg.clone();
                    let mut bg = cell.bg.clone();
                    if cell.flags.contains(Flags::INVERSE) {
                        std::mem::swap(&mut fg, &mut bg);
                    }

                    // Background batching
                    let bg_color = if !is_default_bg(&bg, palette) {
                        Some(palette.resolve(&bg))
                    } else {
                        None
                    };

                    if let Some(color) = bg_color {
                        if let Some(ref mut rect) = current_rect {
                            if rect.line == visual_line
                                && rect.start_col + rect.num_cells as i32 == col as i32
                                && rect.color == color
                            {
                                rect.extend();
                            } else {
                                rects.push(current_rect.take().unwrap());
                                current_rect = Some(LayoutRect::new(visual_line, col as i32, color));
                            }
                        } else {
                            current_rect = Some(LayoutRect::new(visual_line, col as i32, color));
                        }
                    } else if let Some(rect) = current_rect.take() {
                        rects.push(rect);
                    }

                    // Skip spacers and blanks
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) { continue; }
                    if cell.c == ' ' && !cell.flags.intersects(Flags::UNDERLINE | Flags::STRIKEOUT) {
                        continue;
                    }

                    // Text style
                    let fg_color = palette.resolve(&fg);
                    let is_bold = cell.flags.contains(Flags::BOLD);
                    let is_italic = cell.flags.contains(Flags::ITALIC);
                    let font = match (is_bold, is_italic) {
                        (true, true) => state.font_bold_italic.clone(),
                        (true, false) => state.font_bold.clone(),
                        (false, true) => state.font_italic.clone(),
                        (false, false) => state.font.clone(),
                    };

                    let text_style = TextRun {
                        len: cell.c.len_utf8(),
                        font,
                        color: fg_color,
                        background_color: None,
                        underline: if cell.flags.intersects(Flags::ALL_UNDERLINES) {
                            Some(UnderlineStyle {
                                color: Some(fg_color),
                                thickness: px(1.0),
                                wavy: cell.flags.contains(Flags::UNDERCURL),
                            })
                        } else { None },
                        strikethrough: if cell.flags.contains(Flags::STRIKEOUT) {
                            Some(StrikethroughStyle {
                                color: Some(fg_color),
                                thickness: px(1.0),
                            })
                        } else { None },
                    };

                    // Batch text
                    if let Some(ref mut batch) = current_batch {
                        if batch.can_append(&text_style, visual_line, col as i32) {
                            batch.append_char(cell.c);
                        } else {
                            batched_runs.push(current_batch.take().unwrap());
                            current_batch = Some(BatchedTextRun::new(visual_line, col as i32, cell.c, text_style));
                        }
                    } else {
                        current_batch = Some(BatchedTextRun::new(visual_line, col as i32, cell.c, text_style));
                    }
                }
            }

            if let Some(batch) = current_batch { batched_runs.push(batch); }
            if let Some(rect) = current_rect { rects.push(rect); }

            // Paint backgrounds
            for rect in &rects {
                rect.paint(origin, cell_width, line_height, window);
            }

            // Paint text
            for batch in &batched_runs {
                batch.paint(origin, cell_width, line_height, font_size, window, cx);
            }

            // Paint cursor
            use alacritty_terminal::term::TermMode;
            if term.mode().contains(TermMode::SHOW_CURSOR) {
                let cursor_point = term.grid().cursor.point;
                let cursor_visual_line = cursor_point.line.0 + display_offset;
                if cursor_visual_line >= 0 && cursor_visual_line < screen_lines as i32 {
                    let cursor_x = px((f32::from(origin.x) + cursor_point.column.0 as f32 * cell_width_f).floor());
                    let cursor_y = px((f32::from(origin.y) + cursor_visual_line as f32 * line_height_f).floor());
                    let cursor_color = palette.cursor();
                    window.paint_quad(fill(
                        Bounds::new(point(cursor_x, cursor_y), size(cell_width, line_height)),
                        cursor_color,
                    ));
                }
            }
        });
    }
}
```

**Step 2: Add module**

In `src/terminal/mod.rs`:
```rust
pub mod terminal_element;
```

**Step 3: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

Note: This will not compile yet because `Element` trait API depends on GPUI version. Adjust signatures based on actual GPUI main API. The core pattern (request_layout → prepaint → paint with grid iteration) stays the same.

**Step 4: Commit**

```bash
git add src/terminal/terminal_element.rs src/terminal/mod.rs
git commit -m "feat: add TerminalElement custom GPUI Element for terminal rendering"
```

---

### Task 2.6: Create input handler

**Files:**
- Create: `src/terminal/input.rs`

**Step 1: Write input handler**

Reference: Okena's `src/terminal/input.rs` and vendor's `src/input.rs`.

```rust
// src/terminal/input.rs

use alacritty_terminal::term::TermMode;
use gpui::KeyDownEvent;

/// Convert GPUI key event to terminal escape bytes.
/// `app_cursor_mode`: when true, arrow keys send SS3 sequences.
pub fn key_to_bytes(event: &KeyDownEvent, mode: TermMode) -> Option<Vec<u8>> {
    let keystroke = &event.keystroke;
    let mods = &keystroke.modifiers;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    // Ctrl+letter → control character
    if mods.control && !mods.shift && !mods.alt && !mods.platform {
        let key = keystroke.key.as_str();
        if let Some(c) = key.chars().next() {
            if key.len() == 1 && c.is_ascii_alphabetic() {
                return Some(vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]);
            }
        }
    }

    // Tab
    if keystroke.key.as_str() == "tab" {
        return if mods.shift {
            Some(b"\x1b[Z".to_vec())
        } else {
            Some(b"\t".to_vec())
        };
    }

    // Enter
    match keystroke.key.as_str() {
        "enter" | "return" | "kp_enter" => {
            return if mods.shift { Some(b"\n".to_vec()) } else { Some(b"\r".to_vec()) };
        }
        _ => {}
    }

    // macOS Cmd+Arrow for line navigation
    #[cfg(target_os = "macos")]
    if mods.platform && !mods.alt && !mods.control {
        match keystroke.key.as_str() {
            "left" => return Some(vec![0x01]),    // Ctrl+A
            "right" => return Some(vec![0x05]),   // Ctrl+E
            "up" => return Some(b"\x1b[1;5A".to_vec()),
            "down" => return Some(b"\x1b[1;5B".to_vec()),
            "backspace" => return Some(vec![0x15]), // Ctrl+U
            _ => {}
        }
    }

    // macOS Option+Arrow for word navigation
    #[cfg(target_os = "macos")]
    if mods.alt && !mods.platform && !mods.control {
        match keystroke.key.as_str() {
            "left" => return Some(b"\x1bb".to_vec()),
            "right" => return Some(b"\x1bf".to_vec()),
            "backspace" => return Some(vec![0x17]),
            _ => {}
        }
    }

    // Modifier code for CSI
    let modifier_code = 1
        + (if mods.shift { 1 } else { 0 })
        + (if mods.alt { 2 } else { 0 })
        + (if mods.control { 4 } else { 0 });

    // Arrow keys
    match keystroke.key.as_str() {
        "up" | "down" | "right" | "left" => {
            let ch = match keystroke.key.as_str() {
                "up" => 'A', "down" => 'B', "right" => 'C', "left" => 'D',
                _ => unreachable!(),
            };
            if modifier_code > 1 {
                return Some(format!("\x1b[1;{}{}", modifier_code, ch).into_bytes());
            }
            return if app_cursor {
                Some(format!("\x1bO{}", ch).into_bytes())
            } else {
                Some(format!("\x1b[{}", ch).into_bytes())
            };
        }
        _ => {}
    }

    // Let InputHandler handle text-producing keystrokes
    if keystroke.key_char.is_some() {
        return None;
    }

    // Other special keys
    match keystroke.key.as_str() {
        "backspace" => Some(b"\x7f".to_vec()),
        "escape" => Some(b"\x1b".to_vec()),
        "home" => if modifier_code > 1 {
            Some(format!("\x1b[1;{}H", modifier_code).into_bytes())
        } else { Some(b"\x1b[H".to_vec()) },
        "end" => if modifier_code > 1 {
            Some(format!("\x1b[1;{}F", modifier_code).into_bytes())
        } else { Some(b"\x1b[F".to_vec()) },
        "pageup" => Some(b"\x1b[5~".to_vec()),
        "pagedown" => Some(b"\x1b[6~".to_vec()),
        "delete" => Some(b"\x1b[3~".to_vec()),
        "f1" => Some(b"\x1bOP".to_vec()),
        "f2" => Some(b"\x1bOQ".to_vec()),
        "f3" => Some(b"\x1bOR".to_vec()),
        "f4" => Some(b"\x1bOS".to_vec()),
        "f5" => Some(b"\x1b[15~".to_vec()),
        "f6" => Some(b"\x1b[17~".to_vec()),
        "f7" => Some(b"\x1b[18~".to_vec()),
        "f8" => Some(b"\x1b[19~".to_vec()),
        "f9" => Some(b"\x1b[20~".to_vec()),
        "f10" => Some(b"\x1b[21~".to_vec()),
        "f11" => Some(b"\x1b[23~".to_vec()),
        "f12" => Some(b"\x1b[24~".to_vec()),
        _ => {
            let key = keystroke.key.as_str();
            if key.len() == 1 { Some(key.as_bytes().to_vec()) } else { None }
        }
    }
}
```

**Step 2: Add module**

In `src/terminal/mod.rs`:
```rust
pub mod input;
```

**Step 3: Commit**

```bash
git add src/terminal/input.rs src/terminal/mod.rs
git commit -m "feat: add terminal input handler (key_to_bytes)"
```

---

### Task 2.7: Update TerminalBuffer and I/O pipeline

**Files:**
- Modify: `src/ui/terminal_view.rs`
- Modify: `src/terminal/stream_adapter.rs`
- Modify: `src/terminal/mod.rs`

**Step 1: Update TerminalBuffer enum**

Replace `GpuiTerminal(gpui::Entity<gpui_terminal::TerminalView>)` with the new Terminal:

```rust
// src/ui/terminal_view.rs
use std::sync::Arc;
use crate::terminal::Terminal;

pub enum TerminalBuffer {
    Empty,
    Error(String),
    /// Self-built terminal (direct alacritty_terminal + custom Element rendering)
    Terminal {
        terminal: Arc<Terminal>,
        focus_handle: gpui::FocusHandle,
    },
}
```

Update `content_for_status_detection`:
```rust
impl TerminalBuffer {
    pub fn content_for_status_detection(&self) -> Option<String> {
        match self {
            TerminalBuffer::Empty => None,
            TerminalBuffer::Error(s) => Some(s.clone()),
            TerminalBuffer::Terminal { .. } => None,
        }
    }
}
```

Update `render()` to use `TerminalElement`:
```rust
TerminalBuffer::Terminal { terminal, focus_handle } => {
    use crate::terminal::terminal_element::TerminalElement;
    use crate::terminal::colors::ColorPalette;
    div()
        .size_full()
        .child(TerminalElement::new(terminal.clone(), focus_handle.clone(), ColorPalette::default()))
        .into_any_element()
}
```

**Step 2: Simplify stream_adapter.rs**

Remove `RuntimeReader` and `RuntimeWriter`. Keep `tee_output` but simplify — the new pipeline doesn't need tee since ContentExtractor reads from Terminal state directly.

Actually, with the event-driven approach, we can remove tee entirely. The pipeline becomes:

```rust
// In app_root.rs setup:
// 1. Get output channel from runtime
let rx = runtime.subscribe_output(&pane_id).unwrap();

// 2. Create Terminal
let terminal = Arc::new(Terminal::new(pane_id.clone(), TerminalSize::default()));

// 3. Spawn output processing task
let term_clone = terminal.clone();
cx.spawn(|mut cx| async move {
    while let Ok(chunk) = rx.recv_async().await {
        term_clone.process_output(&chunk);
        let _ = cx.update(|_window, cx| cx.notify());
    }
}).detach();

// 4. ContentExtractor reads from Terminal state on dirty signal
// (handled in status publisher subscription)
```

**Step 3: Update all TerminalBuffer::GpuiTerminal match arms**

Files to update:
- `src/ui/app_root.rs` — terminal setup functions, focus handling
- `src/ui/split_pane_container.rs` — rendering
- `src/ui/terminal_area_entity.rs` — buffer storage
- `src/ui/diff_overlay.rs` — buffer storage

In each file, replace `TerminalBuffer::GpuiTerminal(entity)` with `TerminalBuffer::Terminal { terminal, focus_handle }`.

**Step 4: Update app_root.rs terminal setup**

Replace `setup_local_terminal` and `setup_pane_terminal_output`:

```rust
// Before:
//   tee_output(rx) → RuntimeReader → gpui_terminal::TerminalView
//
// After:
//   rx → Terminal.process_output() → cx.notify() → TerminalElement repaints

fn setup_terminal_for_pane(
    &mut self,
    pane_id: &str,
    runtime: &Arc<dyn AgentRuntime>,
    window: &mut Window,
    cx: &mut Context<Self>,
) {
    let rx = match runtime.subscribe_output(&PaneId::from(pane_id)) {
        Some(rx) => rx,
        None => {
            self.set_pane_buffer(pane_id, TerminalBuffer::Error("No output stream".into()));
            return;
        }
    };

    let terminal = Arc::new(Terminal::new(pane_id.to_string(), TerminalSize::default()));
    let focus_handle = cx.focus_handle();

    // Output processing task
    let term_clone = terminal.clone();
    cx.spawn_in(window, |this, mut cx| async move {
        while let Ok(chunk) = rx.recv_async().await {
            term_clone.process_output(&chunk);
            let _ = cx.update(|_window, cx| {
                // Also feed ContentExtractor from terminal state
                // ...
                cx.notify();
            });
        }
    }).detach();

    // Resize callback — notify runtime when terminal resizes
    // (handled inside TerminalElement::paint via terminal.resize() → runtime.resize())

    self.set_pane_buffer(pane_id, TerminalBuffer::Terminal { terminal, focus_handle });
}
```

**Step 5: Update input handling in app_root.rs**

Replace gpui_terminal focus/key handling:

```rust
// Before: TerminalBuffer::GpuiTerminal(entity) → entity is focused, handles keys
// After: TerminalBuffer::Terminal { terminal, focus_handle } → handle keys in app_root

// In handle_key_down or equivalent:
if let TerminalBuffer::Terminal { terminal, .. } = &buffer {
    let mode = terminal.mode();
    if let Some(bytes) = crate::terminal::input::key_to_bytes(event, mode) {
        runtime.send_input(&pane_id, &bytes).ok();
    }
}
```

**Step 6: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

**Step 7: Commit**

```bash
git add src/ui/ src/terminal/
git commit -m "feat: wire new Terminal into UI, replace gpui_terminal references"
```

---

### Task 2.8: Remove gpui-terminal dependency

**Files:**
- Modify: `Cargo.toml`
- Delete: `vendor/gpui-terminal/` (entire directory)
- Modify: `src/terminal/stream_adapter.rs` (remove RuntimeReader/RuntimeWriter)

**Step 1: Remove from Cargo.toml**

```toml
# Remove this line:
gpui-terminal = { path = "vendor/gpui-terminal" }
```

**Step 2: Remove vendor directory**

```bash
rm -rf vendor/gpui-terminal
```

**Step 3: Clean up stream_adapter.rs**

Remove `RuntimeReader`, `RuntimeWriter`. Keep only `tee_output` if still needed, or remove the file entirely if not.

**Step 4: Verify builds**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
```

Should compile with zero references to gpui_terminal.

**Step 5: Run tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test
```

**Step 6: Manual smoke test**

```bash
RUSTUP_TOOLCHAIN=stable cargo run
```

Verify:
- Terminal renders correctly (text, colors, cursor)
- Keyboard input works (typing, Ctrl+C, arrow keys)
- vim / htop / other TUI apps work
- Split panes work
- Sidebar agent status detection still works

**Step 7: Commit**

```bash
git add -A
git commit -m "feat: remove gpui-terminal vendor, terminal is now fully self-built"
```

---

### Task 2.9: Event-driven ContentExtractor

**Files:**
- Modify: `src/terminal/content_extractor.rs`
- Modify: `src/ui/app_root.rs` (status publisher wiring)

**Step 1: Update ContentExtractor to read from Terminal state**

Instead of receiving raw bytes via tee, ContentExtractor now reads from the Terminal's `with_content()`:

```rust
// In the output processing task (app_root.rs):
cx.spawn_in(window, |this, mut cx| async move {
    let mut extractor = ContentExtractor::new();
    while let Ok(chunk) = rx.recv_async().await {
        // Feed VTE parser
        terminal.process_output(&chunk);

        // Feed ContentExtractor with same bytes
        extractor.feed(&chunk);
        let shell_info = ShellPhaseInfo { phase: extractor.shell_phase(), .. };
        let (content, _) = extractor.take_content();

        let _ = cx.update(|_window, cx| {
            // Publish status
            status_publisher.check_status(&pane_id, process_status, shell_info, &content);
            cx.notify();
        });
    }
}).detach();
```

Note: ContentExtractor still receives raw bytes (for OSC 133 parsing). The "event-driven" aspect is that it runs in the same async task as the output processing, not in a separate tee thread.

**Step 2: Remove tee_output usage**

Remove calls to `tee_output()` in `app_root.rs`. The output `rx` goes directly to one task that does both terminal processing and status extraction.

**Step 3: Verify status detection works**

```bash
RUSTUP_TOOLCHAIN=stable cargo run
```

- Start an agent, verify sidebar shows Running/Waiting status correctly
- Verify OSC 133 detection (if shell integration is set up)

**Step 4: Commit**

```bash
git add src/terminal/ src/ui/app_root.rs
git commit -m "refactor: event-driven ContentExtractor, remove tee_output"
```

### Phase 2 Gate: Regression & Functional Tests

> **硬性要求**：以下所有检查项全部通过后方可进入 Phase 3。这是最关键的 Phase，渲染层完全替换，必须确保无回归。

**自动化测试**

```bash
RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -5
# Expected: test result: ok. X passed; 0 failed
```

**编译检查**

```bash
# 确认 gpui-terminal 已彻底移除
grep -r "gpui_terminal" src/ && echo "FAIL: gpui_terminal references remain" || echo "OK: clean"
ls vendor/gpui-terminal 2>/dev/null && echo "FAIL: vendor dir exists" || echo "OK: vendor removed"

# 零新 warning
RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | grep -c "warning"
```

**终端渲染回归清单**（手动，逐项确认）

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 基础文本渲染 | `echo "Hello World"` | 白色文字，等宽对齐 |
| 2 | ANSI 颜色 | `echo -e "\033[31mRed\033[32mGreen\033[34mBlue\033[0m"` | 红绿蓝正确 |
| 3 | 256 色 | `for i in {0..255}; do printf "\033[38;5;${i}m%3d " $i; done` | 256 色渐变正确 |
| 4 | Bold / Italic / Underline | `echo -e "\033[1mBold\033[0m \033[3mItalic\033[0m \033[4mUnderline\033[0m"` | 各样式正确 |
| 5 | 光标位置 | 输入文字 → 退格 → 重新输入 | 光标跟随正确 |
| 6 | 光标形状 | vim 进入 insert mode / normal mode | Bar ↔ Block 切换 |
| 7 | CJK 宽字符 | `echo "中文ABC日本語"` | 中文占 2 格，无错位 |
| 8 | Box drawing | `tree .` 或 `git log --graph` | 边框线条连续，无断裂 |
| 9 | 滚动 | `seq 1 1000` 然后滚轮上翻 | 历史内容可查看 |
| 10 | Alternate screen | `vim` → `:q` | 主/备屏正确切换，退出后恢复 |

**TUI 应用兼容性清单**

| # | 应用 | 验证方法 | 预期结果 |
|---|------|----------|----------|
| 1 | vim/nvim | 打开文件，编辑，保存退出 | 全屏渲染正确，语法高亮正常 |
| 2 | htop | `htop` | 实时刷新，颜色条正确 |
| 3 | less | `cat /etc/hosts \| less` | 分页正常，q 退出正常 |
| 4 | tmux (嵌套) | `tmux` (在 pmux 终端内) | 边框、状态栏正常 |
| 5 | fzf | `ls \| fzf` | 模糊搜索 UI 正常 |
| 6 | Claude Code | 在终端内运行 `claude` | agent UI 正常显示和交互 |

**输入回归清单**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 普通字符 | 输入 a-z, 0-9, 符号 | 正确回显 |
| 2 | Ctrl+C / Ctrl+D | 运行 `sleep 100` → Ctrl+C | 中断正常 |
| 3 | Ctrl+Z | 运行 `sleep 100` → Ctrl+Z | 挂起正常 |
| 4 | 方向键 | bash 中上下翻历史，左右移光标 | 行为正确 |
| 5 | Tab 补全 | 输入 `cd /us` + Tab | 补全为 `/usr/` |
| 6 | Alt+B / Alt+F | macOS Option+Left/Right | 按词跳转 |
| 7 | Cmd+Left / Cmd+Right | macOS Cmd+方向 | 行首/行尾 |
| 8 | Enter / Shift+Enter | 普通回车 / 多行输入 | CR vs LF |
| 9 | 中文输入 | 输入中文 (IME) | 正确插入 |
| 10 | Backspace / Delete | 删前/删后字符 | 正确删除 |

**Agent 状态检测回归**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 状态更新 | 启动 agent → 观察 sidebar | Running → Waiting 状态变化 |
| 2 | ContentExtractor | 终端输出时 sidebar 更新 | 状态及时反映 |
| 3 | OSC 133 | 配置 shell integration 后观察 | 精确状态检测 |

**性能对比**（与 Phase 1 Gate 的基线数据对比）

```bash
# 启动时间不应有明显退步
time RUSTUP_TOOLCHAIN=stable cargo run -- --help 2>/dev/null

# 二进制大小对比
ls -lh target/debug/pmux

# 快速滚动测试: seq 10000 后滚轮快速上翻
# 观察帧率是否有明显卡顿
```

---

## Phase 3: Session Backends (dtach/screen + auto-detect)

**Goal:** Add dtach and screen as session persistence backends, with automatic detection (dtach > tmux > screen > local).

### Task 3.1: Define SessionBackend enum and trait

**Files:**
- Create: `src/runtime/backends/session_backend.rs`

**Step 1: Write the session backend abstraction**

Reference: Okena's `src/terminal/session_backend.rs`.

```rust
// src/runtime/backends/session_backend.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionBackend {
    Auto,
    Dtach,
    Tmux,
    Screen,
    Local,
}

impl Default for SessionBackend {
    fn default() -> Self { Self::Auto }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBackend {
    Dtach,
    Tmux,
    Screen,
    Local,
}

impl SessionBackend {
    /// Resolve Auto to a concrete backend by checking availability
    pub fn resolve(&self) -> ResolvedBackend {
        match self {
            Self::Auto => {
                if is_dtach_available() { ResolvedBackend::Dtach }
                else if is_tmux_available() { ResolvedBackend::Tmux }
                else if is_screen_available() { ResolvedBackend::Screen }
                else { ResolvedBackend::Local }
            }
            Self::Dtach => if is_dtach_available() { ResolvedBackend::Dtach } else { ResolvedBackend::Local },
            Self::Tmux => if is_tmux_available() { ResolvedBackend::Tmux } else { ResolvedBackend::Local },
            Self::Screen => if is_screen_available() { ResolvedBackend::Screen } else { ResolvedBackend::Local },
            Self::Local => ResolvedBackend::Local,
        }
    }
}

impl ResolvedBackend {
    pub fn supports_persistence(&self) -> bool {
        !matches!(self, Self::Local)
    }

    /// Generate session name from terminal/pane ID
    pub fn session_name(&self, terminal_id: &str) -> String {
        let short = &terminal_id[..8.min(terminal_id.len())];
        match self {
            Self::Dtach => format!("pmux-{}", short),
            Self::Tmux => format!("pmux-{}", short),
            Self::Screen => format!("pmux-{}", short),
            Self::Local => String::new(),
        }
    }

    /// Build command args for spawning with this backend
    pub fn build_command(&self, session_name: &str, cwd: &str) -> Option<(String, Vec<String>)> {
        match self {
            Self::Dtach => {
                let socket = dtach_socket_path(session_name);
                Some(("dtach".into(), vec![
                    "-A".into(), socket.to_string_lossy().into(),
                    "-z".into(),  // no suspend
                    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into()),
                ]))
            }
            Self::Tmux => {
                // Use tmux control mode for structured I/O
                // This delegates to TmuxControlModeRuntime
                None
            }
            Self::Screen => {
                Some(("screen".into(), vec![
                    "-dmS".into(), session_name.into(),
                    "-c".into(), "/dev/null".into(),
                ]))
            }
            Self::Local => None,
        }
    }

    /// Kill a session
    pub fn kill_session(&self, session_name: &str) {
        match self {
            Self::Dtach => {
                let socket = dtach_socket_path(session_name);
                let _ = std::fs::remove_file(socket);
            }
            Self::Tmux => {
                let _ = std::process::Command::new("tmux")
                    .args(["kill-session", "-t", session_name])
                    .output();
            }
            Self::Screen => {
                let _ = std::process::Command::new("screen")
                    .args(["-S", session_name, "-X", "quit"])
                    .output();
            }
            Self::Local => {}
        }
    }
}

fn is_dtach_available() -> bool {
    std::process::Command::new("dtach").arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status().is_ok()
}

fn is_tmux_available() -> bool {
    std::process::Command::new("tmux").arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status().is_ok()
}

fn is_screen_available() -> bool {
    std::process::Command::new("screen").arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status().is_ok()
}

fn dtach_socket_path(session_name: &str) -> PathBuf {
    let dir = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("pmux");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}.sock", session_name))
}
```

**Step 2: Add module**

In `src/runtime/backends/mod.rs`:
```rust
pub mod session_backend;
pub use session_backend::{SessionBackend, ResolvedBackend};
```

**Step 3: Commit**

```bash
git add src/runtime/backends/session_backend.rs src/runtime/backends/mod.rs
git commit -m "feat: add SessionBackend enum with dtach/tmux/screen/local + auto-detect"
```

---

### Task 3.2: Implement DtachBackend

**Files:**
- Create: `src/runtime/backends/dtach.rs`

**Step 1: Write DtachBackend**

dtach is simpler than tmux — it provides session persistence for a single PTY. The runtime wraps a local PTY that runs under dtach.

```rust
// src/runtime/backends/dtach.rs

use crate::runtime::{AgentRuntime, PaneId, RuntimeError};
use crate::runtime::backends::session_backend::ResolvedBackend;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct DtachRuntime {
    worktree_path: String,
    panes: Mutex<HashMap<PaneId, DtachPane>>,
    session_backend: ResolvedBackend,
}

struct DtachPane {
    pane_id: PaneId,
    input_tx: flume::Sender<Vec<u8>>,
    output_rx: Mutex<Option<flume::Receiver<Vec<u8>>>>,
    master: Box<dyn portable_pty::MasterPty>,
}

impl DtachRuntime {
    pub fn new(worktree_path: &str, cols: u16, rows: u16) -> Result<Self, RuntimeError> {
        let runtime = Self {
            worktree_path: worktree_path.to_string(),
            panes: Mutex::new(HashMap::new()),
            session_backend: ResolvedBackend::Dtach,
        };
        // Create primary pane
        runtime.create_pane("0", cols, rows)?;
        Ok(runtime)
    }

    fn create_pane(&self, pane_suffix: &str, cols: u16, rows: u16) -> Result<PaneId, RuntimeError> {
        let pane_id = format!("dtach:{}", pane_suffix);
        let session_name = self.session_backend.session_name(&pane_id);

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| RuntimeError::PtyError(e.to_string()))?;

        // Build dtach command
        let socket_path = crate::runtime::backends::session_backend::dtach_socket_path(&session_name);
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut cmd = CommandBuilder::new("dtach");
        cmd.arg("-A").arg(socket_path.to_string_lossy().as_ref());
        cmd.arg("-z");
        cmd.arg(&shell);
        cmd.cwd(&self.worktree_path);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let _child = pair.slave.spawn_command(cmd)
            .map_err(|e| RuntimeError::PtyError(e.to_string()))?;

        let reader = pair.master.try_clone_reader()
            .map_err(|e| RuntimeError::PtyError(e.to_string()))?;
        let writer = pair.master.take_writer()
            .map_err(|e| RuntimeError::PtyError(e.to_string()))?;

        let (output_tx, output_rx) = flume::unbounded();
        let (input_tx, input_rx) = flume::unbounded::<Vec<u8>>();

        // Reader thread
        std::thread::spawn({
            let tx = output_tx;
            let mut reader = reader;
            move || {
                let mut buf = [0u8; 65536];
                loop {
                    match std::io::Read::read(&mut reader, &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send(buf[..n].to_vec()).is_err() { break; }
                        }
                    }
                }
            }
        });

        // Writer thread
        std::thread::spawn({
            let mut writer = writer;
            move || {
                while let Ok(data) = input_rx.recv() {
                    if std::io::Write::write_all(&mut writer, &data).is_err() { break; }
                }
            }
        });

        self.panes.lock().unwrap().insert(pane_id.clone(), DtachPane {
            pane_id: pane_id.clone(),
            input_tx,
            output_rx: Mutex::new(Some(output_rx)),
            master: pair.master,
        });

        Ok(pane_id)
    }
}

impl AgentRuntime for DtachRuntime {
    fn backend_type(&self) -> &'static str { "dtach" }

    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError> {
        if let Some(pane) = self.panes.lock().unwrap().get(pane_id) {
            pane.input_tx.send(bytes.to_vec()).map_err(|e| RuntimeError::IoError(e.to_string()))
        } else {
            Err(RuntimeError::PaneNotFound(pane_id.clone()))
        }
    }

    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>> {
        self.panes.lock().unwrap().get(pane_id)
            .and_then(|p| p.output_rx.lock().unwrap().take())
    }

    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        if let Some(pane) = self.panes.lock().unwrap().get(pane_id) {
            pane.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .map_err(|e| RuntimeError::IoError(e.to_string()))
        } else {
            Err(RuntimeError::PaneNotFound(pane_id.clone()))
        }
    }

    // ... implement remaining AgentRuntime methods (list_panes, focus_pane, etc.)
    // Most can delegate to a simple single-pane implementation similar to LocalPtyRuntime
}
```

**Step 2: Add module**

**Step 3: Commit**

```bash
git add src/runtime/backends/dtach.rs src/runtime/backends/mod.rs
git commit -m "feat: add DtachRuntime session backend"
```

---

### Task 3.3: Implement ScreenBackend

**Files:**
- Create: `src/runtime/backends/screen.rs`

Similar pattern to DtachBackend but using `screen -dmS <name>` to create a detached session and `screen -r <name>` to reattach. Key differences:

- `screen -dmS pmux-xxx /bin/zsh` creates a detached session
- `screen -r pmux-xxx` reattaches
- `screen -S pmux-xxx -X stuff "input\n"` sends input
- `screen -S pmux-xxx -X quit` kills session

Since screen doesn't have a clean output streaming API like tmux control mode, the screen backend spawns the shell inside a PTY that's wrapped by screen, similar to dtach.

**Step 1: Implement ScreenRuntime** (same pattern as DtachRuntime)

**Step 2: Commit**

```bash
git add src/runtime/backends/screen.rs src/runtime/backends/mod.rs
git commit -m "feat: add ScreenRuntime session backend"
```

---

### Task 3.4: Update backend factory with auto-detection

**Files:**
- Modify: `src/runtime/backends/mod.rs`
- Modify: `src/config.rs`

**Step 1: Add `session_backend` to Config**

```rust
// In config.rs
pub struct Config {
    // ... existing fields ...
    pub session_backend: SessionBackend,  // default: Auto
}
```

**Step 2: Update `resolve_backend()` and `create_runtime_from_env()`**

```rust
// In src/runtime/backends/mod.rs
pub fn create_runtime_from_env(
    workspace_path: &str,
    worktree_path: &str,
    branch_name: &str,
    cols: u16,
    rows: u16,
    config: Option<&Config>,
) -> Result<RuntimeCreationResult, RuntimeError> {
    let session_backend = config
        .map(|c| c.session_backend)
        .unwrap_or_default();
    let resolved = session_backend.resolve();

    log::info!("Session backend: {:?} (resolved from {:?})", resolved, session_backend);

    match resolved {
        ResolvedBackend::Dtach => {
            let runtime = DtachRuntime::new(worktree_path, cols, rows)?;
            Ok(RuntimeCreationResult { runtime: Arc::new(runtime), .. })
        }
        ResolvedBackend::Tmux => {
            // Existing TmuxControlModeRuntime
            let runtime = TmuxControlModeRuntime::new(..)?;
            Ok(RuntimeCreationResult { runtime: Arc::new(runtime), .. })
        }
        ResolvedBackend::Screen => {
            let runtime = ScreenRuntime::new(worktree_path, cols, rows)?;
            Ok(RuntimeCreationResult { runtime: Arc::new(runtime), .. })
        }
        ResolvedBackend::Local => {
            let runtime = LocalPtyAgent::new(worktree_path, cols, rows)?;
            Ok(RuntimeCreationResult { runtime: Arc::new(runtime), .. })
        }
    }
}
```

**Step 3: Verify**

```bash
RUSTUP_TOOLCHAIN=stable cargo test
RUSTUP_TOOLCHAIN=stable cargo run
```

Test with:
- `PMUX_BACKEND=dtach cargo run` — verify dtach session is created
- `PMUX_BACKEND=screen cargo run` — verify screen session is created
- Default auto: verify auto-detection picks the best available

**Step 4: Commit**

```bash
git add src/runtime/backends/ src/config.rs
git commit -m "feat: auto-detect session backend (dtach > tmux > screen > local)"
```

### Phase 3 Gate: Regression & Functional Tests

> **硬性要求**：以下所有检查项全部通过后方可进入 Phase 4。重点验证所有 backend 的创建、持久化、恢复。

**自动化测试**

```bash
RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -5
# Expected: test result: ok. X passed; 0 failed
```

**Session Backend 功能测试**

| # | Backend | 测试项 | 验证方法 | 预期结果 |
|---|---------|--------|----------|----------|
| 1 | dtach | 创建 session | `PMUX_BACKEND=dtach cargo run` → 打开 workspace | 终端正常，`dtach` 进程存在 |
| 2 | dtach | 持久化 | 关闭 pmux → 重开 | 重新 attach，历史内容可见 |
| 3 | dtach | socket 文件 | `ls ~/.cache/pmux/` 或 runtime dir | `.sock` 文件存在 |
| 4 | tmux | 创建 session | `PMUX_BACKEND=tmux cargo run` → 打开 workspace | tmux control mode 正常 |
| 5 | tmux | 持久化 | 关闭 pmux → `tmux ls` | session 存活 |
| 6 | tmux | 恢复 | 重开 pmux | 自动 attach，内容恢复 |
| 7 | screen | 创建 session | `PMUX_BACKEND=screen cargo run` → 打开 workspace | screen session 存在 |
| 8 | screen | 持久化 | 关闭 pmux → `screen -ls` | session 列表中可见 |
| 9 | local | 无持久化 | `PMUX_BACKEND=local cargo run` → 关闭 → 重开 | 新 session，无恢复 |
| 10 | auto | 自动检测 | 默认 `cargo run`（不设 PMUX_BACKEND） | 按优先级选择最优 backend |

**自动检测优先级验证**

```bash
# 验证检测逻辑（查看日志输出）
RUST_LOG=info RUSTUP_TOOLCHAIN=stable cargo run 2>&1 | grep -i "session backend"
# Expected: "Session backend: Dtach (resolved from Auto)" 或类似

# 强制 fallback 测试：临时 rename dtach binary
# (如果 dtach 已安装) 验证 fallback 到 tmux
```

**Phase 2 回归（确保新 backend 没有破坏已有功能）**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 终端渲染 | 各 backend 下 `echo "Hello"` | 文字正常 |
| 2 | 键盘输入 | 各 backend 下输入命令 | 输入正常 |
| 3 | vim 兼容 | 各 backend 下运行 vim | TUI 正常 |
| 4 | 分屏 | 各 backend 下 ⌘D | 分屏正常工作 |
| 5 | Agent 状态 | 各 backend 下启动 agent | sidebar 状态正确 |

**清理测试**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | dtach kill | 关闭 workspace | socket 文件被清理 |
| 2 | tmux kill | 关闭 workspace | tmux session 被 kill |
| 3 | screen kill | 关闭 workspace | screen session 被终止 |

---

## Phase 4: Terminal Enhancements

**Goal:** Add search, URL detection, and focus fog to the self-built terminal.

### Task 4.1: Terminal search (Cmd+F)

**Files:**
- Modify: `src/terminal/terminal_core.rs` (add search state)
- Modify: `src/terminal/terminal_element.rs` (render search highlights)
- Modify: `src/ui/app_root.rs` (search UI overlay)

**Step 1: Add search state to Terminal**

```rust
// In terminal_core.rs
pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current_index: Option<usize>,
}

pub struct SearchMatch {
    pub line: i32,
    pub col: usize,
    pub len: usize,
}
```

**Step 2: Add search method**

```rust
impl Terminal {
    pub fn search(&self, query: &str) -> Vec<SearchMatch> {
        self.with_content(|term| {
            let grid = term.grid();
            let mut matches = Vec::new();
            // Iterate visible lines, find query in text
            for row in 0..grid.screen_lines() {
                let mut line_text = String::new();
                for col in 0..grid.columns() {
                    let cell = &grid[alacritty_terminal::index::Point {
                        line: Line(row as i32),
                        column: Column(col),
                    }];
                    line_text.push(cell.c);
                }
                // Find all occurrences
                let mut start = 0;
                while let Some(pos) = line_text[start..].find(query) {
                    matches.push(SearchMatch {
                        line: row as i32,
                        col: start + pos,
                        len: query.len(),
                    });
                    start += pos + 1;
                }
            }
            matches
        })
    }
}
```

**Step 3: Render search highlights in TerminalElement::paint()**

After painting text, paint search match overlays:

```rust
// In terminal_element.rs paint():
if let Some(search) = &self.search_state {
    for (idx, m) in search.matches.iter().enumerate() {
        let is_current = search.current_index == Some(idx);
        let color = if is_current {
            Hsla::from(Rgba { r: 1.0, g: 0.6, b: 0.0, a: 0.7 })
        } else {
            Hsla::from(Rgba { r: 1.0, g: 1.0, b: 0.0, a: 0.4 })
        };
        let pos = point(
            origin.x + m.col as f32 * cell_width,
            origin.y + m.line as f32 * line_height,
        );
        window.paint_quad(fill(
            Bounds::new(pos, size(cell_width * m.len as f32, line_height)),
            color,
        ));
    }
}
```

**Step 4: Commit**

```bash
git add src/terminal/ src/ui/
git commit -m "feat: terminal search with Cmd+F"
```

---

### Task 4.2: URL detection and hover

**Files:**
- Modify: `src/terminal/terminal_core.rs` (URL detection)
- Modify: `src/terminal/terminal_element.rs` (URL rendering, hover)

**Step 1: Add URL detection**

Reference: Okena's `DetectedLink` and `detect_links()`.

```rust
// In terminal_core.rs
pub struct DetectedLink {
    pub line: i32,
    pub col: usize,
    pub len: usize,
    pub text: String,
    pub is_url: bool,
}

impl Terminal {
    pub fn detect_links(&self) -> Vec<DetectedLink> {
        self.with_content(|term| {
            let grid = term.grid();
            let mut links = Vec::new();
            let url_regex = regex::Regex::new(r"https?://[^\s<>\[\]{}|\\^`]+").unwrap();
            let file_regex = regex::Regex::new(r"(?:^|\s)(/[^\s:]+|\.{1,2}/[^\s:]+)(?::(\d+))?(?::(\d+))?").unwrap();

            for row in 0..grid.screen_lines() {
                let mut line_text = String::new();
                for col in 0..grid.columns() {
                    let cell = &grid[alacritty_terminal::index::Point {
                        line: Line(row as i32),
                        column: Column(col),
                    }];
                    line_text.push(cell.c);
                }

                for m in url_regex.find_iter(&line_text) {
                    links.push(DetectedLink {
                        line: row as i32,
                        col: m.start(),
                        len: m.len(),
                        text: m.as_str().to_string(),
                        is_url: true,
                    });
                }
            }
            links
        })
    }
}
```

**Step 2: Render URL underlines in TerminalElement**

Dotted underline for detected URLs, solid underline + bg highlight on hover.

**Step 3: Handle Cmd+Click to open URLs**

```rust
// In mouse handler:
if event.modifiers.platform {
    if let Some(link) = find_link_at(col, row) {
        if link.is_url {
            open::that(&link.text).ok();
        }
    }
}
```

**Step 4: Commit**

```bash
git add src/terminal/
git commit -m "feat: URL detection with hover underline and Cmd+Click to open"
```

---

### Task 4.3: Focus fog effect

**Files:**
- Modify: `src/terminal/terminal_element.rs`

**Step 1: Add fog overlay for unfocused terminals**

At the end of `paint()`, after all terminal content:

```rust
// Phase 5: Fog overlay for unfocused terminals
let is_focused = self.focus_handle.is_focused(window);
if !is_focused {
    let bg_rgba = palette.background();
    // Semi-transparent overlay using the same background color
    // dims text content without affecting the background itself
    let fog = Hsla { h: bg_rgba.h, s: bg_rgba.s, l: bg_rgba.l, a: 0.2 };
    window.paint_quad(fill(bounds, fog));
}
```

**Step 2: Commit**

```bash
git add src/terminal/terminal_element.rs
git commit -m "feat: focus fog effect for unfocused terminal panes"
```

### Phase 4 Gate: Regression & Functional Tests

> **硬性要求**：以下所有检查项全部通过后视为整个重构完成。

**自动化测试**

```bash
RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -5
# Expected: test result: ok. X passed; 0 failed
```

**搜索功能测试**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 打开搜索 | ⌘F | 搜索框出现 |
| 2 | 基础搜索 | 输入 `hello` | 匹配项高亮显示 |
| 3 | 多匹配 | 终端中有多个 `hello` | 所有匹配高亮，当前匹配特殊高亮 |
| 4 | 上/下导航 | Enter / Shift+Enter | 在匹配项间跳转 |
| 5 | 关闭搜索 | Escape | 搜索框消失，高亮清除 |
| 6 | 空结果 | 搜索不存在的文字 | 无高亮，无 panic |

**URL 检测测试**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | HTTP URL | `echo "https://github.com"` | URL 下方有下划线 |
| 2 | 悬浮效果 | 鼠标悬浮在 URL 上 | 背景高亮 + 实线下划线 |
| 3 | Cmd+Click | Cmd+单击 URL | 浏览器打开该 URL |
| 4 | 文件路径 | `echo "/usr/local/bin/zsh"` | 检测为文件路径 |
| 5 | 带行号路径 | `echo "src/main.rs:42:10"` | 检测为 file:line:col |
| 6 | 无 URL | `echo "no links here"` | 无下划线、无检测 |
| 7 | 长 URL 换行 | 输出超长 URL | 正确识别整个 URL |

**焦点雾化测试**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 单 pane | 单个终端，有焦点 | 无雾化效果 |
| 2 | 分屏焦点 | ⌘D 分屏 → 点击另一个 pane | 非焦点 pane 有半透明覆盖 |
| 3 | 焦点切换 | 在 pane 间切换焦点 | 雾化跟随焦点变化 |
| 4 | 文字可读性 | 非焦点 pane | 文字变淡但仍可辨认 |

**全量回归（Phase 1-3 功能不退步）**

| # | 测试项 | 验证方法 | 预期结果 |
|---|--------|----------|----------|
| 1 | 终端渲染 | Phase 2 渲染清单全部重跑 | 全部通过 |
| 2 | TUI 兼容 | vim / htop / less / fzf | 全部正常 |
| 3 | 输入处理 | Phase 2 输入清单全部重跑 | 全部通过 |
| 4 | Session backends | 各 backend 启动 + 恢复 | 全部正常 |
| 5 | Agent 状态 | ContentExtractor + StatusPublisher | 状态检测正确 |
| 6 | 性能 | `seq 10000` + 快速滚动 | 无明显卡顿 |

**最终性能对比**

```bash
# 与 Phase 1 基线数据对比
time RUSTUP_TOOLCHAIN=stable cargo run -- --help 2>/dev/null
ls -lh target/debug/pmux

# 输入延迟: 快速连续输入 "abcdefghij"，观察是否逐字即时回显
# 滚动性能: seq 10000 后快速滚轮，观察帧率
```

---

## Success Criteria

- [ ] gpui-terminal vendor directory completely removed
- [ ] GPUI tracks Zed main branch
- [ ] Terminal renders correctly (text, colors, cursor, CJK, bold/italic/underline)
- [ ] vim / htop / other TUI apps work correctly
- [ ] Keyboard input works (all special keys, Ctrl+, Alt+, macOS Cmd+)
- [ ] Agent status detection works (ContentExtractor + StatusPublisher)
- [ ] Session backends: dtach, tmux (control mode), screen, local — all work
- [ ] Auto-detection picks best available backend
- [ ] Terminal search with Cmd+F
- [ ] URL detection with hover and Cmd+Click
- [ ] Focus fog for unfocused panes
- [ ] `cargo test` passes
- [ ] No performance regression vs current implementation

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| GPUI main has breaking API changes | Phase 1 is isolated; fix before Phase 2 |
| `Element` trait API differs | Check actual Zed main `Element` trait, adjust signatures |
| Performance regression | BatchedTextRun + LayoutRect matching Zed pattern; viewport culling if needed |
| dtach not widely installed | Auto-detection falls back; dtach is `brew install dtach` |
| screen session handling quirks | Test on Linux; screen is more common there |
