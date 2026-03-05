// ui/diff_view/changes_panel.rs - Left panel: commit list with expandable file lists
use crate::git_diff::{ChangedFile, CommitInfo, FileChangeStatus};
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

/// Callback: (file_path, commit_hash, &mut Window, &mut App)
pub type CommitFileSelectCallback =
    Arc<dyn Fn(&str, &str, &mut Window, &mut App) + Send + Sync>;

/// Changes panel: expandable commit list with per-commit file lists
pub struct ChangesPanel {
    commits: Vec<CommitInfo>,
    /// commit_hash -> list of changed files (loaded on expand)
    commit_files: Vec<(String, Vec<ChangedFile>)>,
    expanded_commit: Option<String>,
    selected_file: Option<(String, String)>, // (commit_hash, file_path)
    on_select_file: Option<CommitFileSelectCallback>,
    /// Callback to expand a commit (loads its files)
    on_expand_commit: Option<Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>>,
}

impl ChangesPanel {
    pub fn new(commits: Vec<CommitInfo>) -> Self {
        Self {
            commits,
            commit_files: Vec::new(),
            expanded_commit: None,
            selected_file: None,
            on_select_file: None,
            on_expand_commit: None,
        }
    }

    pub fn with_commit_files(mut self, commit_files: Vec<(String, Vec<ChangedFile>)>) -> Self {
        self.commit_files = commit_files;
        self
    }

    pub fn with_expanded(mut self, commit_hash: Option<String>) -> Self {
        self.expanded_commit = commit_hash;
        self
    }

    pub fn with_selected_file(mut self, selected: Option<(String, String)>) -> Self {
        self.selected_file = selected;
        self
    }

    pub fn on_select_file<F: Fn(&str, &str, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_select_file = Some(Arc::new(callback));
        self
    }

    pub fn on_expand_commit<F: Fn(&str, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_expand_commit = Some(Arc::new(callback));
        self
    }
}

impl RenderOnce for ChangesPanel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        if self.commits.is_empty() {
            return div()
                .id("changes-panel")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(0x888888))
                        .child("No commits"),
                )
                .into_any_element();
        }

        let mut rows: Vec<AnyElement> = Vec::new();
        let mut counter: u64 = 10000; // offset to avoid ID collision with other panels

        for commit in &self.commits {
            let is_expanded = self.expanded_commit.as_deref() == Some(&commit.hash);
            let arrow = if is_expanded { "▼" } else { "▶" };

            let hash = commit.hash.clone();
            let on_expand = self.on_expand_commit.clone();

            // Commit header row
            rows.push(
                div()
                    .id(ElementId::Integer(counter))
                    .flex()
                    .flex_col()
                    .px(px(8.))
                    .py(px(6.))
                    .cursor_pointer()
                    .hover(|s: StyleRefinement| s.bg(rgb(0x303030)))
                    .when(is_expanded, |el| el.bg(rgb(0x2a2d37)))
                    .on_click(move |_event, window, cx| {
                        if let Some(ref cb) = on_expand {
                            cb(&hash, window, cx);
                        }
                    })
                    // First line: arrow + short hash + subject
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(0x888888))
                                    .w(px(12.))
                                    .flex_shrink_0()
                                    .child(arrow),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x0066cc))
                                    .font_weight(FontWeight::BOLD)
                                    .font_family("monospace")
                                    .flex_shrink_0()
                                    .child(SharedString::from(commit.short_hash.clone())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(12.))
                                    .text_color(rgb(0xcccccc))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(SharedString::from(commit.subject.clone())),
                            ),
                    )
                    // Second line: author + date
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.))
                            .pl(px(18.)) // align with subject
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(0x6d6d6d))
                                    .child(SharedString::from(commit.author.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(0x6d6d6d))
                                    .child(SharedString::from(commit.date.clone())),
                            ),
                    )
                    .into_any_element(),
            );
            counter += 1;

            // Expanded: show files for this commit
            if is_expanded {
                let files = self
                    .commit_files
                    .iter()
                    .find(|(h, _)| h == &commit.hash)
                    .map(|(_, f)| f.as_slice())
                    .unwrap_or(&[]);

                if files.is_empty() {
                    rows.push(
                        div()
                            .pl(px(30.))
                            .py(px(4.))
                            .text_size(px(12.))
                            .text_color(rgb(0x888888))
                            .child("Loading...")
                            .into_any_element(),
                    );
                } else {
                    for file in files {
                        let is_selected = self.selected_file.as_ref()
                            == Some(&(commit.hash.clone(), file.path.clone()));

                        let status_color = match &file.status {
                            FileChangeStatus::Added => rgb(0x4caf50),
                            FileChangeStatus::Modified => rgb(0xffc107),
                            FileChangeStatus::Deleted => rgb(0xf44336),
                            FileChangeStatus::Renamed(_) => rgb(0x2196f3),
                        };

                        let on_select = self.on_select_file.clone();
                        let file_path = file.path.clone();
                        let commit_hash = commit.hash.clone();

                        // Show just filename + dir
                        let (dir, name) = split_path(&file.path);

                        rows.push(
                            div()
                                .id(ElementId::Integer(counter))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(4.))
                                .pl(px(30.))
                                .pr(px(8.))
                                .py(px(3.))
                                .min_h(px(24.))
                                .cursor_pointer()
                                .when(is_selected, |el| el.bg(rgb(0x2c313a)))
                                .when(!is_selected, |el| {
                                    el.hover(|s: StyleRefinement| s.bg(rgb(0x303030)))
                                })
                                .on_click(move |_event, window, cx| {
                                    if let Some(ref cb) = on_select {
                                        cb(&file_path, &commit_hash, window, cx);
                                    }
                                })
                                // Status
                                .child(
                                    div()
                                        .w(px(16.))
                                        .flex_shrink_0()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(status_color)
                                        .child(SharedString::from(
                                            file.status.label().to_string(),
                                        )),
                                )
                                // File name + dir
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
                                                .text_size(px(12.))
                                                .text_color(rgb(0xcccccc))
                                                .flex_shrink_0()
                                                .child(SharedString::from(name)),
                                        )
                                        .when(!dir.is_empty(), |el| {
                                            el.child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(0x6d6d6d))
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .child(SharedString::from(dir)),
                                            )
                                        }),
                                )
                                .into_any_element(),
                        );
                        counter += 1;
                    }
                }

                // Divider after expanded commit
                rows.push(
                    div()
                        .h(px(1.))
                        .bg(rgb(0x3d3d3d))
                        .mx(px(8.))
                        .into_any_element(),
                );
            }
        }

        div()
            .id("changes-panel")
            .size_full()
            .overflow_y_scroll()
            .py(px(4.))
            .children(rows)
            .into_any_element()
    }
}

impl IntoElement for ChangesPanel {
    type Element = AnyElement;
    fn into_element(self) -> Self::Element {
        Component::new(self).into_any_element()
    }
}

fn split_path(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(pos) => (path[..pos].to_string(), path[pos + 1..].to_string()),
        None => (String::new(), path.to_string()),
    }
}
