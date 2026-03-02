// ui/terminal_view.rs - Terminal view component with GPUI render
// Renders via gpui-terminal (GpuiTerminal), self-built Terminal, simple div for Error, or placeholder for Empty.
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

/// Content source for TerminalView - gpui-terminal, self-built terminal, error placeholder, or empty.
#[derive(Clone)]
pub enum TerminalBuffer {
    /// Placeholder when pane has no buffer yet (gray bg, "—")
    Empty,
    /// Error: static message when streaming unavailable (no screen snapshot)
    Error(String),
    /// gpui-terminal: embedded TerminalView entity (tee_output → RuntimeReader + ContentExtractor pipeline)
    GpuiTerminal(gpui::Entity<gpui_terminal::TerminalView>),
    /// Self-built terminal: Arc<Terminal> + dedicated FocusHandle
    Terminal {
        terminal: Arc<crate::terminal::Terminal>,
        focus_handle: gpui::FocusHandle,
        resize_callback: Option<Arc<dyn Fn(u16, u16) + Send + Sync>>,
    },
}

impl TerminalBuffer {
    /// Extract text for status detection.
    /// GpuiTerminal/Terminal: returns None (status is published from ContentExtractor background task).
    /// Empty: returns None. Error: returns Some(msg).
    pub fn content_for_status_detection(&self) -> Option<String> {
        match self {
            TerminalBuffer::Empty => None,
            TerminalBuffer::Error(s) => Some(s.clone()),
            TerminalBuffer::GpuiTerminal(_) => None,
            TerminalBuffer::Terminal { .. } => None,
        }
    }
}

/// Terminal view component - renders tmux pane content
pub struct TerminalView {
    pane_id: String,
    title: String,
    buffer: TerminalBuffer,
    scroll_offset: usize,
    is_focused: bool,
    cursor_visible: bool,
}

impl TerminalView {
    pub fn new(pane_id: &str, title: &str) -> Self {
        Self {
            pane_id: pane_id.to_string(),
            title: title.to_string(),
            buffer: TerminalBuffer::Empty,
            scroll_offset: 0,
            is_focused: false,
            cursor_visible: true,
        }
    }

    /// Create with a TerminalBuffer (GpuiTerminal, Error, or Empty)
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

    /// Set whether this pane is focused
    pub fn with_focused(mut self, focused: bool) -> Self {
        self.is_focused = focused;
        self
    }

    /// Set cursor visibility (for blink)
    pub fn with_cursor_visible(mut self, visible: bool) -> Self {
        self.cursor_visible = visible;
        self
    }

    pub fn pane_id(&self) -> &str {
        &self.pane_id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    fn render_error(&self, msg: &str) -> impl IntoElement {
        let lines: Vec<String> = msg.lines().map(|s| s.to_string()).collect();
        let count = lines.len();
        let lines_to_show = 50;
        let start_idx = count.saturating_sub(lines_to_show + self.scroll_offset);
        let end_idx = count.saturating_sub(self.scroll_offset);
        let visible: Vec<&String> = lines.get(start_idx..end_idx).unwrap_or(&[]).iter().collect();
        div()
            .flex()
            .flex_col()
            .children(visible.into_iter().map(|line_text| {
                let text: String = if line_text.is_empty() {
                    " ".into()
                } else {
                    line_text.clone()
                };
                div().child(text)
            }))
    }
}

impl IntoElement for TerminalView {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for TerminalView {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let content_elem: AnyElement = match &self.buffer {
            TerminalBuffer::GpuiTerminal(entity) => {
                div().size_full().child(entity.clone()).into_any_element()
            }
            TerminalBuffer::Terminal { terminal, focus_handle, resize_callback } => {
                use crate::terminal::terminal_element::TerminalElement;
                use crate::terminal::ColorPalette;
                let mut elem = TerminalElement::new(
                    terminal.clone(),
                    focus_handle.clone(),
                    ColorPalette::default(),
                );
                if let Some(cb) = resize_callback {
                    let cb = cb.clone();
                    elem = elem.with_resize_callback(move |cols, rows| cb(cols, rows));
                }
                div().size_full().child(elem).into_any_element()
            }
            TerminalBuffer::Error(msg) => {
                self.render_error(msg).into_any_element()
            }
            TerminalBuffer::Empty => {
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x2e343e))
                    .text_color(rgb(0x6d6d6d))
                    .child("—")
                    .into_any_element()
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
                    .child(content_elem),
            )
    }
}

impl Default for TerminalView {
    fn default() -> Self {
        Self::new("default", "Terminal")
    }
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
        assert_eq!(
            buf.content_for_status_detection(),
            Some("Streaming unavailable".to_string())
        );
    }

    #[test]
    fn test_buffer_empty_returns_none() {
        assert_eq!(TerminalBuffer::Empty.content_for_status_detection(), None);
    }
}
