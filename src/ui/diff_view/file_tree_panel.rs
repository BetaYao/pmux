// ui/diff_view/file_tree_panel.rs - Left panel: nested file tree showing only changed files
use crate::git_diff::{ChangedFile, FileChangeStatus};
use gpui::prelude::*;
use gpui::{AnyElement, App, Component, ElementId, FontWeight, SharedString, StyleRefinement, Window, div, px, rgb};
use std::collections::HashMap;
use std::sync::Arc;

/// Callback: (file_path, &mut Window, &mut App)
pub type FileTreeSelectCallback = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;

/// A node in the file tree
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub full_path: Option<String>,
    pub status: Option<FileChangeStatus>,
    pub children: Vec<FileTreeNode>,
    pub expanded: bool,
}

/// Build a nested file tree from flat changed file paths.
/// Auto-collapses single-child directories (a/b/c.rs → a/b/c.rs).
pub fn build_file_tree(files: &[ChangedFile]) -> Vec<FileTreeNode> {
    // Build raw tree
    let mut root_children: Vec<FileTreeNode> = Vec::new();

    for file in files {
        let parts: Vec<&str> = file.path.split('/').collect();
        insert_path(&mut root_children, &parts, &file.path, &file.status);
    }

    // Sort: directories first, then files, alphabetically within each
    sort_tree(&mut root_children);

    // Collapse single-child directories
    collapse_single_children(&mut root_children);

    root_children
}

fn insert_path(
    children: &mut Vec<FileTreeNode>,
    parts: &[&str],
    full_path: &str,
    status: &FileChangeStatus,
) {
    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        // Leaf node (file)
        children.push(FileTreeNode {
            name: parts[0].to_string(),
            full_path: Some(full_path.to_string()),
            status: Some(status.clone()),
            children: Vec::new(),
            expanded: false,
        });
        return;
    }

    // Directory node
    let dir_name = parts[0];
    let existing = children.iter_mut().find(|c| c.name == dir_name && c.full_path.is_none());

    if let Some(dir_node) = existing {
        insert_path(&mut dir_node.children, &parts[1..], full_path, status);
    } else {
        let mut dir_node = FileTreeNode {
            name: dir_name.to_string(),
            full_path: None,
            status: None,
            children: Vec::new(),
            expanded: true, // directories start expanded
        };
        insert_path(&mut dir_node.children, &parts[1..], full_path, status);
        children.push(dir_node);
    }
}

