// ui/terminal_view.rs - Terminal view component with GPUI render
use crate::terminal::TermBridge;
use gpui::prelude::*;
use gpui::*;
use std::sync::{Arc, Mutex};

/// Terminal content representation
#[derive(Clone)]
pub struct TerminalContent {
    pub lines: Vec<TerminalLine>,
    pub cursor_position: Option<(usize, usize)>,
}

impl TerminalContent {
    pub fn new() -> Self {
        Self { lines: Vec::new(), cursor_position: None }
    }

    pub fn from_string(content: &str) -> Self {
        Self {
            lines: content.lines().map(TerminalLine::new).collect(),
            cursor_position: None,
        }
    }

    pub fn line_count(&self) -> usize { self.lines.len() }

    pub fn to_string(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    pub fn update(&mut self, content: &str) {
        self.lines = content.lines().map(TerminalLine::new).collect();
        self.cursor_position = Self::infer_cursor_position(&self.lines);
    }

    /// Infer cursor position from plain text lines. With tmux capture-pane -p (no ANSI),
    /// we don't get cursor coords; use heuristic: cursor at end of last non-empty line
    /// (typical shell prompt case). Trailing newline yields empty last line → cursor on
    /// previous line.
    fn infer_cursor_position(lines: &[TerminalLine]) -> Option<(usize, usize)> {
        if lines.is_empty() {
            return None;
        }
        let last_non_empty = lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| !l.text.is_empty());
        match last_non_empty {
            Some((row, line)) => Some((row, line.text.len())),
            None => Some((0, 0)),
        }
    }
}

impl Default for TerminalContent {
    fn default() -> Self { Self::new() }
}

/// Single line in terminal
#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub text: String,
    pub styles: Vec<StyleRange>,
}

impl TerminalLine {
    pub fn new(text: &str) -> Self {
        Self { text: text.to_string(), styles: Vec::new() }
    }
    pub fn len(&self) -> usize { self.text.len() }
    pub fn is_empty(&self) -> bool { self.text.is_empty() }
}

/// Style range for a portion of text
#[derive(Debug, Clone)]
pub struct StyleRange {
    pub start: usize,
    pub end: usize,
    pub fg_color: Option<Color>,
    pub bg_color: Option<Color>,
    pub bold: bool,
    pub italic: bool,
}

/// Color representation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self { Self { r, g, b } }

    pub fn to_hsla(&self) -> Hsla {
        let hex = ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32);
        rgb(hex).into()
    }

    pub fn black() -> Self { Self::new(0, 0, 0) }
    pub fn white() -> Self { Self::new(255, 255, 255) }
    pub fn red() -> Self { Self::new(255, 0, 0) }
    pub fn green() -> Self { Self::new(0, 255, 0) }
    pub fn blue() -> Self { Self::new(0, 0, 255) }
    pub fn gray() -> Self { Self::new(128, 128, 128) }
    pub fn dark_gray() -> Self { Self::new(64, 64, 64) }
}

