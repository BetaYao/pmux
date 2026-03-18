// ui/diff_view/mod.rs - Built-in diff view overlay (replaces nvim+diffview)
// Uses uniform_list for virtualized diff rendering (only visible rows created).
pub mod changes_panel;
pub mod diff_panel;
pub mod file_list_panel;
pub mod file_tree_panel;

use crate::git_diff::{self, ChangedFile, CommitInfo, FileDiff};
use crate::syntax_highlight::{HighlightedLine, LineHighlighter};
use diff_panel::{build_flat_rows, build_flat_rows_sbs, render_binary_notice, render_diff_row, render_empty_state, render_no_changes, DiffFlatRow, RejectHunkCallback};
use file_tree_panel::{build_file_tree, FileTreeNode};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Div, FontWeight, ScrollStrategy, SharedString, Stateful, StyleRefinement, UniformListScrollHandle, Window, div, px, rgb, uniform_list};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub type CloseDiffViewCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;

#[derive(Clone, Copy, PartialEq)]
pub enum LeftPanelTab {
    Files,
    Changes,
}

/// Built-in diff view overlay — full-screen, left (file tree / changes) + right (diff content)
pub struct DiffViewOverlay {
    worktree_path: PathBuf,
    branch_name: String,
    main_branch: String,

    // Data — Files tab
    changed_files: Vec<ChangedFile>,
    file_tree: Vec<FileTreeNode>,
    current_file_diff: Option<Arc<FileDiff>>,
    cached_highlighted: Option<Arc<Vec<HighlightedLine>>>,
    /// Pre-computed flat rows for unified mode (one row per diff line).
    diff_flat_rows: Vec<DiffFlatRow>,
    /// Pre-computed flat rows for side-by-side mode (removed/added lines paired).
    diff_flat_rows_sbs: Vec<DiffFlatRow>,

    // Data — Changes tab
    commits: Vec<CommitInfo>,
    commits_loaded: bool,
    commit_files_cache: HashMap<String, Vec<ChangedFile>>,

    // UI state
    left_tab: LeftPanelTab,
    selected_file_path: Option<String>,
    selected_commit_file: Option<(String, String)>,
    expanded_commit: Option<String>,
    expanded_dirs: HashMap<String, bool>,
    loading: bool,
    loading_file: bool,
    error: Option<String>,
    side_by_side: bool,

    on_close: Option<CloseDiffViewCallback>,
    /// Scroll handle for the right-panel diff uniform_list (keyboard scrolling).
    diff_scroll_handle: UniformListScrollHandle,
    /// Current visible top row index (for keyboard scroll calculations).
    scroll_top_row: usize,
}

impl DiffViewOverlay {
    pub fn new(worktree_path: PathBuf, branch_name: String) -> Self {
        Self {
            worktree_path,
            branch_name,
            main_branch: String::new(),
            changed_files: Vec::new(),
            file_tree: Vec::new(),
            current_file_diff: None,
            cached_highlighted: None,
            diff_flat_rows: Vec::new(),
            diff_flat_rows_sbs: Vec::new(),
            commits: Vec::new(),
            commits_loaded: false,
            commit_files_cache: HashMap::new(),
            left_tab: LeftPanelTab::Files,
            selected_file_path: None,
            selected_commit_file: None,
            expanded_commit: None,
            expanded_dirs: HashMap::new(),
            loading: true,
            loading_file: false,
            error: None,
            side_by_side: true,
            on_close: None,
            diff_scroll_handle: UniformListScrollHandle::new(),
            scroll_top_row: 0,
        }
    }

    pub fn set_on_close(&mut self, callback: CloseDiffViewCallback) {
        self.on_close = Some(callback);
    }

    // --- Data loading (async, background thread) ---

