// ui/terminal_view.rs - Terminal view component with GPUI render
// Renders via renderable_content().display_iter() with style-run batching.
use crate::terminal::TerminalEngine;
use crate::ui::terminal_rendering::{group_cells_into_segments, hash_row_content, render_batch_row, StyledSegment};
use alacritty_terminal::term::cell::Flags;
use gpui::prelude::*;
use gpui::*;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

/// Default row cache size (Task 4.4). Configurable via config in future.
pub const DEFAULT_ROW_CACHE_SIZE: usize = 200;

/// Content source for TerminalView - streaming (TerminalEngine) or error placeholder (Error).
#[derive(Clone)]
pub enum TerminalBuffer {
    /// Error: static message when streaming unavailable (no screen snapshot)
    Error(String),
    /// Streaming: TerminalEngine (byte processor with is_tui_active) + row cache for rendering optimization
    Term(Arc<TerminalEngine>, Arc<Mutex<LruCache<u64, Vec<StyledSegment>>>>),
}

fn extract_text_from_display_iter<'a>(
    display_iter: impl Iterator<Item = alacritty_terminal::grid::Indexed<&'a alacritty_terminal::term::cell::Cell>>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut current_row: i32 = i32::MIN;
    for indexed in display_iter {
        if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let row = indexed.point.line.0;
        if row != current_row {
            if !current_line.is_empty() {
                lines.push(current_line.trim_end().to_string());
            }
            current_line = String::new();
            current_row = row;
        }
        current_line.push(indexed.cell.c);
    }
    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }
    lines.join("\n")
}

impl TerminalBuffer {
    /// Create a Term buffer with row cache. Use for new panes.
    /// `cache_size`: LRU capacity, default 200. Uses config.terminal_row_cache_size when set.
    pub fn new_term(engine: TerminalEngine) -> Self {
        Self::new_term_with_cache_size(Arc::new(engine), DEFAULT_ROW_CACHE_SIZE)
    }

    /// Create a Term buffer with configurable row cache size.
    /// Accepts Arc<TerminalEngine> so the same engine can be used for byte processing (e.g. advance_bytes) and rendering.
    pub fn new_term_with_cache_size(engine: Arc<TerminalEngine>, cache_size: usize) -> Self {
        let cap = NonZeroUsize::new(cache_size.max(1)).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self::Term(
            engine,
            Arc::new(Mutex::new(LruCache::new(cap))),
        )
    }

