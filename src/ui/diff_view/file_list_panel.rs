// ui/diff_view/file_list_panel.rs - Left panel: file list with status icons
use crate::git_diff::{ChangedFile, FileChangeStatus};
use gpui::prelude::*;
use gpui::{AnyElement, App, Component, ElementId, FontWeight, SharedString, StyleRefinement, Window, div, px, rgb};
use std::sync::Arc;

pub type FileSelectCallback = Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>;

/// Left panel showing changed files with status icons
pub struct FileListPanel {
    files: Vec<ChangedFile>,
    selected_index: Option<usize>,
    on_select: Option<FileSelectCallback>,
}

impl FileListPanel {
    pub fn new(files: Vec<ChangedFile>) -> Self {
        Self {
            files,
            selected_index: None,
            on_select: None,
        }
    }

    pub fn with_selected(mut self, index: Option<usize>) -> Self {
        self.selected_index = index;
        self
    }

    pub fn on_select<F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_select = Some(Arc::new(callback));
        self
    }
}

impl RenderOnce for FileListPanel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        if self.files.is_empty() {
            return div()
                .id("file-list-panel")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(0x888888))
                        .child("No changes"),
                )
                .into_any_element();
        }

        let rows: Vec<AnyElement> = self
            .files
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                let is_selected = self.selected_index == Some(idx);
                let on_select = self.on_select.clone();

                let status_color = match &file.status {
                    FileChangeStatus::Added => rgb(0x4caf50),
                    FileChangeStatus::Modified => rgb(0xffc107),
                    FileChangeStatus::Deleted => rgb(0xf44336),
                    FileChangeStatus::Renamed(_) => rgb(0x2196f3),
                };

                let status_label = file.status.label();

                // Show just the filename, with directory in muted color
                let (dir, name) = split_path(&file.path);

                div()
                    .id(ElementId::Integer(idx as u64))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .py(px(4.))
                    .min_h(px(28.))
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(rgb(0x2c313a)))
                    .when(!is_selected, |el| {
                        el.hover(|s: StyleRefinement| s.bg(rgb(0x303030)))
                    })
                    .on_click(move |_event, window, cx| {
                        if let Some(ref cb) = on_select {
                            cb(idx, window, cx);
                        }
                    })
                    // Status badge
                    .child(
                        div()
                            .w(px(18.))
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(status_color)
                            .child(SharedString::from(status_label.to_string())),
                    )
                    // File name
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_row()
                            .gap(px(4.))
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(0xcccccc))
                                    .flex_shrink_0()
                                    .child(SharedString::from(name)),
                            )
                            .when(!dir.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(0x6d6d6d))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(SharedString::from(dir)),
                                )
                            }),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .id("file-list-panel")
            .size_full()
            .overflow_y_scroll()
            .py(px(4.))
            .children(rows)
            .into_any_element()
    }
}

impl IntoElement for FileListPanel {
    type Element = AnyElement;
    fn into_element(self) -> Self::Element {
        // Delegate to Component<Self> by rendering
        Component::new(self).into_any_element()
    }
}

/// Split a file path into (directory, filename)
fn split_path(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(pos) => (path[..pos].to_string(), path[pos + 1..].to_string()),
        None => (String::new(), path.to_string()),
    }
}