    pub fn start_loading(&mut self, cx: &mut Context<Self>) {
        let worktree = self.worktree_path.clone();
        cx.spawn(async move |entity, cx| {
            let wt = worktree.clone();
            let (main_branch, files) = blocking::unblock(move || {
                let main = git_diff::detect_diff_base(&wt);
                let files = git_diff::changed_files(&wt);
                (main, files)
            })
            .await;
            let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                this.main_branch = main_branch;
                match files {
                    Ok(f) => {
                        this.file_tree = build_file_tree(&f);
                        this.changed_files = f;
                        this.loading = false;
                    }
                    Err(e) => {
                        this.error = Some(e.to_string());
                        this.loading = false;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_commits(&mut self, cx: &mut Context<Self>) {
        if self.commits_loaded {
            return;
        }
        self.commits_loaded = true;
        let worktree = self.worktree_path.clone();
        cx.spawn(async move |entity, cx| {
            let wt = worktree;
            let result = blocking::unblock(move || git_diff::commits(&wt)).await;
            let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                match result {
                    Ok(c) => this.commits = c,
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_commit_files(&mut self, hash: &str, cx: &mut Context<Self>) {
        if self.commit_files_cache.contains_key(hash) {
            return;
        }
        let worktree = self.worktree_path.clone();
        let hash_owned = hash.to_string();
        cx.spawn(async move |entity, cx| {
            let wt = worktree;
            let h = hash_owned.clone();
            let result = blocking::unblock(move || git_diff::commit_files(&wt, &h)).await;
            let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                match result {
                    Ok(files) => {
                        this.commit_files_cache.insert(hash_owned, files);
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_file_diff_by_path(&mut self, file_path: &str, cx: &mut Context<Self>) {
        self.selected_file_path = Some(file_path.to_string());
        self.selected_commit_file = None;
        self.loading_file = true;
        self.error = None;
        self.current_file_diff = None;
        self.cached_highlighted = None;
        self.diff_flat_rows.clear();
        self.diff_flat_rows_sbs.clear();
        self.scroll_top_row = 0;
        cx.notify();

        let worktree = self.worktree_path.clone();
        let fp = file_path.to_string();
        cx.spawn(async move |entity, cx| {
            let wt = worktree;
            let result = blocking::unblock(move || {
                let diff = git_diff::file_diff(&wt, &fp)?;
                let highlighted = highlight_diff_lines(&diff);
                let flat_rows = build_flat_rows(&diff);
                let flat_rows_sbs = build_flat_rows_sbs(&diff);
                Ok::<_, git_diff::GitDiffError>((diff, highlighted, flat_rows, flat_rows_sbs))
            })
            .await;
            let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                this.loading_file = false;
                match result {
                    Ok((d, hl, rows, rows_sbs)) => {
                        this.diff_flat_rows = rows;
                        this.diff_flat_rows_sbs = rows_sbs;
                        this.current_file_diff = Some(Arc::new(d));
                        this.cached_highlighted = Some(Arc::new(hl));
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_commit_file_diff(
        &mut self,
        file_path: &str,
        commit_hash: &str,
        cx: &mut Context<Self>,
    ) {
        self.selected_commit_file = Some((commit_hash.to_string(), file_path.to_string()));
        self.selected_file_path = None;
        self.loading_file = true;
        self.error = None;
        self.current_file_diff = None;
        self.cached_highlighted = None;
        self.diff_flat_rows.clear();
        self.diff_flat_rows_sbs.clear();
        self.scroll_top_row = 0;
        cx.notify();

        let worktree = self.worktree_path.clone();
        let fp = file_path.to_string();
        let hash = commit_hash.to_string();
        cx.spawn(async move |entity, cx| {
            let wt = worktree;
            let result = blocking::unblock(move || {
                let diff = git_diff::commit_file_diff(&wt, &hash, &fp)?;
                let highlighted = highlight_diff_lines(&diff);
                let flat_rows = build_flat_rows(&diff);
                let flat_rows_sbs = build_flat_rows_sbs(&diff);
                Ok::<_, git_diff::GitDiffError>((diff, highlighted, flat_rows, flat_rows_sbs))
            })
            .await;
            let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                this.loading_file = false;
                match result {
                    Ok((d, hl, rows, rows_sbs)) => {
                        this.diff_flat_rows = rows;
                        this.diff_flat_rows_sbs = rows_sbs;
                        this.current_file_diff = Some(Arc::new(d));
                        this.cached_highlighted = Some(Arc::new(hl));
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn reject_hunk(&mut self, hunk_idx: usize, cx: &mut Context<Self>) {
        let diff = match &self.current_file_diff {
            Some(d) => Arc::clone(d),
            None => return,
        };
        let hunk = match diff.hunks.get(hunk_idx) {
            Some(h) => h.clone(),
            None => return,
        };
        let file_path = diff.path.clone();
        let worktree = self.worktree_path.clone();
        cx.spawn(async move |entity, cx| {
            let wt = worktree;
            let h = hunk;
            let fp = file_path.clone();
            let result = blocking::unblock(move || git_diff::reject_hunk(&wt, &fp, &h)).await;
            let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                match result {
                    Ok(()) => this.load_file_diff_by_path(&file_path, cx),
                    Err(e) => {
                        this.error = Some(format!("Reject hunk failed: {}", e));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    // --- Rendering ---

    fn render_header(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let on_close = self.on_close.clone();
        // Button label shows what you *switch to* (standard toggle UX):
        // When currently in side-by-side mode → clicking switches to Unified
        // When currently in unified mode       → clicking switches to Side-by-Side
        let toggle_label = if self.side_by_side {
            "Unified"
        } else {
            "Side-by-Side"
        };
        let entity = cx.entity();

        div()
            .id("diff-view-header")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .pl(px(80.))
            .pr(px(16.))
            .h(px(36.))
            .border_b_1()
            .border_color(rgb(0x3d3d3d))
            .bg(rgb(0x252526))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child(SharedString::from(format!(
                        "Diff: {} vs {}",
                        self.branch_name, self.main_branch
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .id("diff-view-toggle")
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .bg(rgb(0x3d3d3d))
                            .text_color(rgb(0xcccccc))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
                            .on_click(move |_event, _window, cx| {
                                let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                                    this.side_by_side = !this.side_by_side;
                                    cx.notify();
                                });
                            })
                            .child(SharedString::from(toggle_label.to_string())),
                    )
                    .child(
                        div()
                            .id("diff-view-close-btn")
                            .px(px(12.))
                            .py(px(4.))
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
                            .child("Close (\u{2318}W)"),
                    ),
            )
    }

    fn render_left_panel_tabs(&self, cx: &mut Context<Self>) -> Div {
        let entity_files = cx.entity();
        let entity_changes = cx.entity();
        let is_files = self.left_tab == LeftPanelTab::Files;
        let is_changes = self.left_tab == LeftPanelTab::Changes;

        div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(rgb(0x3d3d3d))
            .child(
                div()
                    .id("tab-files")
                    .flex_1()
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(px(12.))
                    .text_color(if is_files { rgb(0xffffff) } else { rgb(0x888888) })
                    .cursor_pointer()
                    .when(is_files, |el| el.border_b_2().border_color(rgb(0x0066cc)))
                    .when(!is_files, |el| {
                        el.hover(|s: StyleRefinement| s.text_color(rgb(0xcccccc)))
                    })
                    .on_click(move |_event, _window, cx| {
                        let _ = entity_files.update(cx, |this: &mut DiffViewOverlay, cx| {
                            this.left_tab = LeftPanelTab::Files;
                            cx.notify();
                        });
                    })
                    .child("Files"),
            )
            .child(
                div()
                    .id("tab-changes")
                    .flex_1()
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(px(12.))
                    .text_color(if is_changes { rgb(0xffffff) } else { rgb(0x888888) })
                    .cursor_pointer()
                    .when(is_changes, |el| el.border_b_2().border_color(rgb(0x0066cc)))
                    .when(!is_changes, |el| {
                        el.hover(|s: StyleRefinement| s.text_color(rgb(0xcccccc)))
                    })
                    .on_click(move |_event, _window, cx| {
                        let _ = entity_changes.update(cx, |this: &mut DiffViewOverlay, cx| {
                            this.left_tab = LeftPanelTab::Changes;
                            this.load_commits(cx);
                            cx.notify();
                        });
                    })
                    .child("Changes"),
            )
    }

    fn render_left_panel_content(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.left_tab {
            LeftPanelTab::Files => self.render_files_tab(cx),
            LeftPanelTab::Changes => self.render_changes_tab(cx),
        }
    }

    fn render_files_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let tree = self.file_tree.clone();
        let selected = self.selected_file_path.clone();
        let expanded_dirs = self.expanded_dirs.clone();

        file_tree_panel::FileTreePanel::new(tree)
            .with_selected(selected)
            .with_expanded_dirs(expanded_dirs)
            .on_select(move |file_path, _window, cx| {
                let fp = file_path.to_string();
                let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                    this.load_file_diff_by_path(&fp, cx);
                });
            })
            .into_any_element()
    }

    fn render_changes_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity_expand = cx.entity();
        let entity_select = cx.entity();

        // Convert HashMap to Vec for ChangesPanel (small data, typically <50 commits)
        let commit_files_vec: Vec<(String, Vec<ChangedFile>)> = self
            .commit_files_cache
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        changes_panel::ChangesPanel::new(self.commits.clone())
            .with_commit_files(commit_files_vec)
            .with_expanded(self.expanded_commit.clone())
            .with_selected_file(self.selected_commit_file.clone())
            .on_expand_commit(move |hash, _window, cx| {
                let h = hash.to_string();
                let _ = entity_expand.update(cx, |this: &mut DiffViewOverlay, cx| {
                    if this.expanded_commit.as_deref() == Some(&h) {
                        this.expanded_commit = None;
                    } else {
                        this.expanded_commit = Some(h.clone());
                        this.load_commit_files(&h, cx);
                    }
                    cx.notify();
                });
            })
            .on_select_file(move |file_path, commit_hash, _window, cx| {
                let fp = file_path.to_string();
                let ch = commit_hash.to_string();
                let _ = entity_select.update(cx, |this: &mut DiffViewOverlay, cx| {
                    this.load_commit_file_diff(&fp, &ch, cx);
                });
            })
            .into_any_element()
    }

    /// Render the diff panel (right side) using uniform_list for virtualization.
    fn render_diff_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.loading_file {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(rgb(0x888888))
                        .child("Loading..."),
                )
                .into_any_element();
        }

        let diff = match &self.current_file_diff {
            None => return render_empty_state().into_any_element(),
            Some(d) => d,
        };

        if diff.is_binary {
            return render_binary_notice().into_any_element();
        }
        if diff.hunks.is_empty() {
            return render_no_changes().into_any_element();
        }

        // Use paired flat rows for side-by-side mode (proper removed/added alignment)
        let active_flat_rows = if self.side_by_side {
            &self.diff_flat_rows_sbs
        } else {
            &self.diff_flat_rows
        };
        let total_rows = active_flat_rows.len();
        if total_rows == 0 {
            return render_empty_state().into_any_element();
        }

        // Build reject callback (only for overall diff, not commit-specific)
        let is_overall_diff = self.selected_commit_file.is_none();
        let reject_cb: Option<RejectHunkCallback> = if is_overall_diff {
            let entity = cx.entity();
            Some(Arc::new(move |hunk_idx, _window, cx| {
                let _ = entity.update(cx, |this: &mut DiffViewOverlay, cx| {
                    this.reject_hunk(hunk_idx, cx);
                });
            }))
        } else {
            None
        };

        // uniform_list: only renders visible rows.
        // Use a mode-specific element ID so GPUI creates a fresh list (with reset
        // scroll position) whenever the user switches between unified and side-by-side.
        // Without this, GPUI preserves the scroll offset from the previous mode,
        // which can push some rows out of the visible viewport.
        let list_id = if self.side_by_side {
            "diff-content-sbs"
        } else {
            "diff-content-uni"
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family("monospace")
            .text_size(px(12.))
            .child(
                uniform_list(
                    list_id,
                    total_rows,
                    cx.processor(move |this, range: std::ops::Range<usize>, _window, _cx| {
                        let diff = match &this.current_file_diff {
                            Some(d) => d,
                            None => return Vec::new(),
                        };
                        let hl = this.cached_highlighted.as_deref().map(|v| v.as_slice());
                        let side_by_side = this.side_by_side;
                        let flat_rows = if side_by_side {
                            &this.diff_flat_rows_sbs
                        } else {
                            &this.diff_flat_rows
                        };

                        range
                            .map(|idx| {
                                if let Some(row) = flat_rows.get(idx) {
                                    render_diff_row(
                                        row,
                                        diff,
                                        hl,
                                        reject_cb.as_ref(),
                                        side_by_side,
                                    )
                                } else {
                                    div().into_any_element()
                                }
                            })
                            .collect()
                    }),
                )
                .flex_grow()
                .track_scroll(&self.diff_scroll_handle),
            )
            .into_any_element()
    }

    /// Scroll the diff panel by a number of rows (positive = down, negative = up).
    pub fn scroll_diff_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        let total = if self.side_by_side {
            self.diff_flat_rows_sbs.len()
        } else {
            self.diff_flat_rows.len()
        };
        if total == 0 {
            return;
        }
        let new_top = if delta < 0 {
            self.scroll_top_row.saturating_sub((-delta) as usize)
        } else {
            (self.scroll_top_row + delta as usize).min(total.saturating_sub(1))
        };
        self.scroll_top_row = new_top;
        self.diff_scroll_handle
            .scroll_to_item(new_top, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scroll the diff panel to top.
    pub fn scroll_diff_to_top(&mut self, cx: &mut Context<Self>) {
        self.scroll_top_row = 0;
        self.diff_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scroll the diff panel to bottom.
    pub fn scroll_diff_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.diff_scroll_handle.scroll_to_bottom();
        let total = if self.side_by_side {
            self.diff_flat_rows_sbs.len()
        } else {
            self.diff_flat_rows.len()
        };
        self.scroll_top_row = total.saturating_sub(1);
        cx.notify();
    }

    fn render_body(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.loading {
            return div()
                .id("diff-view-body")
                .flex_1()
                .min_h(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(rgb(0x888888))
                        .child("Loading..."),
                );
        }

        if let Some(ref err) = self.error {
            return div()
                .id("diff-view-body")
                .flex_1()
                .min_h(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(0xf44336))
                        .max_w(px(500.))
                        .child(SharedString::from(err.clone())),
                );
        }

        let diff_panel = self.render_diff_panel(cx);

        div()
            .id("diff-view-body")
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_row()
            .overflow_hidden()
            // Left panel
            .child(
                div()
                    .w(px(280.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgb(0x3d3d3d))
                    .bg(rgb(0x252526))
                    .overflow_hidden()
                    .child(self.render_left_panel_tabs(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_hidden()
                            .child(self.render_left_panel_content(cx)),
                    ),
            )
            // Right panel: virtualized diff
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(diff_panel),
            )
    }
}

/// Pre-compute syntax highlighting for all diff lines (called on background thread).
fn highlight_diff_lines(diff: &FileDiff) -> Vec<HighlightedLine> {
    let mut highlighter = LineHighlighter::new(&diff.path);
    let mut result = Vec::new();
    for hunk in &diff.hunks {
        for line in &hunk.lines {
            result.push(highlighter.highlight_line(&line.content));
        }
    }
    result
}

impl Render for DiffViewOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("diff-view-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .child(self.render_header(cx))
            .child(self.render_body(cx))
    }
}
