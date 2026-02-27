// ui/diff_overlay.rs - Diff view overlay component
use crate::ui::terminal_view::{TerminalBuffer, TerminalView};
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

/// Callback type for closing the diff overlay
pub type CloseDiffOverlayCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;

/// Diff overlay - full-screen overlay showing nvim diffview terminal output
pub struct DiffOverlay {
    branch: String,
    pane_target: String,
    buffer: TerminalBuffer,
    on_close: Option<CloseDiffOverlayCallback>,
}

impl DiffOverlay {
    pub fn new(branch: &str, pane_target: &str, buffer: TerminalBuffer) -> Self {
        Self {
            branch: branch.to_string(),
            pane_target: pane_target.to_string(),
            buffer,
            on_close: None,
        }
    }

    pub fn on_close<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(mut self, callback: F) -> Self {
        self.on_close = Some(Arc::new(callback));
        self
    }
}

impl IntoElement for DiffOverlay {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        let branch = self.branch.clone();
        let pane_target = self.pane_target.clone();
        let buffer = self.buffer;
        let on_close = self.on_close.clone();

        // Full overlay - covers entire workspace view (sidebar + content)
        let overlay = div()
            .id("diff-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e));

        // Header: branch name + close button
        let header = div()
            .id("diff-overlay-header")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(16.))
            .py(px(10.))
            .border_b(px(1.))
            .border_color(rgb(0x3d3d3d))
            .bg(rgb(0x252526))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child(format!("Diff: {} vs main", branch)),
            )
            .child(
                div()
                    .id("diff-overlay-close-btn")
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(px(4.))
                    .bg(rgb(0x3d3d3d))
                    .text_color(rgb(0xcccccc))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
                    .on_click(move |_event, window, cx| {
                        if let Some(ref cb) = on_close {
                            cb(window, cx);
                        }
                    })
                    .child("Close (⌘W)"),
            );

        // Body: terminal view showing nvim diffview
        let terminal = TerminalView::with_buffer(&pane_target, &format!("review-{}", branch), buffer)
            .with_focused(true)
            .with_cursor_visible(true);

        overlay
            .child(header)
            .child(
                div()
                    .id("diff-overlay-body")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(terminal),
            )
            .into_any_element()
    }
}
