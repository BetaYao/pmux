// ui/status_bar.rs - Bottom status bar (Zed-style)
use gpui::prelude::*;
use gpui::*;
use crate::agent_status::StatusCounts;

/// Status bar item - left or right aligned
#[derive(Clone)]
pub struct StatusBarItem {
    pub text: String,
    pub title: Option<String>,
}

/// Status bar component - shows context info at bottom of window
pub struct StatusBar {
    /// Left side items (worktree, branch, pane info)
    left_items: Vec<StatusBarItem>,
    /// Right side items (agent status summary, shortcuts)
    right_items: Vec<StatusBarItem>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            left_items: Vec::new(),
            right_items: Vec::new(),
        }
    }

    pub fn with_left(mut self, text: impl Into<String>, title: Option<impl Into<String>>) -> Self {
        self.left_items.push(StatusBarItem {
            text: text.into(),
            title: title.map(|t| t.into()),
        });
        self
    }

    pub fn with_right(mut self, text: impl Into<String>, title: Option<impl Into<String>>) -> Self {
        self.right_items.push(StatusBarItem {
            text: text.into(),
            title: title.map(|t| t.into()),
        });
        self
    }

    /// Build status bar from app context (worktree, branch, status counts, backend)
    pub fn from_context(
        worktree_branch: Option<&str>,
        pane_count: usize,
        focused_pane: usize,
        status_counts: &StatusCounts,
        backend: Option<&str>,
    ) -> Self {
        let mut left = Vec::new();
        if let Some(b) = backend {
            left.push(StatusBarItem {
                text: format!("backend: {}", b),
                title: Some("Runtime backend. Set via config.json or PMUX_BACKEND env. Priority: env > config > default".to_string()),
            });
        }
        if let Some(branch) = worktree_branch {
            left.push(StatusBarItem {
                text: format!("git: ({})", branch),
                title: Some("Current worktree branch".to_string()),
            });
        }
        if pane_count > 0 {
            left.push(StatusBarItem {
                text: format!("Pane {}/{}", focused_pane + 1, pane_count),
                title: Some("Active pane (click to switch)".to_string()),
            });
        }

        let mut right = Vec::new();
        let total = status_counts.total();
        if total > 0 {
            let mut parts = Vec::new();
            if status_counts.running > 0 {
                parts.push(format!("● {} Running", status_counts.running));
            }
            if status_counts.waiting > 0 {
                parts.push(format!("◐ {} Waiting", status_counts.waiting));
            }
if status_counts.idle > 0 {
                parts.push(format!("○ {} Idle", status_counts.idle));
            }
            if status_counts.error > 0 {
                parts.push(format!("✕ {} Error", status_counts.error));
            }
            if !parts.is_empty() {
                right.push(StatusBarItem {
                    text: parts.join("  "),
                    title: Some("Agent status".to_string()),
                });
            }
        }
        right.push(StatusBarItem {
            text: "⌘B Sidebar  ⌘D Split  ⌘⇧D H-Split  ⌘W Close  ⌘R Diff  ⌘1-8 Workspace".to_string(),
            title: Some("Shortcuts".to_string()),
        });

        Self {
            left_items: left,
            right_items: right,
        }
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for StatusBar {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let left_items = self.left_items;
        let right_items = self.right_items;

        div()
            .id("status-bar")
            .w_full()
            .h(px(22.))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(12.))
            .gap(px(16.))
            .bg(rgb(0x252526))
            .border_t_1()
            .border_color(rgb(0x3d3d3d))
            .text_size(px(11.))
            .text_color(rgb(0xcccccc))
            .font_family(".SystemUIFont")
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(16.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .children(
                        left_items
                            .into_iter()
                            .map(|item| {
                                div()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(item.text)
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    )
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap(px(16.))
                    .overflow_hidden()
                    .children(
                        right_items
                            .into_iter()
                            .map(|item| {
                                div()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(rgb(0x888888))
                                    .child(item.text)
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    )
            )
    }
}