fn sort_tree(children: &mut Vec<FileTreeNode>) {
    children.sort_by(|a, b| {
        let a_is_dir = a.full_path.is_none();
        let b_is_dir = b.full_path.is_none();
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    for child in children.iter_mut() {
        if !child.children.is_empty() {
            sort_tree(&mut child.children);
        }
    }
}

/// Collapse single-child directories: if a dir has exactly one child that is also a dir,
/// merge them (e.g., src/ → components/ → Button.rs becomes src/components/Button.rs)
fn collapse_single_children(children: &mut Vec<FileTreeNode>) {
    for child in children.iter_mut() {
        // Recursively collapse children first
        if !child.children.is_empty() {
            collapse_single_children(&mut child.children);
        }

        // If this is a directory with exactly one child that is also a directory, merge
        while child.full_path.is_none()
            && child.children.len() == 1
            && child.children[0].full_path.is_none()
        {
            let grandchild = child.children.remove(0);
            child.name = format!("{}/{}", child.name, grandchild.name);
            child.children = grandchild.children;
        }
    }
}

/// File tree panel component
pub struct FileTreePanel {
    tree: Vec<FileTreeNode>,
    selected_path: Option<String>,
    expanded_dirs: HashMap<String, bool>,
    on_select: Option<FileTreeSelectCallback>,
}

impl FileTreePanel {
    pub fn new(tree: Vec<FileTreeNode>) -> Self {
        // Collect initial expanded state from tree
        let mut expanded_dirs = HashMap::new();
        collect_expanded_state(&tree, "", &mut expanded_dirs);

        Self {
            tree,
            selected_path: None,
            expanded_dirs,
            on_select: None,
        }
    }

    pub fn with_selected(mut self, path: Option<String>) -> Self {
        self.selected_path = path;
        self
    }

    pub fn with_expanded_dirs(mut self, dirs: HashMap<String, bool>) -> Self {
        self.expanded_dirs = dirs;
        self
    }

    pub fn on_select<F: Fn(&str, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_select = Some(Arc::new(callback));
        self
    }
}

fn collect_expanded_state(
    nodes: &[FileTreeNode],
    parent_path: &str,
    expanded: &mut HashMap<String, bool>,
) {
    for node in nodes {
        let node_path = if parent_path.is_empty() {
            node.name.clone()
        } else {
            format!("{}/{}", parent_path, node.name)
        };

        if node.full_path.is_none() {
            // It's a directory
            expanded.entry(node_path.clone()).or_insert(node.expanded);
            collect_expanded_state(&node.children, &node_path, expanded);
        }
    }
}

impl RenderOnce for FileTreePanel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        if self.tree.is_empty() {
            return div()
                .id("file-tree-panel")
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

        let mut rows: Vec<AnyElement> = Vec::new();
        let mut counter: u64 = 0;
        render_nodes(
            &self.tree,
            0,
            "",
            &self.selected_path,
            &self.expanded_dirs,
            &self.on_select,
            &mut rows,
            &mut counter,
        );

        div()
            .id("file-tree-panel")
            .size_full()
            .overflow_y_scroll()
            .py(px(4.))
            .children(rows)
            .into_any_element()
    }
}

impl IntoElement for FileTreePanel {
    type Element = AnyElement;
    fn into_element(self) -> Self::Element {
        Component::new(self).into_any_element()
    }
}

fn render_nodes(
    nodes: &[FileTreeNode],
    depth: usize,
    parent_path: &str,
    selected_path: &Option<String>,
    expanded_dirs: &HashMap<String, bool>,
    on_select: &Option<FileTreeSelectCallback>,
    rows: &mut Vec<AnyElement>,
    counter: &mut u64,
) {
    for node in nodes {
        let node_path = if parent_path.is_empty() {
            node.name.clone()
        } else {
            format!("{}/{}", parent_path, node.name)
        };

        let indent = px((depth * 16 + 4) as f32);
        let id = *counter;
        *counter += 1;

        if node.full_path.is_some() {
            // File node
            let is_selected = selected_path.as_deref() == node.full_path.as_deref();
            let status_color = match &node.status {
                Some(FileChangeStatus::Added) => rgb(0x4caf50),
                Some(FileChangeStatus::Modified) => rgb(0xffc107),
                Some(FileChangeStatus::Deleted) => rgb(0xf44336),
                Some(FileChangeStatus::Renamed(_)) => rgb(0x2196f3),
                None => rgb(0xcccccc),
            };
            let status_label = node
                .status
                .as_ref()
                .map(|s| s.label())
                .unwrap_or("");

            let on_select = on_select.clone();
            let file_path = node.full_path.clone().unwrap_or_default();

            rows.push(
                div()
                    .id(ElementId::Integer(id))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .pl(indent)
                    .pr(px(8.))
                    .py(px(2.))
                    .min_h(px(24.))
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(rgb(0x2c313a)))
                    .when(!is_selected, |el| {
                        el.hover(|s: StyleRefinement| s.bg(rgb(0x303030)))
                    })
                    .on_click(move |_event, window, cx| {
                        if let Some(ref cb) = on_select {
                            cb(&file_path, window, cx);
                        }
                    })
                    // File icon placeholder
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x888888))
                            .w(px(14.))
                            .flex_shrink_0()
                            .child("  "), // no expand triangle for files
                    )
                    // Status badge
                    .child(
                        div()
                            .w(px(16.))
                            .flex_shrink_0()
                            .text_size(px(10.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(status_color)
                            .child(SharedString::from(status_label.to_string())),
                    )
                    // File name
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(13.))
                            .text_color(rgb(0xcccccc))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(node.name.clone())),
                    )
                    .into_any_element(),
            );
        } else {
            // Directory node
            let is_expanded = expanded_dirs
                .get(&node_path)
                .copied()
                .unwrap_or(node.expanded);
            let arrow = if is_expanded { "▼" } else { "▶" };

            // Directory row (not clickable for file selection, just expand/collapse)
            // We don't have mutable state in RenderOnce, so expansion is managed by the parent entity
            rows.push(
                div()
                    .id(ElementId::Integer(id))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .pl(indent)
                    .pr(px(8.))
                    .py(px(2.))
                    .min_h(px(24.))
                    // Directory label
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(0x888888))
                            .w(px(14.))
                            .flex_shrink_0()
                            .child(arrow),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(13.))
                            .text_color(rgb(0xcccccc))
                            .font_weight(FontWeight::SEMIBOLD)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(node.name.clone())),
                    )
                    .into_any_element(),
            );

            // Render children if expanded
            if is_expanded {
                render_nodes(
                    &node.children,
                    depth + 1,
                    &node_path,
                    selected_path,
                    expanded_dirs,
                    on_select,
                    rows,
                    counter,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_file_tree_basic() {
        let files = vec![
            ChangedFile {
                path: "src/main.rs".to_string(),
                status: FileChangeStatus::Modified,
            },
            ChangedFile {
                path: "src/lib.rs".to_string(),
                status: FileChangeStatus::Added,
            },
            ChangedFile {
                path: "README.md".to_string(),
                status: FileChangeStatus::Modified,
            },
        ];

        let tree = build_file_tree(&files);

        // Should have 2 top-level entries: src/ and README.md
        assert_eq!(tree.len(), 2);
        // Directories first
        assert_eq!(tree[0].name, "src");
        assert!(tree[0].full_path.is_none());
        assert_eq!(tree[0].children.len(), 2);
        // Then files
        assert_eq!(tree[1].name, "README.md");
        assert!(tree[1].full_path.is_some());
    }

    #[test]
    fn test_collapse_single_children() {
        let files = vec![ChangedFile {
            path: "src/ui/components/Button.rs".to_string(),
            status: FileChangeStatus::Modified,
        }];

        let tree = build_file_tree(&files);

        // src/ui/components should be collapsed into one node
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "src/ui/components");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "Button.rs");
    }

    #[test]
    fn test_no_collapse_when_multiple_children() {
        let files = vec![
            ChangedFile {
                path: "src/a.rs".to_string(),
                status: FileChangeStatus::Modified,
            },
            ChangedFile {
                path: "src/b.rs".to_string(),
                status: FileChangeStatus::Added,
            },
        ];

        let tree = build_file_tree(&files);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "src");
        assert_eq!(tree[0].children.len(), 2);
    }
}
