// ui/terminal_view.rs - Terminal view component with GPUI render
// No screen text snapshot: content from stream (pipe-pane/control mode) or static error only.
use crate::terminal::{StyledCell, TermBridge};
use gpui::prelude::*;
use gpui::*;
use std::sync::{Arc, Mutex};

#[inline]
fn rgb_u8(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Line height in pixels - enough for 12px font descenders (g, y, p)
const LINE_HEIGHT: f32 = 20.0;

/// Content source for TerminalView - streaming (Term) or error placeholder (Error).
#[derive(Clone)]
pub enum TerminalBuffer {
    /// Error: static message when streaming unavailable (no screen snapshot)
    Error(String),
    /// Streaming: alacritty_terminal::Term with VT parsing (pipe-pane / control mode)
    Term(Arc<Mutex<TermBridge>>),
}

impl TerminalBuffer {
    /// Extract text for status detection. Source: stream (Term) only—never capture-pane.
    pub fn content_for_status_detection(&self) -> Option<String> {
        match self {
            TerminalBuffer::Term(t) => t.lock().ok().map(|term| term.visible_lines().join("\n")),
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
            buffer: TerminalBuffer::Term(Arc::new(Mutex::new(TermBridge::new(80, 24)))),
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Create with TermBridge (for pipe-pane / control mode streaming)
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
        enum LineContent {
            Plain(Vec<String>),
            Colored(Vec<Vec<StyledCell>>),
        }
        let (content, line_count, cursor_pos) = match &self.buffer {
            TerminalBuffer::Error(msg) => {
                let lines: Vec<String> = msg.lines().map(|s| s.to_string()).collect();
                let count = lines.len();
                let lines_to_show = 50;
                let start_idx = count.saturating_sub(lines_to_show + self.scroll_offset);
                let end_idx = count.saturating_sub(self.scroll_offset);
                let visible: Vec<String> = lines.get(start_idx..end_idx).unwrap_or(&[]).to_vec();
                (LineContent::Plain(visible), count, None)
            }
            TerminalBuffer::Term(term) => {
                let term = term.lock().unwrap();
                let lines = term.visible_lines_with_colors();
                let count = lines.len();
                let cursor_pos = term.cursor_position();
                let lines_to_show = 50;
                let start_idx = count.saturating_sub(lines_to_show + self.scroll_offset);
                let end_idx = count.saturating_sub(self.scroll_offset);
                let visible: Vec<Vec<StyledCell>> = lines.get(start_idx..end_idx).unwrap_or(&[]).to_vec();
                (LineContent::Colored(visible), count, cursor_pos)
            }
        };
        let show_cursor = self.is_focused && self.cursor_visible;
        let start_idx = line_count.saturating_sub(50 + self.scroll_offset);

        let line_elements: Vec<AnyElement> = match content {
            LineContent::Plain(lines) => lines
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
                    let line_text = if line_text.is_empty() { " ".to_string() } else { line_text.clone() };
                    let cursor_col_clamped = cursor_col.min(line_text.chars().count());
                    let (before, cursor_cell, after) = if show_cursor_here {
                        let chars: Vec<char> = line_text.chars().collect();
                        let (b, rest) = chars.split_at(cursor_col_clamped);
                        let cell = rest.first().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string());
                        let a: String = rest.iter().skip(1).collect();
                        (b.iter().collect(), cell, a)
                    } else {
                        (line_text.clone(), String::new(), String::new())
                    };
                    div()
                        .h(px(LINE_HEIGHT))
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .child(div().text_color(rgb(0xabb2bf)).child(SharedString::from(before)))
                        .when(show_cursor_here, |el| {
                            el.child(
                                div()
                                    .h(px(LINE_HEIGHT))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(0x74ade8))
                                    .text_color(rgb(0x282c34))
                                    .child(SharedString::from(cursor_cell))
                            )
                        })
                        .child(div().text_color(rgb(0xabb2bf)).child(SharedString::from(after)))
                        .into_any_element()
                })
                .collect(),
            LineContent::Colored(lines) => lines
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let abs_row = start_idx + i;
                    let (show_cursor_here, cursor_col) = if show_cursor
                        && cursor_pos.map(|(r, _)| r) == Some(abs_row)
                    {
                        (true, cursor_pos.unwrap().1)
                    } else {
                        (false, 0)
                    };
                    let cursor_col_clamped = cursor_col.min(row.len());
                    let mut cells: Vec<AnyElement> = Vec::new();
                    for (col, (c, fg, bg)) in row.iter().enumerate() {
                        if show_cursor_here && col == cursor_col_clamped {
                            cells.push(
                                div()
                                    .h(px(LINE_HEIGHT))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(0x74ade8))
                                    .text_color(rgb(0x282c34))
                                    .child(SharedString::from(c.to_string()))
                                    .into_any_element(),
                            );
                        } else {
                            let fg_rgb = rgb(rgb_u8(fg[0], fg[1], fg[2]));
                            let bg_rgb = rgb(rgb_u8(bg[0], bg[1], bg[2]));
                            cells.push(
                                div()
                                    .text_color(fg_rgb)
                                    .bg(bg_rgb)
                                    .child(SharedString::from(c.to_string()))
                                    .into_any_element(),
                            );
                        }
                    }
                    if show_cursor_here && cursor_col_clamped >= row.len() {
                        cells.push(
                            div()
                                .h(px(LINE_HEIGHT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(rgb(0x74ade8))
                                .text_color(rgb(0x282c34))
                                .child(SharedString::from(" "))
                                .into_any_element(),
                        );
                    }
                    div()
                        .h(px(LINE_HEIGHT))
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .children(cells)
                        .into_any_element()
                })
                .collect(),
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
}