    /// Create an empty Term buffer (no byte stream). Use for placeholders when pane has no buffer yet.
    pub fn new_empty_term(cols: usize, rows: usize) -> Self {
        let (_tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(cols, rows, rx);
        Self::new_term_with_cache_size(Arc::new(engine), DEFAULT_ROW_CACHE_SIZE)
    }

    /// Extract text for status detection. Source: stream (Term) only—never capture-pane.
    /// Uses try_renderable_content to avoid blocking.
    pub fn content_for_status_detection(&self) -> Option<String> {
        match self {
            TerminalBuffer::Term(engine, _) => {
                // try_renderable_content returns Option<String>;
                // None means lock failed (advance_bytes thread has the lock)
                engine.try_renderable_content(|_content, display_iter, _screen_lines| {
                    extract_text_from_display_iter(display_iter)
                })
            }
            TerminalBuffer::Error(s) => Some(s.clone()),
        }
    }
}

/// Terminal view component - renders tmux pane content
pub struct TerminalView {
    pane_id: String,
    title: String,
    buffer: TerminalBuffer,
    scroll_offset: usize,
    /// When true, show a blinking cursor at end of last line (indicates ready for input)
    is_focused: bool,
    /// When true (and focused), cursor is visible; when false, hidden (blink off phase)
    cursor_visible: bool,
}

impl TerminalView {
    pub fn new(pane_id: &str, title: &str) -> Self {
        Self {
            pane_id: pane_id.to_string(),
            title: title.to_string(),
            buffer: TerminalBuffer::new_empty_term(80, 24),
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Create with TerminalEngine (for pipe-pane / control mode streaming)
    pub fn with_engine(pane_id: &str, title: &str, engine: Arc<TerminalEngine>) -> Self {
        let cap = NonZeroUsize::new(DEFAULT_ROW_CACHE_SIZE).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            pane_id: pane_id.to_string(),
            title: title.to_string(),
            buffer: TerminalBuffer::Term(engine, Arc::new(Mutex::new(LruCache::new(cap)))),
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Create with a TerminalBuffer (Legacy or Term)
    pub fn with_buffer(pane_id: &str, title: &str, buffer: TerminalBuffer) -> Self {
        Self {
            pane_id: pane_id.to_string(),
            title: title.to_string(),
            buffer,
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Set whether this pane is focused (shows cursor when true)
    pub fn with_focused(mut self, focused: bool) -> Self {
        self.is_focused = focused;
        self
    }

    /// Set cursor visibility (for blink: true=on, false=off)
    pub fn with_cursor_visible(mut self, visible: bool) -> Self {
        self.cursor_visible = visible;
        self
    }

    pub fn pane_id(&self) -> &str { &self.pane_id }
    pub fn title(&self) -> &str { &self.title }
    pub fn set_title(&mut self, title: &str) { self.title = title.to_string(); }
    pub fn scroll_up(&mut self, lines: usize) { self.scroll_offset = self.scroll_offset.saturating_add(lines); }
    pub fn scroll_down(&mut self, lines: usize) { self.scroll_offset = self.scroll_offset.saturating_sub(lines); }
    pub fn reset_scroll(&mut self) { self.scroll_offset = 0; }

    fn should_show_cursor(&self) -> bool {
        let tui_active = match &self.buffer {
            TerminalBuffer::Term(engine, _) => engine.is_tui_active(),
            TerminalBuffer::Error(_) => false,
        };
        !tui_active && self.is_focused && self.cursor_visible
    }

    #[cfg(test)]
    pub fn test_should_show_cursor(&self) -> bool {
        self.should_show_cursor()
    }

    fn render_error(&self, msg: &str) -> Vec<AnyElement> {
        let lines: Vec<String> = msg.lines().map(|s| s.to_string()).collect();
        let count = lines.len();
        let lines_to_show = 50;
        let start_idx = count.saturating_sub(lines_to_show + self.scroll_offset);
        let end_idx = count.saturating_sub(self.scroll_offset);
        let visible: Vec<String> = lines.get(start_idx..end_idx).unwrap_or(&[]).to_vec();
        visible
            .iter()
            .map(|line_text| {
                let text: String = if line_text.is_empty() { " ".into() } else { line_text.clone() };
                render_batch_row(
                    vec![crate::ui::terminal_rendering::StyledSegment {
                        text,
                        fg: alacritty_terminal::vte::ansi::Rgb { r: 0xab, g: 0xb2, b: 0xbf },
                        bg: alacritty_terminal::vte::ansi::Rgb { r: 0x28, g: 0x2c, b: 0x34 },
                        flags: Flags::empty(),
                    }],
                    None,
                    false,
                )
            })
            .collect()
    }

    /// Renders visible terminal rows from display_iter with viewport culling and row-level caching.
    ///
    /// **Pipeline**: (1) Viewport culling: only rows in [visible_start, visible_end) are collected.
    /// (2) For each visible row: group cells into segments via `group_cells_into_segments()`.
    /// (3) Cache: non-cursor rows are cached by content hash; cache hits skip segment rebuild.
    /// (4) Each row → `render_batch_row()` → one flex row element. Cursor row is not cached
    /// (cursor position changes frequently).
    fn render_from_display_iter<'a>(
        &self,
        content: &alacritty_terminal::term::RenderableContent<'_>,
        display_iter: impl Iterator<Item = alacritty_terminal::grid::Indexed<&'a alacritty_terminal::term::cell::Cell>>,
        screen_lines: usize,
        cache: &mut LruCache<u64, Vec<StyledSegment>>,
    ) -> Vec<AnyElement> {
        let visible_start = self.scroll_offset;
        let visible_end = visible_start.saturating_add(screen_lines);

        let mut row_cells: Vec<Vec<alacritty_terminal::grid::Indexed<&'a alacritty_terminal::term::cell::Cell>>> =
            Vec::new();
        let mut current_row_cells: Vec<alacritty_terminal::grid::Indexed<&'a alacritty_terminal::term::cell::Cell>> =
            Vec::new();
        let mut current_row: i32 = i32::MIN;
        let mut viewport_line: usize = 0;

        for indexed in display_iter {
            if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let row = indexed.point.line.0 as i32;

            if row != current_row {
                if !current_row_cells.is_empty() {
                    if visible_start <= viewport_line && viewport_line < visible_end {
                        row_cells.push(std::mem::take(&mut current_row_cells));
                    } else {
                        current_row_cells.clear();
                    }
                }
                if current_row != i32::MIN {
                    viewport_line = viewport_line.saturating_add(1);
                }
                current_row = row;
            }

            if visible_start <= viewport_line && viewport_line < visible_end {
                current_row_cells.push(indexed);
            }
        }

        if !current_row_cells.is_empty() && visible_start <= viewport_line && viewport_line < visible_end {
            row_cells.push(current_row_cells);
        }

        let cursor_line = content.cursor.point.line.0;
        let cursor_col = content.cursor.point.column.0;
        let show_cursor = self.should_show_cursor();
        // #region agent log
        let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/matt.chow/workspace/pmux/.cursor/debug-12d1c8.log").and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{{\"sessionId\":\"12d1c8\",\"id\":\"log_{}\",\"timestamp\":{},\"location\":\"terminal_view.rs:251\",\"message\":\"cursor position from term\",\"data\":{{\"cursor_line\":{},\"cursor_col\":{},\"show_cursor\":{},\"hypothesis\":\"A\"}},\"runId\":\"debug1\",\"hypothesisId\":\"A\"}}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(), cursor_line, cursor_col, show_cursor)
        });
        // #endregion

        let result: Vec<AnyElement> = row_cells
            .into_iter()
            .enumerate()
            .map(|(_idx, cells)| {
                let row_line = cells.first().map(|c| c.point.line.0).unwrap_or(0);
                let is_cursor_row = row_line == cursor_line;

                let segments = group_cells_into_segments(cells.into_iter(), content.colors);

                let cursor = if show_cursor && is_cursor_row {
                    Some(cursor_col)
                } else {
                    None
                };
                let show_cursor_on_row = show_cursor && is_cursor_row;

                // Task 4.3: cache non-cursor rows (cursor row changes frequently)
                if !show_cursor_on_row {
                    let content_hash = hash_row_content(&segments);
                    if let Some(cached_segments) = cache.get(&content_hash) {
                        return render_batch_row(cached_segments.clone(), None, false);
                    }
                    cache.put(content_hash, segments.clone());
                }

                let row = render_batch_row(segments, cursor, show_cursor_on_row);

                row
            })
            .collect();

        result
    }
}

impl IntoElement for TerminalView {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element { Component::new(self) }
}

impl RenderOnce for TerminalView {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let line_elements: Vec<AnyElement> = match &self.buffer {
            TerminalBuffer::Error(msg) => self.render_error(msg),
            TerminalBuffer::Term(engine, cache) => {
                let mut cache_guard = cache.lock().unwrap();

                // Use try_renderable_content to avoid deadlock with advance_bytes thread
                let result = engine.try_renderable_content(|content, display_iter, screen_lines| {
                    self.render_from_display_iter(content, display_iter, screen_lines, &mut cache_guard)
                });

                result.unwrap_or_else(Vec::new)
            }
        };
        div()
            .id("terminal-view")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(0x282c34))
            .text_color(rgb(0xabb2bf))
            .font_family("Menlo")
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(8.))
                    .py(px(6.))
                    .bg(rgb(0x2e343e))
                    .border_b_1()
                    .border_color(rgb(0x3d3d3d))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x999999))
                            .child(format!("🖥 {}", self.title)),
                    ),
            )
            .child(
                div()
                    .id("terminal-content")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .p(px(4.))
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .children(line_elements),
            )
    }
}