/// Content source for TerminalView - either legacy capture-pane or control mode Term.
#[derive(Clone)]
pub enum TerminalBuffer {
    /// Legacy: plain text from capture-pane polling
    Legacy(Arc<Mutex<TerminalContent>>),
    /// Control mode: alacritty_terminal::Term with VT parsing
    Term(Arc<Mutex<TermBridge>>),
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
            buffer: TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new()))),
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Create with a shared content buffer (for legacy capture-pane polling)
    pub fn with_content(pane_id: &str, title: &str, content: Arc<Mutex<TerminalContent>>) -> Self {
        Self {
            pane_id: pane_id.to_string(),
            title: title.to_string(),
            buffer: TerminalBuffer::Legacy(content),
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Create with TermBridge (for tmux control mode)
    pub fn with_term(pane_id: &str, title: &str, term: Arc<Mutex<TermBridge>>) -> Self {
        Self {
            pane_id: pane_id.to_string(),
            title: title.to_string(),
            buffer: TerminalBuffer::Term(term),
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

    pub fn update_content(&mut self, content: &str) {
        if let TerminalBuffer::Legacy(ref c) = self.buffer {
            if let Ok(mut guard) = c.lock() {
                guard.update(content);
            }
        }
    }

    pub fn pane_id(&self) -> &str { &self.pane_id }
    pub fn title(&self) -> &str { &self.title }
    pub fn set_title(&mut self, title: &str) { self.title = title.to_string(); }
    pub fn scroll_up(&mut self, lines: usize) { self.scroll_offset = self.scroll_offset.saturating_add(lines); }
    pub fn scroll_down(&mut self, lines: usize) { self.scroll_offset = self.scroll_offset.saturating_sub(lines); }
    pub fn reset_scroll(&mut self) { self.scroll_offset = 0; }
}

impl IntoElement for TerminalView {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element { Component::new(self) }
}

impl RenderOnce for TerminalView {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (visible_lines, line_count, cursor_pos) = match &self.buffer {
            TerminalBuffer::Legacy(content) => {
                let content = content.lock().unwrap().clone();
                let count = content.line_count();
                let lines_to_show = 50;
                let start_idx = count.saturating_sub(lines_to_show + self.scroll_offset);
                let end_idx = count.saturating_sub(self.scroll_offset);
                let visible: Vec<String> = content.lines[start_idx..end_idx]
                    .iter()
                    .map(|l| l.text.clone())
                    .collect();
                (visible, count, content.cursor_position)
            }
            TerminalBuffer::Term(term) => {
                let term = term.lock().unwrap();
                let lines = term.visible_lines();
                let count = lines.len();
                let cursor_pos = term.cursor_position();
                let lines_to_show = 50;
                let start_idx = count.saturating_sub(lines_to_show + self.scroll_offset);
                let end_idx = count.saturating_sub(self.scroll_offset);
                let visible: Vec<String> = lines[start_idx..end_idx].to_vec();
                (visible, count, cursor_pos)
            }
        };
        let show_cursor = self.is_focused && self.cursor_visible;

        let start_idx = line_count.saturating_sub(50 + self.scroll_offset);
        div()
            .id("terminal-view")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(0x1a1a1a))
            .text_color(rgb(0xcccccc))
            .font_family("Menlo").text_size(px(12.))
            .child(
                div()
                    .flex().flex_row().items_center()
                    .px(px(8.)).py(px(4.))
                    .bg(rgb(0x2d2d2d)).border_b_1().border_color(rgb(0x3d3d3d))
                    .child(
                        div().text_size(px(11.)).text_color(rgb(0x999999))
                            .child(format!("🖥 {}", self.title))
                    )
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
                    .children(
                        visible_lines
                            .iter()
                            .enumerate()
                            .map(|(i, line_text)| {
                                let abs_row = start_idx + i;
                                let (show_cursor_here, cursor_col) = if show_cursor
                                    && cursor_pos.map(|(r, _)| r) == Some(abs_row)
                                {
                                    (true, cursor_pos.unwrap().1)
                                } else {
                                    (false, 0)
                                };
                                let line_text = if line_text.is_empty() {
                                    " ".to_string()
                                } else {
                                    line_text.clone()
                                };
                                let cursor_col_clamped = cursor_col.min(line_text.len());
                                let (before, after) = if show_cursor_here {
                                    let (b, a) = line_text.split_at(cursor_col_clamped);
                                    (b.to_string(), a.to_string())
                                } else {
                                    (line_text.clone(), String::new())
                                };
                                div()
                                    .h(px(14.))
                                    .w_full()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .child(div().child(SharedString::from(before)))
                                    .when(show_cursor_here, |el| {
                                        el.child(
                                            div()
                                                .bg(rgb(0xcccccc))
                                                .text_color(rgb(0x1a1a1a))
                                                .child("▌")
                                        )
                                    })
                                    .child(div().child(SharedString::from(after)))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    )
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
    fn test_terminal_content_creation() {
        let content = TerminalContent::new();
        assert_eq!(content.line_count(), 0);
        assert!(content.cursor_position.is_none());
    }

    #[test]
    fn test_terminal_content_from_string() {
        let content = TerminalContent::from_string("Line 1\nLine 2\nLine 3");
        assert_eq!(content.line_count(), 3);
        assert_eq!(content.lines[0].text, "Line 1");
    }

    #[test]
    fn test_terminal_content_to_string() {
        let content = TerminalContent::from_string("Hello\nWorld");
        assert_eq!(content.to_string(), "Hello\nWorld");
    }

    #[test]
    fn test_terminal_content_update() {
        let mut content = TerminalContent::from_string("Old");
        content.update("New\nLines");
        assert_eq!(content.line_count(), 2);
        assert_eq!(content.lines[0].text, "New");
    }

    #[test]
    fn test_infer_cursor_prompt_with_trailing_newline() {
        let mut content = TerminalContent::new();
        content.update("→ saas-mono git:(main)\n");
        assert_eq!(content.cursor_position, Some((0, 22)));
    }

    #[test]
    fn test_infer_cursor_multiline() {
        let mut content = TerminalContent::new();
        content.update("line1\nline2\n");
        assert_eq!(content.cursor_position, Some((1, 5)));
    }

    #[test]
    fn test_infer_cursor_single_line_no_trailing() {
        let mut content = TerminalContent::new();
        content.update("prompt");
        assert_eq!(content.cursor_position, Some((0, 6)));
    }

    #[test]
    fn test_terminal_line() {
        let line = TerminalLine::new("Hello World");
        assert_eq!(line.text, "Hello World");
        assert_eq!(line.len(), 11);
        assert!(!line.is_empty());
    }

    #[test]
    fn test_empty_terminal_line() {
        let line = TerminalLine::new("");
        assert!(line.is_empty());
    }

    #[test]
    fn test_color_creation() {
        let color = Color::new(100, 150, 200);
        assert_eq!(color.r, 100);
        assert_eq!(color.g, 150);
        assert_eq!(color.b, 200);
    }

    #[test]
    fn test_common_colors() {
        assert_eq!(Color::black().r, 0);
        assert_eq!(Color::white().r, 255);
        assert_eq!(Color::red().r, 255);
        assert_eq!(Color::green().g, 255);
        assert_eq!(Color::blue().b, 255);
    }

    #[test]
    fn test_terminal_view_creation() {
        let view = TerminalView::new("session:0.0", "main");
        assert_eq!(view.pane_id(), "session:0.0");
        assert_eq!(view.title(), "main");
    }

    #[test]
    fn test_terminal_view_update_content() {
        let mut view = TerminalView::new("pane-1", "zsh");
        view.update_content("Test content\nSecond line");
        if let TerminalBuffer::Legacy(content) = &view.buffer {
            let content = content.lock().unwrap();
            assert_eq!(content.line_count(), 2);
            assert_eq!(content.lines[0].text, "Test content");
        } else {
            panic!("expected Legacy buffer");
        }
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
    fn test_color_to_hsla() {
        let color = Color::new(255, 0, 0);
        let _ = color.to_hsla();
    }
}