impl Default for TerminalView {
    fn default() -> Self { Self::new("default", "Terminal") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_view_creation() {
        let view = TerminalView::new("session:0.0", "main");
        assert_eq!(view.pane_id(), "session:0.0");
        assert_eq!(view.title(), "main");
    }

    #[test]
    fn test_terminal_view_scroll() {
        let mut view = TerminalView::new("pane-1", "zsh");
        assert_eq!(view.scroll_offset, 0);
        view.scroll_up(5);
        assert_eq!(view.scroll_offset, 5);
        view.scroll_down(2);
        assert_eq!(view.scroll_offset, 3);
        view.reset_scroll();
        assert_eq!(view.scroll_offset, 0);
    }

    #[test]
    fn test_terminal_view_set_title() {
        let mut view = TerminalView::new("pane-1", "old");
        view.set_title("new");
        assert_eq!(view.title(), "new");
    }

    #[test]
    fn test_buffer_content_for_status_detection() {
        let buf = TerminalBuffer::Error("Streaming unavailable".to_string());
        assert_eq!(buf.content_for_status_detection(), Some("Streaming unavailable".to_string()));
    }

    #[test]
    fn test_cursor_hidden_when_tui_active() {
        // Enter alternate screen (vim/neovim)
        let (tx, rx) = flume::unbounded();
        let engine_tui = Arc::new(TerminalEngine::new(80, 24, rx));
        tx.send(b"\x1b[?1049h".to_vec()).unwrap();
        engine_tui.advance_bytes();
        drop(tx);
        let buffer = TerminalBuffer::new_term_with_cache_size(engine_tui, DEFAULT_ROW_CACHE_SIZE);
        let view = TerminalView::with_buffer("pane-1", "vim", buffer)
            .with_focused(true)
            .with_cursor_visible(true);
        assert!(!view.test_should_show_cursor(), "cursor should be hidden when TUI (alt screen) is active");
    }

    #[test]
    fn test_cursor_shows_when_normal_shell() {
        let view = TerminalView::new("pane-1", "zsh")
            .with_focused(true)
            .with_cursor_visible(true);
        assert!(view.test_should_show_cursor(), "cursor should show when focused in normal shell");
    }

    #[test]
    fn test_cursor_hidden_when_not_focused() {
        let view = TerminalView::new("pane-1", "zsh")
            .with_focused(false)
            .with_cursor_visible(true);
        assert!(!view.test_should_show_cursor(), "cursor should be hidden when pane not focused");
    }
}
